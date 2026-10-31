#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod hub;
mod process;

#[cfg(any(windows, test))]
fn target(args: &[String]) -> Result<String, String> {
    match args {
        [] => Ok("home".into()),
        [flag, page]
            if flag == "--open" && ["home", "setup", "health"].contains(&page.as_str()) =>
        {
            Ok(page.clone())
        }
        _ => Err("unsupported_launch_target".into()),
    }
}

fn hub_args(args: &[String]) -> Result<(bool, Option<&str>), &'static str> {
    let mut check = false;
    let mut component = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check" if !check => check = true,
            "--component" if component.is_none() => {
                let id = args.next().ok_or("hub_invalid_component")?;
                if !hub::valid_component(id) {
                    return Err("hub_invalid_component");
                }
                component = Some(id.as_str());
            }
            _ => return Err("hub_invalid_arguments"),
        }
    }
    Ok((check, component))
}

#[cfg(windows)]
fn desktop() {
    use tauri::{Emitter, Manager};
    struct State {
        target: std::sync::Mutex<String>,
        gate: tokio::sync::Semaphore,
        started: std::time::Instant,
        painted: std::sync::atomic::AtomicBool,
    }
    #[tauri::command]
    async fn load_hub(
        check: bool,
        component: Option<String>,
        state: tauri::State<'_, State>,
    ) -> Result<hub::HubReport, hub::LoadFailure> {
        let _permit = state
            .gate
            .try_acquire()
            .map_err(|_| hub::LoadFailure::new("hub_busy"))?;
        hub::decode_scoped(
            &process::hub(check, component.as_deref())
                .await
                .map_err(hub::LoadFailure::new)?,
            component.as_deref(),
        )
        .map_err(hub::LoadFailure::new)
    }
    #[tauri::command]
    fn launch_target(state: tauri::State<'_, State>) -> String {
        state.target.lock().unwrap().clone()
    }
    #[tauri::command]
    fn first_frame(state: tauri::State<'_, State>) {
        if !state
            .painted
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            println!(
                "{}",
                serde_json::json!({"event":"first_frame", "pid":std::process::id(), "elapsed_ms":state.started.elapsed().as_secs_f64()*1000.0})
            );
        }
    }
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let mut smoke_ms = None;
    if args.first().is_some_and(|a| a == "--smoke-test-ms") {
        smoke_ms = args
            .get(1)
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| (1000..=60000).contains(v));
        if smoke_ms.is_none() {
            std::process::exit(2);
        }
        args.drain(..2);
    }
    let initial = match target(&args) {
        Ok(page) => page,
        Err(_) => std::process::exit(2),
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if let Ok(page) = target(&args.into_iter().skip(1).collect::<Vec<_>>()) {
                *app.state::<State>().target.lock().unwrap() = page.clone();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                    let _ = app.emit_to("main", "workspace-target", page);
                }
            }
        }))
        .manage(State {
            target: std::sync::Mutex::new(initial),
            gate: tokio::sync::Semaphore::new(1),
            started: std::time::Instant::now(),
            painted: std::sync::atomic::AtomicBool::new(false),
        })
        .setup(move |app| {
            if let Some(ms) = smoke_ms {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                    handle.exit(0);
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_hub,
            launch_target,
            first_frame
        ])
        .run(tauri::generate_context!())
        .expect("workspace runtime");
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--export-types") {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/contracts/hub.ts");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, hub::types()).unwrap();
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("--hub-json") {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let args: Vec<_> = std::env::args().skip(2).collect();
        let (check, component) = match hub_args(&args) {
            Ok(parsed) => parsed,
            Err(code) => {
                eprintln!("{code}");
                std::process::exit(2);
            }
        };
        let result = runtime.block_on(async {
            hub::decode_scoped(&process::hub(check, component).await?, component)
        });
        match result {
            Ok(report) => println!("{}", serde_json::to_string(&report).unwrap()),
            Err(code) => {
                eprintln!(
                    "{}",
                    serde_json::to_string(&hub::LoadFailure::new(code)).unwrap()
                );
                std::process::exit(1);
            }
        }
        return;
    }
    #[cfg(windows)]
    desktop();
    #[cfg(not(windows))]
    eprintln!("The desktop shell requires Windows; use npm run dev for the frontend.");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn headless_checks_require_explicit_valid_arguments() {
        assert_eq!(
            hub_args(&[
                "--check".into(),
                "--component".into(),
                "jenkins-bridge".into()
            ])
            .unwrap(),
            (true, Some("jenkins-bridge"))
        );
        for args in [
            vec!["--component"],
            vec!["--component", "bad;command"],
            vec!["--check", "--check"],
            vec!["--unknown"],
        ] {
            assert!(hub_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).is_err());
        }
    }
    #[test]
    fn accepts_only_implemented_routes_and_literal_arguments() {
        assert_eq!(target(&[]).unwrap(), "home");
        assert_eq!(target(&["--open".into(), "setup".into()]).unwrap(), "setup");
        assert!(target(&["--open".into(), "board;calc".into()]).is_err());
        assert!(target(&["--shell".into()]).is_err());
    }
}

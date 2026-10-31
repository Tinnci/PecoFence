#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod hub;
mod process;
mod setup;

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
        setup_gate: tokio::sync::Semaphore,
        /// The private template copy and the sha256 of its last plan.
        pending: std::sync::Mutex<Option<(std::path::PathBuf, String)>>,
    }
    fn spm_exe() -> Result<std::path::PathBuf, setup::SetupFailure> {
        setup::locate(
            std::env::var_os("TINNCI_SPM_EXE").map(Into::into),
            std::env::var_os("USERPROFILE")
                .map(std::path::PathBuf::from)
                .as_deref(),
        )
    }
    fn template_arg(path: &std::path::Path) -> Result<&str, setup::SetupFailure> {
        path.to_str()
            .ok_or_else(|| setup::SetupFailure::new("template_unreadable"))
    }
    #[tauri::command]
    async fn template_plan(
        app: tauri::AppHandle,
        state: tauri::State<'_, State>,
    ) -> Result<setup::ImportPlan, setup::SetupFailure> {
        use tauri_plugin_dialog::DialogExt;
        let _permit = state
            .setup_gate
            .try_acquire()
            .map_err(|_| setup::SetupFailure::new("setup_busy"))?;
        *state.pending.lock().unwrap() = None;
        let spm = spm_exe()?;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        app.dialog()
            .file()
            .set_title("选择项目模板")
            .add_filter("SPM 项目模板", &["json"])
            .pick_file(move |path| {
                let _ = sender.send(path);
            });
        let source = receiver
            .await
            .ok()
            .flatten()
            .and_then(|path| path.into_path().ok())
            .ok_or_else(|| setup::SetupFailure::new("template_cancelled"))?;
        let cache = app
            .path()
            .app_cache_dir()
            .map_err(|_| setup::SetupFailure::new("template_unreadable"))?;
        let copy = setup::snapshot(&source, &cache)?;
        let output = process::spm(
            &spm,
            &setup::import_args(template_arg(&copy)?, false, false),
        )
        .await?;
        let plan = setup::decode_plan(output.code, &output.stdout)?;
        *state.pending.lock().unwrap() = Some((copy, plan.template.sha256.clone()));
        Ok(plan)
    }
    #[tauri::command]
    async fn template_apply(
        sha256: String,
        replace: bool,
        state: tauri::State<'_, State>,
    ) -> Result<setup::ImportPlan, setup::SetupFailure> {
        let _permit = state
            .setup_gate
            .try_acquire()
            .map_err(|_| setup::SetupFailure::new("setup_busy"))?;
        let (copy, planned) = state
            .pending
            .lock()
            .unwrap()
            .clone()
            .filter(|(_, planned)| *planned == sha256)
            .ok_or_else(|| setup::SetupFailure::new("template_stale"))?;
        let output = process::spm(
            &spm_exe()?,
            &setup::import_args(template_arg(&copy)?, true, replace),
        )
        .await?;
        let plan = setup::decode_plan(output.code, &output.stdout)?;
        if plan.template.sha256 != planned {
            return Err(setup::SetupFailure::new("template_stale"));
        }
        *state.pending.lock().unwrap() = None;
        let _ = std::fs::remove_file(copy);
        Ok(plan)
    }
    #[tauri::command]
    async fn project_status(
        state: tauri::State<'_, State>,
    ) -> Result<setup::ProjectStatus, setup::SetupFailure> {
        let _permit = state
            .setup_gate
            .try_acquire()
            .map_err(|_| setup::SetupFailure::new("setup_busy"))?;
        let output = process::spm(&spm_exe()?, &["setup", "status"]).await?;
        setup::decode_status(output.code, &output.stdout)
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
        .plugin(tauri_plugin_dialog::init())
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
            setup_gate: tokio::sync::Semaphore::new(1),
            pending: std::sync::Mutex::new(None),
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
            first_frame,
            template_plan,
            template_apply,
            project_status
        ])
        .run(tauri::generate_context!())
        .expect("workspace runtime");
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--export-types") {
        let contracts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/contracts");
        std::fs::create_dir_all(&contracts).unwrap();
        std::fs::write(contracts.join("hub.ts"), hub::types()).unwrap();
        std::fs::write(contracts.join("setup.ts"), setup::types()).unwrap();
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

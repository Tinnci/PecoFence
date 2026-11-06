#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod hub;
#[cfg_attr(not(windows), allow(dead_code))]
mod logbook;
mod process;
mod setup;
#[cfg(any(windows, test))]
mod snap;

#[cfg(any(windows, test))]
fn target(args: &[String]) -> Result<String, String> {
    match args {
        [] => Ok("home".into()),
        [flag, page]
            if flag == "--open"
                && (["home", "setup", "health", "project", "activity"]
                    .contains(&page.as_str())
                    || page.strip_prefix("model:").is_some_and(|rest| {
                        let (code, tab) = rest.split_once(':').unwrap_or((rest, "overview"));
                        setup::valid_code(code)
                            && [
                                "overview",
                                "issues",
                                "requirements",
                                "branches",
                                "builds",
                                "checklist",
                                "settings",
                            ]
                            .contains(&tab)
                    })) =>
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
        connection_gate: tokio::sync::Semaphore,
        /// One discovery or draft at a time; later requests wait their turn.
        discover_gate: tokio::sync::Semaphore,
        /// Focus reads and writes; a summary can take minutes, so it has its own gate.
        focus_gate: tokio::sync::Semaphore,
        /// Issue and requirement reads: mostly incremental and short, two at a time.
        items_gate: tokio::sync::Semaphore,
        /// Histories never wait behind a multi-minute summary read.
        flow_gate: tokio::sync::Semaphore,
        source_fields_gate: tokio::sync::Semaphore,
        /// Health reads run beside an activation so the page can show its progress.
        service_gate: tokio::sync::Semaphore,
        /// The backup last chosen for restore; the page never names a path.
        backup: std::sync::Mutex<Option<std::path::PathBuf>>,
        /// The private template copy and the sha256 of its last plan.
        pending: std::sync::Mutex<Option<(std::path::PathBuf, String)>>,
    }
    const SETUP_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);
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
    /// Saves a PNG the page drew (an exported view) where the user picks; returns the path,
    /// or None when the dialog was cancelled. Only PNG bytes are written.
    #[tauri::command]
    async fn save_png(
        app: tauri::AppHandle,
        name: String,
        png: Vec<u8>,
    ) -> Result<Option<String>, setup::SetupFailure> {
        use tauri_plugin_dialog::DialogExt;
        if png.len() > 64 * 1024 * 1024 || !png.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(setup::SetupFailure::new("invalid_input"));
        }
        let mut file: String = name
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
            .take(80)
            .collect();
        if !file.to_lowercase().ends_with(".png") {
            file.push_str(".png");
        }
        let (sender, receiver) = tokio::sync::oneshot::channel();
        app.dialog()
            .file()
            .set_title("导出视图")
            .add_filter("PNG", &["png"])
            .set_file_name(&file)
            .save_file(move |path| {
                let _ = sender.send(path);
            });
        let Some(path) = receiver
            .await
            .ok()
            .flatten()
            .and_then(|p| p.into_path().ok())
        else {
            return Ok(None);
        };
        std::fs::write(&path, &png).map_err(|_| setup::SetupFailure::new("export_failed"))?;
        Ok(Some(path.display().to_string()))
    }
    /// Takes a project off the list, or brings it back; SPM refuses one the service runs.
    #[tauri::command]
    async fn project_change(
        state: tauri::State<'_, State>,
        verb: String,
        project: String,
        scope: String,
    ) -> Result<(), setup::SetupFailure> {
        let args = setup::project_args(&verb, &project, &scope)
            .ok_or_else(|| setup::SetupFailure::new("invalid_input"))?;
        let _permit = state
            .setup_gate
            .try_acquire()
            .map_err(|_| setup::SetupFailure::new("setup_busy"))?;
        let output = process::spm(&spm_exe()?, &args, SETUP_DEADLINE).await?;
        setup::decode_focus(output.code, &output.stdout).map(|_| ())
    }
    /// Saves a backup where the user picks; SPM writes it, so its content never reaches the page.
    #[tauri::command]
    async fn backup_export(
        app: tauri::AppHandle,
        name: String,
    ) -> Result<Option<setup::FocusData>, setup::SetupFailure> {
        use tauri_plugin_dialog::DialogExt;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        app.dialog()
            .file()
            .add_filter("JSON", &["json"])
            .set_file_name(&name)
            .save_file(move |path| {
                let _ = sender.send(path);
            });
        let Some(path) = receiver
            .await
            .ok()
            .flatten()
            .and_then(|p| p.into_path().ok())
        else {
            return Ok(None);
        };
        let path = template_arg(&path)?;
        let args = ["focus", "backup", "--output", path];
        let output = process::spm(&spm_exe()?, &args, SETUP_DEADLINE).await?;
        setup::decode_focus(output.code, &output.stdout).map(Some)
    }
    /// Asks which backup to restore and shows what it would change.
    #[tauri::command]
    async fn backup_plan(
        app: tauri::AppHandle,
        state: tauri::State<'_, State>,
    ) -> Result<Option<setup::FocusData>, setup::SetupFailure> {
        use tauri_plugin_dialog::DialogExt;
        *state.backup.lock().unwrap() = None;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        app.dialog()
            .file()
            .add_filter("JSON", &["json"])
            .pick_file(move |path| {
                let _ = sender.send(path);
            });
        let Some(path) = receiver
            .await
            .ok()
            .flatten()
            .and_then(|p| p.into_path().ok())
        else {
            return Ok(None);
        };
        let args = ["focus", "backup-restore", "--input", template_arg(&path)?];
        let output = process::spm(&spm_exe()?, &args, SETUP_DEADLINE).await?;
        let data = setup::decode_focus(output.code, &output.stdout)?;
        *state.backup.lock().unwrap() = Some(path);
        Ok(Some(data))
    }
    #[tauri::command]
    async fn backup_apply(
        state: tauri::State<'_, State>,
    ) -> Result<setup::FocusData, setup::SetupFailure> {
        let path = state
            .backup
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| setup::SetupFailure::new("invalid_input"))?;
        let _permit = state
            .focus_gate
            .acquire()
            .await
            .map_err(|_| setup::SetupFailure::new("setup_busy"))?;
        let path = template_arg(&path)?;
        let args = ["focus", "backup-restore", "--input", path, "--apply"];
        let output = process::spm(&spm_exe()?, &args, SETUP_DEADLINE).await?;
        setup::decode_focus(output.code, &output.stdout)
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
            SETUP_DEADLINE,
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
            SETUP_DEADLINE,
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
        let output = process::spm(&spm_exe()?, &["setup", "status"], SETUP_DEADLINE).await?;
        setup::decode_status(output.code, &output.stdout)
    }
    /// Each source tool's own login, observed by `spm connections` (probes run concurrently).
    #[tauri::command]
    async fn connections(
        state: tauri::State<'_, State>,
    ) -> Result<setup::ConnectionReport, setup::SetupFailure> {
        let _permit = state
            .connection_gate
            .try_acquire()
            .map_err(|_| setup::SetupFailure::new("setup_busy"))?;
        let output = process::spm(
            &spm_exe()?,
            &["connections"],
            std::time::Duration::from_secs(90),
        )
        .await?;
        setup::decode_connections(output.code, &output.stdout)
    }
    /// One discovery list through `spm discover`; the arguments are validated keys.
    #[tauri::command]
    async fn discover(
        list: String,
        args: setup::DiscoverArgs,
        state: tauri::State<'_, State>,
    ) -> Result<setup::Discovery, setup::SetupFailure> {
        let args = setup::discover_args(&list, &args)
            .ok_or_else(|| setup::SetupFailure::new("invalid_input"))?;
        let _permit = state
            .discover_gate
            .acquire()
            .await
            .map_err(|_| setup::SetupFailure::new("setup_busy"))?;
        let output = process::spm(&spm_exe()?, &args, std::time::Duration::from_secs(90)).await?;
        setup::decode_discovery(output.code, &output.stdout)
    }
    /// `spm setup draft`: resolves the scope and detects mappings; with `sha256`, applies the
    /// draft only if it still matches what the user confirmed.
    #[tauri::command]
    async fn draft(
        choices: setup::DraftChoices,
        sha256: Option<String>,
        state: tauri::State<'_, State>,
    ) -> Result<setup::Draft, setup::SetupFailure> {
        let args = setup::draft_args(sha256.is_some(), sha256.as_deref())
            .ok_or_else(|| setup::SetupFailure::new("invalid_input"))?;
        let input =
            serde_json::to_vec(&choices).map_err(|_| setup::SetupFailure::new("invalid_input"))?;
        let _permit = state
            .discover_gate
            .acquire()
            .await
            .map_err(|_| setup::SetupFailure::new("setup_busy"))?;
        let output = process::spm_with(
            &spm_exe()?,
            &args,
            Some(&input),
            std::time::Duration::from_secs(240),
        )
        .await?;
        setup::decode_draft(output.code, &output.stdout)
    }
    /// `spm setup activate`: switches the background service to the project and waits for
    /// its first snapshot.
    #[tauri::command]
    async fn activate(
        project: String,
        scope: String,
        state: tauri::State<'_, State>,
    ) -> Result<setup::Activation, setup::SetupFailure> {
        let args = setup::activate_args(&project, &scope)
            .ok_or_else(|| setup::SetupFailure::new("invalid_input"))?;
        let _permit = state
            .discover_gate
            .acquire()
            .await
            .map_err(|_| setup::SetupFailure::new("setup_busy"))?;
        let output = process::spm(&spm_exe()?, &args, std::time::Duration::from_secs(360)).await?;
        setup::decode_activation(output.code, &output.stdout)
    }
    /// The background service's health: project, gates and unmapped values.
    #[tauri::command]
    async fn service_status(
        state: tauri::State<'_, State>,
    ) -> Result<setup::ServiceHealth, setup::SetupFailure> {
        let _permit = state
            .service_gate
            .try_acquire()
            .map_err(|_| setup::SetupFailure::new("setup_busy"))?;
        let output = process::spm(
            &spm_exe()?,
            &["setup", "service-status"],
            std::time::Duration::from_secs(20),
        )
        .await?;
        setup::decode_service(output.code, &output.stdout)
    }
    /// `spm focus`: list, suggest, save, remove, summary (fresh or cached), refresh one model,
    /// review a change. `model` is written to stdin for save.
    #[tauri::command]
    async fn focus(
        action: String,
        job: Option<String>,
        build: Option<u32>,
        code: Option<String>,
        change: Option<u32>,
        review: Option<String>,
        branch: Option<String>,
        model: Option<setup::FocusModel>,
        source: Option<String>,
        item: Option<String>,
        op: Option<serde_json::Value>,
        path: Option<String>,
        items: Option<Vec<String>>,
        addresses: Option<Vec<String>>,
        instance: Option<String>,
        chip: Option<String>,
        baseband: Option<String>,
        query: Option<String>,
        alias: Option<String>,
        state: tauri::State<'_, State>,
    ) -> Result<setup::FocusData, setup::SetupFailure> {
        let request = setup::FocusRequest {
            action: action.clone(),
            code,
            job,
            build,
            change,
            review,
            branch,
            source,
            item,
            path,
            items,
            addresses,
            instance,
            chip,
            baseband,
            query,
            alias,
        };
        let args =
            setup::route_args(&request).ok_or_else(|| setup::SetupFailure::new("invalid_input"))?;
        let input = match (action.as_str(), &model, &op) {
            ("save" | "preview", Some(model), _) => Some(
                serde_json::to_vec(model).map_err(|_| setup::SetupFailure::new("invalid_input"))?,
            ),
            // SPM validates the operation; only its shape is checked here.
            ("annotate", _, Some(op)) if op.is_object() => Some(op.to_string().into_bytes()),
            ("save" | "preview" | "annotate", _, _) => {
                return Err(setup::SetupFailure::new("invalid_input"));
            }
            _ => None,
        };
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        // Cached and offline reads, and Jenkins reads, never wait behind a running summary;
        // issue and requirement reads have a gate of their own.
        let gate = match action.as_str() {
            "cached"
            | "list"
            | "preview"
            | "changes"
            | "builds"
            | "builds_cached"
            | "build_evidence"
            | "issues_cached"
            | "requirements_cached"
            | "annotate"
            | "history"
            | "related"
            | "platforms"
            | "logs"
            | "traces"
            | "search"
            | "mine"
            | "mine_cached"
            | "needs"
            | "needs_cached"
            | "forecast"
            | "calendar"
            | "calendar_cached"
            | "profile"
            | "profile_refresh"
            | "repos"
            | "chip" => None,
            "issues" | "requirements" | "issues_full" | "requirements_full" => {
                Some(&state.items_gate)
            }
            "flow" | "flows" | "flows_refresh" => Some(&state.flow_gate),
            "source_fields" | "source_fields_refresh" => Some(&state.source_fields_gate),
            _ => Some(&state.focus_gate),
        };
        let _permit = match gate {
            Some(gate) => Some(
                gate.acquire()
                    .await
                    .map_err(|_| setup::SetupFailure::new("setup_busy"))?,
            ),
            None => None,
        };
        let deadline = std::time::Duration::from_secs(match action.as_str() {
            "summary" | "refresh" => 600,
            "suggest" => 180,
            "calendar" | "profile_refresh" => 150,
            "builds" | "build_evidence" => 120,
            "change_files"
            | "change_diff"
            | "change_diffs"
            | "flow"
            | "flows"
            | "flows_refresh"
            | "source_fields"
            | "source_fields_refresh" => 90,
            "issues" | "requirements" | "issues_full" | "requirements_full" => 300,
            _ => 30,
        });
        let started = std::time::Instant::now();
        let run_id = logbook::new_run_id();
        let spm = spm_exe()?;
        // Reading the log is what a live page does every second; it stays out of the log.
        let quiet = matches!(action.as_str(), "logs" | "traces");
        if !quiet {
            logbook::record(
                "info",
                "focus.started",
                serde_json::json!({ "action": action, "span": "focus", "run_id": run_id }),
            );
        }
        let result = match process::spm_run(&spm, &args, input.as_deref(), &run_id, deadline).await
        {
            Ok(output) => setup::decode_focus(output.code, &output.stdout),
            Err(failure) => Err(failure),
        };
        if !quiet {
            note_focus(&action, &run_id, started.elapsed(), &result);
        }
        result
    }
    /// Closes the call's span; a failure is a warning.
    fn note_focus(
        action: &str,
        run_id: &str,
        took: std::time::Duration,
        result: &Result<setup::FocusData, setup::SetupFailure>,
    ) {
        let ms = took.as_millis() as u64;
        match result {
            Err(failure) => logbook::record(
                "warn",
                "focus.failed",
                serde_json::json!({ "action": action, "ms": ms, "code": failure.code, "span": "focus", "run_id": run_id }),
            ),
            Ok(_) => logbook::record(
                "info",
                "focus.finished",
                serde_json::json!({ "action": action, "ms": ms, "span": "focus", "run_id": run_id }),
            ),
        }
    }
    /// A frontend error: its type and where it happened. Its message can hold anything.
    #[tauri::command]
    fn client_error(kind: String, name: String, at: String) {
        logbook::record(
            "error",
            "client.error",
            serde_json::json!({
                "kind": logbook::plain(&kind),
                "name": logbook::plain(&name),
                "at": logbook::plain(&at),
            }),
        );
    }
    /// Adds stage mappings for values that appeared after creation.
    #[tauri::command]
    async fn remap(
        project: String,
        scope: String,
        stages: std::collections::BTreeMap<String, String>,
        state: tauri::State<'_, State>,
    ) -> Result<setup::ImportPlan, setup::SetupFailure> {
        let args = setup::remap_args(&project, &scope, &stages)
            .ok_or_else(|| setup::SetupFailure::new("invalid_input"))?;
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let _permit = state
            .discover_gate
            .acquire()
            .await
            .map_err(|_| setup::SetupFailure::new("setup_busy"))?;
        let output = process::spm(&spm_exe()?, &args, SETUP_DEADLINE).await?;
        setup::decode_plan(output.code, &output.stdout)
    }
    /// Runs the tool's own browser login; only Meegle offers one.
    #[tauri::command]
    async fn connection_sign_in(
        source: String,
        state: tauri::State<'_, State>,
    ) -> Result<setup::ConnectionReport, setup::SetupFailure> {
        if source != "meegle" {
            return Err(setup::SetupFailure::new("sign_in_unsupported"));
        }
        let _permit = state
            .connection_gate
            .try_acquire()
            .map_err(|_| setup::SetupFailure::new("setup_busy"))?;
        let output = process::spm(
            &spm_exe()?,
            &["connections", "sign-in", "meegle"],
            std::time::Duration::from_secs(630),
        )
        .await?;
        setup::decode_sign_in(output.code, &output.stdout)
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
    /// The page's first script calls this once it can paint the splash in the saved theme.
    #[tauri::command]
    fn show_main(window: tauri::WebviewWindow) {
        let _ = window.show();
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
    clear_replaced();
    // What only this machine holds is saved once a day; a failure must not disturb the page.
    tauri::async_runtime::spawn(async {
        if let Ok(spm) = spm_exe() {
            let args = ["focus", "backup", "--daily"];
            let _ = process::spm(&spm, &args, SETUP_DEADLINE).await;
        }
    });
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
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
            connection_gate: tokio::sync::Semaphore::new(1),
            discover_gate: tokio::sync::Semaphore::new(1),
            service_gate: tokio::sync::Semaphore::new(1),
            focus_gate: tokio::sync::Semaphore::new(1),
            flow_gate: tokio::sync::Semaphore::new(1),
            source_fields_gate: tokio::sync::Semaphore::new(2),
            items_gate: tokio::sync::Semaphore::new(2),
            backup: std::sync::Mutex::new(None),
            pending: std::sync::Mutex::new(None),
        })
        .setup(move |app| {
            // A page that fails to load must not leave the window hidden.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                if let Some(window) = handle.get_webview_window("main") {
                    let _ = window.show();
                }
            });
            #[cfg(windows)]
            if let Err(error) = snap::install(app.handle()) {
                logbook::record(
                    "warn",
                    "snap.overlay_unavailable",
                    serde_json::json!({ "error": format!("{error:?}") }),
                );
            }
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
            show_main,
            client_error,
            template_plan,
            template_apply,
            project_status,
            connections,
            connection_sign_in,
            discover,
            draft,
            activate,
            service_status,
            remap,
            focus,
            save_png,
            project_change,
            backup_export,
            backup_plan,
            backup_apply
        ])
        .run(tauri::generate_context!())
        .expect("workspace runtime");
}

/// An install that replaced the running exe left it beside the new one as `<exe>.old*`.
/// The window still running from one keeps it locked, and removal just fails.
#[cfg_attr(not(windows), allow(dead_code))]
fn clear_replaced() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let (Some(dir), Some(name)) = (exe.parent(), exe.file_name().and_then(|n| n.to_str())) else {
        return;
    };
    for path in replaced(dir, name) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn replaced(dir: &std::path::Path, exe: &str) -> Vec<std::path::PathBuf> {
    let old = format!("{exe}.old");
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|f| f.file_name().to_str().is_some_and(|n| n.starts_with(&old)))
        .map(|f| f.path())
        .collect()
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
    {
        logbook::watch_panics();
        logbook::record(
            "info",
            "app.started",
            serde_json::json!({ "version": env!("CARGO_PKG_VERSION") }),
        );
        desktop();
    }
    #[cfg(not(windows))]
    eprintln!("The desktop shell requires Windows; use npm run dev for the frontend.");
}
#[cfg(test)]
mod tests {
    #[test]
    fn only_replaced_copies_of_this_exe_are_cleared() {
        let dir = std::env::temp_dir().join(format!("workspace-old-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["app.exe", "app.exe.old", "app.exe.old3", "other.exe.old"] {
            std::fs::write(dir.join(name), b"").unwrap();
        }
        let mut found: Vec<_> = super::replaced(&dir, "app.exe")
            .iter()
            .map(|p| p.file_name().unwrap().to_owned())
            .collect();
        found.sort();
        assert_eq!(found, ["app.exe.old", "app.exe.old3"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

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
        assert_eq!(
            target(&["--open".into(), "model:E6751SM".into()]).unwrap(),
            "model:E6751SM"
        );
        assert!(target(&["--open".into(), "model:E6751SM&calc".into()]).is_err());
        assert!(target(&["--open".into(), "model:E6751SM:builds".into()]).is_ok());
        assert!(target(&["--open".into(), "model:E6751SM:shell".into()]).is_err());
        assert!(target(&["--shell".into()]).is_err());
    }
}

//! PecoFence — open-source Fences-style desktop organizer for Windows 11.
// GUI subsystem: no console window when launched from Explorer. Logs go to a file (see
// `init_logging`); stderr is still used when a console is attached (e.g. `cargo run`).
#![cfg_attr(not(test), windows_subsystem = "windows")]

mod anchor;
mod app;
mod commands;
mod fence_window;
mod icons;
mod layout;
mod modern_menu;
mod peek;
mod persistence;
mod portal_runtime;
mod rename;
mod settings_host;
mod settings_ui;
mod shadow;
mod spm_transport;
mod state;

use pecofence_platform::com::OleGuard;
use pecofence_platform::window;
use pecofence_shell_contract::{ShellAction, ShellCommand};
use windows_core::Result;

/// WinUI owns the one STA pump, while this resource owns the existing native
/// desktop application. Native COM/windows must retire before OLE is uninitialized.
struct NativeLifetime {
    cell: app::AppCell,
    _ole: OleGuard,
}

impl Drop for NativeLifetime {
    fn drop(&mut self) {
        let application = self.cell.borrow_mut().take();
        if let Some(mut application) = application {
            application.shutdown();
        }
    }
}

fn parse_args() -> app::Args {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| -> Option<String> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let shell_action = value("--shell-command")
        .and_then(|name| ShellAction::from_cli_name(&name))
        .map(|action| {
            let point = value("--shell-x")
                .and_then(|x| x.parse::<i32>().ok())
                .zip(value("--shell-y").and_then(|y| y.parse::<i32>().ok()))
                .unwrap_or_else(|| {
                    let point = window::cursor_pos();
                    (point.x, point.y)
                });
            ShellCommand::new(action, point.0, point.1)
        });
    app::Args {
        light: has("--light"),
        dark: has("--dark"),
        wallpaper_override: value("--wallpaper"),
        portable: has("--portable"),
        config_dir: value("--config-dir").map(std::path::PathBuf::from),
        no_hide_icons: has("--no-hide-icons"),
        exit_after_ms: value("--exit-after").and_then(|v| v.parse().ok()),
        dump_stats: has("--dump-stats"),
        open_settings: has("--open-settings"),
        shell_action,
        portal: value("--portal"),
        test_script: value("--test-script"),
    }
}

/// Log file next to the config: `%LOCALAPPDATA%\PecoFence\pecofence.log` (truncated per run).
fn log_file_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)?;
    let dir = base.join("PecoFence");
    std::fs::create_dir_all(&dir).ok()?;
    // A second instance (PECOFENCE_INSTANCE) logs to its own file instead of truncating the
    // main one.
    let name = match pecofence_core::brand::var("PECOFENCE_INSTANCE") {
        Ok(n) if !n.trim().is_empty() => format!("pecofence.{}.log", n.trim()),
        _ => "pecofence.log".to_string(),
    };
    Some(dir.join(name))
}

/// `RUST_LOG` filters as usual (default `info`). Output goes to the log file and to stderr; the
/// latter only shows up when a console is attached.
fn init_logging() {
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let file = log_file_path().and_then(|p| std::fs::File::create(p).ok());
    match file {
        Some(file) => {
            let file = std::sync::Mutex::new(file);
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_ansi(false)
                .with_writer(file.and(std::io::stderr))
                .init();
        }
        None => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(std::io::stderr)
                .init();
        }
    }
}

/// Panics inside a window procedure or COM callback abort the process (GUI subsystem: no
/// console to print to), so the message and a backtrace go to the log first.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let bt = std::backtrace::Backtrace::force_capture();
        tracing::error!(%location, "PANIC: {info}
{bt}");
    }));
}

fn main() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() == Some(std::ffi::OsStr::new("--validate-workspace")) {
        let path = arguments.next().ok_or_else(|| {
            windows_core::Error::new(
                windows_core::HRESULT(0x80070057u32 as i32),
                "workspace path required",
            )
        })?;
        return pecofence_core::ConfigStore::parse_file(std::path::Path::new(&path))
            .map(|_| ())
            .map_err(|error| {
                windows_core::Error::new(windows_core::HRESULT(0x80070057u32 as i32), error)
            });
    }
    if std::env::args().any(|arg| arg == "--probe-spm") {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                windows_core::Error::new(
                    windows_core::HRESULT(0x80004005u32 as i32),
                    error.to_string(),
                )
            })?;
        return runtime
            .block_on(spm_transport::probe_current())
            .map_err(|error| {
                windows_core::Error::new(
                    windows_core::HRESULT(0x80004005u32 as i32),
                    error.to_string(),
                )
            });
    }
    init_logging();
    install_panic_hook();
    if let Some(dir) = log_file_path().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        let instance =
            pecofence_core::brand::var("PECOFENCE_INSTANCE").unwrap_or_else(|_| "main".into());
        pecofence_platform::crashlog::install(dir, instance.trim());
    }

    let args = parse_args();
    let instance_name = pecofence_core::brand::var("PECOFENCE_INSTANCE").ok();
    let main_instance = instance_name
        .as_deref()
        .is_none_or(|name| name.trim().is_empty());

    if let Some(action) = args.shell_action
        && main_instance
    {
        match window::send_shell_action(action) {
            window::ShellDelivery::Delivered => return Ok(()),
            window::ShellDelivery::Uncertain => {
                tracing::warn!(
                    "shell action delivery timed out or was rejected; avoiding duplicate execution"
                );
                return Ok(());
            }
            window::ShellDelivery::NoInstance => {}
        }
    }

    window::set_process_dpi_awareness_v2();

    // `PECOFENCE_INSTANCE=<name>` runs a second, independent instance (developer testing with
    // `--portable`); the default name keeps one PecoFence per session.
    let [current_name, legacy_name] =
        pecofence_core::brand::instance_mutex_names(instance_name.as_deref());
    let Some(_instance) = window::SingleInstance::acquire(&current_name) else {
        if let Some(action) = args.shell_action
            && main_instance
        {
            for _ in 0..20 {
                std::thread::sleep(std::time::Duration::from_millis(50));
                match window::send_shell_action(action) {
                    window::ShellDelivery::Delivered => return Ok(()),
                    window::ShellDelivery::Uncertain => {
                        tracing::warn!(
                            "shell action delivery timed out or was rejected; avoiding duplicate execution"
                        );
                        return Ok(());
                    }
                    window::ShellDelivery::NoInstance => {}
                }
            }
        }
        tracing::warn!("another PecoFence instance is running; exiting");
        return Ok(());
    };
    let Some(_legacy_instance) = window::SingleInstance::acquire(&legacy_name) else {
        tracing::warn!("a pre-rename instance is running; exit it before starting PecoFence");
        return Ok(());
    };

    // Unlike run_component, run_with retains this lifetime with zero Settings
    // windows. WinUI pumps the native HWNDs, timers and posted command queue too.
    windows_reactor::App::run_with(move |context| {
        let ole = OleGuard::init()?;
        let cell = app::App::create(args, context.clone())?;
        Ok(NativeLifetime { cell, _ole: ole })
    })
}

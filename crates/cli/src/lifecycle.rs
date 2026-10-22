//! `status` and `quit`: the running app's lifetime, from its session record
//! (`pecofence_core::session`) and the process itself.

use super::{Env, Failure, Options, Output};
use pecofence_core::session::{self, ExitKind, Session, State};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Default and largest `quit --wait`, in seconds.
pub const DEFAULT_WAIT: u32 = 30;
pub const MAX_WAIT: u32 = 300;

fn local_dir(env: &Env) -> Result<PathBuf, Failure> {
    (env.var)("LOCALAPPDATA")
        .filter(|value| !value.trim().is_empty())
        .map(|base| Path::new(&base).join("PecoFence"))
        .ok_or_else(|| Failure {
            code: "SESSION_UNAVAILABLE",
            message: "LOCALAPPDATA is not set; the session record lives there".into(),
            action: "Run this on the Windows account that runs PecoFence.".into(),
            exit: 1,
        })
}

fn ago(seconds: i64) -> String {
    let seconds = seconds.max(0);
    match seconds {
        0..60 => format!("{seconds} s"),
        60..3600 => format!("{} min", seconds / 60),
        3600..86_400 => format!("{} h {} min", seconds / 3600, seconds % 3600 / 60),
        _ => format!("{} d {} h", seconds / 86_400, seconds % 86_400 / 3600),
    }
}

fn exit_phrase(kind: Option<ExitKind>) -> &'static str {
    match kind {
        Some(ExitKind::Normal) => "normally after saving",
        Some(ExitKind::PumpFallback) => {
            "after saving; WinUI did not shut down in time, so the native message pump ended it"
        }
        Some(ExitKind::DiscardedChanges) => {
            "after a failed save, discarding the unsaved changes as chosen in its dialog"
        }
        Some(ExitKind::Unclosed) | None => {
            "without its close transaction (Windows session end or a fatal UI error)"
        }
    }
}

fn exit_key(kind: Option<ExitKind>) -> &'static str {
    match kind {
        Some(ExitKind::Normal) => "normal",
        Some(ExitKind::PumpFallback) => "pumpFallback",
        Some(ExitKind::DiscardedChanges) => "discardedChanges",
        Some(ExitKind::Unclosed) | None => "unclosed",
    }
}

/// The state shown to the user: the record's own, unless the process behind an open record
/// is gone (`endedUnexpectedly`) or cannot be checked (`unverified`).
fn derive(record: &Session, alive: Option<bool>) -> &'static str {
    match (record.state, alive) {
        (State::Exited, _) => "exited",
        (State::Crashed, _) => "crashed",
        (_, Some(false)) => "endedUnexpectedly",
        (_, None) => "unverified",
        (State::Closing, Some(true)) => "closing",
        (State::Running, Some(true)) => "running",
    }
}

pub fn status(options: &Options, env: &Env, now: i64) -> Result<Output, Failure> {
    let dir = local_dir(env)?;
    let path = session::path(&dir, options.instance.as_deref());
    let logs = |name: &str| {
        let base = match options.instance.as_deref() {
            Some(instance) => format!("pecofence.{instance}"),
            None => "pecofence".into(),
        };
        dir.join(format!("{base}.{name}"))
    };
    let (current_log, previous_log) = (logs("log"), logs("previous.log"));
    let meta = json!({ "record": path.display().to_string(), "readOnly": true });
    let record = match session::load(&path) {
        Ok(record) => record,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Output {
                data: json!({ "state": "noRecord" }),
                text: format!(
                    "No session record at {}.\nPecoFence has not run since lifecycle records \
                     were added (or under this --instance).\n",
                    path.display()
                ),
                meta,
            });
        }
        Err(error) => {
            return Err(Failure {
                code: "SESSION_INVALID",
                message: format!("{}: {error}", path.display()),
                action: "PecoFence rewrites the record when it next starts.".into(),
                exit: 1,
            });
        }
    };
    let alive = record
        .is_open()
        .then(|| (env.alive)(record.pid, record.started_at))
        .flatten();
    let state = derive(&record, alive);
    let watchdog_alive = record
        .watchdog_pid
        .filter(|_| record.is_open())
        .and_then(|pid| (env.alive)(pid, record.started_at));
    let workspace = Path::new(&record.config_dir).join("workspace.v2.json");
    let saved_at = std::fs::metadata(&workspace)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    let live = matches!(state, "running" | "closing");
    let data = json!({
        "state": state,
        "pid": record.pid,
        "instance": record.instance,
        "version": record.version,
        "exe": record.exe,
        "startedAt": record.started_at,
        "uptimeSeconds": live.then(|| now - record.started_at),
        "endedAt": record.ended_at,
        "exit": (record.state == State::Exited).then(|| exit_key(record.exit)),
        "unsaved": record.unsaved,
        "exitCode": record.exit_code,
        "exitCodeHex": record.exit_code.map(|code| format!("0x{code:08X}")),
        "watchdog": { "pid": record.watchdog_pid, "alive": watchdog_alive },
        "workspace": { "path": workspace.display().to_string(), "savedAt": saved_at },
        "logs": {
            "current": current_log.display().to_string(),
            "previous": previous_log.is_file().then(|| previous_log.display().to_string()),
        },
    });
    let ended = record.ended_at.map(|at| ago(now - at)).unwrap_or_default();
    let mut text = match state {
        "running" | "closing" => format!(
            "PecoFence is {state} (pid {}, {}, up {}){}\n",
            record.pid,
            record.version,
            ago(now - record.started_at),
            if state == "closing" {
                "; it is waiting for the final save or a save-failure decision"
            } else {
                ""
            }
        ),
        "exited" => format!(
            "PecoFence exited {ended} ago, {}{}.\n",
            exit_phrase(record.exit),
            if record.unsaved {
                "; changes were not saved"
            } else {
                ""
            }
        ),
        "crashed" => format!(
            "PecoFence crashed {ended} ago (exit code {}); the watchdog recorded it.\n",
            record
                .exit_code
                .map_or("unknown".into(), |code| format!("0x{code:08X}"))
        ),
        "endedUnexpectedly" => format!(
            "PecoFence (pid {}) is gone without an exit record: it was ended together with its \
             watchdog, or Windows stopped.\n",
            record.pid
        ),
        _ => format!(
            "PecoFence was running as pid {} (cannot check processes on this platform).\n",
            record.pid
        ),
    };
    text.push_str(&format!("  exe:        {}\n", record.exe));
    text.push_str(&format!(
        "  workspace:  {}{}\n",
        workspace.display(),
        saved_at.map_or(String::new(), |at| format!(
            " (saved {} ago)",
            ago(now - at)
        ))
    ));
    if live {
        text.push_str(&format!(
            "  watchdog:   {}\n",
            match (record.watchdog_pid, watchdog_alive) {
                (Some(pid), Some(true)) => format!("pid {pid}, watching"),
                (Some(pid), _) => format!("pid {pid}, not running: a crash would go unrecorded"),
                (None, _) => "not started: a crash would go unrecorded".into(),
            }
        ));
    }
    text.push_str(&format!("  log:        {}\n", current_log.display()));
    if previous_log.is_file() {
        text.push_str(&format!("  previous:   {}\n", previous_log.display()));
    }
    Ok(Output { data, text, meta })
}

pub fn quit(options: &Options, env: &Env) -> Result<Output, Failure> {
    let title = pecofence_shell_contract::control_title(options.instance.as_deref());
    let meta = json!({ "readOnly": false, "forceKill": false });
    let Some(pid) = (env.request_quit)(&title) else {
        return Ok(Output {
            data: json!({ "requested": false, "state": "notRunning" }),
            text: "PecoFence is not running; nothing to close.\n".into(),
            meta,
        });
    };
    let wait = options.wait.unwrap_or(DEFAULT_WAIT);
    if wait == 0 {
        return Ok(Output {
            data: json!({ "requested": true, "pid": pid, "waited": false }),
            text: format!(
                "Asked PecoFence (pid {pid}) to save and exit; not waiting. \
                 `pecofence-cli status` shows when it has.\n"
            ),
            meta,
        });
    }
    if (env.wait_exit)(pid, wait.saturating_mul(1000)) == Some(false) {
        return Err(Failure {
            code: "QUIT_TIMEOUT",
            message: format!("PecoFence (pid {pid}) is still closing after {wait} s"),
            action: "It may be waiting for a save-failure decision in its window; answer it, or \
                     run quit again with a longer --wait. This tool never force-closes."
                .into(),
            exit: 1,
        });
    }
    let record = local_dir(env)
        .ok()
        .map(|dir| session::path(&dir, options.instance.as_deref()))
        .and_then(|path| session::load(&path).ok())
        .filter(|record| record.pid == pid && !record.is_open());
    let (exit, unsaved): (Value, bool) = match &record {
        Some(record) => (json!(exit_key(record.exit)), record.unsaved),
        None => (Value::Null, false),
    };
    let text = match &record {
        Some(record) => format!(
            "PecoFence (pid {pid}) exited {}{}.\n",
            exit_phrase(record.exit),
            if record.unsaved {
                "; changes were not saved"
            } else {
                ""
            }
        ),
        None => format!("PecoFence (pid {pid}) exited; it left no exit record.\n"),
    };
    Ok(Output {
        data: json!({ "requested": true, "pid": pid, "waited": true, "exited": true,
                      "exit": exit, "unsaved": unsaved }),
        text,
        meta,
    })
}

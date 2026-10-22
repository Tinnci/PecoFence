//! This process's lifecycle record (`pecofence_core::session`) and its watchdog. The record is
//! written at start, when a close begins or is cancelled, and at exit; a crash is completed by
//! `pecofence-watchdog`, which now runs for every session, not only while desktop icons are
//! hidden. Writes are best effort: a failed record write never blocks running or closing.

use pecofence_core::session::{self, ExitKind, Session, State};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

struct Record {
    path: PathBuf,
    session: Session,
    /// How the close ended, noted by the close path before the process returns from `main`.
    exit: Option<ExitKind>,
    /// Shutdown found changes that were never saved.
    unsaved: bool,
}

static RECORD: Mutex<Option<Record>> = Mutex::new(None);

/// `%LOCALAPPDATA%\PecoFence`: the log, the icon marker and the session record.
pub fn local_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("PecoFence"))
}

fn instance() -> Option<String> {
    pecofence_core::brand::var("PECOFENCE_INSTANCE")
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

/// Records this process as running and starts its watchdog. Call once, after the instance lock
/// is held (a second launch must not overwrite the running instance's record).
pub fn start(config_dir: &Path, icon_marker: &Path) {
    let Some(dir) = local_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let instance = instance();
    let path = session::path(&dir, instance.as_deref());
    if let Ok(previous) = session::load(&path)
        && previous.is_open()
    {
        // The watchdog could not complete it (killed with the app, or the machine stopped).
        tracing::warn!(
            pid = previous.pid,
            state = ?previous.state,
            "previous session ended without an exit record"
        );
    }
    let pid = pecofence_platform::process::current_pid();
    let exe = std::env::current_exe().unwrap_or_default();
    let mut record = Session::running(
        instance.as_deref(),
        pid,
        &exe,
        config_dir,
        pecofence_core::now_unix(),
    );
    record.watchdog_pid = spawn_watchdog(&exe, pid, icon_marker, &path);
    if let Err(error) = session::store(&path, &record) {
        tracing::warn!(%error, path = %path.display(), "session record not written");
    }
    *RECORD.lock().unwrap_or_else(|e| e.into_inner()) = Some(Record {
        path,
        session: record,
        exit: None,
        unsaved: false,
    });
}

/// `pecofence-watchdog <pid> <icon-marker> <session-record>` from the executable's folder.
fn spawn_watchdog(exe: &Path, pid: u32, marker: &Path, record: &Path) -> Option<u32> {
    let watchdog = exe.parent()?.join("pecofence-watchdog.exe");
    if !watchdog.exists() {
        tracing::warn!(path = %watchdog.display(), "watchdog executable missing; crashes go unrecorded");
        return None;
    }
    let arguments = [
        pid.to_string(),
        marker.to_string_lossy().into_owned(),
        record.to_string_lossy().into_owned(),
    ];
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    match pecofence_platform::process::spawn_detached(&watchdog.to_string_lossy(), &arguments) {
        Ok(watchdog_pid) => {
            tracing::info!(watchdog_pid, "watchdog started");
            Some(watchdog_pid)
        }
        Err(error) => {
            tracing::warn!(%error, "watchdog spawn failed");
            None
        }
    }
}

fn update(change: impl FnOnce(&mut Record)) {
    let mut guard = RECORD.lock().unwrap_or_else(|e| e.into_inner());
    let Some(record) = guard.as_mut() else {
        return;
    };
    let before = record.session.clone();
    change(record);
    if record.session != before
        && let Err(error) = session::store(&record.path, &record.session)
    {
        tracing::warn!(%error, "session record not updated");
    }
}

/// A close began (waiting for the final save) or was cancelled.
pub fn set_closing(closing: bool) {
    update(|record| {
        if record.session.is_open() {
            record.session.state = if closing {
                State::Closing
            } else {
                State::Running
            };
        }
    });
}

/// How the close transaction ended; the first note wins (a later fallback refines `Normal`).
pub fn note_exit(kind: ExitKind) {
    update(|record| {
        record.exit = match (record.exit, kind) {
            (None, kind) | (Some(ExitKind::Normal), kind @ ExitKind::PumpFallback) => Some(kind),
            (existing, _) => existing,
        };
    });
}

/// Shutdown's verdict on unsaved changes (only a forced or failed close leaves any).
pub fn note_unsaved(unsaved: bool) {
    update(|record| record.unsaved = unsaved);
}

/// Called once `main` is returning: completes the record with the exit kind.
pub fn finish() {
    update(|record| {
        if !record.session.is_open() {
            return;
        }
        record.session.state = State::Exited;
        record.session.ended_at = Some(pecofence_core::now_unix());
        record.session.exit = Some(record.exit.unwrap_or(ExitKind::Unclosed));
        record.session.unsaved = record.unsaved;
    });
}

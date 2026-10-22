//! pecofence-watchdog: waits for the main process to exit, then
//! - completes its session record as crashed when the app did not record an exit, and
//! - if it died without restoring the desktop icons (marker file still present), sends the
//!   "show desktop icons" toggle itself.
//!
//! Usage: `pecofence-watchdog <parent-pid> <marker-path> [<session-record>]`

#![windows_subsystem = "windows"]

use pecofence_platform::{desktop, process, shell_icons, window};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(pid) = args.first().and_then(|a| a.parse::<u32>().ok()) else {
        return;
    };
    let Some(marker) = args.get(1).map(PathBuf::from) else {
        return;
    };
    let record = args.get(2).map(PathBuf::from);

    // Block until the parent exits. `None`: it could not be opened (already gone), so how it
    // ended is unknown and the record is left as it is.
    let exit_code = process::wait_for_exit_code(pid);
    if let (Some(record), Some(code)) = (record, exit_code) {
        complete_record(&record, pid, code);
    }

    if !marker.exists() {
        return; // clean shutdown restored the icons already
    }
    // Give Explorer a moment, then restore if still hidden. Explorer may be restarting or hung,
    // so keep trying for a while; if it never works the marker stays, and the next start adopts
    // the hidden icons and restores them at exit. A PecoFence started meanwhile takes them
    // over: the icons and the marker are its own then.
    let instance = pecofence_core::brand::var("PECOFENCE_INSTANCE").ok();
    let [mutex, _] = pecofence_core::brand::instance_mutex_names(instance.as_deref());
    std::thread::sleep(std::time::Duration::from_millis(300));
    for attempt in 0..10 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        if window::SingleInstance::is_held(&mutex) {
            return;
        }
        let restored = !shell_icons::desktop_icons_hidden()
            || desktop::resolve_icon_host(desktop::detect_generation()).is_some_and(|host| {
                shell_icons::set_desktop_icons_hidden(false, host.def_view, host.host)
                    == Some(false)
            });
        if restored {
            let _ = std::fs::remove_file(&marker);
            return;
        }
    }
}

/// Marks the record crashed when it still belongs to `pid` and records no exit. A clean exit
/// already closed it; a newer instance has replaced it with its own pid.
fn complete_record(path: &std::path::Path, pid: u32, code: u32) {
    let Ok(mut record) = pecofence_core::session::load(path) else {
        return;
    };
    if record.mark_crashed(pid, pecofence_core::now_unix(), Some(code)) {
        let _ = pecofence_core::session::store(path, &record);
    }
}

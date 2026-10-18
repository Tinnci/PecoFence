//! Background-only holiday cache and daily refresh. No network or file IO in painting.
use chrono::{Datelike, Local};
use pecofence_platform::{HWND, window};
use pecofence_plugin_widgets::holidays::{self, YearSchedule};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime};

const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
pub struct Updater {
    stop: Arc<(Mutex<bool>, Condvar)>,
}
impl Updater {
    pub fn start(cache: PathBuf, hwnd: HWND, network: bool) -> Self {
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_stop = stop.clone();
        let target = hwnd.0 as isize;
        if let Err(error) = std::thread::Builder::new()
            .name("holiday-cache".into())
            .spawn(move || run(cache, target, network, worker_stop))
        {
            tracing::warn!(%error, "holiday cache worker could not start");
        }
        Self { stop }
    }
}
fn run(cache: PathBuf, target: isize, network: bool, worker_stop: Arc<(Mutex<bool>, Condvar)>) {
    let stopped = || *worker_stop.0.lock().unwrap_or_else(|e| e.into_inner());
    let publish = |schedule| {
        if !stopped() && holidays::publish(schedule) {
            window::post_message(
                HWND(target as *mut _),
                crate::commands::WM_APP_PLUGIN_EVENT,
                0,
                0,
            );
        }
    };
    // Historical caches remain useful when browsing earlier years offline.
    if let Ok(files) = std::fs::read_dir(&cache) {
        for file in files.flatten().take(301) {
            let path = file.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && let Some(year) = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.parse::<i32>().ok())
                && let Ok(schedule) = read_cache(&path, year)
            {
                publish(schedule);
            }
        }
    }
    loop {
        let year = Local::now().year();
        for year in year - 1..=year + 1 {
            if stopped() {
                return;
            }
            let path = cache.join(format!("{year}.json"));
            let cached = read_cache(&path, year).ok();
            let fresh = cached.is_some() && cache_is_fresh(&path, SystemTime::now());
            if let Some(schedule) = cached {
                publish(schedule);
            }
            if !network || fresh {
                continue;
            }
            let downloaded = pecofence_platform::http::get_https(
                "raw.githubusercontent.com",
                &format!("/NateScarlet/holiday-cn/master/{year}.json"),
                holidays::MAX_BYTES,
            )
            .or_else(|_| {
                pecofence_platform::http::get_https(
                    "cdn.jsdelivr.net",
                    &format!("/gh/NateScarlet/holiday-cn@master/{year}.json"),
                    holidays::MAX_BYTES,
                )
            })
            .and_then(|bytes| {
                YearSchedule::parse(&bytes, year, Some(Local::now().date_naive()))
                    .map(|schedule| (bytes, schedule))
                    .map_err(io::Error::other)
            });
            if stopped() {
                return;
            }
            match downloaded {
                Ok((bytes, schedule)) => {
                    if let Err(error) = write_cache(&path, &bytes) {
                        tracing::debug!(year, %error, "holiday cache write unavailable");
                    }
                    publish(schedule);
                }
                Err(error) => {
                    tracing::debug!(year, %error, "holiday refresh unavailable; keeping offline data")
                }
            }
        }
        let guard = worker_stop.0.lock().unwrap_or_else(|e| e.into_inner());
        let (guard, _) = worker_stop
            .1
            .wait_timeout_while(guard, INTERVAL, |stop| !*stop)
            .unwrap_or_else(|e| e.into_inner());
        if *guard {
            return;
        }
    }
}

impl Drop for Updater {
    fn drop(&mut self) {
        *self.stop.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.stop.1.notify_all(); // Never block UI shutdown on an in-flight HTTP request.
    }
}
fn cache_is_fresh(path: &Path, now: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|time| now.duration_since(time).ok())
        .is_some_and(|age| age < INTERVAL)
}
fn read_cache(path: &Path, year: i32) -> io::Result<YearSchedule> {
    let file = std::fs::File::open(path)?;
    let checked = file
        .metadata()?
        .modified()
        .ok()
        .map(|date| chrono::DateTime::<Local>::from(date).date_naive());
    let mut bytes = Vec::new();
    file.take(holidays::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    YearSchedule::parse(&bytes, year, checked).map_err(io::Error::other)
}
fn write_cache(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("cache parent missing"))?;
    std::fs::create_dir_all(parent)?;
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_cache_preserves_valid_data_and_rejects_corrupt_or_mismatched_years() {
        let dir =
            std::env::temp_dir().join(format!("pecofence-holiday-cache-{}", uuid::Uuid::new_v4()));
        let path = dir.join("2026.json");
        let bytes = include_bytes!("../../plugin-widgets/data/holidays/2026.json");
        write_cache(&path, bytes).unwrap();
        assert!(read_cache(&path, 2026).is_ok());
        assert!(read_cache(&path, 2027).is_err());
        assert!(cache_is_fresh(&path, SystemTime::now()));
        assert!(!cache_is_fresh(&path, SystemTime::now() + INTERVAL));
        assert!(!cache_is_fresh(&path, SystemTime::UNIX_EPOCH));
        std::fs::write(&path, b"truncated").unwrap();
        assert!(read_cache(&path, 2026).is_err());
        assert!(
            std::fs::read_dir(&dir)
                .unwrap()
                .all(|f| f.unwrap().path().extension().unwrap() == "json")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

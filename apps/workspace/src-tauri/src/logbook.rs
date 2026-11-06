//! Workspace's part of the shared log, in the format `spm focus logs` reads: JSON lines in
//! `%LOCALAPPDATA%\Tinnci\logs\workspace-<yyyymmdd>.jsonl`. Metadata only: actions, durations
//! and failure codes, never arguments or results. Days past 14 are removed.
use serde_json::{Map, Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MIB: u64 = 1024 * 1024;

/// Days since 1970 to year, month, day (Howard Hinnant's civil_from_days).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn day_name(days: i64) -> String {
    let (y, m, d) = civil(days);
    format!("{y:04}{m:02}{d:02}")
}

fn timestamp(seconds: i64, millis: u32) -> String {
    let (y, mo, d) = civil(seconds.div_euclid(86_400));
    let s = seconds.rem_euclid(86_400);
    format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

fn directory() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("Tinnci").join("logs"))
}

/// Past days older than `KEEP_DAYS` go, then the oldest until the folder fits `FOLDER_LIMIT`.
/// The folder is shared, so this counts every tool's files, as `spm` does.
fn prune(dir: &Path, today: i64, policy: &crate::setup::LogPolicy) {
    let (oldest, current) = (
        day_name(today - i64::from(policy.retention_days) + 1),
        day_name(today),
    );
    let mut days = vec![];
    for file in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = file.file_name();
        let day = name
            .to_str()
            .and_then(|n| n.strip_suffix(".jsonl"))
            .and_then(|n| n.rsplit('-').next())
            .filter(|d| d.len() == 8)
            .map(str::to_owned);
        if let (Some(day), Ok(meta)) = (day, file.metadata()) {
            if !file.file_type().is_ok_and(|t| t.is_file())
                || !day.bytes().all(|b| b.is_ascii_digit())
            {
                continue;
            }
            days.push((day, meta.len(), file.path()));
        }
    }
    days.sort();
    let mut total: u64 = days.iter().map(|d| d.1).sum();
    for (day, length, path) in days {
        if (day < oldest || total > u64::from(policy.max_total_mib) * MIB)
            && day < current
            && std::fs::remove_file(path).is_ok()
        {
            total = total.saturating_sub(length);
        }
    }
}

/// A new id for one call, handed to `spm` so both sides share it.
pub fn new_run_id() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |now| now.as_millis());
    let turn = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("{millis:x}-{:x}-{turn:x}", std::process::id())
}

/// A name from the frontend, or `unknown`: no free text reaches the log.
pub fn plain(text: &str) -> String {
    let fine = (1..=80).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-:".contains(&b));
    if fine { text } else { "unknown" }.to_owned()
}

fn line(ts: &str, level: &str, event: &str, fields: Value) -> String {
    let mut map = Map::new();
    map.insert("ts".into(), json!(ts));
    map.insert("level".into(), json!(level));
    map.insert("app".into(), json!("workspace"));
    map.insert("event".into(), json!(event));
    if let Value::Object(fields) = fields {
        map.extend(fields);
    }
    let mut text = Value::Object(map).to_string();
    text.push('\n');
    text
}

fn append(
    dir: &Path,
    seconds: i64,
    millis: u32,
    level: &str,
    event: &str,
    fields: Value,
) -> std::io::Result<()> {
    let today = seconds.div_euclid(86_400);
    let path = dir.join(format!("workspace-{}.jsonl", day_name(today)));
    std::fs::create_dir_all(dir)?;
    let configured = crate::setup::local_log_policy(dir);
    if let Some(policy) = &configured {
        let marker = dir.join(".retention-checked");
        let previous = std::fs::read_to_string(&marker)
            .ok()
            .and_then(|s| s.parse::<i64>().ok());
        if previous.is_none_or(|at| !(0..60).contains(&(seconds - at))) {
            prune(dir, today, policy);
            let _ = std::fs::write(marker, seconds.to_string());
        }
    }
    let policy = configured.unwrap_or_default();
    let text = line(&timestamp(seconds, millis), level, event, fields);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.try_lock().map_err(std::io::Error::other)?;
    let size = file.metadata()?.len();
    let limit = if level == "info" {
        policy.daily_info_mib
    } else {
        policy.daily_max_mib
    };
    if size.saturating_add(text.len() as u64) > u64::from(limit) * MIB {
        return Ok(());
    }
    file.write_all(text.as_bytes())
}

/// Best effort: a log that cannot be written never fails the work it describes.
pub fn record(level: &str, event: &str, fields: Value) {
    let (Some(dir), Ok(now)) = (directory(), SystemTime::now().duration_since(UNIX_EPOCH)) else {
        return;
    };
    let _ = append(
        &dir,
        now.as_secs() as i64,
        now.subsec_millis(),
        level,
        event,
        fields,
    );
}

/// Panics carry only where they happened: their text can hold anything.
pub fn watch_panics() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()));
        record("error", "app.panic", json!({ "at": at }));
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_policy_is_reloaded_and_cleanup_continues_after_the_first_daily_append() {
        let root =
            std::env::temp_dir().join(format!("workspace-log-policy-{}", std::process::id()));
        let dir = root.join("logs");
        std::fs::create_dir_all(&dir).unwrap();
        let seconds = 1_793_858_987;
        let path = root.join("logging.json");
        let mut policy = crate::setup::LogPolicy {
            retention_days: 2,
            daily_info_mib: 1,
            daily_max_mib: 2,
            ..Default::default()
        };
        std::fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
        append(&dir, seconds, 0, "info", "first", json!({})).unwrap();
        let old = dir.join("workspace-20261102.jsonl");
        std::fs::write(&old, "old").unwrap();
        append(&dir, seconds + 61, 0, "info", "later", json!({})).unwrap();
        assert!(!old.exists());
        let today = dir.join("workspace-20261105.jsonl");
        std::fs::File::options()
            .write(true)
            .open(&today)
            .unwrap()
            .set_len(MIB)
            .unwrap();
        append(&dir, seconds + 62, 0, "info", "skip", json!({})).unwrap();
        assert_eq!(std::fs::metadata(&today).unwrap().len(), MIB);
        policy.daily_info_mib = 2;
        std::fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
        append(&dir, seconds + 63, 0, "info", "changed", json!({})).unwrap();
        assert!(std::fs::metadata(&today).unwrap().len() > MIB);
        std::fs::write(&old, "preserve").unwrap();
        std::fs::write(&path, "invalid").unwrap();
        append(&dir, seconds + 200, 0, "warn", "bad-policy", json!({})).unwrap();
        assert!(old.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn writes_the_shared_format_and_keeps_fourteen_days() {
        // 2026-11-05T06:09:47Z
        let seconds = 1_793_858_987;
        assert_eq!(timestamp(seconds, 7), "2026-11-05T06:09:47.007Z");
        assert_eq!(day_name(seconds / 86_400), "20261105");
        let dir = std::env::temp_dir().join(format!("workspace-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("workspace-20261001.jsonl"), "").unwrap();
        std::fs::write(dir.join("workspace-20261102.jsonl"), "").unwrap();
        append(
            &dir,
            seconds,
            7,
            "warn",
            "focus.failed",
            json!({"action": "flows"}),
        )
        .unwrap();
        assert!(!dir.join("workspace-20261001.jsonl").exists());
        assert!(dir.join("workspace-20261102.jsonl").exists());
        let text = std::fs::read_to_string(dir.join("workspace-20261105.jsonl")).unwrap();
        let entry: Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(entry["app"], "workspace");
        assert_eq!(entry["event"], "focus.failed");
        assert_eq!(entry["action"], "flows");
        assert_eq!(plain("TypeError"), "TypeError");
        assert_eq!(plain("index-abc.js:12"), "index-abc.js:12");
        assert_eq!(plain("token=abc\nfake"), "unknown");
        assert_ne!(new_run_id(), new_run_id());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

//! Workspace's part of the shared log, in the format `spm focus logs` reads: JSON lines in
//! `%LOCALAPPDATA%\Tinnci\logs\workspace-<yyyymmdd>.jsonl`. Metadata only: actions, durations
//! and failure codes, never arguments or results. Days past 14 are removed.
use serde_json::{Map, Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const KEEP_DAYS: i64 = 14;
const DAY_LIMIT: u64 = 20 * 1024 * 1024;

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

fn prune(dir: &Path, today: i64) {
    let oldest = day_name(today - KEEP_DAYS);
    for file in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = file.file_name();
        let day = name
            .to_str()
            .and_then(|n| n.strip_prefix("workspace-"))
            .and_then(|n| n.strip_suffix(".jsonl"));
        if day.is_some_and(|d| d.len() == 8 && d < oldest.as_str()) {
            let _ = std::fs::remove_file(file.path());
        }
    }
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
    match std::fs::metadata(&path) {
        Ok(meta) if meta.len() > DAY_LIMIT && level == "info" => return Ok(()),
        Ok(_) => {}
        Err(_) => {
            std::fs::create_dir_all(dir)?;
            prune(dir, today);
        }
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line(&timestamp(seconds, millis), level, event, fields).as_bytes())
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
        std::fs::remove_dir_all(dir).unwrap();
    }
}

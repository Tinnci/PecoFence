//! The lifecycle record of one PecoFence process, `session.json` beside the log. The app writes
//! it when it starts, when a close begins or is cancelled, and when it exits; the watchdog
//! completes it when the app dies without exiting; `pecofence-cli status` reads it. It holds no
//! workspace data, only process facts.

use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub const SCHEMA: u32 = 1;
const MAX_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Running,
    /// A close was requested and waits for the final save (or a save-failure decision).
    Closing,
    Exited,
    /// The process ended without recording an exit; completed by the watchdog.
    Crashed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExitKind {
    /// The final save completed and WinUI shut down.
    Normal,
    /// The final save completed, but WinUI did not exit in time; the native pump ended it.
    PumpFallback,
    /// A save failed and the user chose to discard the unsaved changes.
    DiscardedChanges,
    /// The process ended without the close transaction (Windows session end, fatal UI error).
    Unclosed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    pub pid: u32,
    pub exe: String,
    pub version: String,
    pub config_dir: String,
    /// Unix seconds.
    pub started_at: i64,
    pub state: State,
    #[serde(default)]
    pub watchdog_pid: Option<u32>,
    #[serde(default)]
    pub ended_at: Option<i64>,
    #[serde(default)]
    pub exit: Option<ExitKind>,
    /// Changes were not saved when the process ended.
    #[serde(default)]
    pub unsaved: bool,
    /// Process exit code seen by the watchdog after a crash.
    #[serde(default)]
    pub exit_code: Option<u32>,
}

impl Session {
    pub fn running(
        instance: Option<&str>,
        pid: u32,
        exe: &Path,
        config_dir: &Path,
        started_at: i64,
    ) -> Self {
        Self {
            schema: SCHEMA,
            instance: instance.map(str::to_string),
            pid,
            exe: exe.display().to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            config_dir: config_dir.display().to_string(),
            started_at,
            state: State::Running,
            watchdog_pid: None,
            ended_at: None,
            exit: None,
            unsaved: false,
            exit_code: None,
        }
    }

    /// Still owned by a live (or presumed live) process.
    pub fn is_open(&self) -> bool {
        matches!(self.state, State::Running | State::Closing)
    }

    /// The watchdog's completion when `pid` ended while the record was still open. Returns
    /// false (and changes nothing) for another process's record or an already closed one.
    pub fn mark_crashed(&mut self, pid: u32, at: i64, exit_code: Option<u32>) -> bool {
        if self.pid != pid || !self.is_open() {
            return false;
        }
        self.state = State::Crashed;
        self.ended_at = Some(at);
        self.exit_code = exit_code;
        true
    }
}

/// `session.json` (or `session.<instance>.json` for a named test instance) in `dir`, the
/// per-user log folder `%LOCALAPPDATA%\PecoFence`.
pub fn path(dir: &Path, instance: Option<&str>) -> PathBuf {
    match instance.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => dir.join(format!("session.{name}.json")),
        None => dir.join("session.json"),
    }
}

pub fn load(path: &Path) -> io::Result<Session> {
    let mut text = String::new();
    std::fs::File::open(path)?
        .take(MAX_BYTES)
        .read_to_string(&mut text)?;
    let session: Session = serde_json::from_str(&text)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if session.schema != SCHEMA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported session schema {}", session.schema),
        ));
    }
    Ok(session)
}

/// Replaces the record atomically, so a reader never sees half a file.
pub fn store(path: &Path, session: &Session) -> io::Result<()> {
    let text = serde_json::to_string_pretty(session).map_err(io::Error::other)?;
    let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Session {
        Session::running(
            None,
            42,
            Path::new(r"C:\PecoFence\pecofence.exe"),
            Path::new(r"C:\Users\a\AppData\Roaming\PecoFence"),
            1_000,
        )
    }

    #[test]
    fn named_instances_keep_their_own_record() {
        let dir = Path::new("logs");
        assert_eq!(path(dir, None), dir.join("session.json"));
        assert_eq!(path(dir, Some(" ")), dir.join("session.json"));
        assert_eq!(path(dir, Some("lab")), dir.join("session.lab.json"));
    }

    #[test]
    fn watchdog_completes_only_its_own_open_record() {
        let mut session = sample();
        assert!(
            !session.mark_crashed(7, 2_000, Some(1)),
            "another process's record"
        );
        assert!(session.mark_crashed(42, 2_000, Some(0xC000_0005)));
        assert_eq!(session.state, State::Crashed);
        assert_eq!(session.exit_code, Some(0xC000_0005));
        let mut exited = sample();
        exited.state = State::Exited;
        exited.exit = Some(ExitKind::Normal);
        assert!(
            !exited.mark_crashed(42, 2_000, None),
            "a clean exit stays clean"
        );
    }

    #[test]
    fn records_round_trip_atomically_and_reject_other_schemas() {
        let dir = std::env::temp_dir().join(format!("pecofence-session-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = path(&dir, None);
        let mut session = sample();
        session.state = State::Closing;
        store(&file, &session).unwrap();
        assert_eq!(load(&file).unwrap(), session);
        assert!(
            std::fs::read_dir(&dir).unwrap().count() == 1,
            "no temp file left"
        );
        std::fs::write(&file, r#"{"schema":9}"#).unwrap();
        assert!(load(&file).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

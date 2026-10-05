//! Exact-format workspace storage. Domain validation belongs to Config; this adapter
//! owns file IO, primary replacement and backup outcomes, not default-workspace policy.

use crate::model::{Config, SCHEMA_VERSION};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub const MAX_FENCES: usize = 64;
pub const MAX_ITEMS: usize = 5000;
pub const DAILY_BACKUPS_KEPT: usize = 7;
const MAX_DOCUMENT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug)]
pub enum FreshReason {
    FirstRun,
    CorruptPrimary {
        reason: String,
        /// Source files are retained in place. No automatic quarantine/migration.
        quarantined: Option<PathBuf>,
    },
    UnreadablePrimary {
        reason: String,
    },
    UnsupportedFormat {
        path: PathBuf,
        schema: Option<u32>,
    },
}

#[derive(Debug)]
pub enum LoadOutcome {
    Primary(Config),
    /// A usable candidate, NOT permission to overwrite the original automatically.
    Recovered(Config, PathBuf),
    /// Only FirstRun permits default creation; all other reasons require explicit recovery.
    Fresh(Config, FreshReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackupStatus {
    Written,
    AlreadyExists,
    Degraded(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveReceipt {
    /// A receipt is returned only after the primary has been replaced successfully.
    pub backup: BackupStatus,
}

enum Stored {
    Absent,
    Valid(Box<Config>),
    Unsupported(Option<u32>),
    Corrupt(String),
    Unavailable(String),
}

pub struct ConfigStore {
    dir: PathBuf,
}

impl ConfigStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn primary_path(&self) -> PathBuf {
        self.dir.join("workspace.v2.json")
    }

    fn bak_path(&self) -> PathBuf {
        self.dir.join("workspace.v2.bak")
    }

    fn backups_dir(&self) -> PathBuf {
        self.dir.join("workspace.v2.backups")
    }

    fn read_text(path: &Path) -> io::Result<String> {
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(MAX_DOCUMENT_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "workspace exceeds the 16 MiB input budget",
            ));
        }
        String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn decode(text: &str) -> Stored {
        let wire: serde_json::Value = match serde_json::from_str(text) {
            Ok(wire) => wire,
            Err(error) => return Stored::Corrupt(error.to_string()),
        };
        let schema = wire
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            .and_then(|version| u32::try_from(version).ok());
        if schema != Some(SCHEMA_VERSION) {
            return Stored::Unsupported(schema);
        }
        let config: Config = match serde_json::from_value(wire) {
            Ok(config) => config,
            Err(error) => return Stored::Corrupt(error.to_string()),
        };
        match config.validate() {
            Ok(()) => Stored::Valid(Box::new(config)),
            Err(error) => Stored::Corrupt(error),
        }
    }

    fn read_stored(path: &Path) -> Stored {
        match Self::read_text(path) {
            Ok(text) => Self::decode(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Stored::Absent,
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                Stored::Corrupt(error.to_string())
            }
            Err(error) => Stored::Unavailable(error.to_string()),
        }
    }

    /// Import/restore accepts the exact supported schema. No aliases or migration chain.
    pub fn parse_file(path: &Path) -> Result<Config, String> {
        match Self::read_stored(path) {
            Stored::Valid(config) => Ok(*config),
            Stored::Unsupported(schema) => Err(format!(
                "unsupported workspace schema {schema:?}; expected {SCHEMA_VERSION}"
            )),
            Stored::Absent => Err(format!("workspace not found: {}", path.display())),
            Stored::Corrupt(error) | Stored::Unavailable(error) => Err(error),
        }
    }

    pub fn export_to(config: &Config, path: &Path) -> io::Result<()> {
        fs::write(path, Self::encode(config)?)
    }

    fn encode(config: &Config) -> io::Result<String> {
        config
            .validate()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let json = serde_json::to_string_pretty(config).map_err(io::Error::other)?;
        if json.len() as u64 > MAX_DOCUMENT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "workspace exceeds the 16 MiB output budget",
            ));
        }
        Ok(json)
    }

    fn reason(path: &Path, stored: Stored) -> FreshReason {
        match stored {
            Stored::Absent => FreshReason::FirstRun,
            Stored::Unsupported(schema) => FreshReason::UnsupportedFormat {
                path: path.to_path_buf(),
                schema,
            },
            Stored::Corrupt(reason) => FreshReason::CorruptPrimary {
                reason: format!("{}: {reason}", path.display()),
                quarantined: None,
            },
            Stored::Unavailable(reason) => FreshReason::UnreadablePrimary {
                reason: format!("{}: {reason}", path.display()),
            },
            Stored::Valid(_) => unreachable!("valid documents are returned before mapping errors"),
        }
    }

    pub fn load(&self) -> LoadOutcome {
        let primary = self.primary_path();
        let mut reason = match Self::read_stored(&primary) {
            Stored::Valid(config) => return LoadOutcome::Primary(*config),
            stored => Self::reason(&primary, stored),
        };
        let backup = self.bak_path();
        match Self::read_stored(&backup) {
            Stored::Valid(config) => return LoadOutcome::Recovered(*config, backup),
            Stored::Absent => {}
            stored if matches!(reason, FreshReason::FirstRun) => {
                reason = Self::reason(&backup, stored);
            }
            _ => {}
        }
        match self.backups() {
            Ok(mut backups) => {
                backups.sort();
                for path in backups.into_iter().rev() {
                    match Self::read_stored(&path) {
                        Stored::Valid(config) => return LoadOutcome::Recovered(*config, path),
                        stored if matches!(reason, FreshReason::FirstRun) => {
                            reason = Self::reason(&path, stored);
                        }
                        _ => {}
                    }
                }
            }
            Err(error) if matches!(reason, FreshReason::FirstRun) => {
                reason = FreshReason::UnreadablePrimary {
                    reason: format!("cannot inspect workspace backups: {error}"),
                };
            }
            Err(_) => {}
        }
        if matches!(reason, FreshReason::FirstRun) {
            // Identify old data, but never deserialize it into the new model.
            for path in [
                self.dir.join("config.json"),
                self.dir.join("config.bak"),
                self.dir.join("backups"),
            ] {
                match fs::symlink_metadata(&path) {
                    Ok(_) => {
                        reason = FreshReason::UnsupportedFormat { path, schema: None };
                        break;
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => {
                        reason = FreshReason::UnreadablePrimary {
                            reason: error.to_string(),
                        };
                        break;
                    }
                }
            }
        }
        if matches!(reason, FreshReason::FirstRun) {
            match fs::read_dir(&self.dir) {
                Ok(entries) => {
                    for entry in entries {
                        match entry {
                            Ok(entry) => {
                                let name = entry.file_name();
                                let name = name.to_string_lossy();
                                if name.starts_with("config.corrupt-")
                                    || name.starts_with("workspace.v2.replaced-")
                                {
                                    reason = FreshReason::UnsupportedFormat {
                                        path: entry.path(),
                                        schema: None,
                                    };
                                    break;
                                }
                            }
                            Err(error) => {
                                reason = FreshReason::UnreadablePrimary {
                                    reason: error.to_string(),
                                };
                                break;
                            }
                        }
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    reason = FreshReason::UnreadablePrimary {
                        reason: error.to_string(),
                    };
                }
            }
        }
        LoadOutcome::Fresh(Config::default(), reason)
    }

    fn backups(&self) -> io::Result<Vec<PathBuf>> {
        let entries = match fs::read_dir(self.backups_dir()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut paths = Vec::new();
        for entry in entries {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                paths.push(path);
            }
        }
        Ok(paths)
    }

    pub fn list_backups(&self) -> Vec<PathBuf> {
        self.backups().unwrap_or_default()
    }

    /// Normal autosave never replaces an unreadable, corrupt or unsupported primary.
    pub fn save(&self, config: &Config) -> io::Result<SaveReceipt> {
        match Self::read_stored(&self.primary_path()) {
            Stored::Absent | Stored::Valid(_) => self.commit(config),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "primary workspace needs explicit recovery before it can be replaced",
            )),
        }
    }

    /// Only an explicitly confirmed import/reset/recovery may replace unusable data.
    /// Preserve raw prior bytes in a unique archive; later .bak rotation cannot erase it.
    pub fn replace(&self, config: &Config) -> io::Result<SaveReceipt> {
        // Validate shape AND serialized budget before creating recovery evidence.
        Self::encode(config)?;
        match fs::File::open(self.primary_path()) {
            Ok(mut source) => {
                let path = self.dir.join(format!(
                    "workspace.v2.replaced-{}.json",
                    uuid::Uuid::new_v4()
                ));
                let mut archive = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(path)?;
                io::copy(&mut source, &mut archive)?;
                archive.sync_all()?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.commit(config)
    }

    fn commit(&self, config: &Config) -> io::Result<SaveReceipt> {
        let json = Self::encode(config)?;
        fs::create_dir_all(&self.dir)?;
        let temporary = self
            .dir
            .join(format!("workspace.v2-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
            drop(file);

            let mut issues = Vec::new();
            if let Err(error) = self.rotate_backup() {
                issues.push(format!("previous-version backup: {error}"));
            }
            // Same-directory replacement. Never remove/rename the old primary first.
            // Atomic replacement is not a promise of power-loss durability or external CAS.
            fs::rename(&temporary, self.primary_path())?;
            let backup = match self.daily_backup(&json) {
                Ok(status) => status,
                Err(error) => {
                    issues.push(format!("daily backup: {error}"));
                    BackupStatus::AlreadyExists
                }
            };
            Ok(SaveReceipt {
                backup: if issues.is_empty() {
                    backup
                } else {
                    BackupStatus::Degraded(issues.join("; "))
                },
            })
        })();
        if result.is_err() {
            // Only this commit's private staging file; the primary remains untouched.
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn rotate_backup(&self) -> io::Result<()> {
        let primary = self.primary_path();
        let mut source = match fs::File::open(&primary) {
            Ok(source) => source,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let temporary = self
            .dir
            .join(format!("workspace.v2-backup-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut backup = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            io::copy(&mut source, &mut backup)?;
            backup.sync_all()?;
            drop(backup);
            fs::rename(&temporary, self.bak_path())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }

    fn daily_backup(&self, json: &str) -> io::Result<BackupStatus> {
        fs::create_dir_all(self.backups_dir())?;
        let path = self
            .backups_dir()
            .join(format!("{}.json", today_yyyy_mm_dd()));
        let status = match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
        {
            Ok(mut file) => {
                file.write_all(json.as_bytes())?;
                file.sync_all()?;
                BackupStatus::Written
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                BackupStatus::AlreadyExists
            }
            Err(error) => return Err(error),
        };
        let mut backups = self.backups()?;
        backups.sort();
        while backups.len() > DAILY_BACKUPS_KEPT {
            fs::remove_file(backups.remove(0))?;
        }
        Ok(status)
    }
}

/// UTC date for human-readable backup filenames.
pub fn today_yyyy_mm_dd() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("pecofence-store-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn store(&self) -> ConfigStore {
            ConfigStore::new(&self.0)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn only_absent_data_is_first_run() {
        let fixture = Fixture::new();
        let store = fixture.store();
        assert!(matches!(
            store.load(),
            LoadOutcome::Fresh(_, FreshReason::FirstRun)
        ));
        let old = fixture.0.join("config.json");
        fs::write(&old, b"{\"schemaVersion\":1}").unwrap();
        assert!(matches!(
            store.load(),
            LoadOutcome::Fresh(_, FreshReason::UnsupportedFormat { .. })
        ));
        assert_eq!(fs::read(old).unwrap(), b"{\"schemaVersion\":1}");
        assert!(!store.primary_path().exists());
    }

    #[test]
    fn exact_schema_required_even_for_empty_document() {
        let fixture = Fixture::new();
        let store = fixture.store();
        let mut wire = serde_json::to_value(Config::default()).unwrap();
        for version in [0, SCHEMA_VERSION - 1, SCHEMA_VERSION + 1] {
            wire["schemaVersion"] = version.into();
            let bytes = serde_json::to_vec(&wire).unwrap();
            fs::write(store.primary_path(), &bytes).unwrap();
            assert!(matches!(
                store.load(),
                LoadOutcome::Fresh(_, FreshReason::UnsupportedFormat { .. })
            ));
            assert!(store.save(&Config::default()).is_err());
            assert_eq!(fs::read(store.primary_path()).unwrap(), bytes);
        }
    }

    #[test]
    fn malformed_and_unreadable_primaries_are_not_defaults_to_save() {
        let fixture = Fixture::new();
        let store = fixture.store();
        fs::write(store.primary_path(), b"{broken").unwrap();
        assert!(matches!(
            store.load(),
            LoadOutcome::Fresh(_, FreshReason::CorruptPrimary { .. })
        ));
        assert!(store.save(&Config::default()).is_err());
        assert_eq!(fs::read(store.primary_path()).unwrap(), b"{broken");
        fs::remove_file(store.primary_path()).unwrap();
        fs::create_dir(store.primary_path()).unwrap();
        assert!(matches!(
            store.load(),
            LoadOutcome::Fresh(_, FreshReason::UnreadablePrimary { .. })
        ));
        assert!(store.save(&Config::default()).is_err());
    }

    #[test]
    fn primary_roundtrip_and_atomic_replacement_keep_previous_version() {
        let fixture = Fixture::new();
        let store = fixture.store();
        let original = Config::default();
        store.save(&original).unwrap();
        let mut next = original.clone();
        next.settings.icon_size = 64;
        store.save(&next).unwrap();
        let LoadOutcome::Primary(loaded) = store.load() else {
            panic!("primary must load");
        };
        assert_eq!(loaded.settings.icon_size, 64);
        assert_eq!(
            ConfigStore::parse_file(&store.bak_path())
                .unwrap()
                .settings
                .icon_size,
            original.settings.icon_size
        );
    }

    #[test]
    fn recovery_candidate_preserves_primary_until_explicit_acceptance() {
        let fixture = Fixture::new();
        let store = fixture.store();
        store.save(&Config::default()).unwrap();
        store.save(&Config::default()).unwrap();
        fs::write(store.primary_path(), b"truncated").unwrap();
        let LoadOutcome::Recovered(candidate, _) = store.load() else {
            panic!("expected verified recovery candidate");
        };
        assert_eq!(fs::read(store.primary_path()).unwrap(), b"truncated");
        assert!(store.save(&candidate).is_err());
        store.replace(&candidate).unwrap();
        assert!(matches!(store.load(), LoadOutcome::Primary(_)));
        let archive = fs::read_dir(&fixture.0)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("workspace.v2.replaced-")
            })
            .unwrap();
        assert_eq!(fs::read(archive).unwrap(), b"truncated");
    }

    #[test]
    fn backup_failure_is_degraded_commit_not_primary_failure() {
        let fixture = Fixture::new();
        let store = fixture.store();
        fs::write(store.backups_dir(), b"blocks a directory").unwrap();
        let receipt = store.save(&Config::default()).unwrap();
        assert!(matches!(receipt.backup, BackupStatus::Degraded(_)));
        assert!(matches!(store.load(), LoadOutcome::Primary(_)));
    }

    #[test]
    fn backup_only_failure_does_not_become_first_run() {
        let fixture = Fixture::new();
        let store = fixture.store();
        fs::write(store.bak_path(), b"{invalid").unwrap();
        assert!(matches!(
            store.load(),
            LoadOutcome::Fresh(_, FreshReason::CorruptPrimary { .. })
        ));
    }

    #[test]
    fn orphaned_recovery_evidence_is_not_first_run() {
        let fixture = Fixture::new();
        let store = fixture.store();
        let archive = fixture.0.join("config.corrupt-fixture.json");
        fs::write(&archive, b"old user data").unwrap();
        assert!(matches!(
            store.load(),
            LoadOutcome::Fresh(_, FreshReason::UnsupportedFormat { .. })
        ));
        assert_eq!(fs::read(archive).unwrap(), b"old user data");
    }

    #[test]
    fn failed_replacement_keeps_existing_primary() {
        let fixture = Fixture::new();
        let store = fixture.store();
        store.save(&Config::default()).unwrap();
        let bytes = fs::read(store.primary_path()).unwrap();
        let invalid = Config {
            schema_version: SCHEMA_VERSION + 1,
            ..Config::default()
        };
        assert!(store.save(&invalid).is_err());
        assert_eq!(fs::read(store.primary_path()).unwrap(), bytes);
    }
}

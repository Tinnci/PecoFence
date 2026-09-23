//! Config persistence: atomic writes, `.bak` rotation, daily backups, load fallbacks (plan §7.5).

use crate::model::{Config, SCHEMA_VERSION};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const MAX_FENCES: usize = 64;
pub const MAX_ITEMS: usize = 5000;
pub const DAILY_BACKUPS_KEPT: usize = 7;

#[derive(Debug)]
pub enum FreshReason {
    FirstRun,
    CorruptPrimary {
        reason: String,
        quarantined: Option<PathBuf>,
    },
    UnreadablePrimary {
        reason: String,
    },
}

#[derive(Debug)]
pub enum LoadOutcome {
    /// Loaded from the primary file.
    Primary(Config),
    /// Primary missing/corrupt; loaded from `.bak` or a daily backup (path given).
    Recovered(Config, PathBuf),
    /// Nothing usable: fresh default (first run or total loss).
    Fresh(Config, FreshReason),
}

impl LoadOutcome {
    pub fn into_config(self) -> Config {
        match self {
            LoadOutcome::Primary(c) | LoadOutcome::Recovered(c, _) | LoadOutcome::Fresh(c, _) => c,
        }
    }
}

pub struct ConfigStore {
    dir: PathBuf,
}

impl ConfigStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Reuse an existing installation's data in place. No copying, rewriting or
    /// renaming of user files is needed just because the product name changed.
    pub fn with_legacy(preferred: impl Into<PathBuf>, legacy: impl Into<PathBuf>) -> Self {
        let preferred = Self::new(preferred);
        let legacy = Self::new(legacy);
        if !preferred.has_saved_data() && legacy.has_saved_data() {
            legacy
        } else {
            preferred
        }
    }

    fn has_saved_data(&self) -> bool {
        self.primary_path().exists() || self.bak_path().exists() || !self.list_backups().is_empty()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn primary_path(&self) -> PathBuf {
        self.dir.join("config.json")
    }
    fn bak_path(&self) -> PathBuf {
        self.dir.join("config.bak")
    }
    fn tmp_path(&self) -> PathBuf {
        self.dir.join("config.json.tmp")
    }
    fn backups_dir(&self) -> PathBuf {
        self.dir.join("backups")
    }

    fn parse(path: &Path) -> Option<Config> {
        Self::parse_file(path).ok()
    }

    /// Reads and validates any config file (import / backup restore), with the reason on error.
    pub fn parse_file(path: &Path) -> Result<Config, String> {
        let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::parse_text(&text)
    }

    fn parse_text(text: &str) -> Result<Config, String> {
        let cfg: Config = serde_json::from_str(text).map_err(|e| e.to_string())?;
        validate(&cfg)?;
        Ok(migrate(cfg))
    }

    /// Writes the config as pretty JSON to an arbitrary path (export).
    pub fn export_to(cfg: &Config, path: &Path) -> io::Result<()> {
        let json = serde_json::to_string_pretty(cfg).map_err(io::Error::other)?;
        fs::write(path, json)
    }

    /// Loads with fallbacks: primary → .bak → newest daily backup → default.
    pub fn load(&self) -> LoadOutcome {
        let primary = self.primary_path();
        let reason = match fs::read_to_string(&primary) {
            Ok(text) => match Self::parse_text(&text) {
                Ok(c) => return LoadOutcome::Primary(c),
                Err(reason) => {
                    let stamp = today_yyyy_mm_dd().replace('-', "");
                    let seconds = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() % 86_400)
                        .unwrap_or(0);
                    let quarantine = self.dir.join(format!(
                        "config.corrupt-{stamp}-{:02}{:02}{:02}.json",
                        seconds / 3600,
                        seconds / 60 % 60,
                        seconds % 60
                    ));
                    let result = if quarantine.exists() {
                        Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            "quarantine filename already exists",
                        ))
                    } else {
                        fs::rename(&primary, &quarantine)
                    };
                    match result {
                        Ok(()) => FreshReason::CorruptPrimary {
                            reason,
                            quarantined: Some(quarantine),
                        },
                        Err(e) => FreshReason::CorruptPrimary {
                            reason: format!("{reason}; quarantine failed: {e}"),
                            quarantined: None,
                        },
                    }
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => FreshReason::FirstRun,
            Err(e) => FreshReason::UnreadablePrimary {
                reason: e.to_string(),
            },
        };
        if let Some(c) = Self::parse(&self.bak_path()) {
            return LoadOutcome::Recovered(c, self.bak_path());
        }
        let mut backups = self.list_backups();
        backups.sort();
        for p in backups.into_iter().rev() {
            if let Some(c) = Self::parse(&p) {
                return LoadOutcome::Recovered(c, p);
            }
        }
        LoadOutcome::Fresh(Config::default(), reason)
    }

    /// Daily backups (`backups/YYYY-MM-DD.json`), unsorted.
    pub fn list_backups(&self) -> Vec<PathBuf> {
        fs::read_dir(self.backups_dir())
            .map(|rd| {
                rd.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.extension().is_some_and(|e| e == "json"))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Atomically writes the config: tmp → fsync → rotate old to .bak → rename tmp over primary.
    pub fn save(&self, cfg: &Config) -> io::Result<()> {
        validate(cfg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_string_pretty(cfg).map_err(io::Error::other)?;
        {
            let mut f = fs::File::create(self.tmp_path())?;
            io::Write::write_all(&mut f, json.as_bytes())?;
            f.sync_all()?;
        }
        let primary = self.primary_path();
        if primary.exists() {
            // Best effort: keep the previous good file as .bak.
            let _ = fs::remove_file(self.bak_path());
            let _ = fs::rename(&primary, self.bak_path());
        }
        fs::rename(self.tmp_path(), &primary)?;
        self.daily_backup(&json)?;
        Ok(())
    }

    /// Writes at most one backup per day and prunes to `DAILY_BACKUPS_KEPT`.
    fn daily_backup(&self, json: &str) -> io::Result<()> {
        let dir = self.backups_dir();
        fs::create_dir_all(&dir)?;
        let today = today_yyyy_mm_dd();
        let path = dir.join(format!("{today}.json"));
        if !path.exists() {
            fs::write(&path, json)?;
        }
        let mut backups = self.list_backups();
        backups.sort();
        while backups.len() > DAILY_BACKUPS_KEPT {
            let oldest = backups.remove(0);
            let _ = fs::remove_file(oldest);
        }
        Ok(())
    }
}

/// Range checks from plan §7.5.
pub fn validate(cfg: &Config) -> Result<(), String> {
    if cfg.schema_version == 0 || cfg.schema_version > SCHEMA_VERSION {
        return Err(format!(
            "unsupported schema version {} (supported: {SCHEMA_VERSION})",
            cfg.schema_version
        ));
    }
    if cfg.items.len() > MAX_ITEMS {
        return Err(format!("too many items: {}", cfg.items.len()));
    }
    for layout in &cfg.layouts {
        if layout.fences.len() > MAX_FENCES {
            return Err(format!("too many fences: {}", layout.fences.len()));
        }
        let inboxes = layout
            .fences
            .iter()
            .filter(|f| f.kind == crate::model::FenceKind::Inbox)
            .count();
        if inboxes > 1 {
            return Err(format!("layout has {inboxes} inbox fences"));
        }
        for f in &layout.fences {
            if let crate::model::FenceContentSpec::Panel { panel } = &f.content {
                if panel.provider.trim().is_empty()
                    || panel.config_version == 0
                    || f.kind != crate::model::FenceKind::Virtual
                    || !f.items.is_empty()
                {
                    return Err("panels require provider/version and an empty virtual fence".into());
                }
            }
            let g = &f.geometry;
            if !(g.w.is_finite() && g.h.is_finite() && g.x.is_finite() && g.y.is_finite()) {
                return Err(format!("fence {} has non-finite geometry", f.title));
            }
            if g.w < 1.0 || g.h < 1.0 || g.w > 8192.0 || g.h > 8192.0 {
                return Err(format!(
                    "fence {} has out-of-range size {}x{}",
                    f.title, g.w, g.h
                ));
            }
            // 0.4–1.8: the per-fence menu offers 0.55 ("更透明") and 1.6 ("更厚实" = a veil).
            if let Some(a) = &f.appearance
                && let Some(o) = a.opacity
                && !(0.2..=2.0).contains(&o)
            {
                return Err(format!("fence {} opacity {o} out of range", f.title));
            }
        }
    }
    Ok(())
}

/// Schema migration chain (currently identity).
pub fn migrate(mut cfg: Config) -> Config {
    if cfg.schema_version < SCHEMA_VERSION {
        cfg.schema_version = SCHEMA_VERSION;
    }
    cfg
}

/// Local-date string without pulling in a date crate (civil-from-days algorithm, UTC-based;
/// backups are named for humans so the timezone offset is irrelevant).
pub fn today_yyyy_mm_dd() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
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
    use crate::model::*;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pecofence-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn sample() -> Config {
        let mut c = Config::default();
        c.layouts.push(Layout {
            fingerprint: vec![],
            fences: vec![Fence::new(
                "A",
                FenceKind::Inbox,
                NormGeometry {
                    monitor: "m".into(),
                    x: 1.0,
                    y: 2.0,
                    w: 300.0,
                    h: 200.0,
                    work_w: 1920.0,
                    work_h: 1000.0,
                    anchor: Anchor::LeftTop,
                },
            )],
        });
        c
    }

    #[test]
    fn save_then_load_primary() {
        let store = ConfigStore::new(tmpdir("primary"));
        store.save(&sample()).unwrap();
        assert!(matches!(store.load(), LoadOutcome::Primary(_)));
        assert!(store.primary_path().exists());
        assert!(!store.tmp_path().exists());
        assert_eq!(store.list_backups().len(), 1);
    }

    #[test]
    fn corrupt_primary_falls_back_to_bak() {
        let store = ConfigStore::new(tmpdir("bak"));
        store.save(&sample()).unwrap();
        store.save(&sample()).unwrap(); // creates .bak
        fs::write(store.primary_path(), "{ not json").unwrap();
        match store.load() {
            LoadOutcome::Recovered(c, from) => {
                assert_eq!(c.layouts[0].fences[0].title, "A");
                assert_eq!(from, store.bak_path());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn nothing_usable_gives_fresh() {
        let store = ConfigStore::new(tmpdir("fresh"));
        assert!(matches!(
            store.load(),
            LoadOutcome::Fresh(_, FreshReason::FirstRun)
        ));
    }

    #[test]
    fn truncated_primary_quarantined_and_reason_visible() {
        let store = ConfigStore::new(tmpdir("truncated"));
        fs::create_dir_all(store.dir()).unwrap();
        let damaged = b"{\"schemaVersion\":";
        fs::write(store.primary_path(), damaged).unwrap();
        match store.load() {
            LoadOutcome::Fresh(
                _,
                FreshReason::CorruptPrimary {
                    reason,
                    quarantined,
                },
            ) => {
                assert!(!reason.is_empty());
                let path = quarantined.unwrap();
                assert_eq!(fs::read(path).unwrap(), damaged);
                assert!(!store.primary_path().exists());
                assert!(store.list_backups().is_empty());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn syntax_error_primary_quarantined() {
        let store = ConfigStore::new(tmpdir("syntax"));
        fs::create_dir_all(store.dir()).unwrap();
        fs::write(store.primary_path(), "{ bad json").unwrap();
        assert!(matches!(
            store.load(),
            LoadOutcome::Fresh(
                _,
                FreshReason::CorruptPrimary {
                    quarantined: Some(_),
                    ..
                }
            )
        ));
    }

    #[test]
    fn future_schema_version_reports_unsupported() {
        let store = ConfigStore::new(tmpdir("future-schema"));
        fs::create_dir_all(store.dir()).unwrap();
        let cfg = Config {
            schema_version: SCHEMA_VERSION + 1,
            ..Config::default()
        };
        fs::write(store.primary_path(), serde_json::to_vec(&cfg).unwrap()).unwrap();
        match store.load() {
            LoadOutcome::Fresh(_, FreshReason::CorruptPrimary { reason, .. }) => {
                assert!(reason.contains(&format!(
                    "unsupported schema version {} (supported: {SCHEMA_VERSION})",
                    cfg.schema_version
                )));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_primary_not_first_run() {
        use std::os::unix::fs::PermissionsExt;
        let store = ConfigStore::new(tmpdir("unreadable"));
        fs::create_dir_all(store.dir()).unwrap();
        fs::write(store.primary_path(), "{}").unwrap();
        fs::set_permissions(store.primary_path(), fs::Permissions::from_mode(0o000)).unwrap();
        let outcome = store.load();
        fs::set_permissions(store.primary_path(), fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            outcome,
            LoadOutcome::Fresh(_, FreshReason::UnreadablePrimary { .. })
        ));
    }

    #[test]
    fn save_after_quarantine_writes_clean_primary() {
        let store = ConfigStore::new(tmpdir("save-quarantine"));
        fs::create_dir_all(store.dir()).unwrap();
        fs::write(store.primary_path(), "{").unwrap();
        let (cfg, quarantine) = match store.load() {
            LoadOutcome::Fresh(
                cfg,
                FreshReason::CorruptPrimary {
                    quarantined: Some(path),
                    ..
                },
            ) => (cfg, path),
            other => panic!("unexpected {other:?}"),
        };
        store.save(&cfg).unwrap();
        assert!(matches!(store.load(), LoadOutcome::Primary(_)));
        assert_eq!(fs::read(quarantine).unwrap(), b"{");
    }

    #[test]
    fn missing_everything_is_first_run() {
        let store = ConfigStore::new(tmpdir("missing-first-run"));
        assert!(matches!(
            store.load(),
            LoadOutcome::Fresh(_, FreshReason::FirstRun)
        ));
    }

    #[test]
    fn renamed_product_reuses_old_configuration_without_changing_it() {
        let parent = tmpdir("rebrand");
        let old = ConfigStore::new(parent.join(crate::brand::LEGACY_DATA_DIR));
        let mut config = sample();
        config.layouts[0].fences[0].title = "My OpenFence files {1}".into();
        config.settings.language = crate::i18n::Language::Japanese;
        old.save(&config).unwrap();
        let original = fs::read(old.primary_path()).unwrap();
        let preferred = parent.join(crate::brand::NAME);
        let selected = ConfigStore::with_legacy(&preferred, old.dir());
        assert_eq!(selected.dir(), old.dir());
        let loaded = selected.load().into_config();
        assert_eq!(
            serde_json::to_value(loaded).unwrap(),
            serde_json::to_value(config).unwrap()
        );
        assert_eq!(fs::read(old.primary_path()).unwrap(), original);
        assert!(!preferred.exists());
    }

    #[test]
    fn current_product_data_wins_and_old_backup_only_installs_still_load() {
        let parent = tmpdir("rebrand-precedence");
        let old = ConfigStore::new(parent.join(crate::brand::LEGACY_DATA_DIR));
        old.save(&sample()).unwrap();
        old.save(&sample()).unwrap();
        fs::remove_file(old.primary_path()).unwrap();
        let preferred = ConfigStore::new(parent.join(crate::brand::NAME));
        let selected = ConfigStore::with_legacy(preferred.dir(), old.dir());
        assert!(matches!(selected.load(), LoadOutcome::Recovered(..)));
        preferred.save(&sample()).unwrap();
        // Even a damaged new config belongs to the new installation. Its normal
        // backup recovery must not silently switch to a stale legacy layout.
        fs::write(preferred.primary_path(), "{ bad json").unwrap();
        assert_eq!(
            ConfigStore::with_legacy(preferred.dir(), old.dir()).dir(),
            preferred.dir()
        );
        assert_eq!(
            ConfigStore::with_legacy(parent.join("fresh"), parent.join("missing")).dir(),
            parent.join("fresh")
        );
    }

    #[test]
    fn validation_rejects_two_inboxes() {
        let mut c = sample();
        let f = c.layouts[0].fences[0].clone();
        c.layouts[0].fences.push(Fence {
            id: uuid::Uuid::new_v4(),
            ..f
        });
        assert!(validate(&c).is_err());
    }

    #[test]
    fn date_formatting_is_sane() {
        let s = today_yyyy_mm_dd();
        assert_eq!(s.len(), 10);
        assert!(s.starts_with("20"));
    }
}

//! Application state: config + layout selection + desktop item catalog + rule routing.

use pecofence_core::geometry::{self, PxRect, WorkArea};
use pecofence_core::portal::{PortalHealth, PortalOutcome, PortalRead, PortalResult};
use pecofence_core::rules::{Cond, Decision, RuleSet, Target, Template};
use pecofence_core::settings_protocol::{DocumentClock, DocumentStamp};
use pecofence_core::{
    AssignedBy, Config, ConfigStore, Container, ContainerId, ContentInstance, ContentSpec, FenceId,
    FenceKind, FenceSnapshot as Fence, FreshReason, IconKey, Item, ItemId, ItemKey, ItemRef,
    ItemSourceSpec, Layout, LoadOutcome, MonitorIdentity, NormGeometry, Origin, SortMode,
    TabDetach, Transition, Workspace, WorkspaceError,
};
use pecofence_platform::RECT;
use pecofence_platform::shell::{self, DesktopEntry, EntryOrigin};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

mod defaults;
mod fences;
mod items;
mod portals;
#[cfg(test)]
mod tests;

use fences::dir_prefix;
pub(crate) use portals::portal_item_id;

const ORPHAN_GC_SECS: i64 = 7 * 24 * 3600;

pub struct AppState {
    pub config: Config,
    store: ConfigStore,
    pub layout: usize,
    document: DocumentClock,
    catalog: HashMap<ItemKey, ItemId>,
    /// Runtime-only items of folder portals (never persisted; rebuilt from the folder).
    portal_items: HashMap<ItemId, Item>,
    /// Portal fence → its current item ids.
    portal_members: HashMap<FenceId, Vec<ItemId>>,
    /// Portal fence → subfolder it has navigated into (absent = its root folder).
    portal_cwd: HashMap<FenceId, PathBuf>,
    pub(crate) portals: crate::portal_runtime::PortalRuntime,
    portal_sources: HashMap<FenceId, ItemSourceSpec>,
    portal_layout: usize,
    pub work_areas: Vec<WorkArea>,
    pub first_run: bool,
    pub recovered_from: Option<PathBuf>,
    pub load_issue: Option<String>,
    pub save_allowed: bool,
    pub persistence_issue: Option<String>,
    pending_explicit_replacement: Option<DocumentStamp>,
}

/// Summary of a desktop sync pass.
#[derive(Debug, Default)]
pub struct SyncReport {
    pub added: usize,
    pub removed: usize,
    pub updated: usize,
}

impl SyncReport {
    pub fn changed(&self) -> bool {
        self.added + self.removed + self.updated > 0
    }
}

pub(crate) fn config_dir(portable: bool) -> PathBuf {
    if portable {
        let exe = std::env::current_exe().unwrap_or_default();
        return exe
            .parent()
            .map(|p| p.join("config"))
            .unwrap_or_else(|| PathBuf::from("config"));
    }
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("PecoFence")
}

impl AppState {
    pub fn load(work_areas: Vec<WorkArea>, portable: bool, config_override: Option<&Path>) -> Self {
        let directory = config_override
            .map(Path::to_path_buf)
            .unwrap_or_else(|| config_dir(portable));
        let store = ConfigStore::new(&directory);
        let outcome = store.load();
        Self::from_load(work_areas, store, outcome)
    }

    fn from_load(work_areas: Vec<WorkArea>, store: ConfigStore, outcome: LoadOutcome) -> Self {
        let committed = matches!(&outcome, LoadOutcome::Primary(_));
        let (config, first_run, recovered_from, load_issue) = match outcome {
            LoadOutcome::Primary(c) => (c, false, None, None),
            LoadOutcome::Recovered(c, from) => (c, false, Some(from), None),
            LoadOutcome::Fresh(c, FreshReason::FirstRun) => (c, true, None, None),
            LoadOutcome::Fresh(
                c,
                FreshReason::CorruptPrimary {
                    reason,
                    quarantined,
                },
            ) => {
                let detail = match quarantined {
                    Some(path) => format!(
                        "Corrupt config: {reason}; quarantined at {}",
                        path.display()
                    ),
                    None => format!("Corrupt config: {reason}"),
                };
                (c, false, None, Some(detail))
            }
            LoadOutcome::Fresh(c, FreshReason::UnreadablePrimary { reason }) => {
                (c, false, None, Some(format!("Unreadable config: {reason}")))
            }
            LoadOutcome::Fresh(c, FreshReason::UnsupportedFormat { path, schema }) => (
                c,
                false,
                None,
                Some(format!(
                    "Unsupported workspace format {:?}: {}",
                    schema,
                    path.display()
                )),
            ),
        };
        if let Some(issue) = &load_issue {
            tracing::warn!(load_issue = %issue, "config.load_issue");
        }
        pecofence_core::i18n::set_language(
            config
                .settings
                .language
                .resolve(pecofence_platform::locale::ui_language()),
        );
        let save_allowed = load_issue.is_none() && recovered_from.is_none();
        let mut state = Self {
            config,
            store,
            layout: 0,
            document: DocumentClock::new(committed),
            catalog: HashMap::new(),
            portal_items: HashMap::new(),
            portal_members: HashMap::new(),
            portal_cwd: HashMap::new(),
            portals: Default::default(),
            portal_sources: HashMap::new(),
            portal_layout: 0,
            work_areas,
            first_run,
            recovered_from,
            load_issue,
            save_allowed,
            persistence_issue: None,
            pending_explicit_replacement: None,
        };
        state.rebuild_catalog();
        if state.save_allowed || !state.config.layouts.is_empty() {
            state.ensure_layout();
        }
        if state.load_issue.is_some() {
            state.document = DocumentClock::new(false);
        }
        state
    }

    pub fn config_path(&self) -> PathBuf {
        self.store.primary_path()
    }

    pub fn backup_files(&self) -> Vec<PathBuf> {
        let mut v = self.store.list_backups();
        v.sort();
        v.reverse();
        v
    }

    /// Replaces the whole configuration (import / backup restore) and re-derives the runtime
    /// state. Fence windows must be resynced by the caller. (Snapshot restore goes through
    /// `restore_snapshot`, which only swaps layouts.)
    pub fn replace_config(&mut self, mut config: Config) -> Result<(), String> {
        config.validate()?;
        // Snapshots are local history, including the "…前" undo snapshot the caller just took:
        // never let an import / backup file wipe them. Merge the file's by id, oldest first.
        let mut snaps = self.config.snapshots.clone();
        for s in std::mem::take(&mut config.snapshots) {
            if !snaps.iter().any(|x| x.id == s.id) {
                snaps.push(s);
            }
        }
        snaps.sort_by_key(|s| s.ts);
        while snaps.len() > pecofence_core::MAX_SNAPSHOTS {
            snaps.remove(0);
        }
        config.snapshots = snaps;
        config.validate()?;
        self.config = config;
        self.document.replace();
        self.save_allowed = true;
        self.load_issue = None;
        self.recovered_from = None;
        self.persistence_issue = None;
        self.rebuild_catalog();
        self.layout = 0;
        self.ensure_layout();
        self.first_run = false;
        self.reset_portal_runtime();
        self.mark_dirty();
        self.pending_explicit_replacement = Some(self.document_stamp());
        Ok(())
    }

    /// Explicit acknowledgement promotes a supported recovered document, not a fresh error fallback.
    pub fn accept_recovery(&mut self) -> Result<(), String> {
        if self.recovered_from.is_none() {
            return Err("No recovered workspace is available".into());
        }
        self.replace_config(self.config.clone())
    }

    /// Explicit user reset is the only error-state path that builds the first-run defaults.
    pub fn reset_workspace(&mut self) -> Result<(), String> {
        self.first_run = true;
        self.replace_config(Config::default())
    }

    // ---- snapshots ---------------------------------------------------------------------------

    pub fn save_snapshot(&mut self, name: &str) -> uuid::Uuid {
        let name = name.trim();
        let name = if name.is_empty() {
            pecofence_core::i18n::format(
                "快照 {0}",
                &[format!("{}", self.config.snapshots.len() + 1)],
            )
        } else {
            name.to_string()
        };
        let snap = pecofence_core::Snapshot {
            id: uuid::Uuid::new_v4(),
            name,
            ts: pecofence_core::now_unix(),
            layouts: self.config.layouts.clone(),
            appearance: None,
        };
        let id = snap.id;
        self.config.snapshots.push(snap);
        while self.config.snapshots.len() > pecofence_core::MAX_SNAPSHOTS {
            self.config.snapshots.remove(0);
        }
        self.mark_dirty();
        id
    }

    /// Restores a snapshot's layouts (settings, rules and items stay as they are) after saving
    /// a `backup_name` snapshot of the current ones. The target is looked up first, so the
    /// backup's eviction at `MAX_SNAPSHOTS` can never remove it. Returns false (and saves
    /// nothing) when `id` is unknown.
    pub fn restore_snapshot_with_backup(&mut self, id: uuid::Uuid, backup_name: &str) -> bool {
        let Some(snap) = self.config.snapshots.iter().find(|s| s.id == id).cloned() else {
            return false;
        };
        // History can outlive catalog GC. Restore the layout, not references to records
        // that no longer exist in the active document.
        let mut layouts = snap.layouts;
        let mut live_panels: HashMap<_, _> = self
            .config
            .layouts
            .iter()
            .flat_map(|l| &l.contents)
            .filter_map(|content| match &content.content {
                ContentSpec::Panel { panel } => {
                    Some(((panel.instance_id, panel.provider.clone()), panel.clone()))
                }
                _ => None,
            })
            .collect();
        if let Some(active) = self.config.layouts.get(self.layout) {
            for content in &active.contents {
                if let ContentSpec::Panel { panel } = &content.content {
                    live_panels.insert((panel.instance_id, panel.provider.clone()), panel.clone());
                }
            }
        }
        for layout in &mut layouts {
            for content in &mut layout.contents {
                if let ContentSpec::Panel { panel } = &mut content.content
                    && let Some(live) =
                        live_panels.get(&(panel.instance_id, panel.provider.clone()))
                {
                    *panel = live.clone();
                }
            }
            for content in &mut layout.contents {
                if let ContentSpec::FileCollection { items, .. } = &mut content.content {
                    items.retain(|r| self.config.items.contains_key(&r.item_id));
                }
            }
        }
        let mut candidate = self.config.clone();
        candidate.layouts = layouts.clone();
        if let Some(appearance) = &snap.appearance {
            appearance.apply(&mut candidate.settings);
        }
        if candidate.validate().is_err() {
            return false;
        }
        let backup = self.save_snapshot(backup_name);
        if snap.appearance.is_some() {
            self.config
                .snapshots
                .iter_mut()
                .find(|s| s.id == backup)
                .unwrap()
                .appearance = Some(pecofence_core::WorkspaceAppearance::capture(
                &self.config.settings,
            ));
        }
        self.config.settings = candidate.settings;
        self.apply_snapshot_layouts(layouts);
        true
    }

    fn apply_snapshot_layouts(&mut self, layouts: Vec<pecofence_core::Layout>) {
        self.config.layouts = layouts;
        self.save_allowed = true;
        self.load_issue = None;
        self.recovered_from = None;
        self.layout = 0;
        self.ensure_layout();
        self.reset_portal_runtime();
        self.mark_dirty();
        self.pending_explicit_replacement = Some(self.document_stamp());
    }

    pub fn delete_snapshot(&mut self, id: uuid::Uuid) -> bool {
        let before = self.config.snapshots.len();
        self.config.snapshots.retain(|s| s.id != id);
        let removed = self.config.snapshots.len() != before;
        if removed {
            self.mark_dirty();
        }
        removed
    }

    /// Fences "swap screen contents": every fence on monitor `a` moves to `b` and vice versa,
    /// keeping its anchored edge gaps (a right-anchored fence stays 16 DIP from the right edge
    /// of the other monitor, whatever its width).
    pub fn swap_monitors(&mut self, a: &str, b: &str) -> usize {
        if a == b {
            return 0;
        }
        let layout = self.layout;
        let areas = self.work_areas.clone();
        let mut n = 0;
        for f in &mut self.config.layouts[layout].containers {
            let to = if f.geometry.monitor == a {
                b
            } else if f.geometry.monitor == b {
                a
            } else {
                continue;
            };
            // Map onto the other work area first (denormalize keeps the anchored gaps against
            // the *stored* work_w/work_h), then store the geometry re-normalised there so x/y
            // and work_w/work_h stay consistent — a fence hosted as a tab has no window to do
            // this through FenceBoundsChanged.
            match areas.iter().find(|w| w.device_path == to) {
                Some(w) => {
                    let px = geometry::denormalize(&f.geometry, w);
                    f.geometry = geometry::normalize(px, w);
                }
                None => f.geometry.monitor = to.to_string(),
            }
            n += 1;
        }
        if n > 0 {
            self.mark_dirty();
        }
        n
    }

    /// Desktop folder moved (OneDrive redirect, another drive): re-point every item record
    /// under the old path to the new one so fence memberships survive. Returns how many.
    pub fn migrate_desktop_path(&mut self, new_desktop: &Path) -> usize {
        let new_s = new_desktop.to_string_lossy().to_string();
        let old = match &self.config.settings.desktop_path {
            Some(o) => o.clone(),
            None => {
                self.config.settings.desktop_path = Some(new_s);
                self.mark_dirty();
                return 0;
            }
        };
        let old_key = ItemKey::from_path(&old);
        let new_key = ItemKey::from_path(&new_s);
        if old_key == new_key {
            return 0;
        }
        let Some(old_prefix) = old_key.as_path().map(dir_prefix) else {
            return 0;
        };
        let Some(new_prefix) = new_key.as_path().map(dir_prefix) else {
            return 0;
        };
        let mut n = 0;
        for item in self.config.items.values_mut() {
            if let Some(path) = item.key.as_path()
                && let Some(rest) = path.strip_prefix(old_prefix.as_str())
            {
                let rest = rest.to_string();
                let moved = format!("{new_prefix}{rest}");
                item.key = ItemKey::Path(moved.clone());
                if let IconKey::ByContent { path, .. } = &mut item.icon_key {
                    *path = moved;
                }
                n += 1;
            }
        }
        self.config.settings.desktop_path = Some(new_s);
        self.rebuild_catalog();
        self.mark_dirty();
        n
    }

    fn rebuild_catalog(&mut self) {
        self.catalog = self
            .config
            .items
            .iter()
            .map(|(id, it)| (it.key.clone(), *id))
            .collect();
    }

    fn device_paths(&self) -> Vec<String> {
        self.work_areas
            .iter()
            .map(|w| w.device_path.clone())
            .collect()
    }

    /// Selects the layout for the current monitors, creating the default one on first use.
    pub fn ensure_layout(&mut self) {
        if !self.save_allowed {
            self.layout = self.config.layout_for(&self.device_paths()).unwrap_or(0);
            return;
        }
        if self.config.layouts.is_empty() && !self.first_run {
            return;
        }
        let paths = self.device_paths();
        if let Some(i) = self.config.layout_for(&paths) {
            self.layout = i;
            if self.portal_layout != self.layout {
                self.reset_portal_runtime();
            }
            return;
        }
        // Reuse the first existing layout's fences if monitors changed (best effort), else
        // build the wizard default.
        let (containers, contents) = if let Some(existing) = self.config.layouts.first() {
            (existing.containers.clone(), existing.contents.clone())
        } else {
            self.default_fences()
        };
        let fingerprint = self
            .work_areas
            .iter()
            .map(|w| MonitorIdentity {
                device_path: w.device_path.clone(),
                work_dip: [w.width_dip(), w.height_dip()],
                dpi: w.dpi,
            })
            .collect();
        self.config.layouts.push(Layout {
            fingerprint,
            containers,
            contents,
        });
        self.layout = self.config.layouts.len() - 1;
        self.reset_portal_runtime();
        self.mark_dirty();
    }

    /// The same versioned template is used by first run and explicit restoration.
    fn default_fences(&mut self) -> (Vec<Container>, Vec<ContentInstance>) {
        self.build_default_fences()
    }

    pub fn mark_dirty(&mut self) {
        self.document.change();
    }

    pub fn document_stamp(&self) -> DocumentStamp {
        self.document.stamp()
    }

    pub fn committed_revision(&self) -> Option<u64> {
        self.document.committed()
    }

    pub fn is_dirty(&self) -> bool {
        self.document.dirty()
    }

    pub fn prepare_save(&self) -> Option<crate::persistence::SavePlan> {
        if !self.is_dirty() || !self.save_allowed {
            return None;
        }
        Some(crate::persistence::SavePlan {
            stamp: self.document_stamp(),
            replace: self.pending_explicit_replacement.is_some(),
            config: self.config.clone(),
        })
    }

    /// Only the application's coordinator may deliver a verified terminal receipt.
    /// An old activation still has an accounted write, but cannot acknowledge this document.
    pub fn apply_save_result(&mut self, result: crate::persistence::WriteResult) -> Option<bool> {
        if result.ticket.stamp.workspace != self.document_stamp().workspace
            || result.ticket.stamp.revision > self.document_stamp().revision
        {
            return None;
        }
        match result.outcome {
            Ok(receipt) => {
                self.document.commit(result.ticket.stamp);
                if result.ticket.replace
                    && self.pending_explicit_replacement.is_some_and(|barrier| {
                        barrier.workspace == result.ticket.stamp.workspace
                            && barrier.revision <= result.ticket.stamp.revision
                    })
                {
                    self.pending_explicit_replacement = None;
                }
                self.persistence_issue = match receipt.backup {
                    pecofence_core::BackupStatus::Degraded(reason) => Some(reason),
                    _ => None,
                };
                tracing::debug!(revision = result.ticket.stamp.revision, "config committed");
                Some(true)
            }
            Err(error) => {
                self.persistence_issue = Some(error.clone());
                tracing::error!(%error, "config commit failed");
                Some(false)
            }
        }
    }

    #[cfg(test)]
    fn save_if_dirty(&mut self) -> bool {
        let Some(plan) = self.prepare_save() else {
            return false;
        };
        let outcome = if plan.replace {
            self.store.replace(&plan.config)
        } else {
            self.store.save(&plan.config)
        };
        self.apply_save_result(crate::persistence::WriteResult {
            ticket: crate::persistence::Ticket {
                id: 0,
                stamp: plan.stamp,
                replace: plan.replace,
            },
            outcome: outcome.map_err(|error| error.to_string()),
        }) == Some(true)
    }
}

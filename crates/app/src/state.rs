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
    pending_explicit_replacement: bool,
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

fn config_dir(portable: bool) -> PathBuf {
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
    pub fn load(work_areas: Vec<WorkArea>, portable: bool) -> Self {
        let directory = config_dir(portable);
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
            pending_explicit_replacement: false,
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
        self.pending_explicit_replacement = true;
        self.load_issue = None;
        self.recovered_from = None;
        self.rebuild_catalog();
        self.layout = 0;
        self.ensure_layout();
        self.first_run = false;
        self.reset_portal_runtime();
        self.mark_dirty();
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
        for layout in &mut layouts {
            for content in &mut layout.contents {
                if let ContentSpec::FileCollection { items, .. } = &mut content.content {
                    items.retain(|r| self.config.items.contains_key(&r.item_id));
                }
            }
        }
        let mut candidate = self.config.clone();
        candidate.layouts = layouts.clone();
        if candidate.validate().is_err() {
            return false;
        }
        self.save_snapshot(backup_name);
        self.apply_snapshot_layouts(layouts);
        true
    }

    fn apply_snapshot_layouts(&mut self, layouts: Vec<pecofence_core::Layout>) {
        self.config.layouts = layouts;
        self.save_allowed = true;
        self.pending_explicit_replacement = true;
        self.load_issue = None;
        self.recovered_from = None;
        self.layout = 0;
        self.ensure_layout();
        self.reset_portal_runtime();
        self.mark_dirty();
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

    /// Stardock-style first-run layout: right column 程序 / 文件夹 / 文件与文档 / 桌面(inbox).
    fn default_fences(&mut self) -> (Vec<Container>, Vec<ContentInstance>) {
        let work = self
            .work_areas
            .iter()
            .find(|w| w.device_path.ends_with("DISPLAY1") || true)
            .cloned()
            .unwrap_or(WorkArea {
                device_path: "primary".into(),
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
                dpi: 96,
                mon_left: 0,
                mon_top: 0,
                mon_right: 1920,
                mon_bottom: 1040,
            });
        let ww = work.width_dip();
        let wh = work.height_dip();
        // Exactly three icon columns at the default icon size, so the column snap applied by
        // sync_fence_windows is a no-op and the right edge stays `gap` inside the work area.
        let icon = self.config.settings.icon_size;
        let col_w = crate::layout::GridMetrics::for_icon_size(
            icon,
            pecofence_core::FenceView::default().label_lines,
        )
        .width_for_columns(3);
        let gap = 16.0f32;
        let x = ww - gap - col_w;
        let heights = [240.0f32, 200.0, 240.0];
        let titles = [
            pecofence_core::i18n::text("程序"),
            pecofence_core::i18n::text("文件夹"),
            pecofence_core::i18n::text("文件与文档"),
        ];
        let mut y = gap;
        let mut layout = Layout {
            fingerprint: Vec::new(),
            containers: Vec::new(),
            contents: Vec::new(),
        };
        let mut ids = Vec::new();
        for (title, h) in titles.iter().zip(heights) {
            let h = h.min((wh - gap * 2.0) / 4.0).max(120.0);
            let geo = geometry::normalize(
                PxRect {
                    left: work.left + (x * work.scale()) as i32,
                    top: work.top + (y * work.scale()) as i32,
                    right: work.left + ((x + col_w) * work.scale()) as i32,
                    bottom: work.top + ((y + h) * work.scale()) as i32,
                },
                &work,
            );
            let mut f = ContentInstance::collection(title, false);
            f.view.icon_size = icon;
            ids.push(f.id);
            layout.containers.push(Container::new(f.id, geo));
            layout.contents.push(f);
            y += h + gap;
        }
        let inbox_h = (wh - gap - y).max(160.0);
        let inbox_geo = geometry::normalize(
            PxRect {
                left: work.left + (x * work.scale()) as i32,
                top: work.top + (y * work.scale()) as i32,
                right: work.left + ((x + col_w) * work.scale()) as i32,
                bottom: work.top + ((y + inbox_h) * work.scale()) as i32,
            },
            &work,
        );
        let mut inbox = ContentInstance::collection(pecofence_core::i18n::text("桌面"), true);
        inbox.view.icon_size = icon;
        layout.containers.push(Container::new(inbox.id, inbox_geo));
        layout.contents.push(inbox);
        self.config.rules = RuleSet::default_presets(ids[0], ids[1], ids[2]);
        (layout.containers, layout.contents)
    }

    pub fn fences(&self) -> Vec<Fence> {
        self.config
            .layouts
            .get(self.layout)
            .map(|l| l.contents.iter().filter_map(|c| l.project(c.id)).collect())
            .unwrap_or_default()
    }

    fn layout_mut(&mut self) -> Result<&mut Layout, WorkspaceError> {
        self.config
            .layouts
            .get_mut(self.layout)
            .ok_or_else(|| WorkspaceError::InvalidLayout("No active workspace layout".into()))
    }

    // ---- tabbed fences ----------------------------------------------------------------------

    /// Unknown/deleted identities are normal for queued native events, not panics.
    pub fn host_of(&self, id: FenceId) -> Option<ContainerId> {
        self.config.layouts.get(self.layout)?.owner_of(id)
    }

    /// Fences that own a window (not hosted as tabs), in layout order.
    pub fn host_fences(&self) -> Vec<Fence> {
        self.config
            .layouts
            .get(self.layout)
            .map(|l| {
                l.containers
                    .iter()
                    .filter_map(|c| l.project(c.active_tab))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn tabs_of(&self, host: ContainerId) -> Vec<FenceId> {
        self.container(host)
            .map(|c| c.tabs.clone())
            .unwrap_or_default()
    }

    pub fn active_tab_of(&self, host: ContainerId) -> Option<FenceId> {
        self.container(host).map(|container| container.active_tab)
    }

    pub fn attach_tab(
        &mut self,
        content: FenceId,
        host: ContainerId,
    ) -> Result<Transition, WorkspaceError> {
        let before = self.tabs_of(host);
        let active = self.container(host).map(|c| c.active_tab);
        let index = self.tabs_of(host).len();
        let change = Workspace::new(self.layout_mut()?)?.attach(content, host, index)?;
        if before != self.tabs_of(host) || active != self.container(host).map(|c| c.active_tab) {
            self.mark_dirty();
        }
        Ok(change)
    }

    /// Live native drag uses a guarded structural inverse, never a cloned content rollback.
    pub fn detach_tab_with_plan(
        &mut self,
        content: FenceId,
        geometry: NormGeometry,
    ) -> Result<(Transition, TabDetach), WorkspaceError> {
        let result = Workspace::new(self.layout_mut()?)?.detach_with_plan(content, geometry)?;
        self.mark_dirty();
        Ok(result)
    }

    pub fn cancel_tab_detach(&mut self, plan: &TabDetach) -> Result<Transition, WorkspaceError> {
        let result = Workspace::new(self.layout_mut()?)?.cancel_detach(plan)?;
        self.mark_dirty();
        Ok(result)
    }

    pub fn reorder_tab(
        &mut self,
        host: ContainerId,
        tab: FenceId,
        to: usize,
    ) -> Result<Transition, WorkspaceError> {
        let before = self.tabs_of(host);
        let change = Workspace::new(self.layout_mut()?)?.reorder(host, tab, to)?;
        if before != self.tabs_of(host) {
            self.mark_dirty();
        }
        Ok(change)
    }

    pub fn set_active_tab(
        &mut self,
        host: ContainerId,
        tab: FenceId,
    ) -> Result<Transition, WorkspaceError> {
        let changed = self.container(host).is_none_or(|c| c.active_tab != tab);
        let transition = Workspace::new(self.layout_mut()?)?.select(host, tab)?;
        if changed {
            self.mark_dirty();
        }
        Ok(transition)
    }

    pub fn fence(&self, id: FenceId) -> Option<Fence> {
        self.config.layouts.get(self.layout)?.project(id)
    }

    pub fn window_content(&self, id: ContainerId) -> Option<Fence> {
        self.fence(self.container(id)?.active_tab)
    }

    pub fn container(&self, id: ContainerId) -> Option<&Container> {
        self.config.layouts.get(self.layout)?.container(id)
    }

    /// Raw domain edits must be followed by `mark_dirty` by the caller.
    pub fn container_mut(&mut self, id: ContainerId) -> Option<&mut Container> {
        self.config.layouts.get_mut(self.layout)?.container_mut(id)
    }

    /// Raw domain edits must be followed by `mark_dirty` by the caller.
    pub fn content_mut(&mut self, id: FenceId) -> Option<&mut ContentInstance> {
        self.config.layouts.get_mut(self.layout)?.content_mut(id)
    }

    pub fn inbox_id(&self) -> Option<FenceId> {
        self.config.layouts.get(self.layout)?.inbox()
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

    pub fn save_if_dirty(&mut self) -> bool {
        if !self.is_dirty() || !self.save_allowed {
            return false;
        }
        let saved = self.document_stamp();
        let result = if self.pending_explicit_replacement {
            self.store.replace(&self.config)
        } else {
            self.store.save(&self.config)
        };
        match result {
            Ok(receipt) => {
                self.document.commit(saved);
                self.pending_explicit_replacement = false;
                self.persistence_issue = match receipt.backup {
                    pecofence_core::BackupStatus::Degraded(reason) => Some(reason),
                    _ => None,
                };
                tracing::debug!(path = %self.store.primary_path().display(), "config saved");
                true
            }
            Err(e) => {
                self.persistence_issue = Some(e.to_string());
                tracing::error!(error = %e, "config save failed");
                false
            }
        }
    }

    // ---- desktop items ---------------------------------------------------------------------

    /// Rules only run automatically while "新项目出现在桌面时自动归类" is on; otherwise new
    /// items go to the inbox until the user applies the rules by hand.
    fn route_decision(&self, entry: &DesktopEntry) -> Decision {
        // The Recycle Bin and friends are not files: they always live in the inbox ("桌面").
        if entry.origin == EntryOrigin::Namespace {
            return Decision::Default(Target::Inbox);
        }
        if self.config.rules.keep_updated {
            self.config.rules.evaluate(&self.facts_for(entry))
        } else {
            Decision::Default(Target::Inbox)
        }
    }

    /// `id` if it is a fence that can hold routed desktop items. A folder portal never shows
    /// `fence.items` (its content comes from the folder), so routing into one would make the
    /// item vanish from every fence.
    pub fn routable_fence(&self, id: FenceId) -> Option<FenceId> {
        let content = self.config.layouts.get(self.layout)?.content(id)?;
        matches!(content.content, ContentSpec::FileCollection { .. }).then_some(id)
    }

    fn target_fence(&self, decision: Decision) -> Option<(FenceId, AssignedBy)> {
        let inbox = self.inbox_id();
        match decision {
            Decision::Skip => None,
            Decision::Route { target, rule } => {
                let fence = match target {
                    Target::Inbox => inbox?,
                    Target::Collection(id) if self.routable_fence(id).is_some() => id,
                    Target::Collection(_) => inbox?,
                };
                Some((fence, AssignedBy::Rule(rule)))
            }
            Decision::Default(target) => {
                let fence = match target {
                    Target::Inbox => inbox?,
                    Target::Collection(id) if self.routable_fence(id).is_some() => id,
                    Target::Collection(_) => inbox?,
                };
                Some((fence, AssignedBy::Default))
            }
        }
    }

    fn facts_for(&self, entry: &DesktopEntry) -> pecofence_core::ItemFacts {
        let last_opened = self
            .catalog
            .get(&ItemKey::from_path(&entry.path.to_string_lossy()))
            .and_then(|id| self.config.items.get(id))
            .and_then(|it| it.last_opened);
        Self::facts_at(entry, last_opened, pecofence_core::now_unix())
    }

    /// Whole days between `now` and the later of last write and last open; `None` when the
    /// file reports no time at all.
    fn idle_days(mtime: i64, last_opened: Option<i64>, now: i64) -> Option<u32> {
        let last = mtime.max(last_opened.unwrap_or(0));
        (last > 0).then(|| ((now - last).max(0) / 86_400) as u32)
    }

    fn facts_at(
        entry: &DesktopEntry,
        last_opened: Option<i64>,
        now: i64,
    ) -> pecofence_core::ItemFacts {
        let lower = entry.file_name.to_lowercase();
        let (shortcut_target, shortcut_target_ext) = if lower.ends_with(".lnk") {
            let t = shell::shortcut_target(&entry.path);
            let ext = t.as_deref().map(pecofence_core::rules::ext_of);
            (t, ext)
        } else if lower.ends_with(".url") {
            (shell::url_shortcut_target(&entry.path), Some(".url".into()))
        } else {
            (None, None)
        };
        pecofence_core::ItemFacts {
            file_name: entry.file_name.clone(),
            is_folder: entry.is_folder,
            size_bytes: entry.size,
            shortcut_target,
            shortcut_target_ext,
            created_minutes_local: pecofence_platform::fileinfo::local_time_parts(entry.created)
                .map(|(m, _)| m),
            created_weekday: pecofence_platform::fileinfo::local_time_parts(entry.created)
                .map(|(_, d)| d),
            origin: match entry.origin {
                EntryOrigin::UserDesktop => Origin::UserDesktop,
                EntryOrigin::PublicDesktop => Origin::PublicDesktop,
                EntryOrigin::Namespace => Origin::Namespace,
            },
            is_hidden: false,
            is_system: false,
            idle_days: Self::idle_days(entry.mtime, last_opened, now),
        }
    }

    /// Reconciles the item table with the current desktop contents. New items are routed by the
    /// rule set; missing items are marked orphaned (membership kept) and garbage-collected
    /// after seven days.
    pub fn sync_desktop(&mut self, entries: &[DesktopEntry]) -> SyncReport {
        let mut report = SyncReport::default();
        let now = pecofence_core::now_unix();
        let mut seen: HashSet<ItemId> = HashSet::new();

        for entry in entries {
            let key = ItemKey::from_path(&entry.path.to_string_lossy());
            let (icon_key, _) =
                crate::icons::icon_key_for(&entry.path, entry.is_folder, entry.mtime);
            if let Some(&id) = self.catalog.get(&key) {
                seen.insert(id);
                if let Some(item) = self.config.items.get_mut(&id) {
                    let mut changed = false;
                    if item.orphaned_since.is_some() {
                        item.orphaned_since = None;
                        changed = true;
                    }
                    if item.mtime != entry.mtime || item.icon_key != icon_key {
                        item.mtime = entry.mtime;
                        item.icon_key = icon_key;
                        changed = true;
                    }
                    if item.size != entry.size {
                        item.size = entry.size;
                        changed = true;
                    }
                    if changed {
                        report.updated += 1;
                        self.mark_dirty();
                    }
                }
                // An item that exists but is in no fence (e.g. its fence was deleted) → route.
                if self.config.content_of_item(self.layout, id).is_none() {
                    let decision = self.route_decision(entry);
                    if let Some((fence, by)) = self.target_fence(decision) {
                        self.config.assign(self.layout, id, fence, by);
                        self.mark_dirty();
                    }
                }
                continue;
            }
            // New item.
            let display_name = shell::display_name(&entry.path).unwrap_or_else(|| {
                let stem = Path::new(&entry.file_name)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| entry.file_name.clone());
                if entry.is_folder {
                    entry.file_name.clone()
                } else {
                    stem
                }
            });
            let item = Item {
                id: uuid::Uuid::new_v4(),
                key: key.clone(),
                origin: match entry.origin {
                    EntryOrigin::UserDesktop => Origin::UserDesktop,
                    EntryOrigin::PublicDesktop => Origin::PublicDesktop,
                    EntryOrigin::Namespace => Origin::Namespace,
                },
                display_name,
                file_id: None,
                mtime: entry.mtime,
                is_folder: entry.is_folder,
                attrs: entry.attributes,
                icon_key,
                orphaned_since: None,
                size: entry.size,
                open_count: 0,
                last_opened: None,
            };
            let id = item.id;
            self.config.items.insert(id, item);
            self.catalog.insert(key, id);
            seen.insert(id);
            let decision = self.route_decision(entry);
            if let Some((fence, by)) = self.target_fence(decision) {
                self.config.assign(self.layout, id, fence, by);
            }
            report.added += 1;
            self.mark_dirty();
        }

        // Orphans.
        let mut to_remove = Vec::new();
        for (id, item) in self.config.items.iter_mut() {
            if seen.contains(id) {
                continue;
            }
            match item.orphaned_since {
                None => {
                    item.orphaned_since = Some(now);
                    report.removed += 1;
                    self.document.change();
                }
                Some(since) if now - since > ORPHAN_GC_SECS => to_remove.push(*id),
                Some(_) => {}
            }
        }
        for id in to_remove {
            if let Some(item) = self.config.items.remove(&id) {
                self.catalog.remove(&item.key);
            }
            for layout in &mut self.config.layouts {
                for content in &mut layout.contents {
                    if let ContentSpec::FileCollection { items, .. } = &mut content.content {
                        items.retain(|r| r.item_id != id);
                    }
                }
            }
            self.mark_dirty();
        }
        report
    }

    /// Live workspace entries, including the current contents of folder portals.
    /// The same file mirrored by several fences is counted once.
    pub fn workspace_item_count(&self) -> usize {
        self.config
            .items
            .values()
            .chain(self.portal_items.values())
            .filter(|item| item.orphaned_since.is_none())
            .map(|item| &item.key)
            .collect::<HashSet<_>>()
            .len()
    }

    /// Items of `fence` in display order (orphans hidden).
    pub fn items_of(&self, fence: &Fence) -> Vec<&Item> {
        let mut items: Vec<(Option<&ItemRef>, &Item)> = if fence.kind == FenceKind::FolderPortal {
            self.portal_members
                .get(&fence.id)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|i| self.portal_items.get(i).map(|it| (None, it)))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            fence
                .items
                .iter()
                .filter_map(|r| self.config.items.get(&r.item_id).map(|it| (Some(r), it)))
                .filter(|(_, it)| it.orphaned_since.is_none())
                .collect()
        };
        match fence.view.sort {
            SortMode::Manual => items.sort_by_key(|(r, it)| {
                (
                    r.and_then(|r| r.manual_index).unwrap_or(u32::MAX),
                    it.display_name.to_lowercase(),
                )
            }),
            SortMode::Name => {
                items.sort_by(|a, b| natural_cmp(&a.1.display_name, &b.1.display_name))
            }
            SortMode::Type => items.sort_by(|a, b| {
                let ka = (
                    !a.1.is_folder,
                    ext_of_key(&a.1.key),
                    a.1.display_name.to_lowercase(),
                );
                let kb = (
                    !b.1.is_folder,
                    ext_of_key(&b.1.key),
                    b.1.display_name.to_lowercase(),
                );
                ka.cmp(&kb)
            }),
            SortMode::Date => items.sort_by_key(|(_, it)| std::cmp::Reverse(it.mtime)),
            SortMode::Size => items.sort_by(|a, b| {
                // Folders first (Explorer), then largest first, ties by name.
                (!a.1.is_folder, std::cmp::Reverse(a.1.size))
                    .cmp(&(!b.1.is_folder, std::cmp::Reverse(b.1.size)))
                    .then_with(|| natural_cmp(&a.1.display_name, &b.1.display_name))
            }),
            SortMode::OpenCount => items.sort_by(|a, b| {
                std::cmp::Reverse(a.1.open_count)
                    .cmp(&std::cmp::Reverse(b.1.open_count))
                    .then_with(|| natural_cmp(&a.1.display_name, &b.1.display_name))
            }),
        }
        if fence.view.sort != SortMode::Manual {
            // Explorer keeps the Recycle Bin and friends ahead of files whatever the order.
            items.sort_by_key(|(_, it)| !it.is_namespace());
        }
        if fence.view.reverse {
            items.reverse();
        }
        items.into_iter().map(|(_, it)| it).collect()
    }

    /// Counts a launch (desktop items only; portal items are rebuilt from the folder).
    pub fn note_opened(&mut self, id: ItemId) {
        if let Some(it) = self.config.items.get_mut(&id) {
            it.open_count = it.open_count.saturating_add(1);
            it.last_opened = Some(pecofence_core::now_unix());
            self.mark_dirty();
        }
    }

    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.config
            .items
            .get(&id)
            .or_else(|| self.portal_items.get(&id))
    }

    /// True for items that live in a folder portal rather than on the desktop.
    pub fn is_portal_item(&self, id: ItemId) -> bool {
        self.portal_items.contains_key(&id)
    }

    /// The folder a portal fence currently shows (its root, or the subfolder navigated into).
    pub fn portal_path(&self, id: FenceId) -> Option<PathBuf> {
        let root = self.portal_root(id)?;
        Some(self.portal_cwd.get(&id).cloned().unwrap_or(root))
    }

    /// The folder a portal was created for.
    pub fn portal_root(&self, id: FenceId) -> Option<PathBuf> {
        let content = self.config.layouts.get(self.layout)?.content(id)?;
        match &content.content {
            ContentSpec::FolderPortal { root, .. } => Some(PathBuf::from(root)),
            _ => None,
        }
    }

    /// True when the portal shows a subfolder of its root (an "up" is possible).
    pub fn portal_navigated(&self, id: FenceId) -> bool {
        self.portal_cwd.contains_key(&id)
    }

    /// Navigates a portal into `dir` (must be inside its root). Returns false if refused.
    pub fn portal_enter(&mut self, id: FenceId, dir: &Path) -> bool {
        self.reconcile_portal_sources();
        let Some(root) = self.portal_root(id) else {
            return false;
        };
        // Lexical navigation policy, not a junction/symlink security sandbox.
        // Split explicitly: Path::components normalizes interior "." away.
        if dir
            .to_string_lossy()
            .split(['/', '\\'])
            .any(|part| part == "." || part == "..")
        {
            return false;
        }
        let root_key = ItemKey::from_path(&root.to_string_lossy());
        let dir_key = ItemKey::from_path(&dir.to_string_lossy());
        let (Some(rk), Some(dk)) = (root_key.as_path(), dir_key.as_path()) else {
            return false;
        };
        if dk == rk {
            self.portal_cwd.remove(&id);
        } else if dk.starts_with(&dir_prefix(rk)) {
            // Preserve casing from the committed read, not another UI-thread directory scan.
            let actual = self
                .portals
                .snapshot(id)
                .and_then(|s| {
                    s.entries
                        .iter()
                        .find(|entry| ItemKey::from_path(&entry.path.to_string_lossy()) == dir_key)
                })
                .map(|entry| entry.path.clone())
                .unwrap_or_else(|| dir.to_path_buf());
            self.portal_cwd.insert(id, actual);
        } else {
            return false;
        }
        self.request_portal_read(id);
        true
    }

    /// One folder up (never above the root). Returns false when already at the root.
    pub fn portal_up(&mut self, id: FenceId) -> bool {
        self.reconcile_portal_sources();
        let Some(cur) = self.portal_cwd.get(&id).cloned() else {
            return false;
        };
        match cur.parent() {
            Some(parent) => self.portal_enter(id, parent),
            None => {
                self.portal_cwd.remove(&id);
                self.request_portal_read(id);
                true
            }
        }
    }

    /// Back to the root folder.
    pub fn portal_home(&mut self, id: FenceId) -> bool {
        self.reconcile_portal_sources();
        if self.portal_cwd.remove(&id).is_some() {
            self.request_portal_read(id);
            true
        } else {
            false
        }
    }

    /// The portal (if any) whose current folder contains this runtime item.
    pub fn portal_of_item(&self, item: ItemId) -> Option<FenceId> {
        self.portal_members
            .iter()
            .find(|(_, ids)| ids.contains(&item))
            .map(|(f, _)| *f)
    }

    pub fn set_portal_navigate(&mut self, id: FenceId, on: bool) {
        if let Some(f) = self.content_mut(id)
            && let ContentSpec::FolderPortal { navigate, .. } = &mut f.content
            && *navigate != on
        {
            *navigate = on;
            self.mark_dirty();
        }
    }

    pub fn set_hide_title_icon(&mut self, id: FenceId, on: bool) {
        if let Some(f) = self.content_mut(id)
            && let ContentSpec::FolderPortal {
                hide_title_icon, ..
            } = &mut f.content
            && *hide_title_icon != on
        {
            *hide_title_icon = on;
            self.mark_dirty();
        }
    }

    /// Title shown for a fence: a navigated portal shows the current folder's name.
    pub fn display_title(&self, f: &Fence) -> String {
        let title = match self.portal_cwd.get(&f.id) {
            Some(cwd) => cwd
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| f.title.clone()),
            None => f.title.clone(),
        };
        match self.portals.snapshot(f.id).map(|s| &s.health) {
            Some(PortalHealth::Loading) => {
                format!("{title} · {}", pecofence_core::i18n::text("正在读取文件夹"))
            }
            Some(PortalHealth::Stale(_)) => format!(
                "{title} · {}",
                pecofence_core::i18n::text("文件夹不可用，内容可能已过期")
            ),
            _ => title,
        }
    }

    /// Creates a folder-portal fence (plan §12 "文件夹门户", pulled into the MVP on request): a
    /// fence whose items are the folder's entries. Files are never moved.
    pub fn new_portal_fence(&mut self, folder: &Path, rect: RECT) -> Option<FenceId> {
        let title = folder
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| folder.to_string_lossy().to_string());
        let id = self.new_fence(&title, rect)?;
        if let Some(f) = self.content_mut(id) {
            f.content = ContentSpec::FolderPortal {
                root: folder.to_string_lossy().to_string(),
                recursive: false,
                filter: None,
                navigate: true,
                hide_title_icon: false,
            };
            f.view.sort = SortMode::Name;
        }
        self.reconcile_portal_sources();
        Some(id)
    }

    /// Forgets only this portal's projection. Item ids include the fence identity.
    fn evict_portal_members(&mut self, id: FenceId) {
        let old_ids = self.portal_members.remove(&id).unwrap_or_default();
        for i in &old_ids {
            self.portal_items.remove(i);
        }
    }

    fn reset_portal_runtime(&mut self) {
        self.portals.reset();
        self.portal_sources.clear();
        self.portal_items.clear();
        self.portal_members.clear();
        self.portal_cwd.clear();
        self.portal_layout = self.layout;
    }

    /// Reconcile activations before issuing requests or accepting results. No filesystem IO.
    pub(crate) fn reconcile_portal_sources(&mut self) -> Vec<FenceId> {
        if self.portal_layout != self.layout {
            self.reset_portal_runtime();
        }
        let sources: HashMap<_, _> = self
            .fences()
            .iter()
            .filter(|f| self.portal_root(f.id).is_some())
            .map(|f| (f.id, f.source.clone()))
            .collect();
        let gone: Vec<_> = self
            .portal_sources
            .keys()
            .filter(|id| !sources.contains_key(id))
            .copied()
            .collect();
        for id in gone {
            self.portals.remove(id);
            self.evict_portal_members(id);
            self.portal_cwd.remove(&id);
        }
        let mut changed = Vec::new();
        for (id, source) in &sources {
            if self.portal_sources.get(id) != Some(source) {
                self.portals.remove(*id);
                self.evict_portal_members(*id);
                self.portal_cwd.remove(id);
                if let Some(path) = self.portal_path(*id) {
                    self.portals.request(*id, path);
                    changed.push(*id);
                }
            }
        }
        self.portal_sources = sources;
        changed
    }

    pub(crate) fn request_portal_read(&mut self, id: FenceId) {
        self.reconcile_portal_sources();
        if let Some(path) = self.portal_path(id) {
            if self
                .portals
                .snapshot(id)
                .is_some_and(|s| s.request.path != path)
            {
                self.evict_portal_members(id);
            }
            self.portals.request(id, path);
        }
    }

    pub(crate) fn next_portal_read(&mut self) -> Option<PortalRead> {
        self.reconcile_portal_sources();
        self.portals.next_read()
    }

    /// Atomic snapshot commit followed by legacy Item conversion at the native boundary.
    /// Neither runtime reads nor health changes mark durable configuration dirty.
    pub(crate) fn accept_portal_result(&mut self, result: PortalResult) -> Option<FenceId> {
        self.reconcile_portal_sources();
        let complete = matches!(result.outcome, PortalOutcome::Complete(_));
        let id = self.portals.complete(result)?;
        if complete {
            let items: Vec<_> = self
                .portals
                .snapshot(id)?
                .entries
                .iter()
                .map(|entry| {
                    let path_str = entry.path.to_string_lossy();
                    let (icon_key, _) =
                        crate::icons::icon_key_for(&entry.path, entry.is_folder, entry.mtime);
                    Item {
                        id: portal_item_id(id, &path_str),
                        key: ItemKey::from_path(&path_str),
                        origin: Origin::Namespace,
                        display_name: entry.display_name.clone(),
                        file_id: None,
                        mtime: entry.mtime,
                        is_folder: entry.is_folder,
                        attrs: entry.attributes,
                        icon_key,
                        orphaned_since: None,
                        size: entry.size,
                        open_count: 0,
                        last_opened: None,
                    }
                })
                .collect();
            self.evict_portal_members(id);
            self.portal_members
                .insert(id, items.iter().map(|it| it.id).collect());
            self.portal_items
                .extend(items.into_iter().map(|it| (it.id, it)));
        }
        Some(id)
    }

    pub fn item_by_path(&self, path: &Path) -> Option<ItemId> {
        self.catalog
            .get(&ItemKey::from_path(&path.to_string_lossy()))
            .copied()
    }

    /// Re-keys an item after a rename so it stays in its fence. Returns false if unknown.
    pub fn rename_item(&mut self, old: &Path, new: &Path) -> bool {
        let old_key = ItemKey::from_path(&old.to_string_lossy());
        let Some(id) = self.catalog.remove(&old_key) else {
            return false;
        };
        let new_key = ItemKey::from_path(&new.to_string_lossy());
        let Some(item) = self.config.items.get_mut(&id) else {
            return false;
        };
        let is_folder = std::fs::metadata(new)
            .map(|m| m.is_dir())
            .unwrap_or(item.is_folder);
        item.key = new_key.clone();
        item.is_folder = is_folder;
        item.display_name = shell::display_name(new).unwrap_or_else(|| {
            new.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        });
        let (icon_key, _) = crate::icons::icon_key_for(new, is_folder, item.mtime);
        item.icon_key = icon_key;
        item.orphaned_since = None;
        // A stale item already keyed by the new path (e.g. deleted then re-created under the old
        // name) would otherwise leave two items with one key; drop it first.
        if let Some(&other) = self.catalog.get(&new_key)
            && other != id
        {
            self.config.items.remove(&other);
            for layout in &mut self.config.layouts {
                for f in &mut layout.contents {
                    if let ContentSpec::FileCollection { items, .. } = &mut f.content {
                        items.retain(|r| r.item_id != other);
                    }
                }
            }
        }
        self.catalog.insert(new_key, id);
        self.mark_dirty();
        true
    }

    pub fn move_items(&mut self, items: &[ItemId], to: FenceId) -> usize {
        // Portal fences show a folder, not desktop membership: nothing can be moved into them,
        // and their (runtime) items cannot be moved out.
        if self.routable_fence(to).is_none() {
            return 0;
        }
        let mut n = 0;
        for &id in items {
            if !self.config.items.contains_key(&id) {
                continue;
            }
            if self
                .config
                .assign(self.layout, id, to, AssignedBy::User)
                .is_some()
            {
                n += 1;
            }
        }
        if n > 0 {
            self.mark_dirty();
        }
        n
    }

    /// Manual arrangement: places `items` (keeping their relative order) before display index
    /// `index` of `fence`. Only for 手动-sorted virtual fences (portals are sort-only; a sorted
    /// fence snaps back, like Explorer with auto-arrange on). Returns true when the order changed.
    pub fn reorder_items(&mut self, fence: FenceId, items: &[ItemId], index: usize) -> bool {
        if self.routable_fence(fence).is_none() {
            return false;
        }
        let Some(f) = self.fence(fence) else {
            return false;
        };
        if f.view.sort != SortMode::Manual {
            return false;
        }
        let reverse = f.view.reverse;
        let mut order: Vec<ItemId> = self.items_of(&f).iter().map(|it| it.id).collect();
        let before_order = order.clone();
        let moved: Vec<ItemId> = order
            .iter()
            .copied()
            .filter(|id| items.contains(id))
            .collect();
        if moved.is_empty() {
            return false;
        }
        let before = order[..index.min(order.len())]
            .iter()
            .filter(|id| items.contains(id))
            .count();
        order.retain(|id| !items.contains(id));
        let at = index.saturating_sub(before).min(order.len());
        order.splice(at..at, moved);
        if order == before_order {
            return false;
        }
        if reverse {
            // manual_index is stored in forward order; the view reverses it on display.
            order.reverse();
        }
        let Some(fm) = self.content_mut(fence) else {
            return false;
        };
        let ContentSpec::FileCollection { items, .. } = &mut fm.content else {
            return false;
        };
        for (i, id) in order.iter().enumerate() {
            if let Some(r) = items.iter_mut().find(|r| r.item_id == *id) {
                r.manual_index = Some(i as u32);
            }
        }
        self.mark_dirty();
        true
    }

    /// Re-runs the rules over every item (user assignments are respected).
    pub fn apply_rules_all(&mut self, entries: &[DesktopEntry]) -> usize {
        self.apply_rules_filtered(entries, false)
    }

    /// The hourly sweep for clock-dependent rules: only an item whose winning rule has an
    /// idle-days condition moves, so a later-added type rule still waits for "立即应用".
    pub fn apply_idle_rules(&mut self, entries: &[DesktopEntry]) -> usize {
        self.apply_rules_filtered(entries, true)
    }

    fn apply_rules_filtered(&mut self, entries: &[DesktopEntry], idle_only: bool) -> usize {
        let mut moved = 0;
        for entry in entries {
            if entry.origin == EntryOrigin::Namespace {
                continue;
            }
            let key = ItemKey::from_path(&entry.path.to_string_lossy());
            let Some(&id) = self.catalog.get(&key) else {
                continue;
            };
            let snapshots = self.fences();
            let current = snapshots
                .iter()
                .find(|f| f.items.iter().any(|r| r.item_id == id));
            if let Some(f) = current
                && f.items
                    .iter()
                    .any(|r| r.item_id == id && r.assigned_by == AssignedBy::User)
            {
                continue;
            }
            let decision = self.config.rules.evaluate(&self.facts_for(entry));
            if idle_only {
                let by_idle_rule = match decision {
                    Decision::Route { rule, .. } => self
                        .config
                        .rules
                        .list
                        .iter()
                        .any(|r| r.id == rule && Self::has_idle_cond(r)),
                    _ => false,
                };
                if !by_idle_rule {
                    continue;
                }
            }
            if let Some((fence, by)) = self.target_fence(decision)
                && self.config.assign(self.layout, id, fence, by).is_some()
            {
                moved += 1;
            }
        }
        if moved > 0 {
            self.mark_dirty();
        }
        moved
    }

    /// Re-files `ids` by the rules as if they had just arrived on the desktop (the user dragged
    /// them out of a fence onto the desktop: an explicit "let go", so an earlier manual
    /// placement no longer pins them). Unmatched items stay where they are (the inbox).
    /// Returns the fences whose membership changed.
    pub fn apply_rules_to(&mut self, ids: &[ItemId], entries: &[DesktopEntry]) -> Vec<FenceId> {
        let mut touched = Vec::new();
        for entry in entries {
            if entry.origin == EntryOrigin::Namespace {
                continue;
            }
            let key = ItemKey::from_path(&entry.path.to_string_lossy());
            let Some(&id) = self.catalog.get(&key) else {
                continue;
            };
            if !ids.contains(&id) {
                continue;
            }
            let from = self
                .fences()
                .iter()
                .find(|f| f.items.iter().any(|r| r.item_id == id))
                .map(|f| f.id);
            let decision = self.config.rules.evaluate(&self.facts_for(entry));
            if let Some((fence, by)) = self.target_fence(decision)
                && Some(fence) != from
                && self.config.assign(self.layout, id, fence, by).is_some()
            {
                touched.extend(from);
                touched.push(fence);
            }
        }
        if !touched.is_empty() {
            self.mark_dirty();
            touched.sort_unstable();
            touched.dedup();
        }
        touched
    }

    // ---- fences -----------------------------------------------------------------------------

    pub fn set_fence_bounds(
        &mut self,
        id: ContainerId,
        rect: RECT,
        rolled: bool,
        expanded_h_px: i32,
    ) {
        let px = PxRect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: if rolled {
                rect.top + expanded_h_px.max(1)
            } else {
                rect.bottom
            },
        };
        let (cx, cy) = px.center();
        let work = self
            .work_areas
            .iter()
            .find(|w| cx >= w.left && cx < w.right && cy >= w.top && cy < w.bottom)
            .or(self.work_areas.first())
            .cloned();
        let Some(work) = work else { return };
        let geo = geometry::normalize(px, &work);
        if let Some(f) = self.container_mut(id) {
            if f.geometry != geo || f.rolled_up != rolled {
                f.expanded_h = geo.h;
                f.geometry = geo;
                f.rolled_up = rolled;
                self.mark_dirty();
            }
        }
    }

    /// Physical rectangle for a fence on the current monitors (expanded height).
    pub fn fence_px_rect(&self, fence: &Fence) -> PxRect {
        let work = geometry::resolve_work_area(&fence.geometry, &self.work_areas)
            .cloned()
            .unwrap_or(WorkArea {
                device_path: String::new(),
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
                dpi: 96,
                mon_left: 0,
                mon_top: 0,
                mon_right: 1920,
                mon_bottom: 1040,
            });
        geometry::denormalize(&fence.geometry, &work)
    }

    #[allow(dead_code)]
    pub fn toggle_rolled(&mut self, id: ContainerId) -> Option<bool> {
        let rolled = {
            let f = self.container_mut(id)?;
            f.rolled_up = !f.rolled_up;
            f.rolled_up
        };
        self.mark_dirty();
        Some(rolled)
    }

    pub fn rename_fence(&mut self, id: FenceId, title: &str) {
        if let Some(f) = self.content_mut(id)
            && f.title != title
        {
            f.title = title.to_string();
            self.mark_dirty();
        }
    }

    pub fn set_icon_size(&mut self, id: FenceId, size: u32) {
        if let Some(f) = self.content_mut(id)
            && f.view.icon_size != size
        {
            f.view.icon_size = size;
            self.mark_dirty();
        }
    }

    pub fn set_auto_height(&mut self, id: ContainerId, on: bool) {
        if let Some(f) = self.container_mut(id)
            && f.auto_height != on
        {
            f.auto_height = on;
            self.mark_dirty();
        }
    }

    /// The catalogued item for a desktop path, if known.
    pub fn item_id_for_path(&self, path: &Path) -> Option<ItemId> {
        self.catalog
            .get(&ItemKey::from_path(&path.to_string_lossy()))
            .copied()
    }

    pub fn set_reverse(&mut self, id: FenceId, on: bool) {
        if let Some(f) = self.content_mut(id)
            && f.view.reverse != on
        {
            f.view.reverse = on;
            self.mark_dirty();
        }
    }

    pub fn set_locked(&mut self, id: ContainerId, on: bool) {
        if let Some(f) = self.container_mut(id)
            && f.locked != on
        {
            f.locked = on;
            self.mark_dirty();
        }
    }

    pub fn set_exclude_from_quick_hide(&mut self, id: ContainerId, on: bool) {
        if let Some(f) = self.container_mut(id)
            && f.exclude_from_quick_hide != on
        {
            f.exclude_from_quick_hide = on;
            self.mark_dirty();
        }
    }

    /// Per-fence appearance override (None = follow the global settings).
    pub fn set_appearance(
        &mut self,
        id: ContainerId,
        backdrop: Option<pecofence_core::Backdrop>,
        opacity: Option<f32>,
    ) {
        if let Some(f) = self.container_mut(id) {
            let mut next = f.appearance.clone().unwrap_or_default();
            next.backdrop = backdrop;
            next.opacity = opacity;
            let next = (next != pecofence_core::AppearanceOverride::default()).then_some(next);
            if f.appearance != next {
                f.appearance = next;
                self.mark_dirty();
            }
        }
    }

    /// Per-fence colour wash / title colour / title size (None fields = theme defaults).
    pub fn set_style(
        &mut self,
        id: ContainerId,
        tint_rgb: Option<[u8; 3]>,
        title_rgb: Option<[u8; 3]>,
        title_size: Option<pecofence_core::TitleSize>,
    ) {
        if let Some(f) = self.container_mut(id) {
            let mut next = f.appearance.clone().unwrap_or_default();
            next.tint_rgb = tint_rgb;
            next.title_rgb = title_rgb;
            next.title_size = title_size;
            let next = (next != pecofence_core::AppearanceOverride::default()).then_some(next);
            if f.appearance != next {
                f.appearance = next;
                self.mark_dirty();
            }
        }
    }

    pub fn set_spacing(&mut self, id: FenceId, spacing: pecofence_core::Spacing) {
        if let Some(f) = self.content_mut(id)
            && f.view.spacing != spacing
        {
            f.view.spacing = spacing;
            self.mark_dirty();
        }
    }

    pub fn set_columns_visible(&mut self, id: FenceId, visible: [bool; 3]) {
        let next = (visible != [true; 3]).then_some(visible);
        if let Some(f) = self.content_mut(id)
            && f.view.columns_visible != next
        {
            f.view.columns_visible = next;
            self.mark_dirty();
        }
    }

    pub fn set_column_widths(&mut self, id: FenceId, widths: [f32; 3]) {
        let next = (widths != crate::layout::DetailColumns::DEFAULT_WIDTHS).then_some(widths);
        if let Some(f) = self.content_mut(id)
            && f.view.column_widths != next
        {
            f.view.column_widths = next;
            self.mark_dirty();
        }
    }

    pub fn set_layout(&mut self, id: FenceId, layout: pecofence_core::ViewLayout) {
        if let Some(f) = self.content_mut(id)
            && f.view.layout != layout
        {
            f.view.layout = layout;
            self.mark_dirty();
        }
    }

    /// Any order but by date ends "按时间分组" (sections only make sense in date order).
    pub fn set_sort(&mut self, id: FenceId, sort: SortMode) {
        let Some(f) = self.content_mut(id) else {
            return;
        };
        let mut changed = false;
        if f.view.sort != sort {
            f.view.sort = sort;
            changed = true;
        }
        if sort != SortMode::Date && f.view.group_by_date {
            f.view.group_by_date = false;
            changed = true;
        }
        if changed {
            self.mark_dirty();
        }
    }

    /// "按时间分组"; turning it on also sorts by date.
    pub fn set_group_by_date(&mut self, id: FenceId, on: bool) {
        let Some(f) = self.content_mut(id) else {
            return;
        };
        let mut changed = false;
        if f.view.group_by_date != on {
            f.view.group_by_date = on;
            changed = true;
        }
        if on && f.view.sort != SortMode::Date {
            f.view.sort = SortMode::Date;
            changed = true;
        }
        if changed {
            self.mark_dirty();
        }
    }

    pub fn new_fence(&mut self, title: &str, rect: RECT) -> Option<FenceId> {
        if !self.save_allowed || self.config.layouts.get(self.layout).is_none() {
            return None;
        }
        let px = PxRect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        let (cx, cy) = px.center();
        let work = self
            .work_areas
            .iter()
            .find(|w| cx >= w.left && cx < w.right && cy >= w.top && cy < w.bottom)
            .or(self.work_areas.first())
            .cloned()?;
        let geo = geometry::normalize(px, &work);
        let mut fence = ContentInstance::collection(title, false);
        fence.view.icon_size = self.config.settings.icon_size;
        let id = fence.id;
        Workspace::new(&mut self.config.layouts[self.layout])
            .ok()?
            .create(fence, geo)
            .ok()?;
        self.mark_dirty();
        Some(id)
    }

    /// "快速添加": a fence for `template` at `rect` plus its rule, inserted ahead of the
    /// existing rules so it wins over the broader first-run presets (an installer is also a
    /// program). `Err(existing)` when the template was added before; `existing` is the fence
    /// its rule still points to, if any.
    pub fn add_template(
        &mut self,
        template: Template,
        rect: RECT,
    ) -> Result<FenceId, Option<FenceId>> {
        let key = template.key();
        if let Some(rule) = self
            .config
            .rules
            .list
            .iter()
            .find(|r| r.template.as_deref() == Some(key))
        {
            let existing = match rule.target {
                Target::Collection(id) => self.routable_fence(id),
                Target::Inbox => self.inbox_id(),
            };
            return Err(existing);
        }
        let id = self.new_fence(&template.title(), rect).ok_or(None)?;
        let rule = template.rule(id);
        // Idle-days rules must stay ahead of the type-only ones ("待清理" before "安装包"),
        // otherwise a plain type rule claims every installer first and the idle rule never fires.
        let at = if Self::has_idle_cond(&rule) {
            0
        } else {
            self.config
                .rules
                .list
                .iter()
                .take_while(|r| Self::has_idle_cond(r))
                .count()
        };
        self.config.rules.list.insert(at, rule);
        self.mark_dirty();
        Ok(id)
    }

    fn has_idle_cond(rule: &pecofence_core::rules::Rule) -> bool {
        rule.all_of
            .iter()
            .any(|c| matches!(c, Cond::IdleDays { .. }))
    }

    /// Deletes a fence; its items go back to the inbox. The inbox itself cannot be deleted.
    pub fn delete_fence(&mut self, id: FenceId) -> bool {
        let Some(inbox) = self.inbox_id() else {
            return false;
        };
        if inbox == id {
            return false;
        }
        let Some(removed) = self.fence(id) else {
            return false;
        };
        if Workspace::new(&mut self.config.layouts[self.layout])
            .and_then(|mut w| w.delete(id))
            .is_err()
        {
            return false;
        }
        self.portals.remove(id);
        self.portal_sources.remove(&id);
        self.portal_cwd.remove(&id);
        self.evict_portal_members(id);
        let ids: Vec<ItemId> = removed.items.iter().map(|r| r.item_id).collect();
        for item in ids {
            self.config
                .assign(self.layout, item, inbox, AssignedBy::Default);
        }
        self.config
            .rules
            .list
            .retain(|r| r.target != Target::Collection(id));
        if self.config.rules.default_target == Target::Collection(id) {
            self.config.rules.default_target = Target::Inbox;
        }
        self.mark_dirty();
        true
    }
}

fn ext_of_key(key: &ItemKey) -> String {
    key.as_path()
        .and_then(|p| p.rsplit_once('.').map(|(_, e)| e.to_string()))
        .unwrap_or_default()
}

/// Case-insensitive natural ordering (digits compared numerically), like Explorer.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let mut na = 0u64;
                while let Some(c) = ai.peek().copied().filter(|c| c.is_ascii_digit()) {
                    na = na.saturating_mul(10).saturating_add(c as u64 - '0' as u64);
                    ai.next();
                }
                let mut nb = 0u64;
                while let Some(c) = bi.peek().copied().filter(|c| c.is_ascii_digit()) {
                    nb = nb.saturating_mul(10).saturating_add(c as u64 - '0' as u64);
                    bi.next();
                }
                if na != nb {
                    return na.cmp(&nb);
                }
            }
            (Some(ca), Some(cb)) => {
                if ca != cb {
                    return ca.cmp(&cb);
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// `key` with exactly one trailing separator, for "is inside" prefix tests. Drive roots from
/// `ItemKey::from_path` already end in a backslash (`d:\`); deeper paths do not.
fn dir_prefix(key: &str) -> String {
    if key.ends_with('\\') {
        key.to_string()
    } else {
        format!("{key}\\")
    }
}

/// Stable per-portal item identity: shared paths never overwrite another source's snapshot.
pub(crate) fn portal_item_id(fence: FenceId, path: &str) -> ItemId {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let key = path.to_lowercase().replace('/', "\\");
    let mut h1 = DefaultHasher::new();
    (fence, &key).hash(&mut h1);
    let mut h2 = DefaultHasher::new();
    (&key, 0x5eedu16).hash(&mut h2);
    uuid::Uuid::from_u128(((h1.finish() as u128) << 64) | h2.finish() as u128)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecofence_core::geometry::WorkArea;

    fn test_state() -> AppState {
        let work = WorkArea {
            device_path: "test".into(),
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
            dpi: 96,
            mon_left: 0,
            mon_top: 0,
            mon_right: 1920,
            mon_bottom: 1040,
        };
        let dir = std::env::temp_dir().join(format!(
            "pecofence-state-test-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut state = AppState {
            config: Config::default(),
            store: ConfigStore::new(dir),
            layout: 0,
            document: DocumentClock::new(true),
            catalog: HashMap::new(),
            portal_items: HashMap::new(),
            portal_members: HashMap::new(),
            portal_cwd: HashMap::new(),
            portals: Default::default(),
            portal_sources: HashMap::new(),
            portal_layout: 0,
            work_areas: vec![work],
            first_run: true,
            recovered_from: None,
            load_issue: None,
            save_allowed: true,
            persistence_issue: None,
            pending_explicit_replacement: false,
        };
        state.ensure_layout();
        state
    }

    fn add_item(state: &mut AppState, fence: FenceId, name: &str) -> ItemId {
        let item = Item {
            id: uuid::Uuid::new_v4(),
            key: ItemKey::from_path(&format!("C:/Users/Me/Desktop/{name}.txt")),
            origin: Origin::UserDesktop,
            display_name: name.to_string(),
            file_id: None,
            mtime: 1,
            is_folder: false,
            attrs: 0,
            icon_key: IconKey::ByExt(".txt".into()),
            orphaned_since: None,
            size: 0,
            open_count: 0,
            last_opened: None,
        };
        let id = item.id;
        state.catalog.insert(item.key.clone(), id);
        state.config.items.insert(id, item);
        state.config.assign(0, id, fence, AssignedBy::User);
        id
    }

    fn names(state: &AppState, fence: FenceId) -> Vec<String> {
        let f = state.fence(fence).unwrap();
        state
            .items_of(&f)
            .iter()
            .map(|i| i.display_name.clone())
            .collect()
    }

    #[test]
    fn host_detach_through_app_state_keeps_content_and_can_restore() {
        let mut state = test_state();
        let (a, b, c) = (
            state.fences()[0].id,
            state.fences()[1].id,
            state.fences()[2].id,
        );
        add_item(&mut state, a, "first");
        add_item(&mut state, b, "second");
        add_item(&mut state, c, "third");
        let host = state.host_of(a).unwrap();
        state.attach_tab(b, host).unwrap();
        state.attach_tab(c, host).unwrap();
        let before = state.tabs_of(host);
        state.document.commit(state.document_stamp());
        let geometry = state.container(host).unwrap().geometry.clone();
        let (transition, change) = state
            .detach_tab_with_plan(a, geometry)
            .expect("the host must be detachable too");
        assert!(state.is_dirty());
        assert_eq!(state.host_of(a), Some(transition.created_containers[0]));
        assert_eq!(state.host_of(c), Some(host));
        assert_eq!(state.active_tab_of(host), Some(c));
        assert_eq!(names(&state, a), vec!["first"]);
        assert_eq!(names(&state, b), vec!["second"]);
        assert_eq!(names(&state, c), vec!["third"]);
        state.rename_fence(a, "edited during drag");
        state.document.commit(state.document_stamp());
        state.cancel_tab_detach(&change).unwrap();
        assert!(state.is_dirty());
        assert_eq!(state.tabs_of(host), before);
        assert_eq!(state.fence(a).unwrap().title, "edited during drag");
    }

    #[test]
    fn rejected_and_recovered_loads_do_not_create_or_autosave_defaults() {
        let baseline = test_state();
        let areas = baseline.work_areas.clone();
        for reason in [
            FreshReason::CorruptPrimary {
                reason: "bad graph".into(),
                quarantined: None,
            },
            FreshReason::UnreadablePrimary {
                reason: "denied".into(),
            },
            FreshReason::UnsupportedFormat {
                path: "workspace.v3.json".into(),
                schema: Some(3),
            },
        ] {
            let dir =
                std::env::temp_dir().join(format!("pecofence-gated-{}", uuid::Uuid::new_v4()));
            let mut state = AppState::from_load(
                areas.clone(),
                ConfigStore::new(&dir),
                LoadOutcome::Fresh(Config::default(), reason),
            );
            assert!(!state.first_run);
            assert!(!state.save_allowed);
            assert!(state.load_issue.is_some());
            state.ensure_layout();
            assert!(state.config.layouts.is_empty());
            state.mark_dirty();
            assert!(!state.save_if_dirty());
            assert!(!dir.exists());
            state.reset_workspace().unwrap();
            assert!(state.save_allowed);
            assert_eq!(state.fences().len(), 4);
            assert!(state.config.validate().is_ok());
        }
        let dir =
            std::env::temp_dir().join(format!("pecofence-recovered-{}", uuid::Uuid::new_v4()));
        let mut state = AppState::from_load(
            areas,
            ConfigStore::new(&dir),
            LoadOutcome::Recovered(baseline.config.clone(), "backup.json".into()),
        );
        assert!(!state.save_allowed);
        assert!(state.recovered_from.is_some());
        assert!(!state.is_dirty());
        state.mark_dirty();
        assert!(!state.save_if_dirty());
        assert!(!dir.exists());
        state.accept_recovery().unwrap();
        assert!(state.save_allowed);
        assert!(state.recovered_from.is_none());
    }

    #[test]
    fn rejected_import_preserves_document_and_gate() {
        let mut state = test_state();
        state.save_allowed = false;
        let before = state.config.clone();
        let mut invalid = before.clone();
        invalid.layouts[0].containers[0].active_tab = FenceId::new_v4();
        assert!(state.replace_config(invalid).is_err());
        assert_eq!(
            serde_json::to_value(&state.config).unwrap(),
            serde_json::to_value(&before).unwrap()
        );
        assert!(!state.save_allowed);
    }

    #[test]
    fn tab_errors_and_noops_do_not_dirty_and_stale_cancel_is_rejected() {
        let mut state = test_state();
        let a = state.fences()[0].id;
        let b = state.fences()[1].id;
        let c = state.fences()[2].id;
        let host = state.host_of(a).unwrap();
        state.document.commit(state.document_stamp());
        state.set_active_tab(host, a).unwrap();
        state.reorder_tab(host, a, 0).unwrap();
        assert!(!state.is_dirty());
        assert!(state.set_active_tab(host, b).is_err());
        assert!(!state.is_dirty());
        state.attach_tab(b, host).unwrap();
        state.attach_tab(c, host).unwrap();
        let geometry = state.container(host).unwrap().geometry.clone();
        let (_, plan) = state.detach_tab_with_plan(b, geometry).unwrap();
        state.reorder_tab(host, c, 0).unwrap();
        state.document.commit(state.document_stamp());
        assert!(state.cancel_tab_detach(&plan).is_err());
        assert!(!state.is_dirty());
        assert_ne!(state.host_of(b), Some(host));
    }

    #[test]
    fn late_container_events_after_delete_or_merge_are_not_panics_or_edits() {
        for deleted in [true, false] {
            let mut state = test_state();
            let victim = state.fences()[0].id;
            let retired = state.host_of(victim).unwrap();
            if deleted {
                assert!(state.delete_fence(victim));
                assert_eq!(state.host_of(victim), None);
            } else {
                let target = state.fences()[1].container_id;
                state.attach_tab(victim, target).unwrap();
                assert_eq!(state.host_of(victim), Some(target));
            }
            assert_eq!(state.active_tab_of(retired), None);
            state.document.commit(state.document_stamp());
            let before = serde_json::to_value(&state.config).unwrap();
            state.set_fence_bounds(retired, RECT::default(), false, 100);
            assert_eq!(state.toggle_rolled(retired), None);
            state.set_auto_height(retired, true);
            assert!(!state.is_dirty());
            assert_eq!(serde_json::to_value(&state.config).unwrap(), before);
        }
        let mut state = test_state();
        state.config.layouts.clear();
        assert_eq!(state.host_of(FenceId::new_v4()), None);
        assert_eq!(state.active_tab_of(ContainerId::new()), None);
    }

    #[test]
    fn portal_read_and_identity_survive_container_remount() {
        let mut state = test_state();
        let portal = state
            .new_portal_fence(Path::new(r"C:\Portal"), RECT::default())
            .unwrap();
        let request = state.next_portal_read().unwrap();
        let host = state.host_of(state.fences()[0].id).unwrap();
        state.attach_tab(portal, host).unwrap();
        state.set_active_tab(host, portal).unwrap();
        let geometry = state.container(host).unwrap().geometry.clone();
        let (_, plan) = state.detach_tab_with_plan(portal, geometry).unwrap();
        state.cancel_tab_detach(&plan).unwrap();
        state.document.commit(state.document_stamp());
        let path = PathBuf::from(r"C:\Portal\kept");
        assert_eq!(
            state.accept_portal_result(PortalResult {
                request,
                outcome: PortalOutcome::Complete(vec![portal_entry(path.clone())]),
            }),
            Some(portal)
        );
        let item = state.items_of(&state.fence(portal).unwrap())[0].id;
        assert_eq!(item, portal_item_id(portal, &path.to_string_lossy()));
        assert!(!state.is_dirty());
        assert!(state.next_portal_read().is_none());
    }

    #[test]
    fn wrong_role_targets_never_receive_desktop_memberships() {
        let mut state = test_state();
        let source = state.fences()[0].id;
        let item = add_item(&mut state, source, "desktop");
        let portal = state
            .new_portal_fence(Path::new(r"C:\Portal"), RECT::default())
            .unwrap();
        assert_eq!(state.routable_fence(portal), None);
        assert_eq!(state.move_items(&[item], portal), 0);
        assert!(!state.reorder_items(portal, &[item], 0));
        assert_eq!(state.config.content_of_item(0, item), Some(source));
        assert_eq!(
            state
                .target_fence(Decision::Default(Target::Collection(portal)))
                .unwrap()
                .0,
            state.inbox_id().unwrap()
        );
        state.content_mut(portal).unwrap().content = ContentSpec::Panel {
            panel: pecofence_core::PanelSpec {
                provider: "test".into(),
                instance_id: uuid::Uuid::new_v4(),
                config_version: 1,
                config: serde_json::Value::Null,
            },
        };
        assert_eq!(state.routable_fence(portal), None);
        assert_eq!(state.move_items(&[item], portal), 0);
    }

    #[test]
    fn workspace_count_includes_portals_deduplicates_paths_and_excludes_orphans() {
        let mut state = test_state();
        let fence = state.fences()[0].id;
        let desktop = add_item(&mut state, fence, "desktop");
        let orphan = add_item(&mut state, fence, "gone");
        state.config.items.get_mut(&orphan).unwrap().orphaned_since = Some(1);
        let mut mirrored = state.config.items[&desktop].clone();
        mirrored.id = uuid::Uuid::new_v4();
        state.portal_items.insert(mirrored.id, mirrored.clone());
        let mut portal_only = mirrored;
        portal_only.id = uuid::Uuid::new_v4();
        portal_only.key = ItemKey::from_path("C:/Projects/portal-only.txt");
        let portal_id = portal_only.id;
        state.portal_items.insert(portal_id, portal_only);
        assert_eq!(state.workspace_item_count(), 2);
        state.portal_items.remove(&portal_id);
        assert_eq!(state.workspace_item_count(), 1);
    }

    #[test]
    fn portal_navigation_preserves_folder_case_from_folded_item_keys() {
        let root = PathBuf::from(r"C:\Portal");
        let nested = root.join("Nested Target").join("日本語 Folder");
        let mut state = test_state();
        let fence = state.new_portal_fence(&root, RECT::default()).unwrap();
        let root_title = state.fence(fence).unwrap().title.clone();
        finish_portal(
            &mut state,
            PortalOutcome::Complete(vec![portal_entry(nested.clone())]),
        );
        let folded = PathBuf::from(nested.to_string_lossy().to_lowercase());
        assert!(state.portal_enter(fence, &folded));
        assert_eq!(state.portal_path(fence), Some(nested));
        assert!(
            state
                .display_title(&state.fence(fence).unwrap())
                .starts_with("日本語 Folder")
        );
        assert!(state.portal_up(fence));
        assert!(
            state
                .display_title(&state.fence(fence).unwrap())
                .starts_with("Nested Target")
        );
        assert!(state.portal_up(fence));
        assert!(
            state
                .display_title(&state.fence(fence).unwrap())
                .starts_with(&root_title)
        );
        assert!(!state.portal_up(fence));
    }

    fn portal_entry(path: PathBuf) -> pecofence_core::portal::PortalEntry {
        pecofence_core::portal::PortalEntry {
            display_name: path.file_name().unwrap().to_string_lossy().into_owned(),
            path,
            is_folder: true,
            attributes: 0,
            mtime: 1,
            size: 0,
        }
    }

    fn finish_portal(state: &mut AppState, outcome: PortalOutcome) -> Option<FenceId> {
        let request = state.next_portal_read().unwrap();
        state.accept_portal_result(PortalResult { request, outcome })
    }

    #[test]
    fn portal_snapshot_health_projection_and_dirty_boundary() {
        use pecofence_core::portal::ReadFailure;
        let mut state = test_state();
        let root = PathBuf::from(r"C:\Portal");
        let id = state.new_portal_fence(&root, RECT::default()).unwrap();
        state.document.commit(state.document_stamp());
        finish_portal(
            &mut state,
            PortalOutcome::Complete(vec![portal_entry(root.join("kept"))]),
        );
        assert_eq!(names(&state, id), ["kept"]);
        for outcome in [
            PortalOutcome::Unavailable(ReadFailure::Open("offline".into())),
            PortalOutcome::Partial {
                entries: vec![portal_entry(root.join("partial"))],
                failure: ReadFailure::Traversal("failed".into()),
            },
        ] {
            state.request_portal_read(id);
            finish_portal(&mut state, outcome);
            assert_eq!(names(&state, id), ["kept"]);
            assert!(matches!(
                state.portals.snapshot(id).unwrap().health,
                PortalHealth::Stale(_)
            ));
        }
        state.request_portal_read(id);
        finish_portal(&mut state, PortalOutcome::Complete(vec![]));
        assert!(names(&state, id).is_empty());
        assert!(!state.is_dirty());
        state.request_portal_read(id);
        finish_portal(
            &mut state,
            PortalOutcome::Complete(vec![portal_entry(root.join("old"))]),
        );
        let old = state.items_of(&state.fence(id).unwrap())[0].id;
        state.request_portal_read(id);
        let late = state.next_portal_read().unwrap();
        assert!(state.portal_enter(id, &root.join("missing")));
        assert!(names(&state, id).is_empty());
        assert!(state.item(old).is_none());
        assert_eq!(
            state.accept_portal_result(PortalResult {
                request: late,
                outcome: PortalOutcome::Complete(vec![portal_entry(root.join("late"))])
            }),
            None
        );
        finish_portal(
            &mut state,
            PortalOutcome::Unavailable(ReadFailure::Open("missing".into())),
        );
        assert_eq!(state.portal_path(id), Some(root.join("missing")));
        assert!(!state.is_dirty());
    }

    #[test]
    fn portal_lifecycle_reconfiguration_import_layout_and_reused_id() {
        for mode in 0..6 {
            let mut state = test_state();
            let id = state
                .new_portal_fence(Path::new(r"C:\Portal"), RECT::default())
                .unwrap();
            let request = state.next_portal_read().unwrap();
            match mode {
                0 => {
                    state.content_mut(id).unwrap().content = ContentSpec::FolderPortal {
                        root: r"C:\Other".into(),
                        recursive: false,
                        filter: None,
                        navigate: true,
                        hide_title_icon: false,
                    };
                }
                1 => state.replace_config(state.config.clone()).unwrap(),
                2 => state.apply_snapshot_layouts(state.config.layouts.clone()),
                3 => {
                    let content = state.config.layouts[0].content(id).unwrap().clone();
                    let geometry = state.fence(id).unwrap().geometry;
                    assert!(state.delete_fence(id));
                    Workspace::new(&mut state.config.layouts[0])
                        .unwrap()
                        .create(content, geometry)
                        .unwrap();
                }
                4 => {
                    // A source option change must invalidate even with the same id/path.
                    state.content_mut(id).unwrap().content = ContentSpec::FolderPortal {
                        root: r"C:\Portal".into(),
                        recursive: true,
                        filter: None,
                        navigate: true,
                        hide_title_icon: false,
                    };
                }
                _ => {
                    let mut other = state.config.layouts[0].clone();
                    other.fingerprint[0].device_path = "other-monitor".into();
                    state.config.layouts.push(other);
                    state.work_areas[0].device_path = "other-monitor".into();
                    state.ensure_layout();
                    // Switching back before the completion still cannot revive the old read.
                    state.work_areas[0].device_path =
                        state.config.layouts[0].fingerprint[0].device_path.clone();
                    state.ensure_layout();
                }
            }
            assert_eq!(
                state.accept_portal_result(PortalResult {
                    request,
                    outcome: PortalOutcome::Complete(vec![portal_entry(r"C:\Portal\late".into())])
                }),
                None,
                "mode {mode}"
            );
            assert!(names(&state, id).is_empty());
            finish_portal(
                &mut state,
                PortalOutcome::Complete(vec![portal_entry(r"C:\Portal\current".into())]),
            );
            assert_eq!(names(&state, id), ["current"]);
        }
    }

    #[test]
    fn portals_on_same_path_have_independent_actionable_items() {
        let mut state = test_state();
        let root = Path::new(r"C:\Portal");
        let a = state.new_portal_fence(root, RECT::default()).unwrap();
        finish_portal(
            &mut state,
            PortalOutcome::Complete(vec![portal_entry(root.join("kept"))]),
        );
        let b = state.new_portal_fence(root, RECT::default()).unwrap();
        finish_portal(
            &mut state,
            PortalOutcome::Complete(vec![portal_entry(root.join("kept"))]),
        );
        let a_item = state.items_of(&state.fence(a).unwrap())[0].id;
        let b_item = state.items_of(&state.fence(b).unwrap())[0].id;
        assert_ne!(a_item, b_item);
        assert_eq!(state.portal_of_item(b_item), Some(b));
        state.request_portal_read(a);
        finish_portal(&mut state, PortalOutcome::Complete(vec![]));
        assert!(state.item(a_item).is_none());
        assert!(state.item(b_item).is_some());
        assert_eq!(names(&state, b), ["kept"]);
        assert!(state.delete_fence(a));
        assert_eq!(names(&state, b), ["kept"]);
    }

    #[test]
    fn portal_navigation_rejects_dot_components_without_reading() {
        let mut state = test_state();
        let root = Path::new(r"C:\Portal");
        let id = state.new_portal_fence(root, RECT::default()).unwrap();
        finish_portal(
            &mut state,
            PortalOutcome::Complete(vec![portal_entry(root.join("kept"))]),
        );
        state.document.commit(state.document_stamp());
        for path in [
            r"C:\Portal\..\Outside",
            r"C:\Portal\.\Child",
            "C:/Portal/Child/../../Outside",
            r"C:\PortalElsewhere",
        ] {
            assert!(!state.portal_enter(id, Path::new(path)));
            assert_eq!(state.portal_path(id), Some(root.to_path_buf()));
            assert_eq!(names(&state, id), ["kept"]);
        }
        assert!(state.next_portal_read().is_none());
        assert!(!state.is_dirty());
    }

    #[test]
    fn reorder_items_places_before_display_index() {
        let mut state = test_state();
        let fence = state.fences()[0].id;
        assert_eq!(state.fence(fence).unwrap().view.sort, SortMode::Manual);
        let a = add_item(&mut state, fence, "a");
        let b = add_item(&mut state, fence, "b");
        let c = add_item(&mut state, fence, "c");
        assert_eq!(names(&state, fence), ["a", "b", "c"]);
        // c before a.
        assert!(state.reorder_items(fence, &[c], 0));
        assert_eq!(names(&state, fence), ["c", "a", "b"]);
        // a,b to the end: already there -> no change.
        assert!(!state.reorder_items(fence, &[a, b], 3));
        assert_eq!(names(&state, fence), ["c", "a", "b"]);
        // Dropping onto its own slot (index right after itself) is a no-op.
        assert!(!state.reorder_items(fence, &[c], 1));
        // b before c.
        assert!(state.reorder_items(fence, &[b], 0));
        assert_eq!(names(&state, fence), ["b", "c", "a"]);
        // Reversed view: display order is reversed, drop index is a display index.
        state.set_reverse(fence, true);
        assert_eq!(names(&state, fence), ["a", "c", "b"]);
        assert!(state.reorder_items(fence, &[b], 0));
        assert_eq!(names(&state, fence), ["b", "a", "c"]);
        // A sorted fence refuses.
        state.set_sort(fence, SortMode::Name);
        assert!(!state.reorder_items(fence, &[a], 0));
    }

    #[test]
    fn dir_prefix_tolerates_drive_roots() {
        assert_eq!(dir_prefix("d:\\"), "d:\\");
        assert_eq!(dir_prefix("d:\\users"), "d:\\users\\");
        assert!("d:\\users".starts_with(&dir_prefix("d:\\")));
        assert!(!"d:\\usersx".starts_with(&dir_prefix("d:\\users")));
    }
}

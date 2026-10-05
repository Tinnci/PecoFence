//! Folder-portal fences: decorations, per-portal folder watchers, creation, refresh, navigation.

use super::*;

impl App {
    /// Pushes the shown fence's portal decorations (folder glyph, up button, navigate flag) and
    /// its display title into the host window.
    pub(super) fn apply_portal_deco(&self, host: pecofence_core::ContainerId) {
        let Some(w) = self.fences.get(&host) else {
            return;
        };
        let Some(active) = self.state.active_tab_of(host) else {
            return;
        };
        let Some(f) = self.state.fence(active) else {
            return;
        };
        let is_portal = f.kind == FenceKind::FolderPortal;
        w.set_portal_deco(
            is_portal.then(|| {
                self.state
                    .portals
                    .snapshot(active)
                    .map(|s| s.health.clone())
                    .unwrap_or(pecofence_core::portal::PortalHealth::Loading)
            }),
            is_portal && !f.hide_title_icon,
            is_portal && self.state.portal_navigated(active),
            is_portal && f.portal_navigate,
        );
        w.set_title(&self.state.display_title(&f));
    }

    /// Starts/stops folder watchers so every portal fence follows its folder.
    pub(super) fn ensure_portal_watchers(&mut self) {
        if !self.state.save_allowed {
            self.portal_watchers.clear();
            return;
        }
        let portals: Vec<(FenceId, PathBuf)> = self
            .state
            .fences()
            .iter()
            .filter_map(|f| self.state.portal_path(f.id).map(|p| (f.id, p)))
            .collect();
        self.portal_watchers
            .retain(|id, (path, _)| portals.iter().any(|(pid, p)| pid == id && p == path));
        for (id, dir) in portals {
            if self.portal_watchers.contains_key(&id) {
                continue;
            }
            let pending = self.fs_pending.clone();
            let control_hwnd = self.control.hwnd().0 as isize;
            match DirWatcher::start(&dir, move |events| {
                if let Ok(mut p) = pending.lock() {
                    p.extend(events);
                }
                window::post_message(
                    HWND(control_hwnd as *mut core::ffi::c_void),
                    WM_APP_FS_CHANGED,
                    0,
                    0,
                );
            }) {
                Ok(w) => {
                    self.portal_watchers.insert(id, (dir, w));
                }
                Err(e) => {
                    tracing::warn!(dir = %dir.display(), error = %e, "portal watcher failed")
                }
            }
        }
    }

    /// Item menu "作为栅栏窗口显示": a new fence that mirrors `folder` (plan §12 文件夹门户).
    pub(super) fn create_portal(&mut self, folder: PathBuf, x: i32, y: i32, near: Option<FenceId>) {
        // The background read validates availability. A failed probe is not an empty folder.
        if let Some(existing) = self
            .state
            .fences()
            .iter()
            .find(|f| {
                self.state.portal_root(f.id).is_some_and(|p| {
                    p.to_string_lossy().to_lowercase() == folder.to_string_lossy().to_lowercase()
                })
            })
            .map(|f| f.id)
        {
            // Already shown: just bring attention to it (switch to its tab if hosted).
            let Some(host) = self.state.host_of(existing) else {
                return;
            };
            if self.state.active_tab_of(host) != Some(existing) {
                self.switch_tab(host, existing);
            }
            if let Some(w) = self.fences.get(&host) {
                w.show(false);
            }
            return;
        }
        let rect = self.place_new_fence(
            4,
            260.0,
            x,
            y,
            near.and_then(|content| self.state.host_of(content)),
        );
        if let Some(id) = self.state.new_portal_fence(&folder, rect) {
            tracing::info!(folder = %folder.display(), %id, "folder portal created");
            self.resync_windows();
            if let Some(w) = self.window_for(id) {
                w.show(true);
            }
            self.schedule_save();
        }
    }

    /// Re-reads only the portals currently showing one of `dirs` (a watcher batch names the
    /// folders it saw; a file operation names its source and destination folders). Folder
    /// comparison is case-insensitive, like NTFS names.
    pub(super) fn refresh_portals_in(&mut self, dirs: &[PathBuf]) {
        if !self.state.save_allowed {
            return;
        }
        let wanted: Vec<String> = dirs
            .iter()
            .map(|d| d.to_string_lossy().to_lowercase())
            .collect();
        let ids: Vec<FenceId> = self
            .state
            .fences()
            .iter()
            .filter_map(|f| self.state.portal_path(f.id).map(|p| (f.id, p)))
            .filter(|(_, p)| wanted.contains(&p.to_string_lossy().to_lowercase()))
            .map(|(id, _)| id)
            .collect();
        for id in ids {
            self.state.request_portal_read(id);
            self.refresh_fence(id);
        }
        self.pump_portal_reads();
        self.ensure_portal_watchers();
    }

    /// Requests every portal folder (overflow, layout/settings changes), never reads inline.
    pub(super) fn refresh_portals(&mut self) {
        if !self.state.save_allowed {
            return;
        }
        self.state.reconcile_portal_sources();
        let ids: Vec<_> = self
            .state
            .fences()
            .iter()
            .filter(|f| self.state.portal_path(f.id).is_some())
            .map(|f| f.id)
            .collect();
        for id in ids {
            self.state.request_portal_read(id);
            self.refresh_fence(id);
        }
        self.pump_portal_reads();
        self.ensure_portal_watchers();
    }

    /// Native host drives the pure coordinator and bounded adapter. Polling is deliberate:
    /// workers never hold a control HWND, including during shutdown or a modal message loop.
    pub(super) fn pump_portal_reads(&mut self) {
        if !self.state.save_allowed {
            window::kill_timer(self.control.hwnd(), TIMER_PORTALS);
            return;
        }
        let mut changed = self.state.reconcile_portal_sources();
        while let Some(result) = self.portal_reader.try_result() {
            if let Some(id) = self.state.accept_portal_result(result) {
                changed.push(id);
            }
        }
        while let Some(request) = self.state.next_portal_read() {
            match self.portal_reader.submit(request) {
                None => break,
                Some(failed) => {
                    if let Some(id) = self.state.accept_portal_result(failed) {
                        changed.push(id);
                    }
                }
            }
        }
        if self.state.portals.has_work() {
            window::set_timer(self.control.hwnd(), TIMER_PORTALS, 50);
        } else {
            window::kill_timer(self.control.hwnd(), TIMER_PORTALS);
        }
        if !changed.is_empty() {
            for id in changed {
                self.refresh_fence(id);
            }
            self.push_workspace_summary();
        }
    }

    pub(super) fn portal_enter(&mut self, fence: FenceId, dir: &std::path::Path) {
        if self.state.portal_enter(fence, dir) {
            self.after_portal_navigation(fence);
        }
    }

    pub(super) fn portal_up(&mut self, fence: FenceId) {
        if self.state.portal_up(fence) {
            self.after_portal_navigation(fence);
        }
    }

    pub(super) fn after_portal_navigation(&mut self, fence: FenceId) {
        self.pump_portal_reads();
        self.ensure_portal_watchers();
        self.refresh_fence(fence);
        let Some(host) = self.state.host_of(fence) else {
            return;
        };
        self.apply_portal_deco(host);
        self.push_workspace_summary();
    }
}

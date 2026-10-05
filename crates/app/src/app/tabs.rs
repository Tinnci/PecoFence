//! Container tab transitions and native HWND/capture reconciliation.

use super::*;

impl App {
    pub(super) fn tab_views(&self, host: ContainerId) -> Vec<TabView> {
        self.state
            .tabs_of(host)
            .into_iter()
            .filter_map(|id| self.state.fence(id))
            .map(|f| TabView {
                id: f.id,
                title: self.state.display_title(&f),
                color: f.appearance.as_ref().and_then(|a| a.tint_rgb),
                title_size: fence_style_for(&f).title_size,
                title_color: fence_style_for(&f).title_color,
            })
            .collect()
    }

    pub(super) fn reorder_tab(&mut self, host: ContainerId, tab: ContentId, to: usize) {
        if self.state.reorder_tab(host, tab, to).is_ok() {
            self.refresh_fence(tab);
            self.schedule_save();
        }
    }

    /// The source container remains alive, retaining its HWND and mouse capture.
    pub(super) fn detach_tab(&mut self, tab: ContentId, x: i32, y: i32, from_drag: bool) {
        let Some(snapshot) = self.state.fence(tab) else {
            return;
        };
        let source = snapshot.container_id;
        if from_drag
            && self
                .fences
                .get(&source)
                .is_some_and(|w| w.take_cancelled_detach())
        {
            return;
        }
        let tabs = self.state.tabs_of(source);
        if tabs.len() < 2 {
            return;
        }
        let Some(work) = self.work_area_at(x, y) else {
            return;
        };
        let scale = work.scale();
        let rect = if from_drag {
            let width = (snapshot.geometry.w * scale).round().max(1.0) as i32;
            let height = (snapshot.geometry.h * scale).round().max(1.0) as i32;
            let left = (x - width / 2).clamp(work.left, (work.right - width).max(work.left));
            let top = (y - (36.0 * scale) as i32 / 2)
                .clamp(work.top, (work.bottom - height).max(work.top));
            RECT {
                left,
                top,
                right: left + width,
                bottom: top + height,
            }
        } else {
            self.place_new_fence_dip(snapshot.geometry.w, snapshot.geometry.h, x, y, Some(source))
        };
        let geometry = pecofence_core::geometry::normalize(
            pecofence_core::geometry::PxRect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            },
            &work,
        );
        let Ok((_, change)) = self.state.detach_tab_with_plan(tab, geometry) else {
            return;
        };
        let detached = change.detached;
        self.resync_windows();
        self.apply_fence_view_with_snap(source, false);
        self.apply_fence_view_with_snap(detached, false);
        if let Some(w) = self.fences.get(&detached) {
            w.restore_geometry(rect, false, rect.bottom - rect.top);
            if let Some(height) = w.auto_height_px() {
                w.restore_geometry(w.rect(), false, height);
            }
            self.queue.push(Command::RaiseFence(w.hwnd()));
            if from_drag && window::key_down(msg::VK_LBUTTON) {
                let hwnd = w.hwnd();
                let current = w.rect();
                let pt = window::cursor_pos();
                let width = current.right - current.left;
                let height = current.bottom - current.top;
                let scale = monitors::dpi_for_window(hwnd).max(96) as f32 / 96.0;
                let left = pt.x - width / 2;
                let top = pt.y - (36.0 * scale) as i32 / 2;
                w.set_bounds(RECT {
                    left,
                    top,
                    right: left + width,
                    bottom: top + height,
                });
                if let Some(source_window) = self.fences.get(&source) {
                    source_window.begin_remote_drag(hwnd, change);
                }
            }
        }
        self.schedule_save();
    }

    pub(super) fn cancel_tab_detach(&mut self, change: pecofence_core::TabDetach) {
        for window in self.fences.values() {
            window.set_merge_hint(false, 0);
        }
        if self.state.cancel_tab_detach(&change).is_err() {
            return;
        }
        self.resync_windows();
        self.apply_fence_view_with_snap(change.source, false);
        if let Some(window) = self.fences.get(&change.source) {
            self.queue.push(Command::RaiseFence(window.hwnd()));
        }
        self.schedule_save();
    }

    pub(super) fn switch_tab(&mut self, host: ContainerId, tab: ContentId) {
        if self.state.set_active_tab(host, tab).is_err() {
            return;
        }
        self.refresh_fence(tab);
        // Switching content must never snap or persist container geometry.
        self.apply_fence_view_with_snap(host, false);
        self.schedule_save();
    }
}

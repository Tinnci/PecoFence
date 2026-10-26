//! Hosting spike: move a board fence into the WinUI workspace window as a child HWND.
//! Debug builds only; see `crate::workspace_spike`.

use super::App;
use crate::workspace_spike::{self, DETAIL_W, NAV_W, SpikeShared, TITLE_H};
use pecofence_core::ContainerId;
use pecofence_platform::{HWND, RECT, child_host, desktop, window};
use std::io::Write;
use std::rc::Rc;

pub struct SpikeState {
    shared: Rc<SpikeShared>,
    fence: ContainerId,
    child: HWND,
    attached: bool,
}

impl App {
    /// `workspace-spike TITLE`: open the workspace window and host that fence in it.
    pub(super) fn open_spike(&mut self, fence: ContainerId) {
        let Some(child) = self.fences.get(&fence).map(|w| w.hwnd()) else {
            tracing::warn!(target: "pecofence::test", "spike: no window for {fence}");
            return;
        };
        let dark = self.theme_mode == pecofence_render::ThemeMode::Dark;
        match workspace_spike::open(&self.reactor_context, self.queue.clone(), dark) {
            Ok(shared) => {
                self.spike = Some(SpikeState {
                    shared,
                    fence,
                    child,
                    attached: false,
                })
            }
            Err(error) => tracing::error!(target: "pecofence::test", %error, "spike: open failed"),
        }
    }

    pub(super) fn spike_layout(&mut self) {
        let Some(spike) = self.spike.as_mut() else {
            return;
        };
        let Some(host) = spike.shared.hwnd.get() else {
            return;
        };
        if !spike.attached {
            if let Some(a) = self.anchor.borrow_mut().as_mut() {
                a.unregister_fence(spike.child);
            }
            if let Some(shadow) = self.fences.get(&spike.fence).and_then(|w| w.shadow_hwnd()) {
                desktop::hide_window(shadow);
            }
            if let Err(error) = child_host::adopt(spike.child, host) {
                tracing::error!(target: "pecofence::test", %error, "spike: SetParent failed");
                return;
            }
            spike.attached = true;
        }
        let (w, h, dpi) = child_host::client(host);
        let s = dpi as f64 / 96.0;
        let rect = RECT {
            left: (NAV_W * s).round() as i32,
            top: (TITLE_H * s).round() as i32,
            right: (w as f64 - DETAIL_W * s).round() as i32,
            bottom: h,
        };
        let placed = child_host::place(spike.child, rect);
        tracing::info!(target: "pecofence::test", placed, ?rect, dpi, "spike: layout");
    }

    /// `spike-probe LABEL PATH`: append one JSON line describing the hosted child.
    pub(super) fn spike_probe(&self, label: &str, path: &str) {
        let Some(spike) = self.spike.as_ref() else {
            return;
        };
        let Some(host) = spike.shared.hwnd.get() else {
            return;
        };
        let p = child_host::probe(spike.child, host);
        let r = p.child_rect_in_parent;
        let (hw, hh, _) = child_host::client(host);
        let line = serde_json::json!({
            "label": label,
            "parent_is_host": p.parent_is_host,
            "z_index": p.z_index,
            "siblings": p.siblings,
            "child_rect": [r.left, r.top, r.right, r.bottom],
            "host_client": [hw, hh],
            "hit_at_center": p.hit_at_center,
            "focus": p.focus,
            "foreground_is_host": p.foreground_is_host,
            "child_dpi": p.child_dpi,
            "host_dpi": p.host_dpi,
            "typed": spike.shared.typed.borrow().clone(),
        });
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{line}");
        }
    }

    /// `spike-move X Y W H`: move and resize the workspace window (physical pixels).
    pub(super) fn spike_move(&self, x: i32, y: i32, w: i32, h: i32) {
        if let Some(host) = self.spike.as_ref().and_then(|s| s.shared.hwnd.get()) {
            let _ = window::set_window_bounds(host, x, y, w, h);
        }
    }

    pub(super) fn spike_dialog(&self, open: bool) {
        if let Some(spike) = self.spike.as_ref() {
            spike.shared.show_dialog(open);
        }
    }

    pub(super) fn spike_host(&self) -> Option<HWND> {
        self.spike.as_ref().and_then(|s| s.shared.hwnd.get())
    }
}

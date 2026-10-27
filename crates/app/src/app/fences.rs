//! Fence windows and geometry: create / sync / resync windows, bounds, column snap, auto height,
//! display changes, monitors, new-fence placement, roll, title rename, hwnd lookup.

use super::*;

pub(super) fn work_areas() -> Vec<WorkArea> {
    monitors::enumerate()
        .into_iter()
        .map(|m| WorkArea {
            device_path: m.device_name.clone(),
            left: m.work_area.left,
            top: m.work_area.top,
            right: m.work_area.right,
            bottom: m.work_area.bottom,
            dpi: m.dpi,
            mon_left: m.bounds.left,
            mon_top: m.bounds.top,
            mon_right: m.bounds.right,
            mon_bottom: m.bounds.bottom,
        })
        .collect()
}

impl App {
    /// Creates/destroys/updates fence windows to match the state.
    pub(super) fn sync_fence_windows(&mut self) {
        if !self.state.save_allowed {
            return;
        }
        // Activate new/reconfigured sources without reading filesystem or Shell on the UI.
        self.state.reconcile_portal_sources();
        self.pump_portal_reads();
        self.ensure_portal_watchers();
        let live_panels: Vec<_> = self
            .state
            .fences()
            .iter()
            .filter_map(|fence| match &fence.content {
                pecofence_core::FenceContentSpec::Panel { panel } => Some(panel.clone()),
                _ => None,
            })
            .collect();
        self.panel_manager.borrow_mut().retain_specs(&live_panels);
        // Only host fences own a window; fences hosted as tabs live inside their host's.
        // Every normalized container owns exactly one HWND, independently of its active tab.
        let hosts = self.state.host_fences();
        let ids: Vec<ContainerId> = hosts.iter().map(|f| f.container_id).collect();
        let gone: Vec<ContainerId> = self
            .fences
            .iter()
            .filter(|(id, w)| !ids.contains(id) || !window::is_window(w.hwnd()))
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            self.panel_mounts.retain(|_, host| *host != id);
            if let Some(w) = self.fences.remove(&id) {
                self.retire_window(w);
            }
        }
        for fence in hosts {
            let host = fence.container_id;
            let active = fence.id;
            let shown = &fence;
            let items = self.item_views(shown);
            let tabs = self.tab_views(host);
            if let Some(w) = self.fences.get(&host) {
                w.set_content(&shown.content);
                w.set_tabs(tabs, active);
                if let Some(view) = w.files() {
                    view.set_group_by_date(shown.view.group_by_date);
                };
                if let Some(view) = w.files() {
                    view.set_items(items);
                };
                // The window may have been kept across a layout switch (same ContainerId, other
                // per-fence view flags): push every persisted flag, not just items/title.
                self.apply_fence_view_with_snap(host, false);
                continue;
            }
            let px = self.state.fence_px_rect(&fence);
            let rect = RECT {
                left: px.left,
                top: px.top,
                right: px.right,
                bottom: px.bottom,
            };
            match FenceWindow::create(
                &self.ctx,
                crate::fence_window::FenceWindowSpec {
                    fence_id: host,
                    content_id: active,
                    title: &fence.title,
                    is_inbox: shown.kind == FenceKind::Inbox,
                    rolled_up: fence.rolled_up,
                    icon_size: shown.view.icon_size,
                    label_lines: shown.view.label_lines,
                    auto_height: fence.auto_height,
                    bounds: rect,
                    expanded_h_px: px.height(),
                    items,
                },
            ) {
                Ok(w) => {
                    w.set_content(&shown.content);
                    w.set_tabs(tabs, active);
                    self.fences.insert(host, w);
                    // Loading/synchronizing a saved fence must preserve its rectangle.
                    // User resizing and explicit icon/spacing changes still snap normally.
                    self.apply_fence_view_with_snap(host, false);
                }
                Err(e) => tracing::error!(title = %fence.title, error = %e, "fence window failed"),
            }
        }
        self.reconcile_panel_mounts();
    }

    /// Reconcile provider lifetimes against all contents, not only the visible tabs. Moving
    /// a panel to another HWND ends its physical mount, never its logical instance.
    fn reconcile_panel_mounts(&mut self) {
        let specs: Vec<_> = self
            .state
            .fences()
            .into_iter()
            .filter_map(|f| match f.content {
                pecofence_core::FenceContentSpec::Panel { panel } => Some(panel),
                _ => None,
            })
            .collect();
        let mut active: HashMap<_, _> = self
            .state
            .host_fences()
            .into_iter()
            .filter_map(|f| match f.content {
                pecofence_core::FenceContentSpec::Panel { panel }
                    if self.fences.contains_key(&f.container_id) =>
                {
                    Some((panel.instance_id.as_u128(), f.container_id))
                }
                _ => None,
            })
            .collect();
        let mut manager = self.panel_manager.borrow_mut();
        for (id, opened) in manager.reconcile_fences(&specs) {
            if let Err(error) = opened {
                tracing::warn!(%error, instance = %id, "panel reconciliation failed");
                active.remove(&id);
                continue;
            }
            if self.panel_mounts.get(&id) == active.get(&id) && self.panel_mounts.contains_key(&id)
            {
                continue;
            }
            let _ = manager.detach_mount(id);
            if !active.contains_key(&id) {
                let _ = manager.set_exposure(id, pecofence_plugin_api::Exposure::Hidden);
            }
            if let Some(container) = active.get(&id)
                && let Some(window) = self.fences.get(container)
            {
                let exposure = if self.ctx.behavior.floating.get() {
                    pecofence_plugin_api::Exposure::Peek
                } else {
                    pecofence_plugin_api::Exposure::Desktop
                };
                let _ = manager.set_exposure(id, exposure);
                let rect = window.rect();
                let dpi = monitors::dpi_for_window(window.hwnd()).max(96);
                let scale = dpi as f32 / 96.0;
                let viewport = pecofence_plugin_api::RectDip {
                    x: 0.0,
                    y: 0.0,
                    w: (rect.right - rect.left) as f32 / scale,
                    h: ((rect.bottom - rect.top) as f32 / scale - window.title_height_dip())
                        .max(0.0),
                };
                if let Err(error) = manager.attach_mount(
                    id,
                    viewport,
                    dpi,
                    pecofence_plugin_api::Presentation::Workspace,
                ) {
                    tracing::warn!(%error, instance = %id, "panel remount failed");
                    active.remove(&id);
                }
            }
        }
        self.panel_mounts = active;
    }

    /// The window a fence is shown in (its own, or its tab host's).
    pub(super) fn window_for(&self, fence: ContentId) -> Option<&FenceWindow> {
        let content = self.state.fence(fence)?;
        self.fences.get(&content.container_id)
    }

    /// After tabs/fences were added, removed, merged or split: create/destroy windows, put new
    /// ones into the desktop z-band and show them.
    pub(super) fn resync_windows(&mut self) {
        let before: Vec<HWND> = self.fences.values().map(|w| w.hwnd()).collect();
        self.sync_fence_windows();
        self.finish_window_sync(&before);
    }

    /// Geometry/content must be applied before a new or retained window becomes visible.
    fn finish_window_sync(&mut self, before: &[HWND]) {
        let entrances: Vec<HWND> = self
            .fences
            .values()
            .map(|w| w.hwnd())
            .filter(|hwnd| !before.contains(hwnd))
            .collect();
        if let Some(a) = self.anchor.borrow_mut().as_mut() {
            a.reanchor("windows resynced");
            a.reconcile_visibility(&entrances);
        }
        self.push_settings_state();
    }

    pub(super) fn refresh_fence(&mut self, id: ContentId) {
        let Some(content) = self.state.fence(id) else {
            return;
        };
        let host = content.container_id;
        let Some(active) = self.state.active_tab_of(host) else {
            return;
        };
        if let Some(w) = self.fences.get(&host) {
            w.set_tabs(self.tab_views(host), active);
            if active == id
                && let Some(f) = self.state.fence(id)
            {
                // Grouping first: the layout glide must target the new sections.
                if let Some(view) = w.files() {
                    view.set_group_by_date(f.view.group_by_date);
                };
                w.set_content(&f.content);
                if let Some(view) = w.files() {
                    view.set_items(self.item_views(&f));
                };
                if let Some(view) = w.files() {
                    view.set_sort_indicator(f.view.sort, f.view.reverse);
                };
            }
        }
        self.apply_portal_deco(host);
        self.apply_auto_height(host);
        self.reconcile_panel_mounts();
    }

    /// Pushes the persisted per-fence flags (lock, quick-hide exclusion, appearance override)
    /// into the window and the anchor.
    pub(super) fn apply_fence_appearance(&mut self, id: ContainerId) {
        let Some(f) = self.state.window_content(id) else {
            return;
        };
        let Some(w) = self.fences.get(&id) else {
            return;
        };
        w.set_locked(f.locked);
        w.set_maximized(
            self.state
                .container(id)
                .is_some_and(|c| c.restore.is_some()),
        );
        let (bd, op) = f
            .appearance
            .as_ref()
            .map(|a| (a.backdrop.map(backdrop_mode_for), a.opacity.unwrap_or(1.0)))
            .unwrap_or((None, 1.0));
        w.set_appearance(bd, op);
        w.set_style(fence_style_for(&f));
        if let Some(a) = self.anchor.borrow_mut().as_mut() {
            a.set_quick_hide_excluded(w.hwnd(), f.exclude_from_quick_hide);
        }
    }

    /// Pushes every persisted per-fence view flag (icon size, auto height, lock/appearance,
    /// rolled state) into an existing window and re-applies the derived geometry rules. This is
    /// the one place that makes a window agree with its `Fence`, whatever path changed the
    /// state (startup, layout switch, display change).
    pub(super) fn apply_fence_view(&mut self, id: ContainerId) {
        self.apply_fence_view_with_snap(id, true);
    }

    pub(super) fn apply_fence_view_with_snap(&mut self, id: ContainerId, snap_geometry: bool) {
        // Geometry-ish flags (rolled, auto height, lock, appearance) belong to the host window;
        // content flags (icon size, layout, sort) to the tab being shown.
        // A window showing a single widget is frameless and never rolled up.
        let frameless = self.is_frameless_widget(id);
        if frameless && self.state.container(id).is_some_and(|c| c.rolled_up) {
            if let Some(container) = self.state.container_mut(id) {
                container.rolled_up = false;
            }
            self.state.mark_dirty();
        }
        if let Some(w) = self.fences.get(&id) {
            w.set_frameless(frameless, self.ctx.theme.borrow().title_height);
        }
        let Some(f) = self.state.window_content(id) else {
            return;
        };
        let shown = &f;
        {
            let Some(w) = self.fences.get(&id) else {
                return;
            };
            w.set_content(&shown.content);
            if w.icon_size() != shown.view.icon_size {
                // Drops cached icons/labels, so only when it actually differs.
                if let Some(view) = w.files() {
                    view.set_icon_size(shown.view.icon_size);
                };
            }
            if let Some(view) = w.files() {
                view.set_layout(shown.view.layout);
            };
            if let Some(view) = w.files() {
                view.set_is_inbox(shown.kind == FenceKind::Inbox);
            };
            if let Some(view) = w.files() {
                view.set_spacing(shown.view.spacing);
            };
            if let Some(view) = w.files() {
                view.set_column_widths(
                    shown
                        .view
                        .column_widths
                        .unwrap_or(crate::layout::DetailColumns::DEFAULT_WIDTHS),
                );
            };
            if let Some(view) = w.files() {
                view.set_columns_visible(shown.view.columns_visible.unwrap_or([true; 3]));
            };
            if let Some(view) = w.files() {
                view.set_group_by_date(shown.view.group_by_date);
            };
            if let Some(view) = w.files() {
                view.set_sort_indicator(shown.view.sort, shown.view.reverse);
            };
            w.set_auto_height(f.auto_height && shown.content.is_files());
            if w.is_rolled() != f.rolled_up {
                w.set_rolled(f.rolled_up);
            }
        }
        self.apply_fence_appearance(id); // locked + backdrop/opacity + quick-hide exclusion
        self.apply_portal_deco(id);
        self.configure_widget_grid(id);
        if snap_geometry {
            self.apply_column_snap(id);
        }
        self.apply_auto_height(id);
    }

    pub(super) fn refresh_all(&mut self) {
        let ids: Vec<ContainerId> = self.fences.keys().copied().collect();
        for id in ids {
            let Some(active) = self.state.active_tab_of(id) else {
                continue;
            };
            if let Some(f) = self.state.fence(active)
                && let Some(w) = self.fences.get(&id)
            {
                w.set_tabs(self.tab_views(id), active);
                if let Some(view) = w.files() {
                    view.set_group_by_date(f.view.group_by_date);
                };
                w.set_content(&f.content);
                if let Some(view) = w.files() {
                    view.set_items(self.item_views(&f));
                };
            }
            self.apply_auto_height(id);
        }
        // Cut items that were moved away (Explorer pasted them) stop being dimmed.
        let before = self.cut_items.len();
        self.cut_items.retain(|id| self.state.item(*id).is_some());
        if self.cut_items.len() != before {
            self.push_cut_items();
        }
    }

    /// Auto-height (task 14): make the window exactly as tall as its grid needs.
    /// Column snapping (Fences): the width is a whole number of icon columns. Interactive
    /// resizing snaps in WM_SIZING; this applies the same rule to loaded fences, icon-size
    /// changes and DPI drift so a fence never sits at an unaligned width.
    pub(super) fn apply_column_snap(&mut self, id: ContainerId) {
        let Some(active) = self.state.active_tab_of(id) else {
            return;
        };
        if self
            .state
            .fence(active)
            .is_some_and(|f| !f.content.is_files())
        {
            // Widgets have their own grid: whole fence icon cells (docs/WIDGET_GRID.md).
            self.apply_widget_grid(id);
            return;
        }

        let Some(f) = self.state.window_content(id) else {
            return;
        };
        let shown = &f;
        let Some(w) = self.fences.get(&id) else {
            return;
        };
        let r = w.rect();
        let scale = monitors::dpi_for_window(w.hwnd()).max(96) as f32 / 96.0;
        let metrics =
            crate::layout::GridMetrics::for_icon_size(shown.view.icon_size, shown.view.label_lines)
                .with_line_h(self.ctx.chrome.label_line_h())
                .with_spacing(shown.view.spacing);
        let rows_layout = match shown.view.layout {
            ViewLayout::Icons => None,
            ViewLayout::List => Some(crate::layout::RowMetrics::list()),
            ViewLayout::Details => Some(crate::layout::RowMetrics::details()),
        };
        let cell = metrics.cell_w * scale;
        let pad = metrics.pad_x * 2.0 * scale;
        let width = (r.right - r.left) as f32;
        let cols = ((width - pad) / cell).round().max(1.0);
        // List / Details rows have no column rhythm: the width stays as the user left it.
        let mut snapped = if rows_layout.is_some() {
            r.right - r.left
        } else {
            (pad + cols * cell).round() as i32
        };
        // The snap itself must never push a fence past the work-area edge: shift it left, and
        // if it would then leave the left edge instead, round the column count down.
        let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
        let mut left = r.left;
        if let Some(wa) = self.work_area_at(cx, cy)
            && left + snapped > wa.right
        {
            left = (wa.right - snapped).max(wa.left);
            if left + snapped > wa.right && cols >= 2.0 {
                snapped = (pad + (cols - 1.0) * cell).round() as i32;
                left = wa.right - snapped;
            }
        }
        // Height: whole rows too (auto-height fences already get an exact row count; rolled
        // fences keep their title-only height).
        let rolled = w.is_rolled();
        let mut bottom = r.bottom;
        if !rolled && !f.auto_height {
            let title_h = (self.ctx.theme.borrow().title_height * scale).round();
            let (row, fixed) = match rows_layout {
                Some(rm) => (
                    rm.row_h * scale,
                    title_h + (rm.header_h + rm.pad_y * 2.0) * scale + 2.0,
                ),
                None => (
                    metrics.cell_h * scale,
                    title_h + metrics.pad_y * 2.0 * scale + 2.0,
                ),
            };
            let h = (r.bottom - r.top) as f32;
            let rows = ((h - fixed) / row).round().max(1.0);
            bottom = r.top + (fixed + rows * row).round() as i32;
        }
        if snapped == r.right - r.left && left == r.left && bottom == r.bottom {
            return;
        }
        let rect = RECT {
            left,
            top: r.top,
            right: left + snapped,
            bottom,
        };
        w.set_bounds(rect);
        let expanded = if rolled {
            w.expanded_height_px()
        } else {
            let h = rect.bottom - rect.top;
            w.apply_height(h, false);
            h
        };
        self.state.set_fence_bounds(id, rect, rolled, expanded);
        self.schedule_save();
    }

    /// A window is frameless when it shows exactly one first-party widget; tabbed
    /// groups keep their title bar so the tab strip stays reachable.
    pub(super) fn is_frameless_widget(&self, id: ContainerId) -> bool {
        self.state.container(id).is_some_and(|c| c.tabs.len() == 1)
            && self.widget_grid(id).is_some()
    }

    /// The grid a widget window snaps to: its provider's span rules and one fence icon
    /// cell at the widget's icon size and the live label line height (text scaling).
    pub(super) fn widget_grid(&self, id: ContainerId) -> Option<crate::fence_window::WidgetGrid> {
        if self.state.container(id)?.tabs.len() != 1 {
            return None;
        }
        let shown = self.state.window_content(id)?;
        let pecofence_core::FenceContentSpec::Panel { panel } = &shown.content else {
            return None;
        };
        let rules = pecofence_plugin_widgets::grid::span_rules(&panel.provider)?;
        let metrics =
            crate::layout::GridMetrics::for_icon_size(shown.view.icon_size, shown.view.label_lines)
                .with_line_h(self.ctx.chrome.label_line_h());
        Some(crate::fence_window::WidgetGrid {
            rules,
            cell: pecofence_plugin_widgets::grid::Cell {
                w: metrics.cell_w,
                h: metrics.cell_h,
            },
        })
    }

    /// Loading and tab switching preserve geometry, but must still update the live
    /// resize rules and presentation (and clear them when a file tab becomes active).
    fn grouped_grid_widget(&self, id: ContainerId) -> bool {
        self.state.container(id).is_some_and(|c| c.tabs.len() > 1)
            && self
                .state
                .window_content(id)
                .is_some_and(|shown| match &shown.content {
                    pecofence_core::FenceContentSpec::Panel { panel } => {
                        pecofence_plugin_widgets::grid::span_rules(&panel.provider).is_some()
                    }
                    _ => false,
                })
    }

    fn configure_widget_grid(&self, id: ContainerId) {
        let grid = self.widget_grid(id);
        let Some(w) = self.fences.get(&id) else {
            return;
        };
        w.set_widget_grid(grid);
        if grid.is_none() && self.grouped_grid_widget(id) {
            if let Some(panel) = w.panel() {
                panel.set_presentation(pecofence_plugin_api::Presentation::Compact);
            }
        }
        if let Some(grid) = grid {
            let scale = monitors::dpi_for_window(w.hwnd()).max(96) as f32 / 96.0;
            let rect = w.rect();
            let height = if w.is_rolled() {
                w.expanded_height_px()
            } else {
                rect.bottom - rect.top
            };
            let span = grid.rules.span_for_size(
                (rect.right - rect.left) as f32 / scale,
                height as f32 / scale,
                grid.cell,
                self.ctx.behavior.text_scale.get(),
            );
            if let Some(panel) = w.panel() {
                panel.set_presentation(span.presentation());
            }
        }
    }

    /// Size a widget window from its stored span (or, for windows saved before spans
    /// existed, the nearest span to its current size), and keep the span, the live
    /// resize grid and the capsule/full presentation in step. Runs on load, DPI and
    /// icon-size changes, text scaling and after every interactive resize.
    pub(super) fn apply_widget_grid(&mut self, id: ContainerId) {
        let grid = self.widget_grid(id);
        let Some(w) = self.fences.get(&id) else {
            return;
        };
        w.set_widget_grid(grid);
        let Some(grid) = grid else {
            if self.grouped_grid_widget(id) {
                if let Some(panel) = w.panel() {
                    panel.set_presentation(pecofence_plugin_api::Presentation::Compact);
                }
            }
            return;
        };
        let text_scale = self.ctx.behavior.text_scale.get();
        let scale = monitors::dpi_for_window(w.hwnd()).max(96) as f32 / 96.0;
        let r = w.rect();
        let rolled = w.is_rolled();
        let current_h = if rolled {
            w.expanded_height_px()
        } else {
            r.bottom - r.top
        };
        let stored = self.state.container(id).and_then(|c| c.span);
        let span = match stored {
            Some(span) => grid.rules.clamp(
                pecofence_plugin_widgets::grid::Span::new(span.columns, span.rows),
                grid.cell,
                text_scale,
            ),
            None => grid.rules.span_for_size(
                (r.right - r.left) as f32 / scale,
                current_h as f32 / scale,
                grid.cell,
                text_scale,
            ),
        };
        if let Some(panel) = w.panel() {
            panel.set_presentation(span.presentation());
        }
        let saved = pecofence_core::WidgetSpan {
            columns: span.columns,
            rows: span.rows,
        };
        if stored != Some(saved)
            && let Some(container) = self.state.container_mut(id)
        {
            container.span = Some(saved);
            self.state.mark_dirty();
            self.schedule_save();
        }
        let (wd, hd) = span.size(grid.cell);
        let (width, height) = ((wd * scale).round() as i32, (hd * scale).round() as i32);
        // Keep the widget on its work area: grow toward the top-left when it would cross
        // the right or bottom edge.
        let mut left = r.left;
        let mut top = r.top;
        if let Some(wa) = self.work_area_at((r.left + r.right) / 2, (r.top + r.bottom) / 2) {
            left = left.min(wa.right - width).max(wa.left);
            top = top.min(wa.bottom - height).max(wa.top);
        }
        let shown_h = if rolled { r.bottom - r.top } else { height };
        if left == r.left && top == r.top && r.right - r.left == width && current_h == height {
            return;
        }
        let rect = RECT {
            left,
            top,
            right: left + width,
            bottom: top + shown_h,
        };
        w.set_bounds(rect);
        if !rolled {
            w.apply_height(height, false);
        }
        self.state.set_fence_bounds(id, rect, rolled, height);
        self.schedule_save();
    }

    pub(super) fn apply_auto_height(&mut self, id: ContainerId) {
        let Some(active) = self.state.active_tab_of(id) else {
            return;
        };
        if self
            .state
            .fence(active)
            .is_some_and(|f| !f.content.is_files())
        {
            return;
        }

        let Some(w) = self.fences.get(&id) else {
            return;
        };
        let Some(h) = w.auto_height_px() else { return };
        let r = w.rect();
        if r.bottom - r.top == h {
            return;
        }
        // Auto-height is a projection of current contents, not a user geometry edit. Keep
        // the document's manually chosen bounds; observations must not schedule persistence.
        // Animation completion likewise publishes geometry only for explicit roll changes.
        w.apply_height(h, true);
    }

    pub(super) fn on_display_changed(&mut self) {
        self.end_peek_now();
        self.state.work_areas = work_areas();
        if !self.state.save_allowed {
            return;
        }
        self.state.ensure_layout();
        self.sync_layout_windows();
    }

    /// One ordered layout transition for monitor changes and restored/imported layouts.
    fn sync_layout_windows(&mut self) {
        let before: Vec<HWND> = self.fences.values().map(|w| w.hwnd()).collect();
        self.sync_fence_windows();
        for fence in self.state.host_fences() {
            if let Some(w) = self.fences.get(&fence.container_id) {
                let px = self.state.fence_px_rect(&fence);
                // State, not the window: sync_fence_windows has just made the window agree, and
                // a fence rolled in the old layout but expanded in the new one needs px.height().
                let h = if fence.rolled_up {
                    window::window_rect_size(w.hwnd()).1
                } else {
                    px.height()
                };
                w.set_bounds(RECT {
                    left: px.left,
                    top: px.top,
                    right: px.right,
                    bottom: px.top + h,
                });
            }
        }
        // Wallpaper regions moved with the monitors: rebuild the backdrops unconditionally.
        self.refresh_visuals(true);
        self.finish_window_sync(&before);
    }

    pub fn toggle_all_fences(&mut self) {
        self.end_peek_now();
        if let Some(a) = self.anchor.borrow_mut().as_mut() {
            a.toggle_fences_hidden();
        }
    }

    /// Human label for a monitor: "显示器 1 (2560×1440)".
    pub(super) fn monitor_labels(&self) -> Vec<(String, String)> {
        self.state
            .work_areas
            .iter()
            .enumerate()
            .map(|(i, w)| {
                (
                    w.device_path.clone(),
                    pecofence_core::i18n::format(
                        "显示器 {0} ({1}×{2})",
                        &[
                            format!("{}", i + 1),
                            format!("{}", w.mon_right - w.mon_left),
                            format!("{}", w.mon_bottom - w.mon_top),
                        ],
                    ),
                )
            })
            .collect()
    }

    pub(super) fn swap_monitors(&mut self, a: &str, b: &str) {
        let n = self.state.swap_monitors(a, b);
        if n == 0 {
            self.settings_toast(pecofence_core::i18n::text("这两个显示器上没有栅栏"));
            return;
        }
        self.end_peek_now();
        self.relayout_from_state();
        self.schedule_save();
        self.settings_toast(&pecofence_core::i18n::format(
            "已交换 {0} 个栅栏的显示器",
            &[n.to_string()],
        ));
    }

    /// Re-applies every fence's saved geometry to its window (after a swap / restore / import).
    pub(super) fn relayout_from_state(&mut self) {
        if !self.state.save_allowed {
            return;
        }
        self.state.ensure_layout();
        self.sync_layout_windows();
        self.refresh_all();
        self.push_settings_state();
    }

    pub(super) fn fence_of_hwnd(&self, hwnd: HWND) -> Option<ContainerId> {
        self.fences
            .iter()
            .find(|(_, w)| w.hwnd() == hwnd)
            .map(|(id, _)| *id)
    }

    /// Menu / Ctrl+wheel: one place that changes a fence's icon size and everything derived
    /// from it (window redraw, column snap, auto height, save).
    pub(super) fn apply_icon_size(&mut self, fence: ContentId, size: u32) {
        self.state.set_icon_size(fence, size);
        if let Some(w) = self.window_for(fence)
            && w.active_fence() == fence
        {
            if let Some(view) = w.files() {
                view.set_icon_size(size);
            };
        }
        let Some(host) = self.state.host_of(fence) else {
            return;
        };
        self.apply_column_snap(host);
        self.apply_auto_height(host);
        self.schedule_save();
    }

    /// Ctrl+wheel: 32 → 48 → 64 → 96, stopping at the ends. 32 / 48 / 96 are the desktop's
    /// small / medium / large; 64 stays as an intermediate step for existing configs.
    pub(super) fn step_icon_size(&mut self, fence: ContentId, larger: bool) {
        let Some(cur) = self.state.fence(fence).map(|f| f.view.icon_size) else {
            return;
        };
        const SIZES: [u32; 4] = [32, 48, 64, 96];
        let next = if larger {
            SIZES.iter().copied().find(|s| *s > cur)
        } else {
            SIZES.iter().rev().copied().find(|s| *s < cur)
        };
        if let Some(next) = next {
            self.apply_icon_size(fence, next);
        }
    }

    pub(super) fn begin_rename(&mut self, fence: ContentId) {
        // Inline rename lands with the FocusSession task; until then use the tray notification
        // to explain. Kept as a queue command so the UI is already wired.
        if let Some(w) = self.window_for(fence) {
            w.set_title_renaming(true);
            crate::rename::begin_inline_rename(
                w.hwnd(),
                fence,
                self.state
                    .fence(fence)
                    .map(|f| f.title.clone())
                    .unwrap_or_default(),
                self.queue.clone(),
                self.theme_mode,
            );
        }
    }

    /// "快速添加" from the settings page: a template's fence beside the inbox plus its rule,
    /// applied to the desktop right away. A template added before just gets shown.
    pub(super) fn add_template(&mut self, template: pecofence_core::rules::Template) {
        // The inbox may be a tab: its window is the host's.
        let inbox = self.state.inbox_id().and_then(|id| self.state.host_of(id));
        let centre = |r: RECT| ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
        let (x, y) = inbox
            .and_then(|id| self.fences.get(&id))
            .map(|w| centre(w.rect()))
            .or_else(|| {
                self.state
                    .work_areas
                    .first()
                    .map(|w| ((w.left + w.right) / 2, (w.top + w.bottom) / 2))
            })
            .unwrap_or((0, 0));
        let rect = self.place_new_fence(3, 200.0, x, y, inbox);
        match self.state.add_template(template, rect) {
            Ok(_) => {
                self.resync_windows();
                let entries = shell::enumerate_desktop();
                let moved = self.state.apply_rules_all(&entries);
                self.refresh_all();
                self.schedule_save();
                self.push_settings_state();
                self.settings_toast(&pecofence_core::i18n::format(
                    "已添加“{0}”，整理了 {1} 个项目",
                    &[template.title(), moved.to_string()],
                ));
            }
            Err(existing) => {
                self.settings_toast(&pecofence_core::i18n::format(
                    "“{0}”已存在",
                    &[template.title()],
                ));
                if let Some(id) = existing {
                    self.post_show_fence(id);
                }
            }
        }
    }

    pub(super) fn create_fence_at(&mut self, rect: RECT) {
        if self
            .state
            .new_fence(pecofence_core::i18n::text("新栅栏"), rect)
            .is_some()
        {
            self.resync_windows();
            self.schedule_save();
        }
    }

    /// Deletes a content instance. Only deleting a container's last tab removes its window.
    /// Caption Close: the window goes away, the fence and its content stay. The tray's
    /// "Closed fences" menu reopens it where it was.
    pub(super) fn close_fence(&mut self, host: ContainerId) {
        if self.state.set_closed(host, true) {
            self.resync_windows();
            self.schedule_save();
        }
    }

    pub(super) fn reopen_fence(&mut self, host: ContainerId) {
        if self.state.set_closed(host, false) {
            self.resync_windows();
            self.schedule_save();
        }
    }

    /// Caption Maximize / Restore: fill the work area of the fence's monitor, or go back
    /// to the bounds it had before.
    pub(super) fn toggle_maximize(&mut self, host: ContainerId) {
        let Some(w) = self.fences.get(&host) else {
            return;
        };
        if w.is_rolled() {
            self.toggle_roll(host);
        }
        let Some(w) = self.fences.get(&host) else {
            return;
        };
        let restore = self.state.container(host).and_then(|c| c.restore.clone());
        let rect = match restore {
            Some(geo) => {
                let Some(work) =
                    pecofence_core::geometry::resolve_work_area(&geo, &self.state.work_areas)
                        .cloned()
                else {
                    return;
                };
                let px = pecofence_core::geometry::denormalize(&geo, &work);
                if let Some(c) = self.state.container_mut(host) {
                    c.restore = None;
                }
                RECT {
                    left: px.left,
                    top: px.top,
                    right: px.right,
                    bottom: px.bottom,
                }
            }
            None => {
                let r = w.rect();
                let Some(wa) = self.work_area_at((r.left + r.right) / 2, (r.top + r.bottom) / 2)
                else {
                    return;
                };
                if let Some(c) = self.state.container_mut(host) {
                    c.restore = Some(c.geometry.clone());
                }
                RECT {
                    left: wa.left,
                    top: wa.top,
                    right: wa.right,
                    bottom: wa.bottom,
                }
            }
        };
        self.state.mark_dirty();
        let maximized = self
            .state
            .container(host)
            .is_some_and(|c| c.restore.is_some());
        w.set_maximized(maximized);
        w.set_bounds(rect);
        self.schedule_save();
    }

    pub(super) fn delete_fence(&mut self, fence: ContentId) {
        if self.state.delete_fence(fence) {
            self.resync_windows();
            self.refresh_all();
            self.schedule_save();
        }
    }

    /// Fences "dock to the top of the screen": the fence moves flush against the top of its
    /// work area, rolls up and expands on hover — a pull-down shelf.
    pub(super) fn dock_to_top(&mut self, fence: ContainerId) {
        let Some(w) = self.fences.get(&fence) else {
            return;
        };
        let r = w.rect();
        let Some(wa) = self.work_area_at((r.left + r.right) / 2, (r.top + r.bottom) / 2) else {
            return;
        };
        let h = r.bottom - r.top;
        let rect = RECT {
            left: r
                .left
                .clamp(wa.left, (wa.right - (r.right - r.left)).max(wa.left)),
            top: wa.top,
            right: r
                .left
                .clamp(wa.left, (wa.right - (r.right - r.left)).max(wa.left))
                + (r.right - r.left),
            bottom: wa.top + h,
        };
        w.set_bounds(rect);
        let expanded = if w.is_rolled() {
            w.expanded_height_px()
        } else {
            h
        };
        self.state
            .set_fence_bounds(fence, rect, w.is_rolled(), expanded);
        if !w.is_rolled() {
            self.toggle_roll(fence);
        }
        self.schedule_save();
    }

    pub(super) fn new_fence_near(&mut self, x: i32, y: i32) {
        let rect = self.place_new_fence(3, 200.0, x, y, None);
        self.create_fence_at(rect);
    }

    pub(super) fn new_spm_panel_near(
        &mut self,
        kind: SpmPanel,
        x: i32,
        y: i32,
        near: Option<ContainerId>,
    ) {
        let (title, provider, (version, config), w, h) = match kind {
            SpmPanel::CiCd => (
                "CI/CD",
                pecofence_plugin_spm::ci_watch::PROVIDER_ID,
                pecofence_plugin_spm::ci_watch::live_config(),
                480.0,
                780.0,
            ),
            SpmPanel::Board => (
                "交付看板",
                pecofence_plugin_spm::board::PROVIDER_ID,
                pecofence_plugin_spm::board::demo_config(),
                1120.0,
                760.0,
            ),
            // spmd serves issues only from its Orion fixture until the Meegle join exists.
            SpmPanel::Issues => (
                "我的问题",
                pecofence_plugin_spm::issues::PROVIDER_ID,
                pecofence_plugin_spm::issues::demo_config(),
                480.0,
                780.0,
            ),
        };
        let rect = self.place_new_fence_dip(w, h, x, y, near);
        if self
            .state
            .new_panel_fence(
                pecofence_core::i18n::text(title),
                rect,
                provider,
                version,
                config,
            )
            .is_some()
        {
            self.resync_windows();
            self.schedule_save();
        }
    }

    /// A board fence that opens in its timeline view. The view is set by a context the board
    /// reads once it is mounted, so the fence never starts on the wrong tab.
    pub(super) fn new_timeline_near(&mut self, x: i32, y: i32, near: Option<ContainerId>) {
        let (version, config) = pecofence_plugin_spm::board::demo_config();
        let rect = self.place_new_fence_dip(1120.0, 760.0, x, y, near);
        let Some(content) = self.state.new_panel_fence(
            pecofence_core::i18n::text("交付时间线"),
            rect,
            pecofence_plugin_spm::board::PROVIDER_ID,
            version,
            config,
        ) else {
            return;
        };
        self.resync_windows();
        self.schedule_save();
        if let Some(pecofence_core::FenceContentSpec::Panel { panel }) =
            self.state.fence(content).map(|f| f.content)
        {
            self.deliver_ci_context(
                pecofence_plugin_spm::board::PROVIDER_ID,
                Some(panel.instance_id.as_u128()),
                "board.timeline",
                "{}",
            );
        }
        self.process_panel_events();
    }

    pub(super) fn new_widget_near(
        &mut self,
        provider: &str,
        x: i32,
        y: i32,
        near: Option<ContainerId>,
    ) {
        let title = match provider {
            pecofence_plugin_widgets::CLOCK_PROVIDER => "数字时钟",
            pecofence_plugin_widgets::CALENDAR_PROVIDER => "日历",
            pecofence_plugin_widgets::TODO_PROVIDER => "待办清单",
            pecofence_plugin_widgets::NOTE_PROVIDER => "便笺",
            pecofence_plugin_widgets::DIAGRAM_PROVIDER => "图表",
            _ => return,
        };
        let Some(rules) = pecofence_plugin_widgets::grid::span_rules(provider) else {
            return;
        };
        // Default span on the grid new fences use (settings icon size, two label lines).
        let metrics =
            crate::layout::GridMetrics::for_icon_size(self.state.config.settings.icon_size, 2)
                .with_line_h(self.ctx.chrome.label_line_h());
        let cell = pecofence_plugin_widgets::grid::Cell {
            w: metrics.cell_w,
            h: metrics.cell_h,
        };
        let span = rules.clamp(rules.default, cell, self.ctx.behavior.text_scale.get());
        let (width, height) = span.size(cell);
        let rect = self.place_new_fence_dip(width, height, x, y, near);
        if let Some(id) =
            self.state
                .new_widget_fence(pecofence_core::i18n::text(title), rect, provider)
        {
            if let Some(host) = self.state.host_of(id)
                && let Some(container) = self.state.container_mut(host)
            {
                container.span = Some(pecofence_core::WidgetSpan {
                    columns: span.columns,
                    rows: span.rows,
                });
            }
            self.resync_windows();
            self.schedule_save();
            if matches!(
                provider,
                pecofence_plugin_widgets::NOTE_PROVIDER
                    | pecofence_plugin_widgets::TODO_PROVIDER
                    | pecofence_plugin_widgets::DIAGRAM_PROVIDER
            ) && let Some(pecofence_core::FenceSnapshot {
                content: pecofence_core::FenceContentSpec::Panel { panel },
                ..
            }) = self.state.fence(id)
            {
                // Ask the provider for its editor, including its live body bounds,
                // colors and template, just like the on-panel Edit action.
                let result =
                    self.panel_manager
                        .borrow_mut()
                        .open_panel(&panel)
                        .and_then(|handle| {
                            let key = handle.key();
                            handle
                                .event(pecofence_plugin_api::PanelEvent::Invoke {
                                    action: 1,
                                    layout_revision: 0,
                                })
                                .map(|update| (key, update))
                        });
                if let Ok((key, update)) = result {
                    self.pending_panel_updates
                        .entry(key)
                        .or_default()
                        .merge(update);
                    self.process_panel_events();
                }
            }
        }
    }

    /// Work area (device px) containing the point, else the primary one.
    pub(super) fn work_area_at(&self, x: i32, y: i32) -> Option<WorkArea> {
        self.state
            .work_areas
            .iter()
            .find(|w| x >= w.left && x < w.right && y >= w.top && y < w.bottom)
            .or(self.state.work_areas.first())
            .cloned()
    }

    /// Picks a screen rectangle `cols` icon columns wide and `h_dip` tall for a new fence that
    /// does not overlap any existing fence: beside `near` (right, left, below, above) first,
    /// then a coarse scan of the work area, finally centred on the point. Result is clamped
    /// into the work area. The width is taken from the grid metrics so the column snap that
    /// runs right after creation is a no-op (otherwise it would grow the fence into a
    /// neighbour or past the work-area edge that was just checked here).
    pub(super) fn place_new_fence(
        &self,
        cols: u32,
        h_dip: f32,
        x: i32,
        y: i32,
        near: Option<ContainerId>,
    ) -> RECT {
        let view = pecofence_core::FenceView::default();
        let w_dip = crate::layout::GridMetrics::for_icon_size(
            self.state.config.settings.icon_size,
            view.label_lines,
        )
        .width_for_columns(cols);
        self.place_new_fence_dip(w_dip, h_dip, x, y, near)
    }

    /// Does `r` lie inside `work` without overlapping an existing fence window (gap/2 slack)?
    pub(super) fn rect_free(&self, r: &RECT, work: &WorkArea) -> bool {
        let fits = r.left >= work.left
            && r.top >= work.top
            && r.right <= work.right
            && r.bottom <= work.bottom;
        fits && !self.overlaps_fence(r, (12.0 * work.scale()) as i32)
    }

    pub(super) fn overlaps_fence(&self, r: &RECT, gap: i32) -> bool {
        self.fences.values().any(|f| {
            let o = f.rect();
            r.left < o.right - gap / 2
                && r.right > o.left + gap / 2
                && r.top < o.bottom - gap / 2
                && r.bottom > o.top + gap / 2
        })
    }

    /// `place_new_fence` for an explicit DIP size.
    pub(super) fn place_new_fence_dip(
        &self,
        w_dip: f32,
        h_dip: f32,
        x: i32,
        y: i32,
        near: Option<ContainerId>,
    ) -> RECT {
        let Some(work) = self.work_area_at(x, y) else {
            return RECT {
                left: x,
                top: y,
                right: x + w_dip as i32,
                bottom: y + h_dip as i32,
            };
        };
        let scale = work.scale();
        let (w, h) = ((w_dip * scale) as i32, (h_dip * scale) as i32);
        // Same 8 DIP gutter that drag-snapping leaves between windows.
        let gap = (8.0 * scale) as i32;
        let at = |left: i32, top: i32| RECT {
            left,
            top,
            right: left + w,
            bottom: top + h,
        };
        let mut candidates = Vec::new();
        if let Some(src) = near.and_then(|id| self.fences.get(&id)).map(|f| f.rect()) {
            candidates.push(at(src.right + gap, src.top));
            candidates.push(at(src.left - gap - w, src.top));
            candidates.push(at(src.left, src.bottom + gap));
            candidates.push(at(src.left, src.top - gap - h));
        }
        candidates.push(at(x - w / 2, y - h / 2));
        if let Some(r) = candidates.iter().find(|r| self.rect_free(r, &work)) {
            return *r;
        }
        // Coarse scan, top-left to bottom-right.
        let step = (24.0 * scale) as i32;
        let mut top = work.top + gap;
        while top + h <= work.bottom {
            let mut left = work.left + gap;
            while left + w <= work.right {
                let r = at(left, top);
                if !self.overlaps_fence(&r, gap) {
                    return r;
                }
                left += step;
            }
            top += step;
        }
        // Everything is covered: centre on the point, clamped into the work area.
        let left = (x - w / 2).clamp(work.left, (work.right - w).max(work.left));
        let top = (y - h / 2).clamp(work.top, (work.bottom - h).max(work.top));
        at(left, top)
    }

    pub(super) fn toggle_roll(&mut self, fence: ContainerId) {
        // Frameless widgets have no title bar to roll up into.
        if self.is_frameless_widget(fence) {
            return;
        }
        if let Some(w) = self.fences.get(&fence) {
            let rolled = !w.is_rolled();
            w.set_rolled(rolled);
            if let Some(f) = self.state.container_mut(fence) {
                f.rolled_up = rolled;
            }
            self.state.mark_dirty();
            self.schedule_save();
        }
    }
}

/// The SPM panels a menu can create.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SpmPanel {
    Board,
    CiCd,
    Issues,
}

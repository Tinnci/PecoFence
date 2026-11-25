use super::*;

impl AppState {
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
    /// Fences that own a window: every container's active tab, except closed ones.
    pub fn host_fences(&self) -> Vec<Fence> {
        self.config
            .layouts
            .get(self.layout)
            .map(|l| {
                l.containers
                    .iter()
                    .filter(|c| !c.closed)
                    .filter_map(|c| l.project(c.active_tab))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Closed fences as (container, active tab title), for the tray's reopen menu.
    pub fn closed_fences(&self) -> Vec<(ContainerId, String)> {
        self.config
            .layouts
            .get(self.layout)
            .map(|l| {
                l.containers
                    .iter()
                    .filter(|c| c.closed)
                    .filter_map(|c| l.project(c.active_tab).map(|f| (c.id, f.title)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Close or reopen a fence window. The fence and its content stay in the layout.
    pub fn set_closed(&mut self, id: ContainerId, closed: bool) -> bool {
        match self.container_mut(id) {
            Some(c) if c.closed != closed => {
                c.closed = closed;
                self.mark_dirty();
                true
            }
            _ => false,
        }
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

    /// Update the business instance in every monitor layout through document revisions.
    pub fn update_panel_config(&mut self, instance_id: u128, config: serde_json::Value) -> bool {
        let mut changed = false;
        for layout in &mut self.config.layouts {
            for content in &mut layout.contents {
                if let pecofence_core::ContentSpec::Panel { panel } = &mut content.content
                    && panel.instance_id.as_u128() == instance_id
                    && panel.config != config
                {
                    panel.config = config.clone();
                    changed = true;
                }
            }
        }
        if changed {
            self.mark_dirty();
        }
        changed
    }

    pub fn new_widget_fence(&mut self, title: &str, rect: RECT, provider: &str) -> Option<FenceId> {
        self.new_panel_fence(title, rect, provider, 1, serde_json::json!({}))
    }

    pub fn new_panel_fence(
        &mut self,
        title: &str,
        rect: RECT,
        provider: &str,
        config_version: u16,
        config: serde_json::Value,
    ) -> Option<FenceId> {
        let id = self.new_fence(title, rect)?;
        self.content_mut(id)?.content = pecofence_core::ContentSpec::Panel {
            panel: pecofence_core::PanelSpec {
                provider: provider.to_owned(),
                instance_id: uuid::Uuid::new_v4(),
                config_version,
                config,
            },
        };
        Some(id)
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
        self.insert_rule(template.rule(id));
        Ok(id)
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

/// `key` with exactly one trailing separator, for "is inside" prefix tests. Drive roots from
/// `ItemKey::from_path` already end in a backslash (`d:\`); deeper paths do not.
pub(super) fn dir_prefix(key: &str) -> String {
    if key.ends_with('\\') {
        key.to_string()
    } else {
        format!("{key}\\")
    }
}

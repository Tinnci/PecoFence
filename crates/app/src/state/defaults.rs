//! Default presentation changes are prepared and validated before the live document changes.
use super::*;
use pecofence_core::default_template::{
    self, DefaultPresetOperation as Operation, DefaultRole as Role, Placement, TemplateSize,
};
use pecofence_core::{ContentId, PanelSpec, WidgetSpan, WorkspaceAppearance};

fn create(role: Role, icon_size: u32) -> ContentInstance {
    let title = pecofence_core::i18n::text(role.title());
    let mut content = if let Some(provider) = role.provider() {
        ContentInstance::panel(
            title,
            PanelSpec {
                provider: provider.into(),
                instance_id: uuid::Uuid::new_v4(),
                config_version: 1,
                config: serde_json::json!({}),
            },
        )
    } else {
        ContentInstance::collection(title, role == Role::Inbox)
    };
    content.default_role = Some(role);
    content.view.icon_size = icon_size;
    content
}

fn arrange(layout: &mut Layout, placements: &[Placement]) {
    let ids: HashMap<_, _> = layout
        .contents
        .iter()
        .filter_map(|c| c.default_role.map(|role| (role, c.id)))
        .collect();
    let selected: HashSet<_> = ids.values().copied().collect();
    let original_owners: HashMap<_, _> = layout
        .containers
        .iter()
        .flat_map(|w| w.tabs.iter().map(move |id| (*id, w.clone())))
        .collect();
    let mut reusable = Vec::new();
    for window in &mut layout.containers {
        let before = window.clone();
        window.tabs.retain(|id| !selected.contains(id));
        if window.tabs.is_empty() {
            reusable.push(before);
        } else if !window.tabs.contains(&window.active_tab) {
            window.active_tab = window.tabs[0];
        }
    }
    layout.containers.retain(|w| !w.tabs.is_empty());
    for placement in placements {
        let tabs: Vec<_> = placement
            .roles
            .iter()
            .filter_map(|r| ids.get(r).copied())
            .collect();
        if tabs.is_empty() {
            continue;
        }
        if let Some(window) = layout
            .containers
            .iter_mut()
            .find(|w| overlaps(&w.geometry, &placement.geometry))
        {
            window.tabs.extend(tabs);
            window.span = None;
            continue;
        }
        let mut window =
            if let Some(index) = reusable.iter().position(|w| w.tabs.contains(&tabs[0])) {
                reusable.remove(index)
            } else {
                let mut window = original_owners
                    .get(&tabs[0])
                    .cloned()
                    .unwrap_or_else(|| Container::new(tabs[0], placement.geometry.clone()));
                window.id = ContainerId::new();
                window
            };
        window.geometry = placement.geometry.clone();
        window.expanded_h = window.geometry.h;
        window.rolled_up = false;
        window.auto_height = false;
        window.active_tab = tabs[0];
        window.tabs = tabs;
        window.span = placement.span;
        layout.containers.push(window);
    }
}

fn overlaps(a: &NormGeometry, b: &NormGeometry) -> bool {
    a.monitor == b.monitor
        && a.x < b.x + b.w + 8.0
        && b.x < a.x + a.w + 8.0
        && a.y < b.y + b.h + 8.0
        && b.y < a.y + a.h + 8.0
}

impl AppState {
    fn template_plan(&self, text_scale: f32, line_h: f32) -> Vec<Placement> {
        let fallback = WorkArea {
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
        };
        let work = self
            .work_areas
            .iter()
            .find(|w| w.mon_left == 0 && w.mon_top == 0)
            .or_else(|| self.work_areas.first())
            .unwrap_or(&fallback);
        let sizes: Vec<_> = Role::ALL
            .into_iter()
            .map(|role| {
                let matches = |c: &&ContentInstance| {
                    c.default_role == Some(role)
                        || (c.default_role.is_none()
                            && role.accepts(&c.content)
                            && (role.provider().is_some()
                                || role == Role::Inbox
                                || c.title == role.title()
                                || c.title == pecofence_core::i18n::text(role.title())))
                };
                let content = self
                    .config
                    .layouts
                    .get(self.layout)
                    .and_then(|l| l.contents.iter().find(matches))
                    .or_else(|| {
                        self.config
                            .layouts
                            .iter()
                            .flat_map(|l| &l.contents)
                            .find(matches)
                    });
                let fallback_view = pecofence_core::FenceView {
                    icon_size: self.config.settings.icon_size,
                    ..Default::default()
                };
                let view = content.map_or(&fallback_view, |c| &c.view);
                let metrics =
                    crate::layout::GridMetrics::for_icon_size(view.icon_size, view.label_lines)
                        .with_line_h(line_h);
                let cell = pecofence_plugin_widgets::grid::Cell {
                    w: metrics.cell_w,
                    h: metrics.cell_h,
                };
                let span = role
                    .provider()
                    .and_then(pecofence_plugin_widgets::grid::span_rules)
                    .map(|rules| rules.clamp(rules.default, cell, text_scale));
                let (w, h) = span.map_or(
                    (
                        metrics.with_spacing(view.spacing).width_for_columns(3),
                        160.0,
                    ),
                    |s| s.size(cell),
                );
                TemplateSize {
                    role,
                    w,
                    h,
                    span: span.map(|s| WidgetSpan {
                        columns: s.columns,
                        rows: s.rows,
                    }),
                }
            })
            .collect();
        let file_width = sizes
            .iter()
            .filter(|s| s.role.provider().is_none())
            .map(|s| s.w)
            .fold(0.0, f32::max);
        default_template::plan(work, &sizes, file_width)
    }

    pub(super) fn build_default_fences(&mut self) -> (Vec<Container>, Vec<ContentInstance>) {
        let contents: Vec<_> = Role::ALL
            .into_iter()
            .map(|r| create(r, self.config.settings.icon_size))
            .collect();
        let mut layout = Layout {
            fingerprint: Vec::new(),
            containers: Vec::new(),
            contents,
        };
        let id = |r| {
            layout
                .contents
                .iter()
                .find(|c| c.default_role == Some(r))
                .unwrap()
                .id
        };
        self.config.rules =
            RuleSet::default_presets(id(Role::Programs), id(Role::Folders), id(Role::Documents));
        arrange(&mut layout, &self.template_plan(1.0, 16.0));
        (layout.containers, layout.contents)
    }

    /// Run once after the first-run renderer has measured the real text grid.
    pub fn reflow_initial_defaults(&mut self, text_scale: f32, line_h: f32) {
        if !self.save_allowed {
            return;
        }
        let plan = self.template_plan(text_scale, line_h);
        if let Some(layout) = self.config.layouts.get_mut(self.layout) {
            arrange(layout, &plan);
        }
        self.mark_dirty();
    }

    pub fn apply_default_preset(
        &mut self,
        operation: Operation,
        text_scale: f32,
        line_h: f32,
    ) -> Result<usize, String> {
        if !self.save_allowed {
            return Err("Workspace is read-only".into());
        }
        let placements = self.template_plan(text_scale, line_h);
        let mut candidate = self.config.clone();
        let current = candidate
            .layouts
            .get_mut(self.layout)
            .ok_or("No active layout")?;
        if operation != Operation::RestoreAppearance {
            // Adopt existing instances first. Renaming a previously tagged component is harmless.
            let defaults = RuleSet::default_presets(
                ContentId::from_u128(1),
                ContentId::from_u128(2),
                ContentId::from_u128(3),
            );
            for (index, role) in [Role::Programs, Role::Folders, Role::Documents]
                .into_iter()
                .enumerate()
            {
                if current
                    .contents
                    .iter()
                    .any(|c| c.default_role == Some(role))
                {
                    continue;
                }
                let target = candidate
                    .rules
                    .list
                    .iter()
                    .find(|r| r.all_of == defaults.list[index].all_of)
                    .and_then(|r| match r.target {
                        Target::Collection(id) => Some(id),
                        _ => None,
                    });
                if let Some(content) = current.contents.iter_mut().find(|c| {
                    c.default_role.is_none()
                        && role.accepts(&c.content)
                        && (target == Some(c.id)
                            || c.title == role.title()
                            || c.title == pecofence_core::i18n::text(role.title()))
                }) {
                    content.default_role = Some(role);
                }
            }
            for role in [
                Role::Inbox,
                Role::Clock,
                Role::Calendar,
                Role::Note,
                Role::Todo,
            ] {
                if current
                    .contents
                    .iter()
                    .any(|c| c.default_role == Some(role))
                {
                    continue;
                }
                if let Some(content) = current
                    .contents
                    .iter_mut()
                    .find(|c| c.default_role.is_none() && role.accepts(&c.content))
                {
                    content.default_role = Some(role);
                }
            }
        }
        let mut added = 0;
        match operation {
            Operation::RestoreLayout => arrange(current, &placements),
            Operation::AddMissingWidgets => {
                for placement in &placements {
                    let mut new_tabs = Vec::new();
                    for role in placement
                        .roles
                        .iter()
                        .copied()
                        .filter(|r| r.provider().is_some())
                    {
                        if candidate.layouts[self.layout]
                            .contents
                            .iter()
                            .any(|c| c.default_role == Some(role))
                        {
                            continue;
                        }
                        let mut content = candidate
                            .layouts
                            .iter()
                            .flat_map(|l| &l.contents)
                            .find(|c| {
                                c.default_role == Some(role)
                                    || (c.default_role.is_none() && role.accepts(&c.content))
                            })
                            .cloned()
                            .unwrap_or_else(|| create(role, candidate.settings.icon_size));
                        content.default_role = Some(role);
                        new_tabs.push(content.id);
                        candidate.layouts[self.layout].contents.push(content);
                        added += 1;
                    }
                    if new_tabs.is_empty() {
                        continue;
                    }
                    let layout = &mut candidate.layouts[self.layout];
                    if layout
                        .containers
                        .iter()
                        .all(|w| !overlaps(&w.geometry, &placement.geometry))
                    {
                        let mut window = Container::new(new_tabs[0], placement.geometry.clone());
                        window.tabs = new_tabs;
                        window.span = placement.span;
                        layout.containers.push(window);
                    } else {
                        // Preserve existing geometry when the intended space is occupied.
                        let owner = layout
                            .containers
                            .iter()
                            .position(|w| {
                                w.tabs.iter().any(|id| {
                                    layout.content(*id).is_some_and(|c| {
                                        c.default_role.is_some_and(|r| r.provider().is_some())
                                    })
                                })
                            })
                            .unwrap_or(0);
                        let window = layout
                            .containers
                            .get_mut(owner)
                            .ok_or("No container for missing widgets")?;
                        window.tabs.extend(new_tabs);
                        window.span = None;
                    }
                }
            }
            Operation::RestoreAppearance => {
                WorkspaceAppearance::capture(&pecofence_core::Settings::default())
                    .apply(&mut candidate.settings);
                for window in &mut current.containers {
                    window.appearance = None;
                }
            }
        }
        candidate.validate()?;
        if candidate.layouts == self.config.layouts && candidate.settings == self.config.settings {
            return Ok(0);
        }
        let name = match operation {
            Operation::RestoreLayout => "恢复默认排布前",
            Operation::AddMissingWidgets => "补齐默认组件前",
            Operation::RestoreAppearance => "恢复默认外观前",
        };
        let snapshot = self.save_snapshot(pecofence_core::i18n::text(name));
        self.config
            .snapshots
            .iter_mut()
            .find(|s| s.id == snapshot)
            .unwrap()
            .appearance = Some(WorkspaceAppearance::capture(&self.config.settings));
        candidate.snapshots = self.config.snapshots.clone();
        self.config = candidate;
        self.mark_dirty();
        tracing::info!(
            version = default_template::VERSION,
            ?operation,
            added,
            "default template applied"
        );
        Ok(added)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> AppState {
        super::super::tests::test_state()
    }
    fn id(state: &AppState, role: Role) -> ContentId {
        state.config.layouts[state.layout]
            .contents
            .iter()
            .find(|c| c.default_role == Some(role))
            .unwrap()
            .id
    }
    #[test]
    fn defaults_seed_all_roles_and_restoration_keeps_business_data_and_is_idempotent() {
        let mut state = state();
        state.first_run = false;
        assert_eq!(state.config.layouts[0].contents.len(), 8);
        let note = id(&state, Role::Note);
        let todo = id(&state, Role::Todo);
        if let ContentSpec::Panel { panel } = &mut state.content_mut(note).unwrap().content {
            panel.config = serde_json::json!({"text":"保留我的便笺"});
        }
        if let ContentSpec::Panel { panel } = &mut state.content_mut(todo).unwrap().content {
            panel.config =
                serde_json::json!({"tasks":[{"id":9,"title":"Keep","done":false}],"nextId":10});
        }
        let before = state.config.layouts[0].contents.clone();
        let rules = state.config.rules.clone();
        state.config.layouts[0].containers[0].geometry.x += 47.0;
        state
            .apply_default_preset(Operation::RestoreLayout, 1.0, 16.0)
            .unwrap();
        assert_eq!(state.config.layouts[0].contents, before);
        assert_eq!(state.config.rules, rules);
        assert_eq!(state.config.snapshots.len(), 1);
        let current = state.config.layouts.clone();
        state
            .apply_default_preset(Operation::RestoreLayout, 1.0, 16.0)
            .unwrap();
        state
            .apply_default_preset(Operation::AddMissingWidgets, 1.0, 16.0)
            .unwrap();
        assert_eq!(state.config.layouts, current);
        assert_eq!(state.config.snapshots.len(), 1);
        state.config.validate().unwrap();
    }
    #[test]
    fn supplement_adopts_legacy_widgets_and_reuses_business_instances_from_other_monitor_layouts() {
        let mut state = state();
        state.first_run = false;
        for content in &mut state.config.layouts[0].contents {
            content.default_role = None;
        }
        state
            .apply_default_preset(Operation::AddMissingWidgets, 1.0, 16.0)
            .unwrap();
        assert_eq!(state.config.layouts[0].contents.len(), 8);
        let note = id(&state, Role::Note);
        if let ContentSpec::Panel { panel } = &mut state.content_mut(note).unwrap().content {
            panel.config = serde_json::json!({"text":"另一显示器布局的数据"});
        }
        let original = state.content_mut(note).unwrap().clone();
        let mut alternative = state.config.layouts[0].clone();
        alternative.fingerprint[0].device_path = "alternative".into();
        state.config.layouts.push(alternative);
        assert!(state.delete_fence(note));
        assert_eq!(
            state
                .apply_default_preset(Operation::AddMissingWidgets, 1.0, 16.0)
                .unwrap(),
            1
        );
        assert_eq!(state.content_mut(note).unwrap(), &original);
        assert_eq!(
            state
                .apply_default_preset(Operation::AddMissingWidgets, 1.0, 16.0)
                .unwrap(),
            0
        );
        state.config.validate().unwrap();
    }
    #[test]
    fn restore_appearance_has_an_undo_snapshot_without_changing_startup_or_widget_configs() {
        let mut state = state();
        state.first_run = false;
        state.config.settings.theme = pecofence_core::ThemeSetting::Dark;
        state.config.settings.widget_paper_brightness = 0.75;
        state.config.settings.autostart = false;
        state.config.settings.desktop_path = Some("custom-desktop".into());
        state.config.layouts[0].containers[0].appearance =
            Some(pecofence_core::AppearanceOverride {
                opacity: Some(0.4),
                ..Default::default()
            });
        let before = state.config.clone();
        state
            .apply_default_preset(Operation::RestoreAppearance, 1.0, 16.0)
            .unwrap();
        assert!(!state.config.settings.autostart);
        assert_eq!(
            state.config.settings.desktop_path,
            before.settings.desktop_path
        );
        assert_eq!(state.config.layouts[0].contents, before.layouts[0].contents);
        for (a, b) in state.config.layouts[0]
            .containers
            .iter()
            .zip(&before.layouts[0].containers)
        {
            assert_eq!(a.geometry, b.geometry);
            assert!(a.appearance.is_none());
        }
        let snapshot = state.config.snapshots.last().unwrap().id;
        assert!(state.restore_snapshot_with_backup(snapshot, "Undo appearance"));
        assert_eq!(state.config.settings, before.settings);
        assert_eq!(state.config.layouts, before.layouts);
        state.config.validate().unwrap();
    }
    #[test]
    fn rejected_budget_and_read_only_operations_leave_the_live_document_and_history_untouched() {
        let mut state = state();
        state.first_run = false;
        let note = id(&state, Role::Note);
        assert!(state.delete_fence(note));
        while state.config.layouts[0].contents.len() < pecofence_core::MAX_CONTENTS {
            let content = ContentInstance::collection("Custom", false);
            let window = Container::new(
                content.id,
                state.config.layouts[0].containers[0].geometry.clone(),
            );
            state.config.layouts[0].contents.push(content);
            state.config.layouts[0].containers.push(window);
        }
        let before = serde_json::to_value(&state.config).unwrap();
        assert!(
            state
                .apply_default_preset(Operation::AddMissingWidgets, 1.0, 16.0)
                .is_err()
        );
        assert_eq!(serde_json::to_value(&state.config).unwrap(), before);
        state.save_allowed = false;
        assert!(
            state
                .apply_default_preset(Operation::RestoreLayout, 1.0, 16.0)
                .is_err()
        );
        assert_eq!(serde_json::to_value(&state.config).unwrap(), before);
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn restoring_a_preset_snapshot_keeps_the_latest_text_of_surviving_widgets() {
        let mut state = super::super::tests::test_state();
        state.first_run = false;
        state.config.layouts[0].containers[0].geometry.x += 35.0;
        state
            .apply_default_preset(Operation::RestoreLayout, 1.0, 16.0)
            .unwrap();
        let snapshot = state.config.snapshots.last().unwrap().id;
        let mut alternate = state.config.layouts[0].clone();
        alternate.fingerprint[0].device_path = "other-screen".into();
        state.config.layouts.push(alternate);
        let note = state.config.layouts[0]
            .contents
            .iter_mut()
            .find(|c| c.default_role == Some(Role::Note))
            .unwrap();
        let ContentSpec::Panel { panel } = &mut note.content else {
            panic!()
        };
        panel.config = serde_json::json!({"text":"恢复位置也要保留新编辑"});
        let business = panel.clone();
        assert!(state.restore_snapshot_with_backup(snapshot, "Before undo"));
        let note = state.config.layouts[0]
            .contents
            .iter()
            .find(|c| c.default_role == Some(Role::Note))
            .unwrap();
        assert_eq!(note.content, ContentSpec::Panel { panel: business });
    }
}

#[cfg(test)]
mod grid_tests {
    use super::*;
    #[test]
    fn restoring_positions_keeps_each_contents_icon_size_and_uses_its_real_grid_width() {
        let mut state = super::super::tests::test_state();
        state.first_run = false;
        let programs = state.config.layouts[0]
            .contents
            .iter_mut()
            .find(|c| c.default_role == Some(Role::Programs))
            .unwrap();
        let id = programs.id;
        programs.view.icon_size = 96;
        programs.view.spacing = pecofence_core::Spacing::Loose;
        let before = state.config.layouts[0].contents.clone();
        state
            .apply_default_preset(Operation::RestoreLayout, 1.5, 24.0)
            .unwrap();
        assert_eq!(state.config.layouts[0].contents, before);
        let owner = state.config.layouts[0].owner_of(id).unwrap();
        let geometry = &state.config.layouts[0].container(owner).unwrap().geometry;
        let minimum = crate::layout::GridMetrics::for_icon_size(96, 2)
            .with_line_h(24.0)
            .with_spacing(pecofence_core::Spacing::Loose)
            .width_for_columns(3);
        assert!(geometry.w >= minimum);
        assert!(geometry.x + geometry.w <= state.work_areas[0].width_dip());
    }
}

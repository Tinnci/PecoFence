use super::*;
use crate::commands::FenceCmd;

impl App {
    /// Fence background / title menu. Frequent actions only; per-fence properties (appearance,
    /// spacing, auto height, quick-hide exclusion, dock, portal flags) live on the settings
    /// page's 「栅栏」 tab, reached through 栅栏选项….
    pub(in crate::app) fn show_fence_menu(&mut self, fence: FenceId, x: i32, y: i32) {
        // `fence` may be a tab: window-level items (roll, lock) act on its host `h`, content
        // items (view, sort, new, delete) on the fence itself.
        let Some(f) = self.state.fence(fence) else {
            return;
        };
        let Some(host) = self.state.host_of(fence) else {
            return;
        };
        let Some(h) = self.state.container(host) else {
            return;
        };
        let locked = h.locked;
        let suggestions = if f.content.is_files() {
            self.fence_suggestions(fence)
        } else {
            Vec::new()
        };
        let menu = PopupMenu::new();
        let rolled = self.fences.get(&host).is_some_and(|w| w.is_rolled());
        menu.item(
            CMD_FENCE_ROLL,
            if rolled {
                pecofence_core::i18n::text("展开")
            } else {
                pecofence_core::i18n::text("卷起")
            },
            false,
            // Frameless widgets have no title bar to roll up into.
            self.is_frameless_widget(host),
        )
        .item(
            CMD_FENCE_RENAME,
            pecofence_core::i18n::text("重命名…"),
            false,
            false,
        );
        let portal_dir = self.state.portal_path(fence);
        if portal_dir.is_some() {
            menu.item(
                CMD_FENCE_OPEN_FOLDER,
                pecofence_core::i18n::text("在资源管理器中打开文件夹"),
                false,
                false,
            );
            if self.state.portal_navigated(fence) {
                menu.item(
                    CMD_PORTAL_UP,
                    pecofence_core::i18n::text("返回上一级\tBackspace"),
                    false,
                    false,
                )
                .item(
                    CMD_PORTAL_HOME,
                    pecofence_core::i18n::text("返回门户根文件夹"),
                    false,
                    false,
                );
            }
        }
        menu.separator();
        if f.content.is_files() {
            let view_menu = PopupMenu::new();
            view_menu
                .check_item(
                    CMD_FENCE_VIEW_ICONS,
                    pecofence_core::i18n::text("图标"),
                    f.view.layout == ViewLayout::Icons,
                    false,
                )
                .check_item(
                    CMD_FENCE_VIEW_LIST,
                    pecofence_core::i18n::text("列表"),
                    f.view.layout == ViewLayout::List,
                    false,
                )
                .check_item(
                    CMD_FENCE_VIEW_DETAILS,
                    pecofence_core::i18n::text("详细信息"),
                    f.view.layout == ViewLayout::Details,
                    false,
                )
                .separator()
                .check_item(
                    CMD_FENCE_ICON_32,
                    pecofence_core::i18n::text("小图标"),
                    f.view.icon_size == 32,
                    false,
                )
                .check_item(
                    CMD_FENCE_ICON_48,
                    pecofence_core::i18n::text("中图标"),
                    f.view.icon_size == 48,
                    false,
                )
                .check_item(
                    CMD_FENCE_ICON_64,
                    pecofence_core::i18n::text("较大图标"),
                    f.view.icon_size == 64,
                    false,
                )
                .check_item(
                    CMD_FENCE_ICON_96,
                    pecofence_core::i18n::text("大图标"),
                    f.view.icon_size == 96,
                    false,
                );
            menu.submenu(pecofence_core::i18n::text("视图"), view_menu);
            let sort_menu = PopupMenu::new();
            sort_menu
                .check_item(
                    CMD_FENCE_SORT_MANUAL,
                    pecofence_core::i18n::text("手动"),
                    f.view.sort == SortMode::Manual,
                    false,
                )
                .check_item(
                    CMD_FENCE_SORT_NAME,
                    pecofence_core::i18n::text("按名称"),
                    f.view.sort == SortMode::Name,
                    false,
                )
                .check_item(
                    CMD_FENCE_SORT_TYPE,
                    pecofence_core::i18n::text("按类型"),
                    f.view.sort == SortMode::Type,
                    false,
                )
                .check_item(
                    CMD_FENCE_SORT_DATE,
                    pecofence_core::i18n::text("按修改日期"),
                    f.view.sort == SortMode::Date,
                    false,
                )
                .check_item(
                    CMD_FENCE_SORT_SIZE,
                    pecofence_core::i18n::text("按大小"),
                    f.view.sort == SortMode::Size,
                    false,
                )
                .check_item(
                    CMD_FENCE_SORT_OPENED,
                    pecofence_core::i18n::text("按使用频率"),
                    f.view.sort == SortMode::OpenCount,
                    false,
                )
                .separator()
                .check_item(
                    CMD_FENCE_SORT_REVERSE,
                    pecofence_core::i18n::text("倒序"),
                    f.view.reverse,
                    false,
                )
                .separator()
                .check_item(
                    CMD_FENCE_SORT_GROUP,
                    pecofence_core::i18n::text("按时间分组"),
                    f.view.group_by_date,
                    false,
                );
            menu.submenu(pecofence_core::i18n::text("排序方式"), sort_menu);
            if !suggestions.is_empty() {
                let suggest_menu = PopupMenu::new();
                for (index, suggestion) in suggestions.iter().enumerate() {
                    suggest_menu.item(
                        CMD_FENCE_SUGGEST_BASE + index as u32,
                        &self.suggestion_label(suggestion),
                        false,
                        false,
                    );
                }
                menu.submenu(pecofence_core::i18n::text("建议"), suggest_menu);
            }
            let new_menu = PopupMenu::new();
            new_menu
                .item(
                    CMD_FENCE_NEW_FOLDER,
                    pecofence_core::i18n::text("文件夹"),
                    false,
                    false,
                )
                .item(
                    CMD_FENCE_NEW_TEXT,
                    pecofence_core::i18n::text("文本文档"),
                    false,
                    false,
                )
                .separator()
                .item(
                    CMD_TAB_NEW,
                    pecofence_core::i18n::text("标签页"),
                    false,
                    false,
                );
            menu.submenu(pecofence_core::i18n::text("新建"), new_menu);
            menu.item(
                CMD_FENCE_PASTE,
                pecofence_core::i18n::text("粘贴\tCtrl+V"),
                false,
                !self.clipboard_port.has_files(),
            );
        }
        menu.submenu(
            pecofence_core::i18n::text("新建小组件"),
            new_widget_menu(WidgetIds {
                clock: CMD_FENCE_NEW_CLOCK,
                calendar: CMD_FENCE_NEW_CALENDAR,
                note: CMD_FENCE_NEW_NOTE,
                todo: CMD_FENCE_NEW_TODO,
                diagram: CMD_FENCE_NEW_DIAGRAM,
                board: CMD_FENCE_NEW_BOARD,
                timeline: CMD_FENCE_NEW_TIMELINE,
                ci_cd: CMD_FENCE_NEW_CI_WATCH,
                issues: CMD_FENCE_NEW_ISSUES,
                planning: CMD_FENCE_NEW_PLANNING,
            }),
        );
        // Tabbed fences (Fences 6): split a tab out, move it along the strip, or merge this
        // fence into another window. Hidden when none of that applies.
        let tabs = self.state.tabs_of(host);
        let tab_idx = tabs.iter().position(|t| *t == fence).unwrap_or(0);
        let attach_hosts = self.state.host_fences();
        let other_hosts: Vec<(u32, String)> = attach_hosts
            .iter()
            .enumerate()
            .filter(|(_, o)| o.container_id != host)
            .map(|(i, o)| (CMD_TAB_ATTACH_BASE + i as u32, o.title.clone()))
            .collect();
        if tabs.len() >= 2 || !other_hosts.is_empty() {
            let tab_menu = PopupMenu::new();
            if tabs.len() >= 2 {
                tab_menu
                    .item(
                        CMD_TAB_MOVE_LEFT,
                        pecofence_core::i18n::text("左移"),
                        false,
                        tab_idx == 0,
                    )
                    .item(
                        CMD_TAB_MOVE_RIGHT,
                        pecofence_core::i18n::text("右移"),
                        false,
                        tab_idx + 1 >= tabs.len(),
                    )
                    .item(
                        CMD_TAB_DETACH,
                        pecofence_core::i18n::text("拆出为独立栅栏"),
                        false,
                        false,
                    );
            }
            if !other_hosts.is_empty() {
                if tabs.len() >= 2 {
                    tab_menu.separator();
                }
                let attach_menu = PopupMenu::new();
                for (id, title) in &other_hosts {
                    attach_menu.item(*id, title, false, false);
                }
                tab_menu.submenu(pecofence_core::i18n::text("合并到…"), attach_menu);
            }
            menu.submenu(pecofence_core::i18n::text("标签页"), tab_menu);
        }
        if !f.content.is_files() {
            let paper_menu = PopupMenu::new();
            for (index, (style, label)) in [
                (pecofence_core::WidgetPaperStyle::Adaptive, "跟随壁纸"),
                (pecofence_core::WidgetPaperStyle::Light, "浅色纸张"),
                (pecofence_core::WidgetPaperStyle::Soft, "柔和纸张"),
                (pecofence_core::WidgetPaperStyle::Dark, "深色纸张"),
            ]
            .iter()
            .enumerate()
            {
                paper_menu.check_item(
                    CMD_PAPER_BASE + index as u32,
                    pecofence_core::i18n::text(label),
                    self.state.config.settings.widget_paper_style == *style,
                    false,
                );
            }
            menu.submenu(pecofence_core::i18n::text("小组件纸张（全局）"), paper_menu);
        }
        // Widget options (clock face, weight, …) come first among the window settings.
        let panel_items = self.panel_menu_items(fence);
        if !panel_items.is_empty() {
            let mut groups: Vec<(String, PopupMenu)> = Vec::new();
            for (index, item) in panel_items.iter().enumerate().take(1000) {
                let id = CMD_PANEL_ITEM_BASE + index as u32;
                match &item.submenu {
                    Some(name) => {
                        let slot = match groups.iter().position(|(g, _)| g == name) {
                            Some(slot) => slot,
                            None => {
                                groups.push((name.clone(), PopupMenu::new()));
                                groups.len() - 1
                            }
                        };
                        groups[slot]
                            .1
                            .check_item(id, &item.label, item.checked, !item.enabled);
                    }
                    None => {
                        menu.check_item(id, &item.label, item.checked, !item.enabled);
                    }
                }
            }
            for (name, submenu) in groups {
                menu.submenu(&name, submenu);
            }
            menu.separator();
        }
        menu.check_item(
            CMD_FENCE_LOCK,
            pecofence_core::i18n::text("锁定位置和大小"),
            locked,
            false,
        );
        menu.separator()
            .item(
                CMD_FENCE_OPTIONS,
                pecofence_core::i18n::text("栅栏选项…"),
                false,
                false,
            )
            .item(
                CMD_FENCE_NEW,
                pecofence_core::i18n::text("新建栅栏"),
                false,
                false,
            )
            .item(
                CMD_FENCE_DELETE,
                pecofence_core::i18n::text("删除栅栏"),
                false,
                f.kind == FenceKind::Inbox,
            );
        let owner = self
            .fences
            .get(&host)
            .map(|w| w.hwnd())
            .unwrap_or(self.control.hwnd());
        let cmd = menu.show_context(owner, x, y);
        match cmd {
            id if (CMD_PAPER_BASE..CMD_PAPER_BASE + 4).contains(&id) => {
                let mut settings = self.state.config.settings.clone();
                settings.widget_paper_style = [
                    pecofence_core::WidgetPaperStyle::Adaptive,
                    pecofence_core::WidgetPaperStyle::Light,
                    pecofence_core::WidgetPaperStyle::Soft,
                    pecofence_core::WidgetPaperStyle::Dark,
                ][(id - CMD_PAPER_BASE) as usize];
                self.apply_settings(settings);
            }
            id if (CMD_FENCE_SUGGEST_BASE
                ..CMD_FENCE_SUGGEST_BASE + suggestions::MAX_FENCE_SUGGESTIONS as u32)
                .contains(&id) =>
            {
                if let Some(suggestion) = suggestions.get((id - CMD_FENCE_SUGGEST_BASE) as usize) {
                    self.apply_suggestion(suggestion);
                }
            }
            id if (CMD_PANEL_ITEM_BASE..CMD_PANEL_ITEM_BASE + 1000).contains(&id) => {
                if let Some(item) = panel_items.get((id - CMD_PANEL_ITEM_BASE) as usize) {
                    self.invoke_panel_menu_item(fence, item.action);
                }
            }
            CMD_FENCE_ROLL => self.toggle_roll(host),
            CMD_FENCE_RENAME => self.begin_rename(fence),
            CMD_TAB_NEW => {
                let rect = self.fences.get(&host).map(|w| w.rect()).unwrap_or(RECT {
                    left: x,
                    top: y,
                    right: x + 240,
                    bottom: y + 200,
                });
                if let Some(id) = self
                    .state
                    .new_fence(pecofence_core::i18n::text("新标签页"), rect)
                {
                    if let Err(error) = self.state.attach_tab(id, host) {
                        self.settings_error(&error.to_string());
                    }
                    self.resync_windows();
                    self.apply_fence_view(host);
                    self.schedule_save();
                }
            }
            CMD_TAB_DETACH => self.detach_tab(fence, x, y, false),
            CMD_TAB_MOVE_LEFT => self.reorder_tab(host, fence, tab_idx.saturating_sub(1)),
            CMD_TAB_MOVE_RIGHT => self.reorder_tab(host, fence, tab_idx + 1),
            CMD_FENCE_PASTE => self.paste_into(fence),
            c if c >= CMD_TAB_ATTACH_BASE
                && c < CMD_TAB_ATTACH_BASE + attach_hosts.len() as u32 =>
            {
                let idx = (c - CMD_TAB_ATTACH_BASE) as usize;
                if let Some(target) = attach_hosts.get(idx).map(|o| o.container_id) {
                    match self.state.attach_tab(fence, target) {
                        Ok(_) => {
                            self.resync_windows();
                            self.apply_fence_view(target);
                            if let Some(w) = self.fences.get(&target) {
                                self.queue
                                    .push(Command::Fence(FenceCmd::RaiseFence(w.hwnd())));
                            }
                            self.schedule_save();
                        }
                        Err(error) => self.settings_error(&error.to_string()),
                    }
                }
            }
            CMD_FENCE_OPEN_FOLDER => {
                if let Some(dir) = portal_dir.as_deref() {
                    let _ = self.shell_port.open(dir, None, None);
                }
            }
            CMD_PORTAL_UP => self.portal_up(fence),
            CMD_PORTAL_HOME => {
                if self.state.portal_home(fence) {
                    self.after_portal_navigation(fence);
                }
            }
            CMD_FENCE_NEW_FOLDER => self.create_desktop_item(fence, true),
            CMD_FENCE_NEW_TEXT => self.create_desktop_item(fence, false),
            CMD_FENCE_NEW_TIMELINE => self.new_timeline_near(x, y, Some(host)),
            CMD_FENCE_NEW_PLANNING => self.new_spm_panel_near(SpmPanel::Planning, x, y, Some(host)),
            CMD_FENCE_NEW_BOARD => self.new_spm_panel_near(SpmPanel::Board, x, y, Some(host)),
            CMD_FENCE_NEW_CI_WATCH => self.new_spm_panel_near(SpmPanel::CiCd, x, y, Some(host)),
            CMD_FENCE_NEW_ISSUES => self.new_spm_panel_near(SpmPanel::Issues, x, y, Some(host)),
            CMD_FENCE_NEW_CLOCK => {
                self.new_widget_near(pecofence_plugin_widgets::CLOCK_PROVIDER, x, y, Some(host))
            }
            CMD_FENCE_NEW_CALENDAR => self.new_widget_near(
                pecofence_plugin_widgets::CALENDAR_PROVIDER,
                x,
                y,
                Some(host),
            ),
            CMD_FENCE_NEW_NOTE => {
                self.new_widget_near(pecofence_plugin_widgets::NOTE_PROVIDER, x, y, Some(host))
            }
            CMD_FENCE_NEW_TODO => {
                self.new_widget_near(pecofence_plugin_widgets::TODO_PROVIDER, x, y, Some(host))
            }
            CMD_FENCE_NEW_DIAGRAM => {
                self.new_widget_near(pecofence_plugin_widgets::DIAGRAM_PROVIDER, x, y, Some(host))
            }
            CMD_FENCE_ICON_32 | CMD_FENCE_ICON_48 | CMD_FENCE_ICON_64 | CMD_FENCE_ICON_96 => {
                let size = match cmd {
                    CMD_FENCE_ICON_32 => 32,
                    CMD_FENCE_ICON_64 => 64,
                    CMD_FENCE_ICON_96 => 96,
                    _ => 48,
                };
                self.apply_icon_size(fence, size);
            }
            CMD_FENCE_VIEW_ICONS | CMD_FENCE_VIEW_LIST | CMD_FENCE_VIEW_DETAILS => {
                let layout = match cmd {
                    CMD_FENCE_VIEW_LIST => ViewLayout::List,
                    CMD_FENCE_VIEW_DETAILS => ViewLayout::Details,
                    _ => ViewLayout::Icons,
                };
                self.state.set_layout(fence, layout);
                if let Some(w) = self.window_for(fence)
                    && w.active_fence() == fence
                {
                    if let Some(view) = w.files() {
                        view.set_layout(layout);
                    };
                }
                self.apply_column_snap(host);
                self.apply_auto_height(host);
                self.schedule_save();
            }
            CMD_FENCE_SORT_MANUAL
            | CMD_FENCE_SORT_NAME
            | CMD_FENCE_SORT_TYPE
            | CMD_FENCE_SORT_DATE
            | CMD_FENCE_SORT_SIZE
            | CMD_FENCE_SORT_OPENED => {
                let sort = match cmd {
                    CMD_FENCE_SORT_NAME => SortMode::Name,
                    CMD_FENCE_SORT_TYPE => SortMode::Type,
                    CMD_FENCE_SORT_DATE => SortMode::Date,
                    CMD_FENCE_SORT_SIZE => SortMode::Size,
                    CMD_FENCE_SORT_OPENED => SortMode::OpenCount,
                    _ => SortMode::Manual,
                };
                self.state.set_sort(fence, sort);
                self.refresh_fence(fence);
                self.schedule_save();
            }
            CMD_FENCE_SORT_REVERSE => {
                self.state.set_reverse(fence, !f.view.reverse);
                self.refresh_fence(fence);
                self.schedule_save();
            }
            CMD_FENCE_SORT_GROUP => {
                // Also switches to date order; `refresh_fence` pushes the flag, the re-sorted
                // items, the header indicator and the auto height.
                self.state.set_group_by_date(fence, !f.view.group_by_date);
                self.refresh_fence(fence);
                self.schedule_save();
            }
            CMD_FENCE_LOCK => self.set_fence_locked(host, !locked),
            CMD_FENCE_OPTIONS => self.open_fence_options(fence),
            CMD_FENCE_NEW => {
                let rect = self.place_new_fence(3, 200.0, x, y, Some(host));
                self.create_fence_at(rect);
            }
            CMD_FENCE_DELETE => self.delete_fence(fence),
            _ => return,
        }
        // An open settings page shows these fences too; keep it current.
        self.push_settings_state();
    }

    /// Right-click on the Details header: Explorer's column chooser (名称 always on).
    pub(in crate::app) fn show_header_menu(&mut self, fence: FenceId, x: i32, y: i32) {
        let Some(f) = self.state.fence(fence) else {
            return;
        };
        let mut vis = f.view.columns_visible.unwrap_or([true; 3]);
        let menu = PopupMenu::new();
        menu.check_item(CMD_COL_NAME, pecofence_core::i18n::text("名称"), true, true)
            .check_item(
                CMD_COL_DATE,
                pecofence_core::i18n::text("修改日期"),
                vis[0],
                false,
            )
            .check_item(
                CMD_COL_TYPE,
                pecofence_core::i18n::text("类型"),
                vis[1],
                false,
            )
            .check_item(
                CMD_COL_SIZE,
                pecofence_core::i18n::text("大小"),
                vis[2],
                false,
            )
            .separator()
            .item(
                CMD_COL_RESET_WIDTHS,
                pecofence_core::i18n::text("恢复默认列宽"),
                false,
                false,
            );
        let owner = self
            .window_for(fence)
            .map(|w| w.hwnd())
            .unwrap_or(self.control.hwnd());
        let cmd = menu.show_context(owner, x, y);
        match cmd {
            CMD_COL_DATE | CMD_COL_TYPE | CMD_COL_SIZE => {
                let i = match cmd {
                    CMD_COL_DATE => 0,
                    CMD_COL_TYPE => 1,
                    _ => 2,
                };
                vis[i] = !vis[i];
                self.state.set_columns_visible(fence, vis);
                if let Some(w) = self.window_for(fence)
                    && w.active_fence() == fence
                {
                    if let Some(view) = w.files() {
                        view.set_columns_visible(vis);
                    };
                }
                if let Some(host) = self.state.host_of(fence) {
                    self.apply_auto_height(host);
                }
                self.schedule_save();
            }
            CMD_COL_RESET_WIDTHS => {
                let widths = crate::layout::DetailColumns::DEFAULT_WIDTHS;
                self.state.set_column_widths(fence, widths);
                if let Some(w) = self.window_for(fence)
                    && w.active_fence() == fence
                {
                    if let Some(view) = w.files() {
                        view.set_column_widths(widths);
                    };
                }
                self.schedule_save();
            }
            _ => {}
        }
    }
}

//! Popup menus (tray, item, fence, details header) and their command ids.

use crate::commands::{DesktopCmd, FenceCmd, SettingsCmd};

use super::*;

fn new_widget_menu(clock: u32, calendar: u32, note: u32, todo: u32, diagram: u32) -> PopupMenu {
    let menu = PopupMenu::new();
    for (id, title) in [
        (note, "便笺"),
        (todo, "待办清单"),
        (diagram, "图表"),
        (clock, "数字时钟"),
        (calendar, "日历"),
    ] {
        menu.item(id, pecofence_core::i18n::text(title), false, false);
    }
    menu
}

// Tray / menu command ids.
const CMD_TOGGLE_FENCES: u32 = 1;
const CMD_NEW_FENCE: u32 = 2;
const CMD_APPLY_RULES: u32 = 3;
const CMD_SETTINGS: u32 = 4;
const CMD_REPAIR_ICONS: u32 = 5;
const CMD_HIDE_ICONS_AGAIN: u32 = 6;
const CMD_EXIT: u32 = 9;
const CMD_PEEK: u32 = 10;
const CMD_NEW_CLOCK: u32 = 11;
const CMD_NEW_CALENDAR: u32 = 12;
const CMD_NEW_TODO: u32 = 13;
const CMD_NEW_NOTE: u32 = 14;
const CMD_NEW_DIAGRAM: u32 = 15;
const CMD_ITEM_OPEN: u32 = 100;
const CMD_ITEM_LOCATION: u32 = 101;
const CMD_ITEM_PROPERTIES: u32 = 102;
const CMD_ITEM_TO_INBOX: u32 = 103;
const CMD_ITEM_PORTAL: u32 = 104;
const CMD_ITEM_RENAME: u32 = 105;
const CMD_ITEM_DELETE: u32 = 106;
const CMD_ITEM_SHELL: u32 = 107;
const CMD_FENCE_OPEN_FOLDER: u32 = 332;
const CMD_FENCE_NEW_CLOCK: u32 = 334;
const CMD_FENCE_NEW_CALENDAR: u32 = 335;
const CMD_FENCE_NEW_TODO: u32 = 336;
const CMD_FENCE_NEW_NOTE: u32 = 337;
const CMD_FENCE_NEW_DIAGRAM: u32 = 338;
const CMD_ITEM_MOVE_BASE: u32 = 200;
const CMD_NEW_FENCE_HERE: u32 = 250;
/// Command-id range handed to Explorer's `IContextMenu`.
const CMD_SHELL_FIRST: u32 = 0x1000;
const CMD_SHELL_LAST: u32 = 0x6FFF;
const CMD_FENCE_ROLL: u32 = 301;
const CMD_FENCE_DELETE: u32 = 303;
const CMD_FENCE_NEW: u32 = 304;
const CMD_FENCE_ICON_32: u32 = 310;
const CMD_FENCE_ICON_48: u32 = 311;
const CMD_FENCE_ICON_64: u32 = 312;
const CMD_FENCE_ICON_96: u32 = 313;
const CMD_FENCE_SORT_MANUAL: u32 = 320;
const CMD_FENCE_SORT_NAME: u32 = 321;
const CMD_FENCE_SORT_TYPE: u32 = 322;
const CMD_FENCE_SORT_DATE: u32 = 323;
const CMD_FENCE_SORT_SIZE: u32 = 324;
const CMD_FENCE_SORT_REVERSE: u32 = 325;
const CMD_FENCE_SORT_OPENED: u32 = 326;
const CMD_FENCE_SORT_GROUP: u32 = 327;
const CMD_FENCE_LOCK: u32 = 333;
const CMD_FENCE_RENAME: u32 = 330;
/// Opens the settings page on the 「栅栏」 tab for this fence.
const CMD_FENCE_OPTIONS: u32 = 331;
const CMD_FENCE_NEW_FOLDER: u32 = 340;
const CMD_FENCE_NEW_TEXT: u32 = 341;
const CMD_FENCE_PASTE: u32 = 342;
const CMD_FENCE_VIEW_ICONS: u32 = 360;
const CMD_FENCE_VIEW_LIST: u32 = 361;
const CMD_FENCE_VIEW_DETAILS: u32 = 362;
const CMD_TAB_NEW: u32 = 370;
const CMD_TAB_DETACH: u32 = 371;
const CMD_TAB_MOVE_LEFT: u32 = 372;
const CMD_TAB_MOVE_RIGHT: u32 = 373;
/// Details header context menu (column chooser).
const CMD_COL_NAME: u32 = 420;
const CMD_COL_DATE: u32 = 421;
const CMD_COL_TYPE: u32 = 422;
const CMD_COL_SIZE: u32 = 423;
const CMD_COL_RESET_WIDTHS: u32 = 424;
const CMD_PORTAL_UP: u32 = 380;
const CMD_PORTAL_HOME: u32 = 381;
/// 「建议」 entries: base + index into `fence_suggestions`.
const CMD_FENCE_SUGGEST_BASE: u32 = 390;
const _: () =
    assert!(CMD_FENCE_SUGGEST_BASE + suggestions::MAX_FENCE_SUGGESTIONS as u32 <= CMD_COL_NAME);
/// "合并到 ▸ <host>" entries: base + index into `host_fences()`.
const CMD_TAB_ATTACH_BASE: u32 = 2000;
/// Options a widget adds to its window menu (`PanelInstance::menu_items`), by index.
const CMD_PANEL_ITEM_BASE: u32 = 3000;
const CMD_PAPER_BASE: u32 = 4000;
/// Tray "Closed fences" entries, one per closed fence.
const CMD_REOPEN_BASE: u32 = 5000;
const _: () =
    assert!(CMD_PAPER_BASE >= CMD_PANEL_ITEM_BASE + 1000 && CMD_PAPER_BASE + 4 <= CMD_SHELL_FIRST);
const _: () = assert!(
    CMD_PANEL_ITEM_BASE >= CMD_TAB_ATTACH_BASE + 1000
        && CMD_PANEL_ITEM_BASE + 1000 <= CMD_SHELL_FIRST
);
// The open-ended BASE ranges must not swallow neighbouring fixed ids (a 400 base once
// captured the title-colour / spacing items and merged the fence into a random host).
const _: () = assert!(
    CMD_TAB_ATTACH_BASE > CMD_COL_RESET_WIDTHS && CMD_TAB_ATTACH_BASE + 1000 <= CMD_SHELL_FIRST
);

impl App {
    pub(super) fn show_tray_menu(&mut self, x: i32, y: i32) {
        let hidden = self
            .anchor
            .borrow()
            .as_ref()
            .is_some_and(|a| a.fences_hidden());
        let menu = PopupMenu::new();
        menu.item(
            CMD_TOGGLE_FENCES,
            if hidden {
                pecofence_core::i18n::text("显示所有栅栏")
            } else {
                pecofence_core::i18n::text("隐藏所有栅栏")
            },
            false,
            false,
        )
        .item(
            CMD_NEW_FENCE,
            pecofence_core::i18n::text("新建栅栏"),
            false,
            false,
        )
        .submenu(
            pecofence_core::i18n::text("新建小组件"),
            new_widget_menu(
                CMD_NEW_CLOCK,
                CMD_NEW_CALENDAR,
                CMD_NEW_NOTE,
                CMD_NEW_TODO,
                CMD_NEW_DIAGRAM,
            ),
        )
        .item(
            CMD_PEEK,
            &match self.peek_hotkey {
                Some(h) => pecofence_core::i18n::format(
                    "浮现所有栅栏\t{0}",
                    &[peek_hotkey_label(h).to_string()],
                ),
                None => pecofence_core::i18n::text("浮现所有栅栏").to_string(),
            },
            false,
            false,
        )
        .item(
            CMD_APPLY_RULES,
            pecofence_core::i18n::text("立即应用整理规则"),
            false,
            false,
        );
        // Fences closed with their title-bar Close button come back from here.
        let closed = self.state.closed_fences();
        if !closed.is_empty() {
            let reopen = PopupMenu::new();
            for (i, (_, title)) in closed.iter().enumerate() {
                reopen.item(CMD_REOPEN_BASE + i as u32, title, false, false);
            }
            menu.submenu(pecofence_core::i18n::text("已关闭的栅栏"), reopen);
        }
        // Autostart and monitor swapping live on the settings page; the icon repair stays as
        // the rescue path when the desktop is left without icons.
        menu.separator()
            .item(
                CMD_HIDE_ICONS_AGAIN,
                if pecofence_platform::shell_icons::desktop_icons_hidden() {
                    pecofence_core::i18n::text("恢复显示桌面图标")
                } else {
                    pecofence_core::i18n::text("重新隐藏桌面图标")
                },
                false,
                false,
            )
            .item(
                CMD_REPAIR_ICONS,
                pecofence_core::i18n::text("修复桌面图标（显示真实图标）"),
                false,
                false,
            )
            .item(
                CMD_SETTINGS,
                pecofence_core::i18n::text("设置…"),
                false,
                false,
            )
            .separator()
            .item(
                CMD_EXIT,
                if self.persistence.closing() == Closing::Open {
                    pecofence_core::i18n::text("退出 PecoFence")
                } else {
                    pecofence_core::i18n::text("取消退出")
                },
                false,
                false,
            );
        let cmd = menu.show(self.control.hwnd(), x, y);
        if self.persistence.closing() != Closing::Open && !matches!(cmd, CMD_SETTINGS | CMD_EXIT) {
            return;
        }
        match cmd {
            CMD_TOGGLE_FENCES => self.toggle_all_fences(),
            CMD_PEEK => self.queue.push(Command::Desktop(DesktopCmd::TogglePeek)),
            CMD_NEW_FENCE => self
                .queue
                .push(Command::Fence(FenceCmd::NewFence { x, y: y - 300 })),
            CMD_NEW_CLOCK => {
                self.new_widget_near(pecofence_plugin_widgets::CLOCK_PROVIDER, x, y, None)
            }
            CMD_NEW_CALENDAR => {
                self.new_widget_near(pecofence_plugin_widgets::CALENDAR_PROVIDER, x, y, None)
            }
            CMD_NEW_NOTE => {
                self.new_widget_near(pecofence_plugin_widgets::NOTE_PROVIDER, x, y, None)
            }
            CMD_NEW_TODO => {
                self.new_widget_near(pecofence_plugin_widgets::TODO_PROVIDER, x, y, None)
            }
            CMD_NEW_DIAGRAM => {
                self.new_widget_near(pecofence_plugin_widgets::DIAGRAM_PROVIDER, x, y, None)
            }
            CMD_APPLY_RULES => self.queue.push(Command::Desktop(DesktopCmd::ApplyRulesNow)),
            CMD_REPAIR_ICONS => {
                let _ = self.set_desktop_icons_hidden(false);
            }
            CMD_HIDE_ICONS_AGAIN => {
                let _ = self.set_desktop_icons_hidden(
                    !pecofence_platform::shell_icons::desktop_icons_hidden(),
                );
            }
            CMD_SETTINGS => self
                .queue
                .push(Command::Settings(SettingsCmd::OpenSettings)),
            CMD_EXIT => self.queue.push(Command::Desktop(DesktopCmd::Quit)),
            id if (CMD_REOPEN_BASE..CMD_REOPEN_BASE + closed.len() as u32).contains(&id) => {
                self.reopen_fence(closed[(id - CMD_REOPEN_BASE) as usize].0);
            }
            _ => {}
        }
    }

    pub(super) fn show_item_menu(&mut self, fence: FenceId, items: Vec<ItemId>, x: i32, y: i32) {
        if items.is_empty() {
            return;
        }
        let menu = PopupMenu::new();
        // Namespace items (Recycle Bin, ...) have no file: rename and "open location" are off.
        let single_namespace = matches!(
            items[..],
            [only] if self.state.item(only).is_some_and(|it| it.is_namespace())
        );
        menu.item(
            CMD_ITEM_OPEN,
            pecofence_core::i18n::text("打开\tEnter"),
            false,
            false,
        )
        .item(
            CMD_ITEM_LOCATION,
            pecofence_core::i18n::text("打开文件所在位置"),
            false,
            items.len() != 1 || single_namespace,
        )
        .item(
            CMD_ITEM_RENAME,
            pecofence_core::i18n::text("重命名\tF2"),
            false,
            items.len() != 1 || single_namespace,
        )
        .item(
            CMD_ITEM_DELETE,
            pecofence_core::i18n::text("删除\tDelete"),
            false,
            single_namespace,
        )
        .separator();
        let single_folder: Option<PathBuf> = match items[..] {
            [only] => self
                .state
                .item(only)
                .filter(|it| it.is_folder)
                .and_then(|it| it.key.as_path().map(PathBuf::from)),
            _ => None,
        };
        if single_folder.is_some() {
            menu.item(
                CMD_ITEM_PORTAL,
                pecofence_core::i18n::text("作为栅栏窗口显示（文件夹门户）"),
                false,
                false,
            );
        }
        let inbox = self.state.inbox_id();
        let from_portal = self.state.portal_path(fence).is_some();
        let move_menu = PopupMenu::new();
        let destinations: Vec<_> = self
            .state
            .fences()
            .into_iter()
            .filter(|f| f.content.is_files())
            .collect();
        let targets: Vec<(u32, String, bool)> = destinations
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let label = if self.state.portal_path(f.id).is_some() {
                    pecofence_core::i18n::format(
                        "{0}（移动文件到该文件夹）",
                        &[self.state.display_title(f).to_string()],
                    )
                } else if from_portal {
                    pecofence_core::i18n::format(
                        "{0}（移动文件到桌面）",
                        std::slice::from_ref(&f.title),
                    )
                } else {
                    f.title.clone()
                };
                (CMD_ITEM_MOVE_BASE + i as u32, label, f.id == fence)
            })
            .collect();
        for (id, title, disabled) in &targets {
            move_menu.item(*id, title, false, *disabled);
        }
        menu.submenu(pecofence_core::i18n::text("移动到栅栏"), move_menu);
        menu.item(
            CMD_ITEM_TO_INBOX,
            if self.state.portal_path(fence).is_some() {
                pecofence_core::i18n::text("移出到桌面（移动文件）")
            } else {
                pecofence_core::i18n::text("移出栅栏（放回「桌面」）")
            },
            false,
            inbox == Some(fence),
        );
        menu.separator();
        let paths = self.shell_paths_of(&items);
        menu.item(
            CMD_ITEM_PROPERTIES,
            pecofence_core::i18n::text("属性\tAlt+Enter"),
            false,
            false,
        );
        menu.item(
            CMD_ITEM_SHELL,
            pecofence_core::i18n::text("Windows 文件操作…"),
            false,
            paths.is_empty(),
        );
        let owner = self
            .window_for(fence)
            .map(|w| w.hwnd())
            .unwrap_or(self.control.hwnd());
        let cmd = menu.show_context(owner, x, y);
        if cmd == CMD_ITEM_SHELL {
            let native = PopupMenu::new();
            let mut sm = match ShellContextMenu::for_paths(
                &paths.iter().map(|p| p.as_path()).collect::<Vec<_>>(),
            ) {
                Ok(menu) => menu,
                Err(error) => {
                    tracing::warn!(%error, "Windows file actions unavailable");
                    return;
                }
            };
            match sm.populate(
                native.handle(),
                0,
                CMD_SHELL_FIRST,
                CMD_SHELL_LAST,
                window::key_down(msg::VK_SHIFT),
            ) {
                Ok(0) => return,
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(%error, "Windows file actions could not be populated");
                    return;
                }
            }
            let cmd = native.show_classic_context(owner, x, y);
            if sm.contains(cmd) {
                let verb = sm.verb(cmd).unwrap_or_default();
                tracing::info!(cmd, %verb, count = paths.len(), "shell verb invoked");
                // Shift at the moment of choosing (Shift+click on 删除 = permanent, as in Explorer).
                let shift = window::key_down(msg::VK_SHIFT);
                if let Err(e) = sm.invoke(cmd, owner, POINT { x, y }, shift) {
                    tracing::warn!(error = %e, %verb, "shell verb failed");
                }
                // Deletions / moves are picked up by the desktop watcher. What a namespace item's
                // verb changes (Empty Recycle Bin, hide This PC) arrives as a shell notification;
                // a resync is requested here too in case that channel is unavailable.
                if items
                    .iter()
                    .any(|id| self.state.item(*id).is_some_and(|it| it.is_namespace()))
                {
                    window::post_message(self.control.hwnd(), WM_APP_FS_CHANGED, 0, 0);
                }
            }
            return;
        }
        match cmd {
            CMD_ITEM_OPEN => {
                for id in &items {
                    self.launch(*id);
                }
            }
            CMD_ITEM_LOCATION => {
                if let Some(item) = items.first().and_then(|id| self.state.item(*id))
                    && !item.is_namespace()
                    && let Some(p) = item.key.as_path()
                {
                    let _ = std::process::Command::new("explorer.exe")
                        .arg(format!("/select,{p}"))
                        .spawn();
                }
            }
            CMD_ITEM_PROPERTIES => self.show_properties(fence, &items),
            CMD_ITEM_TO_INBOX => {
                if let Some(inbox) = inbox {
                    self.move_items(&items, inbox);
                }
            }
            CMD_ITEM_PORTAL => {
                if let Some(folder) = single_folder {
                    self.create_portal(folder, x, y, Some(fence));
                }
            }
            CMD_ITEM_RENAME => {
                if let [item] = items[..] {
                    self.begin_item_rename(fence, item);
                }
            }
            CMD_ITEM_DELETE => self.delete_items(fence, &items, window::key_down(msg::VK_SHIFT)),
            c if c >= CMD_ITEM_MOVE_BASE && c < CMD_ITEM_MOVE_BASE + 1000 => {
                let idx = (c - CMD_ITEM_MOVE_BASE) as usize;
                if let Some(target) = destinations.get(idx).map(|f| f.id) {
                    self.move_items(&items, target);
                }
            }
            _ => {}
        }
    }

    /// Fence background / title menu. Frequent actions only; per-fence properties (appearance,
    /// spacing, auto height, quick-hide exclusion, dock, portal flags) live on the settings
    /// page's 「栅栏」 tab, reached through 栅栏选项….
    /// Right-click options the widget shown in `fence` offers, if it is a panel.
    fn panel_menu_items(&mut self, fence: FenceId) -> Vec<pecofence_plugin_api::PanelMenuItem> {
        let Some(pecofence_core::FenceSnapshot {
            content: pecofence_core::FenceContentSpec::Panel { panel },
            ..
        }) = self.state.fence(fence)
        else {
            return Vec::new();
        };
        self.panel_manager
            .borrow_mut()
            .open_panel(&panel)
            .map(|handle| handle.menu_items())
            .unwrap_or_default()
    }

    /// Delivers a chosen widget option like any other panel action.
    fn invoke_panel_menu_item(&mut self, fence: FenceId, action: u64) {
        let Some(pecofence_core::FenceSnapshot {
            content: pecofence_core::FenceContentSpec::Panel { panel },
            ..
        }) = self.state.fence(fence)
        else {
            return;
        };
        let result = self
            .panel_manager
            .borrow_mut()
            .open_panel(&panel)
            .and_then(|handle| {
                let key = handle.key();
                handle
                    .event(pecofence_plugin_api::PanelEvent::Invoke {
                        action,
                        layout_revision: 0,
                    })
                    .map(|update| (key, update))
            });
        match result {
            Ok((key, update)) => {
                self.pending_panel_updates
                    .entry(key)
                    .or_default()
                    .merge(update);
                self.process_panel_events();
            }
            Err(error) => tracing::warn!(%error, "panel menu action failed"),
        }
    }

    pub(super) fn show_fence_menu(&mut self, fence: FenceId, x: i32, y: i32) {
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
            new_widget_menu(
                CMD_FENCE_NEW_CLOCK,
                CMD_FENCE_NEW_CALENDAR,
                CMD_FENCE_NEW_NOTE,
                CMD_FENCE_NEW_TODO,
                CMD_FENCE_NEW_DIAGRAM,
            ),
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
    pub(super) fn show_header_menu(&mut self, fence: FenceId, x: i32, y: i32) {
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

    /// Marquee on the empty desktop: confirm with a one-item menu, then create the fence there.
    pub(super) fn offer_new_fence(&mut self, rect: RECT) {
        // Menus need a visible owner window from this thread; any fence will do.
        let Some(owner) = self.fences.values().map(|w| w.hwnd()).next() else {
            return;
        };
        let pt = window::cursor_pos();
        let menu = PopupMenu::new();
        menu.item(
            CMD_NEW_FENCE_HERE,
            pecofence_core::i18n::text("在此新建栅栏"),
            false,
            false,
        );
        if menu.show_context(owner, pt.x, pt.y) == CMD_NEW_FENCE_HERE {
            self.create_fence_at(rect);
        }
    }
}

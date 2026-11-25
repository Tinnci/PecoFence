//! Popup menus (tray, item, fence, details header) and their command ids.

mod fence_menu;
mod item_menu;

use crate::commands::{DesktopCmd, FenceCmd, SettingsCmd};

use super::*;

/// The 「新建小组件」 submenu: general widgets first, then the SPM workspace views.
struct WidgetIds {
    clock: u32,
    calendar: u32,
    note: u32,
    todo: u32,
    diagram: u32,
    board: u32,
    timeline: u32,
    ci_cd: u32,
    issues: u32,
    planning: u32,
}

fn new_widget_menu(ids: WidgetIds) -> PopupMenu {
    let menu = PopupMenu::new();
    menu.item(0, pecofence_core::i18n::text("通用"), false, true);
    for (id, title) in [
        (ids.note, "便笺"),
        (ids.todo, "待办清单"),
        (ids.diagram, "图表"),
        (ids.clock, "数字时钟"),
        (ids.calendar, "日历"),
    ] {
        menu.item(id, pecofence_core::i18n::text(title), false, false);
    }
    menu.separator();
    menu.item(0, pecofence_core::i18n::text("SPM"), false, true);
    for (id, title) in [
        (ids.board, "交付看板"),
        (ids.timeline, "交付时间线"),
        (ids.ci_cd, "CI/CD"),
        (ids.issues, "我的问题"),
        (ids.planning, "交付计划"),
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
const CMD_NEW_BOARD: u32 = 16;
const CMD_NEW_CI_WATCH: u32 = 17;
const CMD_NEW_TIMELINE: u32 = 18;
/// The SPM section's CI/CD row: reuses the CI/CD fence (reopening or unrolling it) or creates one.
const CMD_OPEN_CI_WATCH: u32 = 19;
const CMD_NEW_ISSUES: u32 = 20;
const CMD_NEW_PLANNING: u32 = 21;
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
const CMD_FENCE_NEW_BOARD: u32 = 343;
const CMD_FENCE_NEW_CI_WATCH: u32 = 344;
const CMD_FENCE_NEW_TIMELINE: u32 = 345;
const CMD_FENCE_NEW_ISSUES: u32 = 346;
const CMD_FENCE_NEW_PLANNING: u32 = 347;
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
        menu.item(0, pecofence_core::i18n::text("SPM"), false, true)
            .item(CMD_OPEN_CI_WATCH, &self.ci_tray_label(), false, false)
            .separator();
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
            new_widget_menu(WidgetIds {
                clock: CMD_NEW_CLOCK,
                calendar: CMD_NEW_CALENDAR,
                note: CMD_NEW_NOTE,
                todo: CMD_NEW_TODO,
                diagram: CMD_NEW_DIAGRAM,
                board: CMD_NEW_BOARD,
                timeline: CMD_NEW_TIMELINE,
                ci_cd: CMD_NEW_CI_WATCH,
                issues: CMD_NEW_ISSUES,
                planning: CMD_NEW_PLANNING,
            }),
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
            CMD_OPEN_CI_WATCH => self.open_ci_watch_now(Default::default(), false),
            CMD_NEW_PLANNING => self.new_spm_panel_near(SpmPanel::Planning, x, y, None),
            CMD_NEW_BOARD => self.new_spm_panel_near(SpmPanel::Board, x, y, None),
            CMD_NEW_TIMELINE => self.new_timeline_near(x, y, None),
            CMD_NEW_CI_WATCH => self.new_spm_panel_near(SpmPanel::CiCd, x, y, None),
            CMD_NEW_ISSUES => self.new_spm_panel_near(SpmPanel::Issues, x, y, None),
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

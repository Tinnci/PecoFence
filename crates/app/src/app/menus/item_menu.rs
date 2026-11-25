use super::*;

impl App {
    pub(in crate::app) fn show_item_menu(
        &mut self,
        fence: FenceId,
        items: Vec<ItemId>,
        x: i32,
        y: i32,
    ) {
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
}

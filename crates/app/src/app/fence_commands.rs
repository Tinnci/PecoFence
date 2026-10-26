//! Commands owned by the fence feature.
use super::*;
use crate::commands::FenceCmd;

impl App {
    pub(super) fn handle_fence(&mut self, cmd: FenceCmd) {
        match cmd {
            FenceCmd::FenceBoundsChanged { fence, rect }
            | FenceCmd::FenceRollFinished { fence, rect } => {
                let roll_finished = matches!(cmd, FenceCmd::FenceRollFinished { .. });
                let (rolled, expanded) = self
                    .fences
                    .get(&fence)
                    .map(|w| (w.is_rolled(), w.expanded_height_px()))
                    .unwrap_or((false, rect.bottom - rect.top));
                self.state.set_fence_bounds(fence, rect, rolled, expanded);
                // Moving or resizing a maximized fence leaves the maximized state, as on
                // Windows: Maximize shows again and Restore has nothing to return to.
                if !roll_finished
                    && self
                        .state
                        .container(fence)
                        .is_some_and(|c| c.restore.is_some())
                    && self
                        .work_area_at((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2)
                        .is_some_and(|wa| {
                            (wa.left, wa.top, wa.right, wa.bottom)
                                != (rect.left, rect.top, rect.right, rect.bottom)
                        })
                {
                    if let Some(c) = self.state.container_mut(fence) {
                        c.restore = None;
                    }
                    self.state.mark_dirty();
                    if let Some(w) = self.fences.get(&fence) {
                        w.set_maximized(false);
                    }
                }
                // A resized widget keeps the span it was snapped to.
                if !roll_finished
                    && !rolled
                    && let Some(grid) = self.widget_grid(fence)
                {
                    let scale = self.fences.get(&fence).map_or(1.0, |w| {
                        monitors::dpi_for_window(w.hwnd()).max(96) as f32 / 96.0
                    });
                    let span = grid.rules.span_for_size(
                        (rect.right - rect.left) as f32 / scale,
                        (rect.bottom - rect.top) as f32 / scale,
                        grid.cell,
                        self.ctx.behavior.text_scale.get(),
                    );
                    if let Some(container) = self.state.container_mut(fence) {
                        container.span = Some(pecofence_core::WidgetSpan {
                            columns: span.columns,
                            rows: span.rows,
                        });
                    }
                    self.apply_widget_grid(fence);
                }
                self.schedule_save();
                self.apply_auto_height(fence);
            }
            FenceCmd::ToggleRollUp(fence) => self.toggle_roll(fence),
            FenceCmd::CommitExpanded(fence) => {
                if let Some(w) = self.fences.get(&fence) {
                    w.commit_expanded();
                }
                if let Some(f) = self.state.container_mut(fence) {
                    f.rolled_up = false;
                }
                self.state.mark_dirty();
                self.schedule_save();
                // A committed peek keeps whatever height the peek had; apply the auto-height
                // rule like every other path that expands a fence.
                self.apply_auto_height(fence);
            }
            FenceCmd::LaunchItem(item) => self.launch(item),
            FenceCmd::LaunchDrop { item, name, opened } => {
                self.launch_drop_finished(item, &name, opened)
            }
            FenceCmd::MoveItems { items, to } => self.move_items(&items, to),
            FenceCmd::ReorderItems {
                fence,
                items,
                index,
            } => {
                if self.state.reorder_items(fence, &items, index) {
                    self.refresh_fence(fence);
                    self.schedule_save();
                }
            }
            FenceCmd::DropIntoFolder {
                paths,
                folder,
                fence,
                mode,
            } => self.transfer_files_into_folder(paths, folder, fence, mode),
            FenceCmd::PortalEnter { fence, path } => self.portal_enter(fence, &path),
            FenceCmd::PortalUp(fence) => self.portal_up(fence),
            FenceCmd::SwitchTab { host, tab } => self.switch_tab(host, tab),
            FenceCmd::SetColumnWidths { fence, widths } => {
                self.state.set_column_widths(fence, widths);
                self.schedule_save();
            }
            FenceCmd::MergeHint { target, x } => {
                for w in self.fences.values() {
                    w.set_merge_hint(!target.0.is_null() && w.hwnd() == target, x);
                }
            }
            FenceCmd::MergeFence { fence, into, x } => {
                // The slot the gap sat at (read before the hints — and the gap — are cleared).
                let host = self.fence_of_hwnd(into);
                let index = host
                    .and_then(|h| self.fences.get(&h))
                    .map(|w| w.merge_slot_at(x));
                for w in self.fences.values() {
                    w.set_merge_hint(false, 0);
                }
                if let Some(host) = host
                    && host != fence
                {
                    if self.state.container(fence).is_none() || self.state.container(host).is_none()
                    {
                        return;
                    }
                    let tabs = self.state.tabs_of(fence);
                    let Some(active) = self.state.active_tab_of(fence) else {
                        return;
                    };
                    let start = index.unwrap_or(self.state.tabs_of(host).len());
                    for (slot, content) in (start..).zip(tabs) {
                        if let Err(error) = self.state.attach_tab(content, host) {
                            self.settings_error(&error.to_string());
                            break;
                        }
                        // Land where the insertion gap was, preserving the source tab order.
                        if let Err(error) = self.state.reorder_tab(host, content, slot) {
                            self.settings_error(&error.to_string());
                            break;
                        }
                    }
                    if self
                        .state
                        .fence(active)
                        .is_some_and(|f| f.container_id == host)
                    {
                        let _ = self.state.set_active_tab(host, active);
                    }
                    self.resync_windows();
                    self.apply_fence_view_with_snap(host, false);
                    if let Some(w) = self.fences.get(&host) {
                        self.queue
                            .push(Command::Fence(FenceCmd::RaiseFence(w.hwnd())));
                    }
                    self.schedule_save();
                }
            }
            FenceCmd::DetachTab {
                tab,
                x,
                y,
                from_drag,
            } => self.detach_tab(tab, x, y, from_drag),
            FenceCmd::CancelDetach { change } => self.cancel_tab_detach(*change),
            FenceCmd::ReorderTab { host, tab, to } => self.reorder_tab(host, tab, to),
            FenceCmd::MoveItemsToInbox { items } => {
                // Dropped on the bare desktop: back to the inbox, then straight through the
                // rules — the same routing a new desktop item gets, now instead of whenever the
                // watcher next reconciles (which used to leave the icon in the rolled-up inbox
                // for a second or two).
                if let Some(inbox) = self.state.inbox_id() {
                    self.move_items(&items, inbox);
                    let entries = shell::enumerate_desktop();
                    let touched = self.state.apply_rules_to(&items, &entries);
                    tracing::debug!(
                        items = items.len(),
                        refiled = ?touched,
                        "dropped on the desktop: inbox, then rules"
                    );
                    for id in touched {
                        self.refresh_fence(id);
                    }
                    self.schedule_save();
                }
            }
            FenceCmd::ExternalDrop { paths, to, mode } if self.state.portal_path(to).is_some() => {
                self.transfer_files_into_portal(paths, to, mode);
            }
            FenceCmd::ExternalDrop {
                paths,
                to,
                mode: TransferMode::Link,
            } => {
                // Create shortcut: the .lnk lands on the desktop and goes straight into `to`
                // (a PendingRoute bypasses the rules), the originals stay where they are.
                if let Some(desktop) = shell::user_desktop() {
                    let made = self.create_shortcuts_in(&paths, &desktop);
                    for path in made {
                        self.pending_routes.push(PendingRoute {
                            fence: to,
                            path,
                            since: Instant::now(),
                            rename: false,
                        });
                    }
                    let report = self.sync_desktop_if_available("shortcut drop");
                    self.route_pending_creation();
                    if report.changed() {
                        self.refresh_all();
                        self.schedule_save();
                    }
                }
            }
            FenceCmd::ExternalDrop { paths, to, mode } => {
                let copy = mode == TransferMode::Copy;
                let desktop_dirs: Vec<PathBuf> = [shell::user_desktop(), shell::public_desktop()]
                    .into_iter()
                    .flatten()
                    .collect();
                let mut ids = Vec::new();
                let mut foreign = Vec::new();
                for p in paths {
                    if let Some(id) = self.state.item_by_path(&p) {
                        ids.push(id);
                    } else if p
                        .parent()
                        .is_some_and(|parent| desktop_dirs.iter().any(|d| d == parent))
                    {
                        // On the desktop but not yet synced: sync then retry.
                        let entries = shell::enumerate_desktop();
                        self.state.sync_desktop(&entries);
                        if let Some(id) = self.state.item_by_path(&p) {
                            ids.push(id);
                        }
                    } else {
                        foreign.push(p);
                    }
                }
                if !foreign.is_empty() {
                    // Like Fences: dropping a file from elsewhere onto a fence moves it to the
                    // desktop and puts it in that fence (a PendingRoute bypasses the rules).
                    if let Some(desktop) = shell::user_desktop() {
                        let routed: Vec<PathBuf> = foreign
                            .iter()
                            .filter_map(|p| p.file_name().map(|n| desktop.join(n)))
                            .collect();
                        for path in &routed {
                            self.pending_routes.push(PendingRoute {
                                fence: to,
                                path: path.clone(),
                                since: Instant::now(),
                                rename: false,
                            });
                        }
                        let owner = self.window_for(to).map(|w| w.hwnd());
                        self.start_fileop(FileOp {
                            paths: foreign,
                            dest: desktop,
                            copy,
                            rename_on_collision: false,
                            owner,
                            then: FileOpThen::ToDesktop {
                                routed,
                                copy,
                                what: "dropped files placed on the desktop",
                            },
                        });
                    }
                }
                if !ids.is_empty() {
                    self.move_items(&ids, to);
                }
            }
            FenceCmd::ExternalUrlDrop { url, name, to } => self.create_url_shortcut(url, name, to),
            FenceCmd::FenceMenu { fence, x, y } => self.show_fence_menu(fence, x, y),
            FenceCmd::HeaderMenu { fence, x, y } => self.show_header_menu(fence, x, y),
            FenceCmd::ItemMenu { fence, items, x, y } => self.show_item_menu(fence, items, x, y),
            FenceCmd::ClipboardVerb { fence, items, cut } => {
                self.clipboard_verb(fence, &items, cut)
            }
            FenceCmd::Paste { fence } => self.paste_into(fence),
            FenceCmd::NewFolder { fence } => self.create_desktop_item(fence, true),
            FenceCmd::StepIconSize { fence, larger } => self.step_icon_size(fence, larger),
            FenceCmd::ItemProperties { fence, items } => self.show_properties(fence, &items),
            FenceCmd::RefreshFence(fence) => self.manual_refresh(fence),
            FenceCmd::RenameFence { fence, title } => {
                self.end_item_rename_visuals();
                let title = title.trim();
                if !title.is_empty() {
                    self.state.rename_fence(fence, title);
                    self.refresh_fence(fence);
                    self.schedule_save();
                    self.push_settings_state();
                }
                if let Some(w) = self.window_for(fence) {
                    w.redraw();
                }
            }
            FenceCmd::DeleteFence(fence) => self.delete_fence(fence),
            FenceCmd::NewFence { x, y } => self.new_fence_near(x, y),
            FenceCmd::NewFenceRect(rect) => self.offer_new_fence(rect),
            FenceCmd::RaiseFence(hwnd) => {
                if let Some(a) = self.anchor.borrow_mut().as_mut() {
                    a.raise_fence(hwnd);
                }
            }
            FenceCmd::SortColumn { fence, sort } => {
                // Explorer: clicking the active column flips the direction.
                if let Some(f) = self.state.fence(fence) {
                    let reverse = f.view.sort == sort && !f.view.reverse;
                    self.state.set_sort(fence, sort);
                    self.state.set_reverse(fence, reverse);
                    self.refresh_fence(fence);
                    self.schedule_save();
                }
            }
            FenceCmd::RenameItem { fence, item } => self.begin_item_rename(fence, item),
            FenceCmd::RenameItemCommit { item, name } => {
                self.end_item_rename_visuals();
                self.rename_item_file(item, &name);
            }
            FenceCmd::EndItemRename { commit } => crate::rename::end_active(commit),
            FenceCmd::DeleteItems {
                fence,
                items,
                permanent,
            } => self.delete_items(fence, &items, permanent),
            FenceCmd::CloseFence(host) => self.close_fence(host),
            FenceCmd::ToggleMaximize(host) => self.toggle_maximize(host),
        }
    }
}

//! Commands owned by the desktop feature.
use super::*;
use crate::commands::DesktopCmd;

impl App {
    pub(super) fn handle_desktop(&mut self, cmd: DesktopCmd) {
        match cmd {
            DesktopCmd::DragOutFinished => {
                // The shell performs the file operation itself, often asynchronously: what is
                // not on the desktop yet arrives through the folder watcher a moment later.
                let report = self.sync_desktop_if_available("drag out");
                self.route_pending_creation();
                self.refresh_portals();
                if report.changed() {
                    tracing::info!(?report, "desktop resynced right after the drag-out");
                    self.refresh_all();
                    self.schedule_save();
                }
            }
            DesktopCmd::DeviceLost => self.recover_device(),
            DesktopCmd::ToggleAllFences => self.toggle_all_fences(),
            DesktopCmd::TogglePeek => self.toggle_peek(),
            DesktopCmd::EndPeek => self.end_peek(),
            DesktopCmd::ApplyRulesNow => {
                let entries = shell::enumerate_desktop();
                let moved = self.state.apply_rules_all(&entries);
                self.refresh_all();
                self.schedule_save();
                if let Some(t) = &self.tray {
                    t.show_info(
                        "PecoFence",
                        &pecofence_core::i18n::format(
                            "已按规则整理 {0} 个项目。",
                            &[moved.to_string()],
                        ),
                        false,
                    );
                }
            }
            DesktopCmd::ShellAction(command) => {
                // The private window can receive messages from other processes. Keep an
                // out-of-range coordinate from reaching placement arithmetic.
                let (x, y) = self
                    .work_area_at(command.x, command.y)
                    .map(|work| {
                        (
                            command.x.clamp(work.left, work.right.saturating_sub(1)),
                            command.y.clamp(work.top, work.bottom.saturating_sub(1)),
                        )
                    })
                    .unwrap_or((0, 0));
                match command.action {
                    ShellAction::NewFence => self.new_fence_near(x, y),
                    ShellAction::NewTodo => {
                        self.new_widget_near(pecofence_plugin_widgets::TODO_PROVIDER, x, y, None)
                    }
                    ShellAction::NewNote => {
                        self.new_widget_near(pecofence_plugin_widgets::NOTE_PROVIDER, x, y, None)
                    }
                    ShellAction::OpenSettings => self.open_settings(),
                }
            }
            DesktopCmd::RedrawAll => {
                // Also the rename edit's cancel path: the edited label may unfold again.
                self.end_item_rename_visuals();
                for w in self.fences.values() {
                    w.redraw();
                }
            }
            DesktopCmd::Quit => {
                if self.persistence.closing() == Closing::Open {
                    if matches!(
                        crate::rename::active_target(),
                        Some(crate::rename::RenameTarget::PanelInput { .. })
                    ) {
                        // The native editor still owns unsent text. Deliver its
                        // commit before closing the document mutation gate.
                        crate::rename::end_active(true);
                        // Provider submission can enqueue PersistConfig in a second
                        // drain pass. Re-enter after those commands, not beside them.
                        window::post_message(self.control.hwnd(), WM_APP_UPDATE_QUIT, 0, 0);
                        return;
                    }
                    window::kill_timer(self.control.hwnd(), TIMER_SAVE);
                    crate::lifecycle::set_closing(true);
                    self.persistence.begin_close();
                    self.push_settings_state();
                    self.pump_persistence();
                } else {
                    self.cancel_document_close();
                }
            }
            #[cfg(debug_assertions)]
            DesktopCmd::WorkspaceSpikeLayout => self.spike_layout(),
            DesktopCmd::FadeOutDone(hwnd) => {
                // Dropping the window destroys it (RevokeDragDrop + DestroyWindow).
                self.dying.retain(|w| w.hwnd() != hwnd);
            }
        }
    }
}

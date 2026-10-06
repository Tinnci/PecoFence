//! UI integration of the window-free save coordinator. No filesystem writes happen here.
use super::*;
use crate::persistence::WriteResult;

impl App {
    pub(super) fn mutations_allowed(&self) -> bool {
        self.state.save_allowed && self.persistence.closing() == Closing::Open
    }

    pub(super) fn writer_busy_or_dirty(&self) -> bool {
        self.persistence.busy() || (self.state.save_allowed && self.state.is_dirty())
    }

    pub(super) fn request_save(&mut self) {
        self.persistence.request();
        self.pump_persistence();
        self.push_settings_state();
    }

    pub(super) fn cancel_document_close(&mut self) {
        if !matches!(
            self.persistence.closing(),
            Closing::Waiting | Closing::Failed
        ) {
            return;
        }
        self.persistence.keep_running();
        self.sync_fence_windows();
        window::post_message(self.control.hwnd(), WM_APP_FS_CHANGED, 0, 0);
        self.push_settings_state();
    }

    fn receive_save(&mut self, result: WriteResult) {
        if !self.persistence.accept(&result) {
            return;
        }
        let stamp = result.ticket.stamp;
        let accepted = self.state.apply_save_result(result);
        if accepted.is_none() {
            // The old write is settled, but the new activation still needs its own commit.
            if self.persistence.closing() == Closing::Failed {
                self.persistence.begin_close();
            } else {
                self.persistence.request();
            }
        } else if accepted == Some(true) {
            if self.save_notice.as_ref().is_some_and(|(wanted, _)| {
                wanted.workspace == stamp.workspace && wanted.revision <= stamp.revision
            }) {
                let (_, notice) = self.save_notice.take().unwrap();
                self.settings_toast(&notice);
            }
            if let Some(issue) = self.state.persistence_issue.clone() {
                self.settings_error(&issue);
            }
        } else if let Some(issue) = self.state.persistence_issue.clone() {
            self.settings_error(&issue);
        }
        self.push_settings_state();
    }

    pub(super) fn pump_persistence(&mut self) {
        while let Some(result) = self.writer.poll() {
            self.receive_save(result);
        }
        if self.persistence.ready() {
            if let Some(plan) = self.state.prepare_save() {
                if let Some(request) = self.persistence.start(plan) {
                    let ticket = request.ticket;
                    if let Err(error) = self.writer.submit(request) {
                        self.receive_save(WriteResult {
                            ticket,
                            outcome: Err(error),
                        });
                    }
                }
            } else {
                self.persistence.no_work();
            }
        }
        if self.persistence.closing() == Closing::Failed {
            use pecofence_platform::dialogs::{self, SaveFailureChoice};
            let owner = self
                .settings
                .as_ref()
                .map_or(self.control.hwnd(), |view| view.hwnd());
            let detail = self
                .state
                .persistence_issue
                .as_deref()
                .unwrap_or("unknown persistence failure");
            let message = format!(
                "{}\n\n{detail}",
                pecofence_core::i18n::text(
                    "保存失败。取消：返回工作区；重试：再次保存；继续：放弃未保存的修改并退出。"
                )
            );
            match dialogs::save_failure(owner, "PecoFence", &message) {
                SaveFailureChoice::KeepRunning => self.cancel_document_close(),
                SaveFailureChoice::Retry => self.persistence.begin_close(),
                SaveFailureChoice::Discard => {
                    // No write is in flight when this choice is offered.
                    if self.persistence.discard_failed() {
                        self.finish_document_close();
                        return;
                    }
                }
            }
            self.push_settings_state();
        }
        if self
            .persistence
            .can_close(self.state.save_allowed && self.state.is_dirty())
        {
            self.finish_document_close();
        } else if self.persistence.busy() || self.persistence.closing() == Closing::Waiting {
            window::set_timer(self.control.hwnd(), TIMER_PERSISTENCE, 50);
        } else {
            window::kill_timer(self.control.hwnd(), TIMER_PERSISTENCE);
        }
    }

    fn finish_document_close(&mut self) {
        if !self.persistence.finish_close() {
            return;
        }
        window::kill_timer(self.control.hwnd(), TIMER_PERSISTENCE);
        if let Some(anchor) = self.anchor.borrow_mut().as_mut() {
            anchor.restore_desktop_icons();
        }
        window::post_quit(0);
    }
}

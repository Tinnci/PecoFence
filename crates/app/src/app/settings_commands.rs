//! Commands owned by the settings feature.
use super::*;
use crate::commands::SettingsCmd;

impl App {
    pub(super) fn handle_settings(&mut self, cmd: SettingsCmd) {
        match cmd {
            SettingsCmd::OpenSettings => self.open_settings(),
            SettingsCmd::OpenOptionsForFence(fence) => self.open_fence_options(fence),
            SettingsCmd::SettingsClosed { source } => {
                if self.settings.as_ref().is_some_and(|h| h.source() == source) {
                    self.settings = None;
                    self.settings_session.close();
                }
            }
            SettingsCmd::SettingsRequest { source, request } => {
                if self
                    .settings
                    .as_ref()
                    .is_some_and(|host| host.source() == source && host.is_alive())
                {
                    self.on_settings_request(*request);
                }
            }
        }
    }
}

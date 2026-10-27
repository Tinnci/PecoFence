//! Commands owned by the panel feature.
use super::*;
use crate::commands::PanelCmd;

impl App {
    pub(super) fn handle_panel(&mut self, cmd: PanelCmd) {
        match cmd {
            PanelCmd::AppEvent { topic, json } => {
                self.ci_app_event(&topic, &json);
                self.process_panel_events();
            }
            PanelCmd::ConfigChanged { key, bytes } => {
                let active_fence = self.state.fences().iter().find_map(|f| match &f.content {
                    pecofence_core::FenceContentSpec::Panel { panel }
                        if panel.instance_id.as_u128() == key.id =>
                    {
                        Some(f.id)
                    }
                    _ => None,
                });
                if active_fence.is_some() {
                    let outcome = self.panel_manager.borrow_mut().persist_config(key, bytes);
                    match outcome {
                        Ok(config) => {
                            if self.state.update_panel_config(key.id, config) {
                                self.schedule_save();
                            }
                        }
                        Err(error) => tracing::warn!(%error, "panel config update rejected"),
                    }
                }
            }
            PanelCmd::WakeChanged { key } => {
                self.panel_manager.borrow_mut().refresh_wake(key);
            }
            PanelCmd::TextRequested {
                key,
                action,
                initial,
                multiline,
                style,
            } => {
                let hwnd = self.state.fences().iter().find_map(|f| match &f.content {
                    pecofence_core::FenceContentSpec::Panel { panel }
                        if panel.instance_id.as_u128() == key.id =>
                    {
                        self.window_for(f.id)
                            .map(|w| (w.hwnd(), w.title_height_dip()))
                    }
                    _ => None,
                });
                if let Some((hwnd, title_height)) = hwnd {
                    crate::rename::begin_panel_input(
                        hwnd,
                        key,
                        action,
                        initial,
                        self.queue.clone(),
                        self.theme_mode,
                        multiline,
                        title_height,
                        style,
                    );
                }
            }
            PanelCmd::TextCommit { key, action, text } => {
                match self.panel_manager.borrow().submit_text(key, action, text) {
                    Ok(update) => {
                        self.pending_panel_updates
                            .entry(key)
                            .or_default()
                            .merge(update);
                    }
                    Err(error) => tracing::warn!(%error, "panel text input rejected"),
                }
                self.process_panel_events();
            }
        }
    }
}

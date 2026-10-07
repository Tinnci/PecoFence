//! Native Settings application boundary: owned projections, typed decisions,
//! config adoption, import / export / restore and settings diff application.

use super::*;
use crate::settings_ui::{MonitorChoice, SettingsView, SnapshotChoice};
use pecofence_core::settings_protocol::{Action, Admission, Rejection, Request, SettingsCommand};
use std::result::Result;

/// Keep the publication gate balanced even if a command unwinds.
struct SettingsDispatchGuard {
    flag: Rc<Cell<bool>>,
    previous: bool,
}

impl SettingsDispatchGuard {
    fn new(flag: Rc<Cell<bool>>) -> Self {
        let previous = flag.replace(true);
        Self { flag, previous }
    }
}

impl Drop for SettingsDispatchGuard {
    fn drop(&mut self) {
        self.flag.set(self.previous);
    }
}

impl App {
    /// Replaces the configuration wholesale (import / backup) and rebuilds everything.
    fn adopt_config(&mut self, mut cfg: pecofence_core::Config, what: &str) -> Result<(), String> {
        self.end_peek_now();
        let old_settings = self.state.config.settings.clone();
        if !self.state.config.layouts.is_empty() {
            cfg.snapshots.push(pecofence_core::Snapshot {
                id: uuid::Uuid::new_v4(),
                name: pecofence_core::i18n::format("{0}前", &[what.to_string()]),
                ts: pecofence_core::now_unix(),
                layouts: self.state.config.layouts.clone(),
            });
            while cfg.snapshots.len() > pecofence_core::MAX_SNAPSHOTS {
                cfg.snapshots.remove(0);
            }
        }
        self.state.replace_config(cfg)?;
        self.reconcile_adopted_workspace(old_settings);
        self.settings_commit(&pecofence_core::i18n::format("已{0}", &[what.to_string()]));
        Ok(())
    }

    /// Reset, import and recovery reconcile through one path. Capture prior settings
    /// before replacing the document, rather than temporarily restoring an old root.
    fn reconcile_adopted_workspace(&mut self, old_settings: pecofence_core::Settings) {
        self.end_peek_now();
        if let Some(desk) = shell::user_desktop() {
            self.state.migrate_desktop_path(&desk);
        }
        // What the file (plus the desktop-path migration) wants the settings to be.
        let new_settings = self.state.config.settings.clone();
        self.sync_desktop_if_available("config adopted");
        self.relayout_from_state();
        // Re-apply the settings through the diff path so the anchor / Run-key side effects
        // (hide_real_icons, quick_hide.enabled, show_desktop, autostart) and the ctx.behavior
        // cells update exactly as if the user had changed them on the page.
        self.apply_settings_from(old_settings, new_settings, true);
        self.apply_desktop_icons_hidden(self.state.config.settings.hide_real_icons);
        // apply_settings only refreshes on a visual / icon diff; a wholesale swap (per-fence
        // tint, layouts) needs these regardless.
        self.sync_peek_hotkey();
        self.apply_icon_variant();
        self.refresh_visuals(true);
        self.push_settings_state();
    }

    fn export_config(&mut self) -> Result<bool, String> {
        let owner = self.settings.as_ref().and_then(|h| h.hwnd());
        let name = format!(
            "pecofence-{}.json",
            pecofence_core::config_store::today_yyyy_mm_dd()
        );
        let path = pecofence_platform::filedialog::save_json(
            owner,
            pecofence_core::i18n::text("导出 PecoFence 配置"),
            &name,
        )
        .map_err(|e| e.to_string())?;
        let Some(path) = path else {
            return Ok(false);
        };
        pecofence_core::ConfigStore::export_to(&self.state.config, &path)
            .map_err(|e| e.to_string())?;
        self.settings_toast(&pecofence_core::i18n::format(
            "已导出到 {0}",
            &[format!("{}", path.display())],
        ));
        Ok(true)
    }

    fn import_config(&mut self) -> Result<bool, String> {
        let owner = self.settings.as_ref().and_then(|h| h.hwnd());
        let path = pecofence_platform::filedialog::open_json(
            owner,
            pecofence_core::i18n::text("导入 PecoFence 配置"),
        )
        .map_err(|e| e.to_string())?;
        let Some(path) = path else {
            return Ok(false);
        };
        self.restore_from_file(&path, pecofence_core::i18n::text("导入配置"))?;
        Ok(true)
    }

    fn restore_from_file(&mut self, path: &std::path::Path, what: &str) -> Result<(), String> {
        let cfg = pecofence_core::ConfigStore::parse_file(path)?;
        self.adopt_config(cfg, what)
    }

    pub(super) fn open_settings(&mut self) {
        if let Some(h) = &self.settings
            && h.is_alive()
        {
            h.activate();
            return;
        }
        // Component retirement may have run inside a modal message pump before the
        // queued SettingsClosed command is drained. Retire that activation now;
        // its late close/request events cannot affect the replacement.
        self.settings = None;
        self.settings_session.close();
        match SettingsHost::open(
            &self.reactor_context,
            self.theme_mode,
            self.ctx.theme.borrow().liquid_glass,
            self.queue.clone(),
        ) {
            Ok(mut host) => {
                // Caption icon: the same drawn fence glyph as the tray, at 16/32 DIP.
                let scale = host
                    .hwnd()
                    .map(|hwnd| monitors::dpi_for_window(hwnd).max(96) as f32 / 96.0)
                    .unwrap_or(1.0);
                let accent = self.ctx.theme.borrow().accent_rgb8();
                let dark = self.theme_mode == ThemeMode::Dark;
                let small = (16.0 * scale).round() as i32;
                let big = (32.0 * scale).round() as i32;
                host.set_icons(
                    (small, tray_icon_image(small, accent, dark)),
                    (big, tray_icon_image(big, accent, dark)),
                );
                self.settings_session.open(host.source());
                self.settings = Some(host);
                self.push_settings_state();
                tracing::info!("settings: native controls ready");
            }
            Err(e) => {
                tracing::error!(error = %e, "settings window failed");
                if let Some(t) = &self.tray {
                    t.show_info(
                        "PecoFence",
                        &pecofence_core::i18n::format("无法打开设置窗口：{0}", &[e.to_string()]),
                        true,
                    );
                }
            }
        }
    }

    fn settings_view(&self) -> SettingsView {
        let fences = self.state.fences();
        let contents = fences.iter().map(|f| self.content_options(f)).collect();
        let mem_mb = pecofence_platform::memstats::MemoryStats::current()
            .ok()
            .map(|m| m.private_working_set as f64 / (1024.0 * 1024.0));
        let snapshots = self
            .state
            .config
            .snapshots
            .iter()
            .map(|s| SnapshotChoice {
                id: s.id,
                name: s.name.clone(),
                date: pecofence_platform::fileinfo::format_local_datetime(s.ts),
                content_count: s.layouts.iter().map(|l| l.contents.len()).sum(),
            })
            .collect();
        let monitors = self
            .monitor_labels()
            .into_iter()
            .map(|(id, label)| MonitorChoice { id, label })
            .collect();
        SettingsView {
            stamp: self.state.document_stamp(),
            settings: self.state.config.settings.clone(),
            writable: self.mutations_allowed(),
            saving: self.persistence.busy(),
            closing: self.persistence.closing() != Closing::Open,
            dirty: self.state.is_dirty(),
            committed_revision: self.state.committed_revision(),
            save_issue: self.state.persistence_issue.clone(),
            load_issue: self.state.load_issue.clone(),
            recovered_from: self.state.recovered_from.clone(),
            desktop_icons_hidden: Some(pecofence_platform::shell_icons::desktop_icons_hidden()),
            rules: self.state.config.rules.clone(),
            contents,
            snapshots,
            backups: self.state.backup_files(),
            monitors,
            version: env!("CARGO_PKG_VERSION"),
            config_path: self.state.config_path().to_path_buf(),
            memory_mb: mem_mb,
            item_count: self.state.workspace_item_count(),
        }
    }

    pub(super) fn push_settings_state(&self) {
        if self.settings_dispatching.get() {
            return;
        }
        if let (Some(host), Some(client)) = (&self.settings, self.settings_session.client()) {
            host.update(client, self.settings_view());
        }
    }

    /// File watcher/navigation updates must not rebuild controls while the user is
    /// editing a name or has a select popup open.
    pub(super) fn push_workspace_summary(&self) {
        if self.settings_dispatching.get() {
            return;
        }
        if let Some(h) = &self.settings {
            h.summary(
                self.state.document_stamp(),
                self.state.workspace_item_count(),
            );
        }
    }

    /// Call after mutating `self.state.config.settings` anywhere other than apply_settings:
    /// records a document revision and keeps the read-only projection in sync.
    pub(super) fn settings_mutated(&mut self) {
        self.state.mark_dirty();
        self.schedule_save();
        self.push_settings_state();
    }

    pub(super) fn settings_toast(&self, text: &str) {
        if let Some(h) = &self.settings {
            h.notify(text, false);
        }
    }

    /// A queued write is not a saved workspace. Report the actual primary commit.
    fn settings_commit(&mut self, success: &str) {
        if !self.state.is_dirty() && self.state.committed_revision().is_some() {
            self.settings_toast(success);
        } else {
            self.save_notice = Some((self.state.document_stamp(), success.to_string()));
            self.request_save();
        }
        self.push_settings_state();
    }

    pub(super) fn settings_error(&self, text: &str) {
        if let Some(h) = &self.settings {
            h.notify(text, true);
        }
    }

    pub(super) fn on_settings_request(&mut self, request: Request) {
        let old_language = pecofence_core::i18n::language();
        let result = match self.settings_session.admit(
            &request,
            self.state.document_stamp(),
            self.mutations_allowed(),
        ) {
            Admission::Replay(receipt) => {
                if let Some(host) = &self.settings {
                    host.decision(receipt);
                }
                self.push_settings_state();
                return;
            }
            Admission::Reject(reason) => Err(reason),
            Admission::Apply => {
                let _dispatch = SettingsDispatchGuard::new(self.settings_dispatching.clone());
                self.apply_settings_command(request.command.clone())
            }
        };
        let (rejected, cancelled) = match result {
            Ok(cancelled) => (None, cancelled),
            Err(reason) => (Some(reason), false),
        };
        let receipt =
            self.settings_session
                .record(request, self.state.document_stamp(), rejected, cancelled);
        if let Some(host) = &self.settings {
            host.decision(receipt);
        }
        self.push_settings_state();
        if old_language != pecofence_core::i18n::language()
            && let Some(host) = &self.settings
        {
            host.update_language();
        }
    }

    /// The same typed use cases serve native controls and opt-in test intents.
    /// Success means the command was applied, not that a deferred write already committed.
    pub(super) fn apply_settings_command(
        &mut self,
        command: SettingsCommand,
    ) -> Result<bool, Rejection> {
        if self.persistence.closing() != Closing::Open
            && !matches!(
                &command,
                SettingsCommand::Action {
                    action: Action::CancelClose
                }
            )
        {
            return Err(Rejection::ReadOnly);
        }
        if !self.state.save_allowed && !command.permitted_read_only() {
            return Err(Rejection::ReadOnly);
        }
        match command {
            SettingsCommand::SetSetting { change } => {
                if matches!(
                    change,
                    pecofence_core::settings_protocol::SettingChange::Autostart(_)
                ) && pecofence_platform::process::is_packaged()
                {
                    // Windows owns the MSIX startup task. A handoff to Windows
                    // Settings is not proof that the requested switch changed.
                    shell::shell_execute(
                        std::path::Path::new("ms-settings:startupapps"),
                        None,
                        None,
                    )
                    .map_err(|e| Rejection::Backend(e.to_string()))?;
                    return Err(Rejection::Backend(
                        pecofence_core::i18n::text(
                            "MSIX 启动由 Windows 管理，请在“启动应用”中更改。",
                        )
                        .into(),
                    ));
                }
                let mut candidate = self.state.config.clone();
                change
                    .apply(&mut candidate.settings)
                    .map_err(Rejection::Invalid)?;
                candidate.validate().map_err(Rejection::Invalid)?;
                let expected = candidate.settings.clone();
                self.apply_settings(candidate.settings);
                if self.state.config.settings != expected {
                    return Err(Rejection::Backend("setting could not be applied".into()));
                }
            }
            SettingsCommand::SetContent {
                content_id,
                container_id,
                change,
            } => {
                self.apply_content_change(content_id, container_id, change)
                    .map_err(Rejection::Invalid)?;
            }
            SettingsCommand::SetContainer {
                content_id,
                container_id,
                change,
            } => {
                self.apply_container_change(content_id, container_id, change)
                    .map_err(Rejection::Invalid)?;
            }
            SettingsCommand::Rule { change } => {
                if change
                    .apply(&mut self.state.config)
                    .map_err(Rejection::Invalid)?
                {
                    self.state.mark_dirty();
                    self.schedule_save();
                }
            }
            SettingsCommand::Action { action } => {
                action.validate().map_err(Rejection::Invalid)?;
                match action {
                    Action::RetrySave => self.request_save(),
                    Action::CancelClose => {
                        if !matches!(
                            self.persistence.closing(),
                            Closing::Waiting | Closing::Failed
                        ) {
                            return Err(Rejection::Invalid(
                                "application is not waiting to close".into(),
                            ));
                        }
                        self.cancel_document_close();
                    }
                    Action::ApplyRules => {
                        let entries = shell::enumerate_desktop();
                        let moved = self.state.apply_rules_all(&entries);
                        self.refresh_all();
                        self.schedule_save();
                        self.settings_toast(&pecofence_core::i18n::format(
                            "已按规则整理 {0} 个项目",
                            &[moved.to_string()],
                        ));
                    }
                    Action::AddTemplate { template } => self.add_template(
                        pecofence_core::rules::Template::parse(&template)
                            .ok_or_else(|| Rejection::Invalid("unknown template".into()))?,
                    ),
                    Action::OpenConfigFolder => {
                        let path = self.state.config_path();
                        let directory = path.parent().ok_or_else(|| {
                            Rejection::Invalid("workspace has no parent directory".into())
                        })?;
                        std::process::Command::new("explorer.exe")
                            .arg(directory)
                            .spawn()
                            .map_err(|e| Rejection::Backend(e.to_string()))?;
                    }
                    Action::SaveSnapshot { name } => {
                        self.state.save_snapshot(&name);
                        self.settings_commit(pecofence_core::i18n::text("已保存快照"));
                    }
                    Action::RestoreSnapshot { id } => {
                        if !self
                            .state
                            .restore_snapshot_with_backup(id, pecofence_core::i18n::text("恢复前"))
                        {
                            return Err(Rejection::Invalid("snapshot not found".into()));
                        }
                        self.end_peek_now();
                        self.relayout_from_state();
                        self.refresh_portals();
                        self.settings_commit(pecofence_core::i18n::text("已恢复快照"));
                    }
                    Action::DeleteSnapshot { id } => {
                        if !self.state.delete_snapshot(id) {
                            return Err(Rejection::Invalid("snapshot not found".into()));
                        }
                        self.schedule_save();
                    }
                    Action::SwapMonitors { first, second } => {
                        let monitors = self.monitor_labels();
                        if !monitors.iter().any(|(id, _)| id == &first)
                            || !monitors.iter().any(|(id, _)| id == &second)
                        {
                            return Err(Rejection::Invalid("monitor not found".into()));
                        }
                        self.swap_monitors(&first, &second);
                    }
                    Action::ExportConfig => {
                        if !self.state.save_allowed && self.state.recovered_from.is_none() {
                            return Err(Rejection::ReadOnly);
                        }
                        return self
                            .export_config()
                            .map(|completed| !completed)
                            .map_err(Rejection::Backend);
                    }
                    Action::NewWorkspace { .. } => {
                        let old = self.state.config.settings.clone();
                        self.state.reset_workspace().map_err(Rejection::Invalid)?;
                        self.reconcile_adopted_workspace(old);
                        self.settings_commit(pecofence_core::i18n::text("已保存新工作区。"));
                    }
                    Action::AcceptRecovery { .. } => {
                        let old = self.state.config.settings.clone();
                        self.state.accept_recovery().map_err(Rejection::Invalid)?;
                        self.reconcile_adopted_workspace(old);
                        self.settings_commit(pecofence_core::i18n::text("已保存恢复的工作区。"));
                    }
                    Action::ImportConfig { .. } => {
                        return self
                            .import_config()
                            .map(|completed| !completed)
                            .map_err(Rejection::Backend);
                    }
                    Action::RestoreBackup { path, .. } => {
                        if !self.state.backup_files().contains(&path) {
                            return Err(Rejection::Invalid(
                                "backup is not in this workspace".into(),
                            ));
                        }
                        self.restore_from_file(&path, pecofence_core::i18n::text("恢复备份"))
                            .map_err(Rejection::Backend)?;
                    }
                    Action::RepairIcons => self
                        .set_desktop_icons_hidden(false)
                        .map_err(Rejection::Backend)?,
                    Action::HideDesktopIcons => self
                        .set_desktop_icons_hidden(true)
                        .map_err(Rejection::Backend)?,
                }
            }
        }
        Ok(false)
    }

    /// Both rescue buttons and the tray use the same idempotent operation. A second
    /// request must still repair Explorer's state if it changed outside PecoFence.
    pub(super) fn set_desktop_icons_hidden(&mut self, hidden: bool) -> Result<(), String> {
        if self.apply_desktop_icons_hidden(hidden) {
            self.state.config.settings.hide_real_icons = hidden;
            self.settings_mutated();
            self.desktop_icons_setting_changed();
            self.settings_toast(if hidden {
                pecofence_core::i18n::text("Windows 桌面图标已重新隐藏。")
            } else {
                pecofence_core::i18n::text("桌面图标已显示，可点击“重新隐藏桌面图标”恢复隐藏。")
            });
            Ok(())
        } else {
            self.push_settings_state();
            let issue =
                pecofence_core::i18n::text("桌面图标状态未能更改，请等待资源管理器恢复后重试。");
            self.settings_error(issue);
            Err(issue.into())
        }
    }

    /// The special desktop items (Recycle Bin, ...) enter or leave the inbox fence together
    /// with the hide-real-icons setting: resync now rather than at the next folder event.
    pub(super) fn desktop_icons_setting_changed(&mut self) {
        self.sync_desktop_if_available("hide-real-icons toggled");
        self.refresh_all();
        self.push_workspace_summary();
    }

    fn apply_desktop_icons_hidden(&self, hidden: bool) -> bool {
        let mut guard = self.anchor.borrow_mut();
        let Some(a) = guard.as_mut() else {
            return false;
        };
        if hidden {
            return a.hide_desktop_icons();
        }
        a.restore_desktop_icons();
        if pecofence_platform::shell_icons::desktop_icons_hidden()
            && let Some(h) = desktop::resolve_icon_host(a.generation)
        {
            let _ = pecofence_platform::shell_icons::set_desktop_icons_hidden(
                false, h.def_view, h.host,
            );
            // Clear our recovery marker only after Explorer actually restored the icons.
            a.restore_desktop_icons();
        }
        !pecofence_platform::shell_icons::desktop_icons_hidden()
    }

    /// Applies an application-owned settings candidate, reacting to what changed.
    pub(super) fn apply_settings(&mut self, new: pecofence_core::Settings) {
        let old = self.state.config.settings.clone();
        self.apply_settings_from(old, new, false);
    }

    fn apply_settings_from(
        &mut self,
        old: pecofence_core::Settings,
        mut new: pecofence_core::Settings,
        force_integrations: bool,
    ) {
        if !force_integrations && new == old {
            return;
        }
        if force_integrations || new.autostart != old.autostart {
            if let Err(issue) = pecofence_platform::autostart::set_product_enabled(new.autostart) {
                new.autostart = old.autostart;
                self.settings_error(&issue.to_string());
            }
            if pecofence_platform::process::is_packaged() {
                // Store installs: Windows owns the startup task, so hand the user the switch.
                let _ = shell::shell_execute(
                    std::path::Path::new("ms-settings:startupapps"),
                    None,
                    None,
                );
            }
        }
        if (force_integrations || new.hide_real_icons != old.hide_real_icons)
            && !self.apply_desktop_icons_hidden(new.hide_real_icons)
        {
            new.hide_real_icons = old.hide_real_icons;
            self.settings_error(pecofence_core::i18n::text(
                "桌面图标状态未能更改，请稍后重试。",
            ));
        }
        let icons_toggled = new.hide_real_icons != old.hide_real_icons;
        if (force_integrations || new.quick_hide.enabled != old.quick_hide.enabled)
            && let Some(a) = self.anchor.borrow_mut().as_mut()
        {
            a.quick_hide_enabled = new.quick_hide.enabled;
        }
        if (force_integrations || new.show_desktop != old.show_desktop)
            && let Some(a) = self.anchor.borrow_mut().as_mut()
        {
            a.behavior = match new.show_desktop {
                ShowDesktopSetting::KeepVisible => ShowDesktopBehavior::KeepVisible,
                ShowDesktopSetting::HideWithDesktop => ShowDesktopBehavior::HideWithDesktop,
            };
        }
        self.ctx.behavior.hover_peek.set(new.roll_up.hover_peek);
        self.ctx.behavior.snapping.set(new.snapping.enabled);
        self.ctx
            .behavior
            .click_to_expand
            .set(new.roll_up.click_to_expand);
        self.ctx
            .behavior
            .title_on_hover
            .set(new.roll_up.title_on_hover);
        self.ctx
            .behavior
            .hide_inactive_scrollbar
            .set(new.roll_up.hide_inactive_scrollbar);
        let chrome_changed = new.roll_up.title_on_hover != old.roll_up.title_on_hover
            || new.roll_up.hide_inactive_scrollbar != old.roll_up.hide_inactive_scrollbar;
        if chrome_changed {
            for w in self.fences.values() {
                w.redraw();
            }
        }
        self.ctx
            .behavior
            .backdrop
            .set(backdrop_mode_for(new.backdrop));
        let visual_changed = new.theme != old.theme
            || new.theme_style != old.theme_style
            || new.backdrop != old.backdrop;
        let icons_changed = new.icons != old.icons;
        self.state.config.settings = new;
        self.refresh_language();
        self.state.mark_dirty();
        self.schedule_save();
        self.sync_peek_hotkey();
        if icons_toggled {
            self.desktop_icons_setting_changed();
        }
        if icons_changed {
            self.apply_icon_variant();
        }
        if visual_changed {
            self.refresh_visuals(true);
        }
        self.push_settings_state();
    }

    pub(super) fn refresh_language(&mut self) {
        let language = self
            .state
            .config
            .settings
            .language
            .resolve(pecofence_platform::locale::ui_language());
        if language == pecofence_core::i18n::language() {
            return;
        }
        pecofence_core::i18n::set_language(language);
        tracing::info!(language = language.tag(), "interface language changed");
        if !self.settings_dispatching.get()
            && let Some(host) = &self.settings
        {
            host.update_language();
        }
        for window in self.fences.values() {
            window.redraw();
        }
        self.push_settings_state();
    }
}

#[cfg(test)]
mod tests {
    use super::SettingsDispatchGuard;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn dispatch_gate_restores_outer_scope() {
        let flag = Rc::new(Cell::new(false));
        {
            let _outer = SettingsDispatchGuard::new(flag.clone());
            assert!(flag.get());
            {
                let _inner = SettingsDispatchGuard::new(flag.clone());
                assert!(flag.get());
            }
            assert!(flag.get());
        }
        assert!(!flag.get());
    }

    #[test]
    fn dispatch_gate_resets_during_unwind() {
        let flag = Rc::new(Cell::new(false));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _dispatch = SettingsDispatchGuard::new(flag.clone());
            panic!("synthetic command failure");
        }));
        assert!(result.is_err());
        assert!(!flag.get());
    }
}

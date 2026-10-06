//! Settings host window + page protocol: state JSON, toasts, message handling, config adoption,
//! import / export / restore, settings diff application.

use super::*;
use pecofence_core::settings_protocol::{
    self as protocol, Action, Admission, ClientMessage, Rejection, ServerMessage, SettingsCommand,
};
use std::result::Result;

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
        let owner = self.settings.as_ref().map(|h| h.hwnd());
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
        let owner = self.settings.as_ref().map(|h| h.hwnd());
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
        if let Some(h) = &self.settings {
            window::show_normal(h.hwnd());
            window::bring_to_front(h.hwnd());
            return;
        }
        if self.web_env.is_none() {
            match WebEnvironment::create() {
                Ok(env) => self.web_env = Some(env),
                Err(e) => {
                    tracing::error!(error = %e, "WebView2 environment failed");
                    if let Some(t) = &self.tray {
                        t.show_info(
                            "PecoFence",
                            pecofence_core::i18n::text("无法创建 WebView2 环境，请确认已安装 Microsoft Edge WebView2 运行时。"),
                            true,
                        );
                    }
                    return;
                }
            }
        }
        match SettingsHost::open(
            &self.settings_class,
            self.web_env.as_ref().unwrap(),
            self.theme_mode,
            self.ctx.theme.borrow().liquid_glass,
            self.queue.clone(),
        ) {
            Ok(mut host) => {
                // Caption icon: the same drawn fence glyph as the tray, at 16/32 DIP.
                let scale = monitors::dpi_for_window(host.hwnd()).max(96) as f32 / 96.0;
                let accent = self.ctx.theme.borrow().accent_rgb8();
                let dark = self.theme_mode == ThemeMode::Dark;
                let small = (16.0 * scale).round() as i32;
                let big = (32.0 * scale).round() as i32;
                host.set_icons(
                    (small, tray_icon_image(small, accent, dark)),
                    (big, tray_icon_image(big, accent, dark)),
                );
                self.settings = Some(host);
            }
            Err(e) => {
                tracing::error!(error = %e, "settings window failed");
                if let Some(t) = &self.tray {
                    t.show_info(
                        "PecoFence",
                        pecofence_core::i18n::text("无法打开设置窗口（WebView2 初始化失败），请检查 Microsoft Edge WebView2 运行时。"),
                        true,
                    );
                }
            }
        }
    }

    fn settings_view(&self) -> serde_json::Value {
        let localization = pecofence_core::i18n::ui_payload();
        let fences: Vec<serde_json::Value> = self
            .state
            .fences()
            .iter()
            .map(|f| self.fence_options_json(f))
            .collect();
        let mem_mb = pecofence_platform::memstats::MemoryStats::current()
            .map(|m| m.private_working_set as f64 / (1024.0 * 1024.0))
            .unwrap_or(0.0);
        let snapshots: Vec<serde_json::Value> = self
            .state
            .config
            .snapshots
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "name": s.name,
                    "ts": s.ts,
                    "date": pecofence_platform::fileinfo::format_local_datetime(s.ts),
                    "fenceCount": s.layouts.iter().map(|l| l.contents.len()).sum::<usize>(),
                })
            })
            .collect();
        let backups: Vec<serde_json::Value> = self
            .state
            .backup_files()
            .iter()
            .map(|p| {
                serde_json::json!({
                    "path": p.to_string_lossy(),
                    "name": p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                })
            })
            .collect();
        let monitors: Vec<serde_json::Value> = self
            .monitor_labels()
            .into_iter()
            .map(|(id, label)| serde_json::json!({ "id": id, "label": label }))
            .collect();
        let accent = {
            let [r, g, b] = self.ctx.theme.borrow().accent_rgb8();
            format!("#{r:02X}{g:02X}{b:02X}")
        };
        serde_json::json!({
            "type": "state",
            "locale": localization["locale"],
            "translations": localization["translations"],
            "settings": self.state.config.settings,
            "loadIssue": self.state.load_issue,
            "saveAllowed": self.mutations_allowed(),
            "writable": self.mutations_allowed(),
            "saving": self.persistence.busy(),
            "closing": self.persistence.closing() != Closing::Open,
            "documentDirty": self.state.is_dirty(),
            "saveHealth": self.state.persistence_issue,
            "recoveredFrom": self.state.recovered_from,
            "desktopIconsHidden": pecofence_platform::shell_icons::desktop_icons_hidden(),
            "rules": self.state.config.rules,
            "fences": fences,
            "snapshots": snapshots,
            "backups": backups,
            "monitors": monitors,
            "tintPalette": fence_options::tint_palette_json(),
            "version": env!("CARGO_PKG_VERSION"),
            "configPath": self.state.config_path().to_string_lossy(),
            "memoryMb": mem_mb,
            "itemCount": self.state.workspace_item_count(),
            "themeMode": if self.theme_mode == ThemeMode::Dark { "dark" } else { "light" },
            "accent": accent,
        })
    }

    pub(super) fn push_settings_state(&self) {
        if let (Some(page), Some(client)) =
            (self.settings_session.page(), self.settings_session.client())
        {
            let sequence = self
                .settings_view_sequence
                .get()
                .checked_add(1)
                .expect("settings view sequence exhausted");
            self.settings_view_sequence.set(sequence);
            self.post_settings_message(ServerMessage::Snapshot {
                protocol: protocol::VERSION,
                page,
                client,
                stamp: self.state.document_stamp(),
                sequence,
                view: self.settings_view(),
            });
            self.post_settings_message(ServerMessage::Persistence {
                page,
                client,
                stamp: self.state.document_stamp(),
                committed_revision: self.state.committed_revision(),
                issue: self.state.persistence_issue.clone(),
            });
        }
    }

    fn post_settings_message(&self, message: ServerMessage) {
        if let Some(host) = &self.settings {
            match serde_json::to_string(&message) {
                Ok(json) => host.post_json(&json),
                Err(error) => tracing::error!(%error, "settings response serialization failed"),
            }
        }
    }

    /// File watcher/navigation updates must not rebuild controls while the user is
    /// editing a name or has a select popup open.
    pub(super) fn push_workspace_summary(&self) {
        if let Some(h) = &self.settings {
            h.post_json(
                &serde_json::json!({
                    "type": "workspaceSummary",
                    "page": self.settings_session.page(),
                    "workspace": self.state.document_stamp().workspace,
                    "fenceCount": self.state.fences().len(),
                    "itemCount": self.state.workspace_item_count(),
                })
                .to_string(),
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
            h.post_json(&serde_json::json!({ "type": "toast", "text": text }).to_string());
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
            h.post_json(
                &serde_json::json!({ "type": "toast", "text": text, "error": true }).to_string(),
            );
        }
    }

    pub(super) fn on_settings_message(&mut self, json: &str) {
        let message = if json.len() > protocol::MAX_REQUEST_BYTES {
            Err("settings request exceeds 64 KiB".to_string())
        } else {
            serde_json::from_str::<ClientMessage>(json).map_err(|error| error.to_string())
        };
        let message = match message {
            Ok(message) => message,
            Err(detail) => {
                self.post_settings_message(ServerMessage::ProtocolError { detail });
                return;
            }
        };
        match message {
            ClientMessage::Ready {
                protocol: version,
                page,
            } if version == protocol::VERSION => {
                self.settings_session.open(page);
                tracing::info!("settings: page ready");
                self.push_settings_state();
                if let Some(fence) = self.settings_focus_fence.take() {
                    self.post_show_fence(fence);
                }
            }
            ClientMessage::Ready { .. } => {
                self.post_settings_message(ServerMessage::ProtocolError {
                    detail: format!(
                        "unsupported settings protocol; expected {}",
                        protocol::VERSION
                    ),
                })
            }
            ClientMessage::Request(request) => {
                let result = match self.settings_session.admit(
                    &request,
                    self.state.document_stamp(),
                    self.mutations_allowed(),
                ) {
                    Admission::Replay(receipt) => {
                        self.post_settings_message(ServerMessage::Receipt(receipt));
                        self.push_settings_state();
                        return;
                    }
                    Admission::Reject(reason) => Err(reason),
                    Admission::Apply => self.apply_settings_command(request.command.clone()),
                };
                let (rejected, cancelled) = match result {
                    Ok(cancelled) => (None, cancelled),
                    Err(reason) => (Some(reason), false),
                };
                let receipt = self.settings_session.record(
                    *request,
                    self.state.document_stamp(),
                    rejected,
                    cancelled,
                );
                self.post_settings_message(ServerMessage::Receipt(receipt));
                self.push_settings_state();
            }
        }
    }

    /// The same typed use cases serve the page and opt-in native test intents.
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
                    Action::RepairIcons => self.set_desktop_icons_hidden(false),
                    Action::HideDesktopIcons => self.set_desktop_icons_hidden(true),
                }
            }
        }
        Ok(false)
    }

    /// Both rescue buttons and the tray use the same idempotent operation. A second
    /// request must still repair Explorer's state if it changed outside PecoFence.
    pub(super) fn set_desktop_icons_hidden(&mut self, hidden: bool) {
        if self.apply_desktop_icons_hidden(hidden) {
            self.state.config.settings.hide_real_icons = hidden;
            self.settings_mutated();
            self.desktop_icons_setting_changed();
            self.settings_toast(if hidden {
                pecofence_core::i18n::text("Windows 桌面图标已重新隐藏。")
            } else {
                pecofence_core::i18n::text("桌面图标已显示，可点击“重新隐藏桌面图标”恢复隐藏。")
            });
        } else {
            self.push_settings_state();
            self.settings_error(pecofence_core::i18n::text(
                "桌面图标状态未能更改，请等待资源管理器恢复后重试。",
            ));
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

    /// Applies a full settings object from the page, reacting to what changed.
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
        if let Some(host) = &self.settings {
            host.update_language();
        }
        for window in self.fences.values() {
            window.redraw();
        }
        self.push_settings_state();
    }
}

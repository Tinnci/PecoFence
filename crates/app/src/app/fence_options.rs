//! Options shared by the fence context menu and native Settings:
//! one setter per property (state + window + anchor + save), an owned read-only
//! projection, and typed content/container edits with identity checks.
//!
//! Window-level properties (appearance, lock, quick-hide exclusion, auto height, dock) act on
//! the fence's host window when the fence is a tab; content properties (icon size, spacing,
//! portal flags) act on the fence itself, exactly like the menu did.

use super::*;
use crate::settings_ui::ContentOptions;
use pecofence_core::{ContainerId, ContentId, FenceSnapshot};
use std::result::Result;

impl App {
    pub(super) fn set_fence_auto_height(&mut self, fence: FenceId, on: bool) {
        let Some(host) = self.state.host_of(fence) else {
            return;
        };
        self.state.set_auto_height(host, on);
        if let Some(w) = self.fences.get(&host) {
            w.set_auto_height(on);
        }
        self.apply_auto_height(host);
        self.schedule_save();
    }

    pub(super) fn set_fence_locked(&mut self, host: ContainerId, on: bool) {
        self.state.set_locked(host, on);
        if let Some(w) = self.fences.get(&host) {
            w.set_locked(on);
        }
        self.schedule_save();
    }

    pub(super) fn set_fence_quick_hide_excluded(&mut self, fence: FenceId, on: bool) {
        let Some(host) = self.state.host_of(fence) else {
            return;
        };
        self.state.set_exclude_from_quick_hide(host, on);
        if let Some(w) = self.fences.get(&host)
            && let Some(a) = self.anchor.borrow_mut().as_mut()
        {
            a.set_quick_hide_excluded(w.hwnd(), on);
        }
        self.schedule_save();
    }

    /// `None` = default opacity; otherwise one of the presets (or any imported value).
    pub(super) fn set_fence_opacity(&mut self, fence: FenceId, opacity: Option<f32>) {
        let Some(host) = self.state.host_of(fence) else {
            return;
        };
        self.state.set_appearance(host, None, opacity);
        self.apply_fence_appearance(host);
        self.schedule_save();
    }

    /// Tint / title colour / title size in one go (`None` fields = theme defaults). A title
    /// that followed the tint keeps following it; clearing the tint clears such a title.
    pub(super) fn set_fence_style(
        &mut self,
        fence: FenceId,
        tint: Option<[u8; 3]>,
        title_rgb: Option<[u8; 3]>,
        title_size: Option<TitleSize>,
    ) {
        let Some(host) = self.state.host_of(fence) else {
            return;
        };
        self.state.set_style(host, tint, title_rgb, title_size);
        self.apply_fence_appearance(host);
        // Tab colour bars follow the tint.
        self.refresh_fence(fence);
        self.schedule_save();
    }

    pub(super) fn set_fence_tint(&mut self, fence: FenceId, tint: Option<[u8; 3]>) {
        let Some(host) = self.state.host_of(fence) else {
            return;
        };
        let (old_tint, mut title_rgb, title_size) = self.style_of(host);
        let follow = title_rgb.is_some() && title_rgb == old_tint;
        if follow {
            title_rgb = tint;
        }
        self.set_fence_style(fence, tint, title_rgb, title_size);
    }

    /// Title appearance belongs to the container, just like its tint.
    fn set_fence_title_style(
        &mut self,
        fence: FenceId,
        color: Option<[u8; 3]>,
        size: Option<TitleSize>,
    ) {
        let Some(host) = self.state.host_of(fence) else {
            return;
        };
        let (tint, _, _) = self.style_of(host);
        self.state.set_style(host, tint, color, size);
        self.apply_fence_appearance(host);
        self.refresh_fence(fence);
        self.schedule_save();
    }

    pub(super) fn set_fence_spacing(&mut self, fence: FenceId, spacing: Spacing) {
        let Some(host) = self.state.host_of(fence) else {
            return;
        };
        self.state.set_spacing(fence, spacing);
        if let Some(w) = self.window_for(fence)
            && w.active_fence() == fence
        {
            w.set_spacing(spacing);
        }
        self.apply_column_snap(host);
        self.apply_auto_height(host);
        self.schedule_save();
    }

    pub(super) fn set_fence_portal_navigate(&mut self, fence: FenceId, on: bool) {
        let Some(host) = self.state.host_of(fence) else {
            return;
        };
        self.state.set_portal_navigate(fence, on);
        self.apply_portal_deco(host);
        self.schedule_save();
    }

    pub(super) fn set_fence_title_icon(&mut self, fence: FenceId, show: bool) {
        let Some(host) = self.state.host_of(fence) else {
            return;
        };
        self.state.set_hide_title_icon(fence, !show);
        self.apply_portal_deco(host);
        self.schedule_save();
    }

    /// (tint, title colour, title size) of the host window's appearance override.
    fn style_of(&self, host: ContainerId) -> (Option<[u8; 3]>, Option<[u8; 3]>, Option<TitleSize>) {
        self.state
            .container(host)
            .and_then(|f| f.appearance.as_ref())
            .map(|a| (a.tint_rgb, a.title_rgb, a.title_size))
            .unwrap_or((None, None, None))
    }

    /// Opens the settings window on the 「栅栏」 page with `fence` selected.
    pub(super) fn open_fence_options(&mut self, fence: FenceId) {
        self.open_settings();
        self.post_show_fence(fence);
    }

    pub(super) fn post_show_fence(&self, fence: FenceId) {
        if let Some(h) = &self.settings {
            h.show_content(fence);
        }
    }

    /// Window-level values come from the container, even when editing an inactive tab.
    pub(super) fn content_options(&self, f: &FenceSnapshot) -> ContentOptions {
        let host_id = f.container_id;
        ContentOptions::from_snapshot(
            f,
            self.state
                .window_content(host_id)
                .map(|active| active.title)
                .unwrap_or_else(|| f.title.clone()),
            self.state
                .fences()
                .iter()
                .filter(|content| content.container_id == host_id)
                .map(|content| content.title.clone())
                .collect(),
        )
    }

    /// Reject edits captured before the content moved to another container.
    fn check_settings_pair(
        &self,
        content: ContentId,
        container: ContainerId,
    ) -> Result<FenceSnapshot, String> {
        let snapshot = self
            .state
            .fence(content)
            .ok_or("content no longer exists")?;
        if !matches_container(snapshot.container_id, Some(container)) {
            return Err(pecofence_core::i18n::text("内容已移动，请刷新设置后再编辑。").into());
        }
        Ok(snapshot)
    }

    pub(super) fn apply_content_change(
        &mut self,
        content: ContentId,
        container: ContainerId,
        change: pecofence_core::settings_protocol::ContentChange,
    ) -> Result<(), String> {
        use pecofence_core::settings_protocol::ContentChange as C;
        let snapshot = self.check_settings_pair(content, container)?;
        match change {
            C::Title(title) if !title.trim().is_empty() && title.len() <= 1024 => {
                self.state.rename_fence(content, title.trim());
                self.refresh_fence(content);
                self.schedule_save();
            }
            C::Title(_) => return Err("invalid title".into()),
            C::IconSize(size)
                if snapshot.content.is_files() && matches!(size, 32 | 48 | 64 | 96) =>
            {
                self.apply_icon_size(content, size)
            }
            C::IconSize(_) => return Err("invalid file-view icon size".into()),
            C::Spacing(spacing) if snapshot.content.is_files() => {
                self.set_fence_spacing(content, spacing)
            }
            C::Spacing(_) => return Err("content has no file view".into()),
            C::PortalNavigate(value) if snapshot.kind == FenceKind::FolderPortal => {
                self.set_fence_portal_navigate(content, value)
            }
            C::PortalTitleIcon(value) if snapshot.kind == FenceKind::FolderPortal => {
                self.set_fence_title_icon(content, value)
            }
            _ => return Err("content is not a folder portal".into()),
        }
        Ok(())
    }

    pub(super) fn apply_container_change(
        &mut self,
        content: ContentId,
        container: ContainerId,
        change: pecofence_core::settings_protocol::ContainerChange,
    ) -> Result<(), String> {
        use pecofence_core::settings_protocol::{ContainerChange as C, TitleColor};
        self.check_settings_pair(content, container)?;
        match change {
            C::AutoHeight(value) => self.set_fence_auto_height(content, value),
            C::Locked(value) => self.set_fence_locked(container, value),
            C::ExcludeFromQuickHide(value) => self.set_fence_quick_hide_excluded(content, value),
            C::Opacity(value)
                if value.is_none_or(|v| v.is_finite() && (0.2..=2.0).contains(&v)) =>
            {
                self.set_fence_opacity(content, value)
            }
            C::Opacity(_) => return Err("invalid opacity".into()),
            C::Tint(value) => self.set_fence_tint(content, value),
            C::TitleColor(value) => {
                let (tint, _, size) = self.style_of(container);
                let color = match value {
                    TitleColor::Theme => None,
                    TitleColor::Tint => tint,
                    TitleColor::White => Some([255; 3]),
                    TitleColor::Black => Some([0; 3]),
                    TitleColor::Custom(rgb) => Some(rgb),
                };
                self.set_fence_title_style(content, color, size);
            }
            C::TitleSize(value) => {
                let (_, color, _) = self.style_of(container);
                self.set_fence_title_style(
                    content,
                    color,
                    (value != TitleSize::Normal).then_some(value),
                );
            }
            C::DockTop => self.dock_to_top(container),
        }
        Ok(())
    }
}

fn matches_container(actual: ContainerId, supplied: Option<ContainerId>) -> bool {
    supplied == Some(actual)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_pair_requires_current_container() {
        let original = ContainerId(uuid::Uuid::from_u128(1));
        let moved = ContainerId(uuid::Uuid::from_u128(2));
        assert!(matches_container(original, Some(original)));
        assert!(!matches_container(moved, Some(original)));
        assert!(!matches_container(original, None));
    }

    #[test]
    fn property_values_are_not_coerced() {
        use pecofence_core::settings_protocol::ContainerChange;
        assert!(
            serde_json::from_str::<ContainerChange>(r#"{"property":"locked","value":"true"}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<ContainerChange>(r#"{"property":"locked","value":false}"#)
                .is_ok()
        );
        assert!(
            serde_json::from_str::<ContainerChange>(r#"{"property":"tint","value":"oops"}"#)
                .is_err()
        );
    }
}

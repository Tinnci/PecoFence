//! Per-fence options shared by the fence context menu and the settings page's 「栅栏」 tab:
//! one setter per property (state + window + anchor + save) and the page protocol
//! (`fences[]` details in the state JSON, `setFence` messages, `showFence` navigation).
//!
//! Window-level properties (appearance, lock, quick-hide exclusion, auto height, dock) act on
//! the fence's host window when the fence is a tab; content properties (icon size, spacing,
//! portal flags) act on the fence itself, exactly like the menu did.

use super::*;
use pecofence_core::{ContainerId, ContentId, FenceSnapshot};

/// Fences-style colour choices for the per-fence tint (name, rgb); the settings page builds
/// its 色调 options from this list (`tintPalette` in the state JSON).
const TINT_PALETTE: &[(&str, [u8; 3])] = &[
    ("红", [0xE7, 0x48, 0x56]),
    ("橙", [0xF7, 0x63, 0x0C]),
    ("黄", [0xFF, 0xB9, 0x00]),
    ("绿", [0x10, 0x89, 0x3E]),
    ("青", [0x00, 0xB7, 0xC3]),
    ("蓝", [0x00, 0x78, 0xD4]),
    ("紫", [0x87, 0x64, 0xB8]),
    ("粉", [0xE3, 0x00, 0x8C]),
    ("灰", [0x7A, 0x75, 0x74]),
];

pub(super) fn tint_palette_json() -> serde_json::Value {
    TINT_PALETTE
        .iter()
        .map(|(name, rgb)| serde_json::json!({ "name": pecofence_core::i18n::text(name), "hex": hex(*rgb) }))
        .collect()
}

/// "更透明" / "更厚实" opacity presets (the menu's former values).
const OPACITY_CLEAR: f32 = 0.55;
const OPACITY_SOLID: f32 = 1.6;

fn hex(rgb: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

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
        let already_open = self.settings.is_some();
        self.open_settings();
        if self.settings.is_none() {
            return;
        }
        if already_open {
            self.post_show_fence(fence);
        } else {
            // The page is still loading; `ready` delivers the state and then this.
            self.settings_focus_fence = Some(fence);
        }
    }

    pub(super) fn post_show_fence(&self, fence: FenceId) {
        if let Some(h) = &self.settings {
            h.post_json(&serde_json::json!({ "type": "showFence", "id": fence }).to_string());
        }
    }

    /// The per-fence block of the state JSON. Window-level values come from the host so a
    /// tab shows (and edits) what its window actually uses.
    pub(super) fn fence_options_json(&self, f: &FenceSnapshot) -> serde_json::Value {
        let host_id = f.container_id;
        let h = f;
        let (tint, _, _) = self.style_of(host_id);
        let (_, title_rgb, title_size) = self.style_of(host_id);
        let opacity = h.appearance.as_ref().and_then(|a| a.opacity);
        let opacity = match opacity {
            None => "default",
            Some(o) if o < 1.0 => "clear",
            Some(_) => "solid",
        };
        let title_color = match title_rgb {
            None => "theme".to_string(),
            Some(rgb) if tint == Some(rgb) => "tint".to_string(),
            Some([0xFF, 0xFF, 0xFF]) => "white".to_string(),
            Some([0x00, 0x00, 0x00]) => "black".to_string(),
            Some(rgb) => hex(rgb),
        };
        let title_size = match title_size.unwrap_or_default() {
            TitleSize::Small => "small",
            TitleSize::Normal => "normal",
            TitleSize::Large => "large",
        };
        let spacing = match f.view.spacing {
            Spacing::Compact => "compact",
            Spacing::Normal => "normal",
            Spacing::Loose => "loose",
        };
        let portal = (f.kind == FenceKind::FolderPortal).then(|| {
            serde_json::json!({
                "navigate": f.portal_navigate,
                "titleIcon": !f.hide_title_icon,
            })
        });
        serde_json::json!({
            "id": f.id,
            "contentId": f.id,
            "containerId": f.container_id,
            "isCollection": matches!(&f.content, pecofence_core::FenceContentSpec::Files { source: pecofence_core::ItemSourceSpec::Desktop }),
            "title": f.title,
            "kind": match f.kind {
                FenceKind::Inbox => "inbox",
                FenceKind::Virtual => "virtual",
                FenceKind::FolderPortal => "portal",
            },
            "host": self.state.window_content(host_id).filter(|active| active.id != f.id).map(|active| active.title),
            "iconSize": f.view.icon_size,
            "spacing": spacing,
            "autoHeight": h.auto_height,
            "locked": h.locked,
            "excludeFromQuickHide": h.exclude_from_quick_hide,
            "opacity": opacity,
            "tint": tint.map(hex),
            "titleColor": title_color,
            "titleSize": title_size,
            "portal": portal,
        })
    }

    /// `{"type":"setFence","id":…,"prop":…,"value":…}` from the page.
    pub(super) fn on_set_fence(&mut self, v: &serde_json::Value) {
        let Some(fence) = v
            .get("contentId")
            .and_then(|i| i.as_str())
            .and_then(|i| uuid::Uuid::parse_str(i).ok())
            .map(ContentId)
        else {
            return;
        };
        let Some(snapshot) = self.state.fence(fence) else {
            return;
        };
        let supplied_container = v
            .get("containerId")
            .and_then(|i| i.as_str())
            .and_then(|i| uuid::Uuid::parse_str(i).ok())
            .map(ContainerId);
        if !matches_container(snapshot.container_id, supplied_container) {
            self.settings_error(pecofence_core::i18n::text(
                "内容已移动，请刷新设置后再编辑。",
            ));
            self.push_settings_state();
            return;
        }
        let Some(prop) = v.get("prop").and_then(|p| p.as_str()) else {
            return;
        };
        let value = v.get("value").cloned().unwrap_or(serde_json::Value::Null);
        if !valid_property_value(prop, &value) {
            self.settings_error(pecofence_core::i18n::text("属性值无效。"));
            self.push_settings_state();
            return;
        }
        let as_bool = || value.as_bool().unwrap_or(false);
        let as_str = || value.as_str().unwrap_or("");
        match prop {
            "title" => {
                let title = as_str().trim();
                if !title.is_empty() {
                    self.state.rename_fence(fence, title);
                    self.refresh_fence(fence);
                    if let Some(w) = self.window_for(fence) {
                        w.redraw();
                    }
                    self.schedule_save();
                }
            }
            "iconSize" => {
                if let Some(size) = value.as_u64()
                    && matches!(size, 32 | 48 | 64 | 96)
                {
                    self.apply_icon_size(fence, size as u32);
                }
            }
            "spacing" => {
                let spacing = match as_str() {
                    "compact" => Spacing::Compact,
                    "loose" => Spacing::Loose,
                    _ => Spacing::Normal,
                };
                self.set_fence_spacing(fence, spacing);
            }
            "autoHeight" => self.set_fence_auto_height(fence, as_bool()),
            "locked" => self.set_fence_locked(snapshot.container_id, as_bool()),
            "excludeFromQuickHide" => self.set_fence_quick_hide_excluded(fence, as_bool()),
            "opacity" => {
                let op = match as_str() {
                    "clear" => Some(OPACITY_CLEAR),
                    "solid" => Some(OPACITY_SOLID),
                    _ => None,
                };
                self.set_fence_opacity(fence, op);
            }
            "tint" => {
                let tint = value.as_str().and_then(parse_hex);
                self.set_fence_tint(fence, tint);
            }
            "titleColor" => {
                let Some(host) = self.state.host_of(fence) else {
                    return;
                };
                let (tint, _, _) = self.style_of(host);
                let (_, _, title_size) = self.style_of(host);
                let title_rgb = match as_str() {
                    "theme" => None,
                    "tint" => tint,
                    "white" => Some([0xFF, 0xFF, 0xFF]),
                    "black" => Some([0x00, 0x00, 0x00]),
                    other => parse_hex(other),
                };
                self.set_fence_title_style(fence, title_rgb, title_size);
            }
            "titleSize" => {
                let (_, title_rgb, _) = self.style_of(snapshot.container_id);
                let size = match as_str() {
                    "small" => Some(TitleSize::Small),
                    "large" => Some(TitleSize::Large),
                    _ => None,
                };
                self.set_fence_title_style(fence, title_rgb, size);
            }
            "portalNavigate" => self.set_fence_portal_navigate(fence, as_bool()),
            "portalTitleIcon" => self.set_fence_title_icon(fence, as_bool()),
            "dockTop" => self.dock_to_top(snapshot.container_id),
            _ => {
                tracing::warn!(prop, "unknown fence property from settings page");
                return;
            }
        }
        self.push_settings_state();
    }
}

fn matches_container(actual: ContainerId, supplied: Option<ContainerId>) -> bool {
    supplied == Some(actual)
}

fn valid_property_value(prop: &str, value: &serde_json::Value) -> bool {
    match prop {
        "autoHeight" | "locked" | "excludeFromQuickHide" | "portalNavigate" | "portalTitleIcon" => {
            value.is_boolean()
        }
        "title" => value.as_str().is_some_and(|s| !s.trim().is_empty()),
        "iconSize" => value
            .as_u64()
            .is_some_and(|n| matches!(n, 32 | 48 | 64 | 96)),
        "spacing" => matches!(value.as_str(), Some("compact" | "normal" | "loose")),
        "opacity" => matches!(value.as_str(), Some("default" | "clear" | "solid")),
        "tint" => value.is_null() || value.as_str().and_then(parse_hex).is_some(),
        "titleColor" => {
            matches!(value.as_str(), Some("theme" | "tint" | "white" | "black"))
                || value.as_str().and_then(parse_hex).is_some()
        }
        "titleSize" => matches!(value.as_str(), Some("small" | "normal" | "large")),
        "dockTop" => value.is_null(),
        _ => false,
    }
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
        assert!(!valid_property_value("locked", &serde_json::json!("true")));
        assert!(!valid_property_value(
            "spacing",
            &serde_json::json!("invalid")
        ));
        assert!(!valid_property_value("iconSize", &serde_json::json!(49)));
        assert!(!valid_property_value("tint", &serde_json::json!("oops")));
        assert!(valid_property_value("locked", &serde_json::json!(false)));
        assert!(valid_property_value("tint", &serde_json::Value::Null));
    }
}

//! Owned, read-only projection for the native Settings adapter.
//!
//! The application remains the authority for the workspace and persistence.
//! Controls hold drafts and send typed commands; no browser bridge or Config copy
//! is involved in the UI boundary.

use pecofence_core::rules::RuleSet;
use pecofence_core::settings_protocol::{DocumentStamp, TitleColor};
use pecofence_core::{ContainerId, ContentId, Settings, Spacing, TitleSize};
use std::path::PathBuf;
use uuid::Uuid;

pub const TINT_PALETTE: &[(&str, [u8; 3])] = &[
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

#[derive(Clone, Debug)]
pub struct SettingsView {
    pub stamp: DocumentStamp,
    pub settings: Settings,
    pub writable: bool,
    pub saving: bool,
    pub closing: bool,
    pub dirty: bool,
    pub committed_revision: Option<u64>,
    pub save_issue: Option<String>,
    pub load_issue: Option<String>,
    pub recovered_from: Option<PathBuf>,
    pub desktop_icons_hidden: Option<bool>,
    pub rules: RuleSet,
    pub contents: Vec<ContentOptions>,
    pub snapshots: Vec<SnapshotChoice>,
    pub backups: Vec<PathBuf>,
    pub monitors: Vec<MonitorChoice>,
    pub version: &'static str,
    pub config_path: PathBuf,
    pub memory_mb: Option<f64>,
    pub item_count: usize,
}

#[derive(Clone, Debug)]
pub struct ContentOptions {
    pub content_id: ContentId,
    pub container_id: ContainerId,
    pub title: String,
    pub window_title: String,
    pub window_contents: Vec<String>,
    pub is_file_view: bool,
    pub is_collection: bool,
    pub is_inbox: bool,
    pub portal: Option<PortalOptions>,
    pub icon_size: u32,
    pub spacing: Spacing,
    pub auto_height: bool,
    pub locked: bool,
    pub exclude_from_quick_hide: bool,
    pub opacity: Option<f32>,
    pub tint: Option<[u8; 3]>,
    pub title_color: TitleColor,
    pub title_size: TitleSize,
}

impl ContentOptions {
    pub fn from_snapshot(
        content: &pecofence_core::FenceSnapshot,
        window_title: String,
        window_contents: Vec<String>,
    ) -> Self {
        let appearance = content.appearance.as_ref();
        let tint = appearance.and_then(|a| a.tint_rgb);
        let title_color = match appearance.and_then(|a| a.title_rgb) {
            None => TitleColor::Theme,
            Some(rgb) if tint == Some(rgb) => TitleColor::Tint,
            Some([255, 255, 255]) => TitleColor::White,
            Some([0, 0, 0]) => TitleColor::Black,
            Some(rgb) => TitleColor::Custom(rgb),
        };
        Self {
            content_id: content.id,
            container_id: content.container_id,
            title: content.title.clone(),
            window_title,
            window_contents,
            is_file_view: content.content.is_files(),
            is_inbox: content.kind == pecofence_core::FenceKind::Inbox,
            is_collection: matches!(
                &content.content,
                pecofence_core::FenceContentSpec::Files {
                    source: pecofence_core::ItemSourceSpec::Desktop
                }
            ),
            portal: (content.kind == pecofence_core::FenceKind::FolderPortal).then_some(
                PortalOptions {
                    navigate: content.portal_navigate,
                    title_icon: !content.hide_title_icon,
                },
            ),
            icon_size: content.view.icon_size,
            spacing: content.view.spacing,
            auto_height: content.auto_height,
            locked: content.locked,
            exclude_from_quick_hide: content.exclude_from_quick_hide,
            opacity: appearance.and_then(|a| a.opacity),
            tint,
            title_color,
            title_size: appearance.and_then(|a| a.title_size).unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PortalOptions {
    pub navigate: bool,
    pub title_icon: bool,
}

#[derive(Clone, Debug)]
pub struct SnapshotChoice {
    pub id: Uuid,
    pub name: String,
    pub date: String,
    pub content_count: usize,
}

#[derive(Clone, Debug)]
pub struct MonitorChoice {
    pub id: String,
    pub label: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecofence_core::*;

    fn layout() -> Layout {
        let inbox = ContentInstance::collection("Inbox {0}", true);
        let mut portal = ContentInstance::portal("Portal", r"C:\synthetic");
        portal.view.icon_size = 96;
        portal.view.spacing = Spacing::Loose;
        let panel = ContentInstance::panel(
            "Panel",
            PanelSpec {
                provider: "synthetic".into(),
                instance_id: Uuid::new_v4(),
                config_version: 1,
                config: serde_json::json!({}),
            },
        );
        let mut container = Container::new(
            inbox.id,
            NormGeometry {
                monitor: "synthetic".into(),
                x: 0.0,
                y: 0.0,
                w: 300.0,
                h: 240.0,
                work_w: 1920.0,
                work_h: 1080.0,
                anchor: Anchor::LeftTop,
            },
        );
        container.tabs = vec![inbox.id, portal.id, panel.id];
        container.appearance = Some(AppearanceOverride {
            opacity: Some(0.65),
            tint_rgb: Some([1, 2, 3]),
            title_rgb: Some([1, 2, 3]),
            title_size: Some(TitleSize::Large),
            ..Default::default()
        });
        container.locked = true;
        container.auto_height = true;
        container.exclude_from_quick_hide = true;
        Layout {
            fingerprint: vec![],
            containers: vec![container],
            contents: vec![inbox, portal, panel],
        }
    }

    #[test]
    fn native_projection_preserves_content_and_shared_window_identity() {
        let layout = layout();
        let titles = layout
            .contents
            .iter()
            .map(|c| c.title.clone())
            .collect::<Vec<_>>();
        let mut options = Vec::new();
        for content in &layout.contents {
            let snapshot = layout.project(content.id).unwrap();
            let projected =
                ContentOptions::from_snapshot(&snapshot, titles[0].clone(), titles.clone());
            assert_eq!(projected.content_id, content.id);
            assert_eq!(projected.container_id, layout.containers[0].id);
            assert_eq!(projected.title, content.title);
            assert_eq!(projected.window_title, "Inbox {0}");
            assert_eq!(projected.window_contents, titles);
            assert!(projected.locked && projected.auto_height && projected.exclude_from_quick_hide);
            assert_eq!(projected.opacity, Some(0.65));
            assert_eq!(projected.tint, Some([1, 2, 3]));
            assert!(matches!(projected.title_color, TitleColor::Tint));
            assert_eq!(projected.title_size, TitleSize::Large);
            options.push(projected);
        }
        assert!(options[0].is_collection && options[0].is_file_view);
        assert!(options[0].is_inbox);
        assert!(options[0].portal.is_none());
        assert!(options[1].is_file_view && !options[1].is_collection);
        assert!(!options[1].is_inbox);
        assert!(options[1].portal.as_ref().unwrap().navigate);
        assert_eq!(options[1].icon_size, 96);
        assert_eq!(options[1].spacing, Spacing::Loose);
        assert!(!options[2].is_file_view && !options[2].is_collection);
        assert!(!options[2].is_inbox);
        assert!(options[2].portal.is_none());
    }

    #[test]
    fn arbitrary_imported_appearance_is_not_rounded_to_a_preset() {
        let mut layout = layout();
        layout.containers[0].appearance = Some(AppearanceOverride {
            opacity: Some(1.37),
            tint_rgb: Some([13, 71, 222]),
            title_rgb: Some([9, 8, 7]),
            title_size: None,
            ..Default::default()
        });
        let snapshot = layout.project(layout.contents[1].id).unwrap();
        let options = ContentOptions::from_snapshot(&snapshot, "Window".into(), vec![]);
        assert_eq!(options.opacity, Some(1.37));
        assert_eq!(options.tint, Some([13, 71, 222]));
        assert!(matches!(options.title_color, TitleColor::Custom([9, 8, 7])));
        assert_eq!(options.title_size, TitleSize::Normal);
    }
}

//! Schema 2 workspace data. Containers own windows; content owns business identity.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 2;
pub const MAX_SNAPSHOTS: usize = 20;
pub const MAX_CONTENTS: usize = 64;
pub const MAX_ITEMS: usize = 5000;

macro_rules! identity {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);
        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
            pub fn new_v4() -> Self {
                Self::new()
            }
            pub fn from_u128(value: u128) -> Self {
                Self(Uuid::from_u128(value))
            }
            pub fn as_u128(&self) -> u128 {
                self.0.as_u128()
            }
            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
        impl std::str::FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                s.parse().map(Self)
            }
        }
    };
}
identity!(ContainerId);
identity!(ContentId);
/// Native file-rendering DTO identity, never a container/window identity.
pub type FenceId = ContentId;
pub type ItemId = Uuid;
pub type RuleId = Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub settings: Settings,
    pub items: HashMap<ItemId, Item>,
    pub layouts: Vec<Layout>,
    pub rules: crate::rules::RuleSet,
    #[serde(default)]
    pub undo_log: Vec<Assignment>,
    #[serde(default)]
    pub snapshots: Vec<Snapshot>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            settings: Settings::default(),
            items: HashMap::new(),
            layouts: Vec::new(),
            rules: crate::rules::RuleSet::default(),
            undo_log: Vec::new(),
            snapshots: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    pub id: Uuid,
    pub name: String,
    pub ts: i64,
    pub layouts: Vec<Layout>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub language: crate::i18n::Language,
    pub backdrop: Backdrop,
    pub theme: ThemeSetting,
    pub theme_style: ThemeStyle,
    pub icon_size: u32,
    pub quick_hide: QuickHideSettings,
    pub roll_up: RollUpSettings,
    pub show_desktop: ShowDesktopSetting,
    pub hide_real_icons: bool,
    pub show_real_icons_when_fences_hidden: bool,
    pub snapping: SnappingSettings,
    pub zorder: ZOrderSetting,
    pub autostart: bool,
    pub telemetry: bool,
    pub peek: PeekSettings,
    pub icons: IconSettings,
    pub desktop_path: Option<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            language: crate::i18n::Language::System,
            backdrop: Backdrop::Acrylic,
            theme: ThemeSetting::FollowWindowsMode,
            theme_style: ThemeStyle::Fluent,
            icon_size: 48,
            quick_hide: QuickHideSettings::default(),
            roll_up: RollUpSettings::default(),
            show_desktop: ShowDesktopSetting::KeepVisible,
            hide_real_icons: true,
            show_real_icons_when_fences_hidden: false,
            snapping: SnappingSettings::default(),
            zorder: ZOrderSetting::InsertAboveHost,
            autostart: true,
            telemetry: false,
            peek: PeekSettings::default(),
            icons: IconSettings::default(),
            desktop_path: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeekSettings {
    pub enabled: bool,
    pub dim: bool,
    pub hotkey: PeekHotkey,
}
impl Default for PeekSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            dim: true,
            hotkey: PeekHotkey::CtrlAltSpace,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PeekHotkey {
    WinSpace,
    #[default]
    CtrlAltSpace,
    WinShiftSpace,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Backdrop {
    Acrylic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeSetting {
    FollowWindowsMode,
    FollowAppMode,
    Light,
    Dark,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeStyle {
    #[default]
    Fluent,
    LiquidGlass,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShowDesktopSetting {
    KeepVisible,
    HideWithDesktop,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZOrderSetting {
    InsertAboveHost,
    HwndBottom,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickHideSettings {
    pub enabled: bool,
    pub scope: QuickHideScope,
    pub always_show_at_startup: bool,
    pub delay_ms: u32,
    pub auto_hide_idle_sec: u32,
    pub auto_show_on_use: bool,
    #[serde(default)]
    pub wallpaper_engine_class_whitelist: Vec<String>,
}
impl Default for QuickHideSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            scope: QuickHideScope::All,
            always_show_at_startup: true,
            delay_ms: 300,
            auto_hide_idle_sec: 0,
            auto_show_on_use: true,
            wallpaper_engine_class_whitelist: Vec::new(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuickHideScope {
    All,
    LooseOnly,
    FencesOnly,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RollUpSettings {
    pub double_click_title: bool,
    pub auto_on_screen_edge: bool,
    pub hover_peek: bool,
    pub hover_open_ms: u32,
    pub close_grace_ms: u32,
    pub click_to_expand: bool,
    pub title_on_hover: bool,
    pub hide_inactive_scrollbar: bool,
}
impl Default for RollUpSettings {
    fn default() -> Self {
        Self {
            double_click_title: true,
            auto_on_screen_edge: true,
            hover_peek: true,
            hover_open_ms: 400,
            close_grace_ms: 400,
            click_to_expand: false,
            title_on_hover: false,
            hide_inactive_scrollbar: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnappingSettings {
    pub enabled: bool,
    pub gap_px: i32,
    pub size_to_cells: bool,
    pub guide_lines: bool,
}
impl Default for SnappingSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            gap_px: 8,
            size_to_cells: false,
            guide_lines: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorIdentity {
    pub device_path: String,
    pub work_dip: [f32; 2],
    pub dpi: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Layout {
    pub fingerprint: Vec<MonitorIdentity>,
    pub containers: Vec<Container>,
    pub contents: Vec<ContentInstance>,
}

/// Window-only persistent properties. No file source, title, view or memberships.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Container {
    pub id: ContainerId,
    pub geometry: NormGeometry,
    pub rolled_up: bool,
    pub expanded_h: f32,
    pub auto_height: bool,
    pub appearance: Option<AppearanceOverride>,
    pub exclude_from_quick_hide: bool,
    pub locked: bool,
    pub tabs: Vec<ContentId>,
    pub active_tab: ContentId,
}
impl Container {
    pub fn new(content: ContentId, geometry: NormGeometry) -> Self {
        Self {
            id: ContainerId::new(),
            expanded_h: geometry.h,
            geometry,
            rolled_up: false,
            auto_height: false,
            appearance: None,
            exclude_from_quick_hide: false,
            locked: false,
            tabs: vec![content],
            active_tab: content,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentInstance {
    pub id: ContentId,
    pub title: String,
    pub view: FenceView,
    pub content: ContentSpec,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ContentSpec {
    FileCollection {
        inbox: bool,
        items: Vec<ItemRef>,
    },
    FolderPortal {
        root: String,
        recursive: bool,
        filter: Option<String>,
        navigate: bool,
        hide_title_icon: bool,
    },
    Panel {
        panel: PanelSpec,
    },
}
impl ContentInstance {
    pub fn collection(title: &str, inbox: bool) -> Self {
        Self {
            id: ContentId::new(),
            title: title.into(),
            view: FenceView::default(),
            content: ContentSpec::FileCollection {
                inbox,
                items: Vec::new(),
            },
        }
    }
    pub fn portal(title: &str, root: &str) -> Self {
        Self {
            id: ContentId::new(),
            title: title.into(),
            view: FenceView::default(),
            content: ContentSpec::FolderPortal {
                root: root.into(),
                recursive: false,
                filter: None,
                navigate: true,
                hide_title_icon: false,
            },
        }
    }
    pub fn panel(title: &str, panel: PanelSpec) -> Self {
        Self {
            id: ContentId::new(),
            title: title.into(),
            view: FenceView::default(),
            content: ContentSpec::Panel { panel },
        }
    }
    pub fn items(&self) -> &[ItemRef] {
        match &self.content {
            ContentSpec::FileCollection { items, .. } => items,
            _ => &[],
        }
    }
    pub fn items_mut(&mut self) -> Option<&mut Vec<ItemRef>> {
        match &mut self.content {
            ContentSpec::FileCollection { items, .. } => Some(items),
            _ => None,
        }
    }
    pub fn contains_item(&self, id: ItemId) -> bool {
        self.items().iter().any(|r| r.item_id == id)
    }
    pub fn is_inbox(&self) -> bool {
        matches!(
            self.content,
            ContentSpec::FileCollection { inbox: true, .. }
        )
    }
    pub fn is_collection(&self) -> bool {
        matches!(self.content, ContentSpec::FileCollection { .. })
    }
}

/// Native Files rendering remains bespoke. These descriptors are projection-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FenceKind {
    Virtual,
    Inbox,
    FolderPortal,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemSourceSpec {
    Desktop,
    Folder {
        path: String,
        recursive: bool,
        filter: Option<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FenceContentSpec {
    Files { source: ItemSourceSpec },
    Panel { panel: PanelSpec },
}
impl FenceContentSpec {
    pub fn is_files(&self) -> bool {
        matches!(self, Self::Files { .. })
    }
}

/// Owned, read-only render DTO. Intentionally neither Serialize nor Deserialize.
#[derive(Clone, Debug, PartialEq)]
pub struct FenceSnapshot {
    pub id: ContentId,
    pub container_id: ContainerId,
    pub title: String,
    pub kind: FenceKind,
    pub source: ItemSourceSpec,
    pub content: FenceContentSpec,
    pub geometry: NormGeometry,
    pub rolled_up: bool,
    pub expanded_h: f32,
    pub auto_height: bool,
    pub view: FenceView,
    pub appearance: Option<AppearanceOverride>,
    pub exclude_from_quick_hide: bool,
    pub locked: bool,
    pub portal_navigate: bool,
    pub hide_title_icon: bool,
    pub items: Vec<ItemRef>,
}
impl FenceSnapshot {
    pub fn contains_item(&self, id: ItemId) -> bool {
        self.items.iter().any(|r| r.item_id == id)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PanelSpec {
    pub provider: String,
    pub instance_id: Uuid,
    pub config_version: u16,
    pub config: serde_json::Value,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Anchor {
    LeftTop,
    LeftBottom,
    LeftVCenter,
    RightTop,
    RightBottom,
    RightVCenter,
    HCenterTop,
    HCenterBottom,
    Center,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NormGeometry {
    pub monitor: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub work_w: f32,
    pub work_h: f32,
    pub anchor: Anchor,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortMode {
    Manual,
    Name,
    Type,
    Date,
    Size,
    OpenCount,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewLayout {
    #[default]
    Icons,
    List,
    Details,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FenceView {
    pub icon_size: u32,
    pub sort: SortMode,
    pub label_lines: u8,
    pub reverse: bool,
    pub layout: ViewLayout,
    pub spacing: Spacing,
    pub column_widths: Option<[f32; 3]>,
    pub columns_visible: Option<[bool; 3]>,
    pub group_by_date: bool,
}
impl Default for FenceView {
    fn default() -> Self {
        Self {
            icon_size: 48,
            sort: SortMode::Manual,
            label_lines: 2,
            reverse: false,
            layout: ViewLayout::Icons,
            spacing: Spacing::Normal,
            column_widths: None,
            columns_visible: None,
            group_by_date: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceOverride {
    pub tint_rgb: Option<[u8; 3]>,
    pub opacity: Option<f32>,
    pub backdrop: Option<Backdrop>,
    pub title_rgb: Option<[u8; 3]>,
    pub title_size: Option<TitleSize>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TitleSize {
    Small,
    #[default]
    Normal,
    Large,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Spacing {
    Compact,
    #[default]
    Normal,
    Loose,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IconSettings {
    pub tint_rgb: Option<[u8; 3]>,
    pub tint_strength: f32,
    pub chameleon: bool,
}
impl Default for IconSettings {
    fn default() -> Self {
        Self {
            tint_rgb: None,
            tint_strength: 0.6,
            chameleon: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssignedBy {
    User,
    Rule(RuleId),
    /// Fallback/default routing, not a manual pin and not a legacy migration.
    Default,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemRef {
    pub item_id: ItemId,
    pub manual_index: Option<u32>,
    pub assigned_by: AssignedBy,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKey {
    Path(String),
    Pidl(String),
}
impl ItemKey {
    pub fn from_path(path: &str) -> Self {
        let mut s = path.replace('/', "\\").to_lowercase();
        while s.ends_with('\\') && s.len() > 3 {
            s.pop();
        }
        Self::Path(s)
    }
    pub fn as_path(&self) -> Option<&str> {
        match self {
            Self::Path(p) => Some(p),
            Self::Pidl(_) => None,
        }
    }
    pub fn is_namespace(&self) -> bool {
        matches!(self, Self::Path(p) if p.starts_with("::{"))
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    #[default]
    UserDesktop,
    PublicDesktop,
    Namespace,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IconKey {
    ByExt(String),
    ByContent { path: String, mtime: i64 },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: ItemId,
    pub key: ItemKey,
    pub origin: Origin,
    pub display_name: String,
    #[serde(default)]
    pub file_id: Option<u128>,
    pub mtime: i64,
    pub is_folder: bool,
    pub attrs: u32,
    pub icon_key: IconKey,
    #[serde(default)]
    pub orphaned_since: Option<i64>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub open_count: u32,
    #[serde(default)]
    pub last_opened: Option<i64>,
}
impl Item {
    pub fn is_namespace(&self) -> bool {
        self.key.is_namespace()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub ts: i64,
    pub item_id: ItemId,
    pub from: Option<ContentId>,
    pub to: ContentId,
    pub rule_id: Option<RuleId>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PendingCreation {
    pub content_id: ContentId,
    pub since: std::time::Instant,
}

impl Layout {
    pub fn container(&self, id: ContainerId) -> Option<&Container> {
        self.containers.iter().find(|c| c.id == id)
    }
    pub fn container_mut(&mut self, id: ContainerId) -> Option<&mut Container> {
        self.containers.iter_mut().find(|c| c.id == id)
    }
    pub fn content(&self, id: ContentId) -> Option<&ContentInstance> {
        self.contents.iter().find(|c| c.id == id)
    }
    pub fn content_mut(&mut self, id: ContentId) -> Option<&mut ContentInstance> {
        self.contents.iter_mut().find(|c| c.id == id)
    }
    pub fn owner_of(&self, id: ContentId) -> Option<ContainerId> {
        self.containers
            .iter()
            .find(|c| c.tabs.contains(&id))
            .map(|c| c.id)
    }
    pub fn inbox(&self) -> Option<ContentId> {
        self.contents.iter().find(|c| c.is_inbox()).map(|c| c.id)
    }
    pub fn project(&self, id: ContentId) -> Option<FenceSnapshot> {
        let content = self.content(id)?;
        let container = self.container(self.owner_of(id)?)?;
        let (kind, source, spec, navigate, hide_icon) = match &content.content {
            ContentSpec::FileCollection { inbox, .. } => {
                let source = ItemSourceSpec::Desktop;
                (
                    if *inbox {
                        FenceKind::Inbox
                    } else {
                        FenceKind::Virtual
                    },
                    source.clone(),
                    FenceContentSpec::Files { source },
                    true,
                    false,
                )
            }
            ContentSpec::FolderPortal {
                root,
                recursive,
                filter,
                navigate,
                hide_title_icon,
            } => {
                let source = ItemSourceSpec::Folder {
                    path: root.clone(),
                    recursive: *recursive,
                    filter: filter.clone(),
                };
                (
                    FenceKind::FolderPortal,
                    source.clone(),
                    FenceContentSpec::Files { source },
                    *navigate,
                    *hide_title_icon,
                )
            }
            ContentSpec::Panel { panel } => (
                FenceKind::Virtual,
                ItemSourceSpec::Desktop,
                FenceContentSpec::Panel {
                    panel: panel.clone(),
                },
                false,
                false,
            ),
        };
        Some(FenceSnapshot {
            id,
            container_id: container.id,
            title: content.title.clone(),
            kind,
            source,
            content: spec,
            geometry: container.geometry.clone(),
            rolled_up: container.rolled_up,
            expanded_h: container.expanded_h,
            auto_height: container.auto_height,
            view: content.view.clone(),
            appearance: container.appearance.clone(),
            exclude_from_quick_hide: container.exclude_from_quick_hide,
            locked: container.locked,
            portal_navigate: navigate,
            hide_title_icon: hide_icon,
            items: content.items().to_vec(),
        })
    }

    /// Reject invalid ownership; never repair persisted data.
    pub fn validate(&self) -> Result<(), String> {
        if self.contents.len() > MAX_CONTENTS || self.containers.len() > MAX_CONTENTS {
            return Err("layout exceeds 64 containers or contents".into());
        }
        let mut identities = HashSet::new();
        let mut contents = HashSet::new();
        let mut members = HashSet::new();
        let mut panels = HashSet::new();
        let mut inboxes = 0;
        for c in &self.contents {
            if c.id.0.is_nil() || !identities.insert(c.id.0) || !contents.insert(c.id) {
                return Err("duplicate or nil content identity".into());
            }
            validate_content(c)?;
            if c.is_inbox() {
                inboxes += 1;
            }
            for item in c.items() {
                if item.item_id.is_nil() || !members.insert(item.item_id) {
                    return Err("duplicate or nil collection membership".into());
                }
            }
            if let ContentSpec::Panel { panel } = &c.content
                && !panels.insert(panel.instance_id)
            {
                return Err("duplicate panel business instance".into());
            }
        }
        if !self.contents.is_empty() && inboxes != 1 {
            return Err("nonempty layout must have exactly one inbox collection".into());
        }
        if members.len() > MAX_ITEMS {
            return Err("layout membership budget exceeded".into());
        }
        let mut owned = HashSet::new();
        for c in &self.containers {
            if c.id.0.is_nil() || !identities.insert(c.id.0) {
                return Err("duplicate, colliding or nil container identity".into());
            }
            validate_geometry(&c.geometry)?;
            if !c.expanded_h.is_finite() || !(1.0..=8192.0).contains(&c.expanded_h) {
                return Err("invalid expanded height".into());
            }
            if let Some(a) = &c.appearance
                && let Some(o) = a.opacity
                && (!o.is_finite() || !(0.2..=2.0).contains(&o))
            {
                return Err("opacity out of range".into());
            }
            if c.tabs.is_empty() || !c.tabs.contains(&c.active_tab) {
                return Err("empty container or invalid active tab".into());
            }
            for tab in &c.tabs {
                if !contents.contains(tab) || !owned.insert(*tab) {
                    return Err("dangling or multiply-owned content".into());
                }
            }
        }
        if owned != contents {
            return Err("unowned content".into());
        }
        let mut monitors = HashSet::new();
        for monitor in &self.fingerprint {
            if !monitors.insert(&monitor.device_path)
                || monitor.dpi == 0
                || monitor.work_dip.iter().any(|v| !v.is_finite() || *v < 1.0)
            {
                return Err("invalid monitor fingerprint".into());
            }
        }
        Ok(())
    }
}
pub(crate) fn validate_geometry(g: &NormGeometry) -> Result<(), String> {
    if [g.x, g.y, g.w, g.h, g.work_w, g.work_h]
        .iter()
        .any(|v| !v.is_finite())
        || !(1.0..=8192.0).contains(&g.w)
        || !(1.0..=8192.0).contains(&g.h)
        || g.work_w < 1.0
        || g.work_h < 1.0
    {
        return Err("invalid container geometry".into());
    }
    Ok(())
}
pub(crate) fn validate_content(c: &ContentInstance) -> Result<(), String> {
    if let Some(widths) = c.view.column_widths
        && widths
            .iter()
            .any(|w| !w.is_finite() || *w <= 0.0 || *w > 8192.0)
    {
        return Err("invalid file view column widths".into());
    }
    match &c.content {
        ContentSpec::FileCollection { items, .. } if items.len() > MAX_ITEMS => {
            Err("collection exceeds item budget".into())
        }
        ContentSpec::FolderPortal { root, .. } if root.trim().is_empty() => {
            Err("portal requires a root".into())
        }
        ContentSpec::Panel { panel }
            if panel.provider.trim().is_empty()
                || panel.config_version == 0
                || panel.instance_id.is_nil() =>
        {
            Err("panel requires provider, instance identity and version".into())
        }
        _ => Ok(()),
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err("unsupported schema version".into());
        }
        if self.items.len() > MAX_ITEMS {
            return Err("item budget exceeded".into());
        }
        if self.snapshots.len() > MAX_SNAPSHOTS {
            return Err("snapshot budget exceeded".into());
        }
        if !self.settings.icons.tint_strength.is_finite()
            || !(0.0..=1.0).contains(&self.settings.icons.tint_strength)
        {
            return Err("invalid icon tint strength".into());
        }
        for (id, item) in &self.items {
            if id.is_nil() || *id != item.id {
                return Err("invalid item table identity".into());
            }
        }
        self.validate_layouts(&self.layouts, true)?;
        let mut snapshots = HashSet::new();
        for snapshot in &self.snapshots {
            if snapshot.id.is_nil() || !snapshots.insert(snapshot.id) {
                return Err("duplicate or nil snapshot identity".into());
            }
            // Saved memberships may refer to files removed from the current item table.
            self.validate_layouts(&snapshot.layouts, false)?;
        }
        let mut rules = HashSet::new();
        for rule in &self.rules.list {
            if rule.id.is_nil() || !rules.insert(rule.id) {
                return Err("duplicate or nil rule identity".into());
            }
        }
        let check_target = |target: crate::rules::Target| -> Result<(), String> {
            if let crate::rules::Target::Collection(id) = target {
                let matches: Vec<_> = self.layouts.iter().filter_map(|l| l.content(id)).collect();
                if matches.is_empty() || matches.iter().any(|c| !c.is_collection()) {
                    return Err("rule target is not an existing collection".into());
                }
            }
            Ok(())
        };
        check_target(self.rules.default_target)?;
        for rule in &self.rules.list {
            check_target(rule.target)?;
        }
        Ok(())
    }
    fn validate_layouts(&self, layouts: &[Layout], current: bool) -> Result<(), String> {
        let mut fingerprints = HashSet::new();
        // Content can be placed in alternative monitor layouts, but IDs cannot change roles.
        let mut containers = HashSet::new();
        let mut contents = HashSet::new();
        for layout in layouts {
            layout.validate()?;
            let mut fingerprint: Vec<_> =
                layout.fingerprint.iter().map(|m| &m.device_path).collect();
            fingerprint.sort_unstable();
            if !fingerprints.insert(fingerprint) {
                return Err("duplicate layout fingerprint".into());
            }
            for c in &layout.containers {
                containers.insert(c.id.0);
            }
            for c in &layout.contents {
                contents.insert(c.id.0);
                if current
                    && c.items()
                        .iter()
                        .any(|r| !self.items.contains_key(&r.item_id))
                {
                    return Err("collection references missing item".into());
                }
            }
        }
        if !containers.is_disjoint(&contents) {
            return Err("identity changes role between layouts".into());
        }
        Ok(())
    }
    pub fn layout_for(&self, device_paths: &[String]) -> Option<usize> {
        let mut wanted: Vec<&str> = device_paths.iter().map(String::as_str).collect();
        wanted.sort_unstable();
        self.layouts.iter().position(|l| {
            let mut have: Vec<&str> = l
                .fingerprint
                .iter()
                .map(|m| m.device_path.as_str())
                .collect();
            have.sort_unstable();
            have == wanted
        })
    }
    pub fn content_mut(&mut self, layout: usize, id: ContentId) -> Option<&mut ContentInstance> {
        self.layouts.get_mut(layout)?.content_mut(id)
    }
    pub fn content_of_item(&self, layout: usize, item: ItemId) -> Option<ContentId> {
        self.layouts
            .get(layout)?
            .contents
            .iter()
            .find(|c| c.contains_item(item))
            .map(|c| c.id)
    }
    /// Rules and membership edits target collections, never portals/panels/windows.
    pub fn assign(
        &mut self,
        layout: usize,
        item: ItemId,
        to: ContentId,
        by: AssignedBy,
    ) -> Option<Assignment> {
        if !self.items.contains_key(&item) {
            return None;
        }
        let from = self.content_of_item(layout, item);
        if from == Some(to) {
            return None;
        }
        let l = self.layouts.get_mut(layout)?;
        if !l.content(to)?.is_collection() {
            return None;
        }
        for content in &mut l.contents {
            if let Some(items) = content.items_mut() {
                items.retain(|r| r.item_id != item);
            }
        }
        let rule_id = match &by {
            AssignedBy::Rule(r) => Some(*r),
            _ => None,
        };
        l.content_mut(to)?.items_mut()?.push(ItemRef {
            item_id: item,
            manual_index: None,
            assigned_by: by,
        });
        let a = Assignment {
            ts: now_unix(),
            item_id: item,
            from,
            to,
            rule_id,
        };
        if rule_id.is_some() {
            self.undo_log.push(a.clone());
            if self.undo_log.len() > 50 {
                self.undo_log.drain(..self.undo_log.len() - 50);
            }
        }
        Some(a)
    }
}
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

use crate::backdrop::Image;
use crate::theme::Theme;
use pecofence_plugin_api::design::PixelGrid;
use std::cell::RefCell;
use std::rc::Rc;
use windows_canvas::{ColorF, Rect, RoundedRect};

/// A "按时间分组" section header band in surface DIPs (already scrolled; for rows already
/// offset below the fixed column header, like the rows themselves).
pub struct GroupHeaderDraw<'a> {
    pub y: f32,
    pub h: f32,
    pub text: &'a str,
}

/// A wallpaper crop for one fence. The bitmap is uploaded once per `key` (a per-window cache
/// key the owner invalidates whenever the crop changes) and stretched over `height` DIPs: the
/// surface height at rest, the taller end of a height animation while one runs — the moving
/// bottom edge then reveals or conceals finished pixels instead of re-cropping per frame.
#[derive(Clone, Copy)]
pub struct BackdropCrop<'a> {
    pub image: &'a Image,
    pub key: &'a str,
    pub height: f32,
}

/// What to paint behind the fence content.
pub enum Backdrop<'a> {
    /// Solid fallback colour from the theme.
    Solid,
    /// A pre-blurred, pre-tinted crop of the wallpaper (any size; stretched to the fence).
    MicaLike(BackdropCrop<'a>),
    /// A pre-blurred acrylic crop of the wallpaper plus the DPI scale (for 1:1 grain tiling).
    Glass(BackdropCrop<'a>, f32),
    /// GPU-resident wallpaper under a material shader: Liquid Glass, or paper when the
    /// theme is paper (`seed` then varies its fibres per window).
    GpuGlass {
        material: &'a RefCell<crate::gpu_glass::GpuGlass>,
        wallpaper: &'a Rc<Vec<crate::MonitorBackdrop>>,
        rect: [i32; 4],
        scale: f32,
        seed: f32,
    },
    /// DWM paints the material (system Acrylic); only a light tint layer is drawn here.
    SystemMaterial,
}

/// One icon cell to draw, in content-local DIPs (already scrolled).
pub struct ItemCell<'a> {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub icon_key: &'a str,
    pub icon: Option<&'a Image>,
    pub label: &'a str,
    /// PointerOver fill alpha 0..=1 (SubtleFillColorSecondary fades over 83 ms).
    pub hover: f32,
    /// Selection fill / stroke alpha 0..=1 (fades over 83 ms on select / deselect).
    pub selection: f32,
    /// Button held on this item: SubtleFillColorTertiary on top (WinUI ListViewItem pressed).
    pub pressed: bool,
    /// Keyboard cursor item after keyboard navigation: WinUI focus visual (2 px + 1 px ring).
    pub focused: bool,
    pub dimmed: bool,
    /// Icon extraction failed for good: draw a generic document glyph, not a loading tile.
    pub failed: bool,
    /// A folder being hovered by a drag: Explorer's drop highlight (selection look), alpha
    /// 0..=1 (fades over 83 ms).
    pub drop_target: f32,
    /// Whole-cell opacity 0..=1: an item fading in after it appeared or fading out after it
    /// was removed (layout motion). State layers are skipped below 1.
    pub alpha: f32,
    /// Icon bitmap opacity 0..=1 while it cross-fades in over the loading tile (167 ms).
    pub icon_alpha: f32,
    /// The selected / focused item's full name (Windows desktop: the focused icon's label
    /// unfolds past the two-line limit, overlapping the row below). Painted last.
    pub full_label: Option<(&'a str, u32)>,
}

/// Which details column a header caption / sort indicator refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HeaderColumn {
    Name,
    Date,
    Type,
    Size,
}

/// Column x/width split used by the details view (mirrors the app's `DetailColumns`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowColumns {
    pub name_x: f32,
    pub name_w: f32,
    pub date: Option<(f32, f32)>,
    pub type_: Option<(f32, f32)>,
    pub size: Option<(f32, f32)>,
}

/// One row of the List / Details view, in content DIPs (already scrolled, below the header).
pub struct RowCell<'a> {
    pub y: f32,
    pub h: f32,
    pub icon_key: &'a str,
    pub icon: Option<&'a Image>,
    pub name: &'a str,
    pub date: &'a str,
    pub type_name: &'a str,
    pub size: &'a str,
    pub hover: f32,
    pub selection: f32,
    pub pressed: bool,
    pub focused: bool,
    pub dimmed: bool,
    pub failed: bool,
    /// Drop-target highlight alpha 0..=1 (see [`ItemCell::drop_target`]).
    pub drop_target: f32,
    /// Whole-row opacity (see [`ItemCell::alpha`]).
    pub alpha: f32,
    /// Icon cross-fade (see [`ItemCell::icon_alpha`]).
    pub icon_alpha: f32,
}

/// Overlay scrollbar in content DIPs (WinUI ScrollBar). `width` runs 2 (resting indicator)
/// to 6 DIP (expanded, pointer near the bar or dragging it) and `alpha` 0..=1 fades the whole
/// bar (inactive-scrollbar rule); `parts` 0..=1 reveals the expanded state's 12 DIP track fill
/// and the two arrow buttons at its ends. All are sampled from client tweens by the caller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarDraw {
    pub thumb_y: f32,
    pub thumb_h: f32,
    pub width: f32,
    pub alpha: f32,
    /// Track fill + arrow buttons opacity (83 ms with the expand / contract).
    pub parts: f32,
    /// Vertical extent of the bar (top of the scrollable band, its height): the track fill
    /// spans it, the buttons sit at its ends, the thumb travels between them.
    pub band_top: f32,
    pub band_h: f32,
    /// PointerOver fill alpha of the up / down button.
    pub arrow_hover: [f32; 2],
    /// The up / down button is held (RepeatButton pressed).
    pub arrow_pressed: [bool; 2],
}

impl ScrollbarDraw {
    pub const REST_WIDTH: f32 = 2.0;
    pub const EXPANDED_WIDTH: f32 = 6.0;
    /// WinUI ScrollBarSize: the expanded track's width.
    pub const BAR_W: f32 = 12.0;
    /// Height of each arrow RepeatButton at the ends of the track.
    pub const BUTTON_H: f32 = 12.0;
    /// Segoe Fluent Icons CaretUpSolid8 / CaretDownSolid8 (ScrollBar arrow glyphs).
    pub(super) const ARROW_UP: &'static str = "\u{EDDB}";
    pub(super) const ARROW_DOWN: &'static str = "\u{EDDC}";
    /// Arrow glyph scale while its button is pressed (ScrollBarButtonArrowScalePressed).
    pub(super) const ARROW_SCALE_PRESSED: f32 = 0.875;

    /// Left edge of a thumb `width` DIPs wide inside a surface `total_w` wide: the thumb keeps
    /// its centre on the 12 DIP track's centre line, 6 DIP from the right edge (the track and
    /// the arrow buttons span `total_w - 12 .. total_w`), so 2 DIP sits at `total_w - 7` and
    /// 6 DIP at `total_w - 9` — WinUI scales the thumb about its centre inside the track.
    pub fn thumb_x(total_w: f32, width: f32) -> f32 {
        total_w - ScrollbarDraw::BAR_W / 2.0 - width / 2.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct HeaderLabels<'a> {
    pub name: &'a str,
    pub date: &'a str,
    pub type_: &'a str,
    pub size: &'a str,
}

impl Default for HeaderLabels<'static> {
    fn default() -> Self {
        Self {
            name: "名称",
            date: "修改日期",
            type_: "类型",
            size: "大小",
        }
    }
}

pub struct RowsDraw<'a> {
    pub rows: &'a [RowCell<'a>],
    /// Section headers interleaved with the rows (drawn in the scrolling band).
    pub group_headers: &'a [GroupHeaderDraw<'a>],
    pub columns: RowColumns,
    pub header_labels: HeaderLabels<'a>,
    pub icon_size: f32,
    /// 0 for the List layout (no header).
    pub header_h: f32,
    pub sort_column: Option<HeaderColumn>,
    pub sort_descending: bool,
    /// Per-column hover fill alpha (0..=1); absent = 0.
    pub header_hover: &'a [(HeaderColumn, f32)],
    /// Header cell with the button held (fires on release, like a Win32 header control).
    pub header_pressed: Option<HeaderColumn>,
    pub scrollbar: Option<ScrollbarDraw>,
    pub empty_text: Option<&'a str>,
    /// Fence-wide drop wash + accent ring alpha 0..=1 (a drag hovers the fence itself).
    pub drop_highlight: f32,
    pub marquee: Option<Rect>,
    /// Insertion caret (accent bar) while an internal drag reorders the rows, and its alpha.
    pub insert_caret: Option<Rect>,
    pub insert_caret_alpha: f32,
    /// 0 = accent selection (active window), 1 = neutral grey (another window is active).
    pub selection_inactive: f32,
}

/// Pixel snapping for hairlines. The panel applies a uniform DIP→px scale, so a 1 DIP stroke
/// at a .5 DIP offset smears over two device pixels at 125 / 150 %; Explorer's hairlines are
/// exactly one device pixel, layout-rounded to the grid at every DPI.
#[derive(Clone, Copy, Debug)]
pub(super) struct Px {
    pub(super) grid: PixelGrid,
}

impl Px {
    pub(super) fn new(scale: f32) -> Self {
        Self {
            grid: PixelGrid::new(scale),
        }
    }

    pub(super) fn hair(self) -> f32 {
        self.grid.hairline()
    }
    pub(super) fn snap(self, v: f32) -> f32 {
        self.grid.snap(v)
    }

    /// Snaps every edge of `r` to the grid, then insets by half a hairline so a `hair()`-wide
    /// stroke covers whole pixels.
    pub(super) fn stroke_rect(self, r: Rect) -> Rect {
        let h = self.hair() / 2.0;
        let (l, t, rr, b) = (
            self.snap(r.left) + h,
            self.snap(r.top) + h,
            self.snap(r.right) - h,
            self.snap(r.bottom) - h,
        );
        Rect {
            left: l,
            top: t,
            right: rr.max(l),
            bottom: b.max(t),
        }
    }

    /// A rounded hairline ring inside `r` with the given outer corner radius.
    pub(super) fn ring(self, r: Rect, radius: f32) -> RoundedRect {
        RoundedRect::uniform(self.stroke_rect(r), (radius - self.hair() / 2.0).max(0.0))
    }
}

/// Per-fence look: colour wash, title colour and size (Fences per-fence appearance).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FenceStyle {
    pub tint: Option<ColorF>,
    pub title_color: Option<ColorF>,
    /// 0 = small (12), 1 = normal (14), 2 = large (16).
    pub title_size: u8,
}

/// Title-row decorations of a folder portal: a folder glyph before the title and, once the
/// portal has navigated into a subfolder, an "up" button at the far left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TitleDeco {
    pub folder_icon: bool,
    pub up_button: bool,
    /// Windows caption buttons (Minimize, Maximize/Restore, Close) at the right end,
    /// in place of the roll-up chevron: app-style fences whose panel publishes chrome.
    pub captions: bool,
}

/// One of the Windows caption buttons, right to left: Close is at the window edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CaptionButton {
    Minimize,
    Maximize,
    Close,
}

impl CaptionButton {
    pub const ALL: [CaptionButton; 3] = [Self::Minimize, Self::Maximize, Self::Close];

    fn slot(self) -> usize {
        match self {
            Self::Close => 0,
            Self::Maximize => 1,
            Self::Minimize => 2,
        }
    }
}

/// Interaction state of the title row, sampled per draw: fade alphas (0..=1) from the app's
/// client tweens plus the instant pressed flags (WinUI swaps pressed brushes at KeyTime 0).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TitleState {
    /// Title-row reveal ("title on hover"): 0 leaves only glass and rim.
    pub title: f32,
    /// The title row is hovered: full-row pill (single-title fences) and the chevron reveal.
    pub hover: f32,
    /// Up button (portal) hover fill / pressed.
    pub up_hover: f32,
    pub up_pressed: bool,
    /// Roll-up chevron hover fill / pressed.
    pub chevron_hover: f32,
    pub chevron_pressed: bool,
    /// Rolled item-count caption.
    pub count: f32,
    /// Another fence is being dragged over this title (drop here to merge it as a tab): the
    /// accent wash + ring alpha, 83 ms in / 167 ms out.
    pub merge_hint: f32,
    /// Caption button under the pointer / held down (`TitleDeco::captions`).
    pub caption_hover: Option<CaptionButton>,
    pub caption_pressed: Option<CaptionButton>,
    /// The fence fills its work area: the middle caption button shows Restore.
    pub maximized: bool,
}

impl TitleDeco {
    /// Width of the up button zone (DIPs) — the app hit-tests the same rectangle.
    pub const UP_X: f32 = 6.0;
    pub const UP_W: f32 = 28.0;
    pub const ICON_W: f32 = 24.0;
    /// Roll-up chevron button: a 32 × 32 box at the right end of the title row.
    pub const CHEVRON_W: f32 = 32.0;
    pub const CHEVRON_RIGHT_MARGIN: f32 = 4.0;

    /// The up button box (DIPs) for a title row `title_h` tall.
    pub fn up_box(title_h: f32) -> Rect {
        Rect::from_xywh(Self::UP_X, (title_h - 28.0) / 2.0, Self::UP_W, 28.0)
    }

    /// The chevron box (DIPs) for a fence `width` wide with a title row `title_h` tall; the
    /// app hit-tests the same rectangle.
    pub fn chevron_box(width: f32, title_h: f32) -> Rect {
        Rect::from_xywh(
            width - Self::CHEVRON_W - Self::CHEVRON_RIGHT_MARGIN,
            (title_h - Self::CHEVRON_W) / 2.0,
            Self::CHEVRON_W,
            Self::CHEVRON_W,
        )
    }

    /// Panel command buttons: 28 × 28 boxes, 30 DIPs apart, right to left before the chevron
    /// (or before the caption buttons).
    pub const COMMAND_W: f32 = 30.0;
    /// Windows caption button width (title bar height tall).
    pub const CAPTION_W: f32 = 46.0;

    /// A caption button's box: full title-row height, flush with the right edge.
    pub fn caption_box(width: f32, title_h: f32, button: CaptionButton) -> Rect {
        Rect::from_xywh(
            width - (button.slot() + 1) as f32 * Self::CAPTION_W,
            0.0,
            Self::CAPTION_W,
            title_h,
        )
    }

    /// Where the title row's right-hand controls (chevron or caption buttons) begin.
    pub fn controls_left(&self, width: f32) -> f32 {
        if self.captions {
            width - 3.0 * Self::CAPTION_W - 4.0
        } else {
            width - Self::CHEVRON_W - Self::CHEVRON_RIGHT_MARGIN
        }
    }
    /// Widest status line before it ellipsizes.
    pub const STATUS_MAX_W: f32 = 220.0;

    /// Box of panel command `index` (0 = nearest the chevron or caption buttons); the app
    /// hit-tests the same rectangle.
    pub fn command_box(&self, width: f32, title_h: f32, index: usize) -> Rect {
        let right = self.controls_left(width);
        Rect::from_xywh(
            right - (index + 1) as f32 * Self::COMMAND_W,
            (title_h - 28.0) / 2.0,
            28.0,
            28.0,
        )
    }

    /// Where the title text starts.
    pub fn title_x(&self) -> f32 {
        let mut x = 16.0;
        if self.up_button {
            x += Self::UP_W;
        }
        if self.folder_icon {
            x += Self::ICON_W;
        }
        x
    }
}

/// One tab header in the title row (DIPs, x/w along the title).
pub struct TabDraw<'a> {
    pub key: u128,
    pub x: f32,
    pub w: f32,
    pub title: &'a str,
    pub title_size: u8,
    pub title_color: Option<ColorF>,
    pub active: bool,
    /// PointerOver pill alpha 0..=1.
    pub hover: f32,
    /// Button held on this tab (TabViewItem pressed: layer fill, tertiary caption).
    pub pressed: bool,
    /// The tab's fence tint, drawn as a thin bar under the caption.
    pub color: Option<ColorF>,
    /// Files are being dragged over this tab: drawn like a folder drop target (selection
    /// fill + accent ring), alpha 0..=1.
    pub drop_target: f32,
    /// This tab is being dragged along the strip: it rides under the pointer (`x` is the
    /// pointer-driven position, not its slot) and paints above its neighbours.
    pub dragging: bool,
    /// Whole-pill opacity 0..=1 (a tab that just joined the strip fades in).
    pub alpha: f32,
}

/// One panel command in the title row, sampled per draw.
#[derive(Clone, Copy, Debug)]
pub struct TitleCommandDraw {
    pub glyph: char,
    pub enabled: bool,
    /// Hover pill alpha 0..=1.
    pub hover: f32,
    pub pressed: bool,
}

/// Width of the "N 项" caption a rolled fence shows before its chevron (DIPs).
pub const ROLLED_COUNT_W: f32 = 64.0;

/// Nested controls share the fence's radius, inset from its outer edge and limited by
/// their own height. This also applies to hover, pressed, drop and merge feedback.
pub fn header_control_radius(theme: &Theme, rect: Rect) -> f32 {
    theme.material().control_radius(theme, rect)
}
pub fn tab_text_padding(theme: &Theme) -> f32 {
    theme.material().tab_padding()
}

/// Counts are secondary: a short collapsed fence must retain space for its title/tabs.
pub fn header_count_width(width: f32, tabs: usize, title_x: f32) -> f32 {
    let (left, captions) = if tabs > 1 {
        (
            8.0,
            tabs as f32 * 48.0 + tabs.saturating_sub(1) as f32 * 4.0,
        )
    } else {
        (title_x, 64.0)
    };
    if width - left - 44.0 - captions >= ROLLED_COUNT_W {
        ROLLED_COUNT_W
    } else {
        0.0
    }
}

/// Rotation of the ChevronUp glyph (degrees) for a roll progress 0 (expanded) ..= 1 (rolled):
/// half a turn, so the resting rolled state is ChevronDown.
pub fn chevron_angle(roll_t: f32) -> f32 {
    180.0 * roll_t.clamp(0.0, 1.0)
}

/// `BitmapCache` key of the acrylic noise tile (one upload per device).
/// How much of the fence plate is the (blurred, tinted) wallpaper crop and how much is the real
/// desktop behind the window showing through. 1.0 = the old fully opaque plate; the default lets
/// the desktop bleed through, which is what "transparency" means to a user. The per-fence
/// opacity setting (外观 → 更透明 / 更厚实) scales it.
pub const GLASS_OPACITY: f32 = 0.55;

pub(super) fn glass_alpha(opacity: f32) -> f32 {
    (GLASS_OPACITY * opacity).clamp(0.1, 1.0)
}

pub(super) const NOISE_BITMAP_KEY: &str = "backdrop:noise";

/// Extra selection-fill alpha of a hovered selected item (LISS_HOTSELECTED / WinUI
/// SelectedPointerOver): dark accent 0x33 -> 0x40, light 0x29 -> 0x36.
pub(super) const HOT_SELECTED_LIFT: f32 = 0x0D as f32 / 255.0;

pub struct ContentDraw<'a> {
    pub items: &'a [ItemCell<'a>],
    /// Section headers between the row blocks of grouped items.
    pub group_headers: &'a [GroupHeaderDraw<'a>],
    pub icon_size: f32,
    pub label_lines: u8,
    pub line_h: f32,
    /// Cell top pad above the icon and gap between icon and label (GridMetrics).
    pub icon_top: f32,
    pub label_gap: f32,
    /// Overlay scrollbar thumb when the content overflows.
    pub scrollbar: Option<ScrollbarDraw>,
    pub empty_text: Option<&'a str>,
    /// Fence-wide drop wash + accent ring alpha 0..=1 (a drag hovers the fence itself).
    pub drop_highlight: f32,
    /// Rubber-band selection rectangle in content DIPs (already scrolled), while dragging.
    pub marquee: Option<Rect>,
    /// Insertion caret (accent bar) while an internal drag reorders the grid, and its alpha.
    pub insert_caret: Option<Rect>,
    pub insert_caret_alpha: f32,
    /// 0 = accent selection (active window), 1 = neutral grey (another window is active).
    pub selection_inactive: f32,
}

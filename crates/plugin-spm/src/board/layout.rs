//! Width- and content-driven layout for the board.
//!
//! Coordinates are logical DIPs (before text scaling). Nodes outside the
//! scrolling region are cropped: fills and hit areas are intersected with it,
//! and a text line is drawn only when it fits completely, so scrolled content
//! never paints over the fixed header and never reflows while it scrolls.

#[path = "timeline.rs"]
pub(super) mod timeline;
pub use timeline::Zoom;

mod cards;
mod compact;
mod detail;
mod timeline_view;
mod workspace;

use cards::{card, card_height};
use compact::{capsule, compact};
use detail::detail;
use timeline_view::timeline;
use workspace::workspace;

use super::model::{self, DisplayColumn, Filter, Signal, Tone};
pub use super::palette::Palette;
use super::text::{BODY, Fitter, Lang, age, short_time};
use pecofence_plugin_api::scene::NodeStyle;
use pecofence_plugin_api::{FontRole, RectDip, TextAlign};
use spm_contracts::{
    BoardCard, BoardSnapshot, CardEvidenceResponse, EvidenceItem, ProgressState, RecordId,
    SourceRef, Swimlane,
};

pub const PAD: f32 = 16.0;
pub const GAP: f32 = 12.0;
pub const CARD_GAP: f32 = 8.0;
pub const CARD_MIN: f32 = 176.0;
pub const RAIL: f32 = 64.0;
pub const DETAIL_DOCK_MIN: f32 = 1100.0;
pub const COMPACT_MAX_W: f32 = 600.0;
pub const COMPACT_MAX_H: f32 = 420.0;

/// Content starts here: the header is drawn by the host from the panel's chrome.
pub(super) const TOP: f32 = 12.0;
pub(super) const NUMBER: f32 = 13.0;
pub(super) const TITLE: f32 = 14.0;
pub(super) const META: f32 = 12.0;

/// Something a control does. Actions index a per-layout table.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    View(BoardView),
    /// The CI/CD segment of the capsule: open the CI/CD fence at its focus.
    OpenCi,
    /// 「在 CI/CD 中跟踪」 on a Gerrit or Jenkins source: follow it and open the fence there.
    TrackCi(SourceRef),
    Zoom(Zoom),
    Event(RecordId, String),
    OpenEvidence,
    ShowOnBoard,
    Refresh,
    CopySummary,
    RestartDemo,
    Model(Option<String>),
    CycleModel,
    Attention,
    Find,
    ClearQuery,
    ClearFilters,
    Select(RecordId),
    CloseDetail,
    ToggleLane(Swimlane),
    Tab(DisplayColumn),
    ToggleDone,
    OpenSource(SourceRef),
    Copy(String),
    SelectEvidence(String),
    RetryEvidence,
    RetryLoad,
}

/// Presentation state belongs to the panel instance, never BoardConfig.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoardView {
    #[default]
    Board,
    Timeline,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub key: String,
    pub rect: RectDip,
    pub role: &'static str,
    pub name: String,
    pub text: String,
    pub action: Option<usize>,
    pub style: NodeStyle,
}

/// Card geometry for keyboard navigation, in board content coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct NavCard {
    pub id: RecordId,
    pub column: usize,
    pub lane: usize,
    pub y: f32,
    pub h: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub nodes: Vec<Draft>,
    pub targets: Vec<Target>,
    pub nav: Vec<NavCard>,
    pub board: Option<RectDip>,
    pub board_content: f32,
    pub detail: Option<RectDip>,
    pub detail_content: f32,
    /// Columns in tab mode, in display order.
    pub tabs: Vec<DisplayColumn>,
    pub mode: Mode,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Board,
    /// One column at a time, chosen with tabs.
    Tabs,
    /// The detail replaces the board.
    Detail,
    Compact,
    CompactDetail,
    Capsule,
}

/// Everything layout reads. Built by the panel from its state.
pub struct Input<'a> {
    pub board: Option<&'a BoardSnapshot>,
    /// The latest CI snapshot from the monitor (empty when none has arrived).
    pub ci: &'a spm_contracts::CiWatchSnapshot,
    pub view: BoardView,
    pub zoom: Zoom,
    pub evidence_open: bool,
    pub previous_error: Option<&'a str>,
    pub loading_hint: Option<&'a str>,
    pub filter: &'a Filter,
    pub selected: Option<&'a RecordId>,
    pub detail_open: bool,
    pub collapsed: &'a [Swimlane],
    pub tab: DisplayColumn,
    pub done_open: bool,
    pub scroll: f32,
    pub detail_scroll: f32,
    pub evidence: EvidenceView<'a>,
    pub preview_event: Option<&'a str>,
    pub preview_evidence: EvidenceView<'a>,
    pub selected_evidence: Option<&'a str>,
    pub status: Option<(&'a str, Option<Tone>)>,
    pub demo: Option<(usize, usize, &'a str)>,
    pub refreshing: bool,
    pub capsule: bool,
    pub now: chrono::DateTime<chrono::Utc>,
    pub lang: Lang,
}

#[derive(Clone, Copy, Debug)]
pub enum EvidenceView<'a> {
    None,
    Loading,
    Failed(&'a str),
    Loaded(&'a CardEvidenceResponse),
}

pub(super) struct Builder<'a> {
    pub(super) out: Layout,
    pub(super) pal: &'a Palette,
    pub(super) fit: &'a Fitter<'a>,
    pub(super) lang: Lang,
    pub(super) clip: Option<RectDip>,
}

pub(super) fn rect(x: f32, y: f32, w: f32, h: f32) -> RectDip {
    RectDip {
        x,
        y,
        w: w.max(0.0),
        h: h.max(0.0),
    }
}

pub(super) fn intersect(a: RectDip, b: RectDip) -> Option<RectDip> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let r = (a.x + a.w).min(b.x + b.w);
    let bottom = (a.y + a.h).min(b.y + b.h);
    (r > x && bottom > y).then(|| rect(x, y, r - x, bottom - y))
}

pub(super) fn style(
    size: f32,
    weight: u16,
    font: FontRole,
    foreground: [f32; 4],
    fill: Option<[f32; 4]>,
    radius: f32,
    align: TextAlign,
) -> NodeStyle {
    NodeStyle {
        size,
        weight,
        font,
        foreground,
        fill,
        stroke: None,
        radius,
        text_inset: [0.0, 0.0],
        align,
        path: None,
    }
}

impl<'a> Builder<'a> {
    pub(super) fn push(
        &mut self,
        key: String,
        mut r: RectDip,
        role: &'static str,
        name: String,
        text: String,
        action: Option<usize>,
        style: NodeStyle,
    ) -> bool {
        if let Some(clip) = self.clip {
            if !text.is_empty() {
                // Text never reflows while scrolling: draw it whole or not at all.
                if r.y < clip.y - 0.5 || r.y + r.h > clip.y + clip.h + 0.5 {
                    return false;
                }
            } else {
                match intersect(r, clip) {
                    Some(clipped) => r = clipped,
                    None => return false,
                }
            }
        }
        if r.w <= 0.0 || r.h <= 0.0 {
            return false;
        }
        self.out.nodes.push(Draft {
            key,
            rect: r,
            role,
            name,
            text,
            action,
            style,
        });
        true
    }

    pub(super) fn fill(&mut self, key: String, r: RectDip, color: [f32; 4], radius: f32) {
        self.push(
            key,
            r,
            "presentation",
            String::new(),
            String::new(),
            None,
            style(
                META,
                400,
                FontRole::Text,
                color,
                Some(color),
                radius,
                TextAlign::TopLeading,
            ),
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn text(
        &mut self,
        key: String,
        r: RectDip,
        text: &str,
        size: f32,
        weight: u16,
        color: [f32; 4],
        align: TextAlign,
    ) {
        if text.is_empty() {
            return;
        }
        self.push(
            key,
            r,
            "text",
            text.to_owned(),
            text.to_owned(),
            None,
            style(size, weight, BODY, color, None, 0.0, align),
        );
    }

    /// A paper-style title in the serif face; measure it with `fit.with_font(SERIF)`.
    fn serif(&mut self, key: String, r: RectDip, text: &str, size: f32, color: [f32; 4]) {
        if text.is_empty() {
            return;
        }
        self.push(
            key,
            r,
            "heading",
            text.to_owned(),
            text.to_owned(),
            None,
            style(
                size,
                600,
                FontRole::Serif,
                color,
                None,
                0.0,
                TextAlign::TopLeading,
            ),
        );
    }

    fn glyph(&mut self, key: String, r: RectDip, glyph: &str, size: f32, color: [f32; 4]) {
        self.push(
            key,
            r,
            "presentation",
            String::new(),
            glyph.to_owned(),
            None,
            style(
                size,
                400,
                FontRole::Icons,
                color,
                None,
                0.0,
                TextAlign::Center,
            ),
        );
    }

    fn target(&mut self, target: Target) -> usize {
        self.out.targets.push(target);
        self.out.targets.len() - 1
    }

    /// An invisible pointer and keyboard target over already painted content.
    fn hit(&mut self, key: String, r: RectDip, role: &'static str, name: String, target: Target) {
        let action = self.target(target);
        let pushed = self.push(
            key,
            r,
            role,
            name,
            String::new(),
            Some(action),
            style(
                META,
                400,
                FontRole::Text,
                self.pal.primary,
                None,
                6.0,
                TextAlign::TopLeading,
            ),
        );
        if !pushed {
            self.out.targets.pop();
        }
    }

    /// A labelled control with an optional leading glyph. Returns its width.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn button(
        &mut self,
        key: &str,
        x: f32,
        y: f32,
        h: f32,
        glyph: Option<&str>,
        label: &str,
        on: bool,
        target: Target,
        max_w: f32,
    ) -> f32 {
        let pal = *self.pal;
        let label_w = if label.is_empty() {
            0.0
        } else {
            self.fit.width(label, META, 600)
        };
        let glyph_w = if glyph.is_some() {
            16.0 + if label.is_empty() { 0.0 } else { 6.0 }
        } else {
            0.0
        };
        let w = (label_w.ceil() + glyph_w + 26.0).min(max_w).max(h);
        let r = rect(x, y, w, h);
        let (fill, ink) = if on {
            (pal.chip_on, pal.chip_on_text)
        } else {
            (pal.chip, pal.primary)
        };
        self.fill(format!("{key}:bg"), r, fill, h / 2.0);
        if pal.hc && !on {
            self.outline(&format!("{key}:hc"), r, pal.primary, 1.0, h / 2.0, pal.base);
        }
        let mut cx = x + 12.0;
        if let Some(glyph) = glyph {
            self.glyph(
                format!("{key}:glyph"),
                rect(cx, y, 16.0, h),
                glyph,
                14.0,
                ink,
            );
            cx += glyph_w;
        }
        if !label.is_empty() {
            let (fitted, _) = self
                .fit
                .fit(label, META, 600, (x + w - 10.0 - cx).max(1.0), 1);
            self.text(
                format!("{key}:label"),
                rect(cx, y, x + w - 10.0 - cx, h),
                &fitted,
                META,
                600,
                ink,
                TextAlign::MiddleLeading,
            );
        }
        let name = if label.is_empty() {
            glyph.unwrap_or_default().to_owned()
        } else {
            label.to_owned()
        };
        self.hit(format!("{key}:hit"), r, "button", name, target);
        w
    }

    /// Rounded outline drawn as two fills, since scene strokes are rectangular.
    fn outline(
        &mut self,
        key: &str,
        r: RectDip,
        color: [f32; 4],
        width: f32,
        radius: f32,
        inner: [f32; 4],
    ) {
        self.fill(format!("{key}:outer"), r, color, radius);
        self.fill(
            format!("{key}:inner"),
            rect(
                r.x + width,
                r.y + width,
                r.w - 2.0 * width,
                r.h - 2.0 * width,
            ),
            inner,
            (radius - width).max(0.0),
        );
    }

    fn line_h(&self, size: f32) -> f32 {
        self.fit.line_height(size).ceil()
    }
}

pub fn build(input: &Input, viewport: RectDip, pal: &Palette, fit: &Fitter) -> Layout {
    let mut b = Builder {
        out: Layout::default(),
        pal,
        fit,
        lang: input.lang,
        clip: None,
    };
    if let Some(ground) = pal.ground {
        b.fill("ground".into(), viewport, ground, 0.0);
    }
    let header_h = if input.capsule { 0.0 } else { 38.0 };
    if !input.capsule {
        let mut x = PAD;
        for (view, label) in [
            (BoardView::Board, "Board"),
            (BoardView::Timeline, "Timeline"),
        ] {
            x += b.button(
                &format!("view:{view:?}"),
                x,
                4.0,
                28.0,
                None,
                input.lang.t(label),
                input.view == view,
                Target::View(view),
                160.0,
            ) + 4.0;
        }
    }
    let start = b.out.nodes.len();
    let content = rect(viewport.x, viewport.y, viewport.w, viewport.h - header_h);
    if input.capsule {
        b.out.mode = Mode::Capsule;
        capsule(&mut b, input, viewport);
    } else if input.view == BoardView::Timeline && input.board.is_some() {
        timeline::workspace(&mut b, input, content);
    } else if viewport.w < COMPACT_MAX_W || viewport.h < COMPACT_MAX_H {
        compact(&mut b, input, content);
    } else {
        workspace(&mut b, input, content);
    }
    for node in &mut b.out.nodes[start..] {
        node.rect.y += header_h;
    }
    if let Some(r) = &mut b.out.board {
        r.y += header_h;
    }
    if let Some(r) = &mut b.out.detail {
        r.y += header_h;
    }
    b.out
}

// ---------------------------------------------------------------------------
// Shared pieces

pub(super) struct CardInfo<'a> {
    pub(super) card: &'a BoardCard,
    signals: Vec<Signal>,
    pub(super) column: DisplayColumn,
}

fn visible_cards<'a>(board: &'a BoardSnapshot, filter: &Filter) -> Vec<CardInfo<'a>> {
    let degraded = model::degraded_sources(board);
    board
        .cards
        .iter()
        .map(|card| CardInfo {
            signals: model::signals(card, &degraded),
            column: DisplayColumn::of(card.stage.stage),
            card,
        })
        .filter(|info| filter.matches(info.card, &info.signals))
        .collect()
}

pub(super) fn count_text(n: usize, complete: bool) -> String {
    if complete {
        n.to_string()
    } else {
        format!("≥{n}")
    }
}

pub(super) fn lane_name(lane: &Swimlane) -> String {
    lane.model.clone().unwrap_or_else(|| lane.project.clone())
}

pub(super) fn status_line(b: &mut Builder, input: &Input, r: RectDip) {
    if let Some((text, tone)) = input.status {
        let color = tone.map_or(b.pal.secondary, |t| b.pal.tone(t));
        let (fitted, _) = b.fit.fit(text, META, 400, r.w, 1);
        b.text(
            "status".into(),
            r,
            &fitted,
            META,
            400,
            color,
            TextAlign::MiddleLeading,
        );
    }
}

/// "Data as of …" plus degraded and stale sources. Returns the drawn height.
pub(super) fn freshness(
    b: &mut Builder,
    board: &BoardSnapshot,
    input: &Input,
    x: f32,
    y: f32,
    w: f32,
) -> f32 {
    let lang = b.lang;
    let tz = board.project_timezone.as_str();
    let mut text = format!(
        "{} · {}",
        lang.f("Data as of {}", &[&short_time(board.computed_at, tz)]),
        lang.f("rev {}", &[&board.revision.0.to_string()])
    );
    text.push_str(" · ");
    text.push_str(&lang.f("Times in {}", &[tz]));
    let mut warnings = Vec::new();
    for source in &board.sources {
        let name = model::system_label(source.source);
        if source.error.is_some() || source.coverage != spm_contracts::CoverageState::Complete {
            warnings.push(lang.f("{} did not refresh", &[name]));
        } else if (input.now - source.observed_at).num_seconds()
            > i64::from(source.freshness_ttl_secs)
        {
            warnings.push(lang.f(
                "{} is {} old",
                &[name, &age(source.observed_at, input.now, lang)],
            ));
        }
    }
    if !board.totals_complete {
        warnings.push(lang.t("Totals may be low").to_owned());
    }
    let info_w = b.fit.width(&text, META, 400).min(w * 0.55);
    let (fitted, _) = b.fit.fit(&text, META, 400, info_w + 1.0, 1);
    b.text(
        "fresh:info".into(),
        rect(x, y, info_w + 2.0, 20.0),
        &fitted,
        META,
        400,
        b.pal.secondary,
        TextAlign::MiddleLeading,
    );
    if !warnings.is_empty() {
        let wx = x + info_w + 14.0;
        b.glyph(
            "fresh:warn:glyph".into(),
            rect(wx, y, 16.0, 20.0),
            "\u{E823}",
            12.0,
            b.pal.warning,
        );
        let joined = warnings.join(" · ");
        let (fitted, _) = b
            .fit
            .fit(&joined, META, 600, (x + w - wx - 20.0).max(1.0), 1);
        b.text(
            "fresh:warn".into(),
            rect(wx + 20.0, y, x + w - wx - 20.0, 20.0),
            &fitted,
            META,
            600,
            b.pal.warning,
            TextAlign::MiddleLeading,
        );
    }
    20.0
}

pub(super) fn scrollbar(
    b: &mut Builder,
    key: &str,
    region: RectDip,
    offset: f32,
    content: f32,
    x: f32,
) {
    let ratio = (region.h / content).clamp(0.08, 1.0);
    let h = region.h * ratio;
    let travel = region.h - h;
    let t = (offset / (content - region.h).max(1.0)).clamp(0.0, 1.0);
    let color = [
        b.pal.secondary[0],
        b.pal.secondary[1],
        b.pal.secondary[2],
        0.45,
    ];
    b.fill(
        format!("{key}:scroll"),
        rect(x, region.y + travel * t + 2.0, 3.0, h - 4.0),
        color,
        1.5,
    );
}

pub(super) fn loading(b: &mut Builder, input: &Input, viewport: RectDip) {
    let lang = b.lang;
    let pal = *b.pal;
    match input.previous_error {
        Some(error) => {
            b.glyph(
                "load:glyph".into(),
                rect(PAD, 20.0, 24.0, 28.0),
                "\u{E783}",
                18.0,
                pal.danger,
            );
            b.text(
                "load:title".into(),
                rect(PAD + 32.0, 20.0, viewport.w - 64.0, 28.0),
                lang.t("The board did not load."),
                16.0,
                700,
                pal.primary,
                TextAlign::MiddleLeading,
            );
            let (fitted, lines) = b
                .fit
                .fit(error, META, 400, viewport.w - 2.0 * PAD - 32.0, 3);
            let lh = b.line_h(META);
            b.text(
                "load:error".into(),
                rect(
                    PAD + 32.0,
                    52.0,
                    viewport.w - 2.0 * PAD - 32.0,
                    lines as f32 * lh,
                ),
                &fitted,
                META,
                400,
                pal.secondary,
                TextAlign::TopLeading,
            );
            b.button(
                "load:retry",
                PAD + 32.0,
                60.0 + lines as f32 * lh,
                32.0,
                Some("\u{E72C}"),
                lang.t("Try again"),
                false,
                Target::RetryLoad,
                200.0,
            );
        }
        None => {
            let retry_w = if input.loading_hint.is_some() {
                // Waiting too long: offer the one action that can help.
                let w = b.fit.width(lang.t("Try again"), META, 600) + 50.0;
                b.button(
                    "load:retry",
                    viewport.w - PAD - w,
                    12.0,
                    32.0,
                    Some("\u{E72C}"),
                    lang.t("Try again"),
                    false,
                    Target::RetryLoad,
                    w,
                );
                w + 8.0
            } else {
                0.0
            };
            b.text(
                "load:title".into(),
                rect(PAD, 12.0, viewport.w - 2.0 * PAD - retry_w, 32.0),
                input.loading_hint.unwrap_or(lang.t("Loading the board…")),
                16.0,
                600,
                pal.secondary,
                TextAlign::MiddleLeading,
            );
            // Skeleton columns keep the layout stable when the data arrives.
            let compact = viewport.w < COMPACT_MAX_W || viewport.h < COMPACT_MAX_H;
            let n = if compact { 1.0 } else { 4.0 };
            let cw = (viewport.w - 2.0 * PAD - (n - 1.0) * GAP) / n;
            for i in 0..n as usize {
                let x = PAD + i as f32 * (cw + GAP);
                b.fill(
                    format!("skel:col:{i}"),
                    rect(x, 60.0, cw, viewport.h - 100.0),
                    pal.column,
                    10.0,
                );
                for j in 0..3 {
                    let y = 72.0 + j as f32 * 96.0;
                    if y + 84.0 > viewport.h - 44.0 {
                        break;
                    }
                    b.fill(
                        format!("skel:card:{i}:{j}"),
                        rect(x + 6.0, y, cw - 12.0, 84.0),
                        pal.card,
                        8.0,
                    );
                    b.fill(
                        format!("skel:line:{i}:{j}:a"),
                        rect(x + 20.0, y + 14.0, (cw - 40.0) * 0.4, 10.0),
                        pal.skeleton,
                        3.0,
                    );
                    b.fill(
                        format!("skel:line:{i}:{j}:b"),
                        rect(x + 20.0, y + 34.0, cw - 40.0, 10.0),
                        pal.skeleton,
                        3.0,
                    );
                    b.fill(
                        format!("skel:line:{i}:{j}:c"),
                        rect(x + 20.0, y + 54.0, (cw - 40.0) * 0.7, 10.0),
                        pal.skeleton,
                        3.0,
                    );
                }
            }
        }
    }
    status_line(
        b,
        input,
        rect(PAD, viewport.h - 30.0, viewport.w - 2.0 * PAD, 24.0),
    );
}

// ---------------------------------------------------------------------------
// Detail

pub(super) fn section(b: &mut Builder, key: &str, label: &str, x: f32, y: f32, w: f32) -> f32 {
    b.fill(
        format!("{key}:rule"),
        rect(x, y + 8.0, w, 1.0),
        b.pal.rule,
        0.0,
    );
    b.text(
        format!("{key}:label"),
        rect(x, y + 14.0, w, 20.0),
        label,
        META,
        700,
        b.pal.secondary,
        TextAlign::MiddleLeading,
    );
    y + 38.0
}

/// A label column and a value that wraps. Returns the next y.
#[allow(clippy::too_many_arguments)]
pub(super) fn row(
    b: &mut Builder,
    key: &str,
    label: &str,
    glyph: Option<(&str, [f32; 4])>,
    value: &str,
    note: &str,
    x: f32,
    y: f32,
    w: f32,
    emphasis: bool,
) -> f32 {
    let pal = *b.pal;
    let label_w = 132.0;
    let vx = x + label_w + if glyph.is_some() { 20.0 } else { 0.0 };
    let vw = (x + w - vx).max(40.0);
    let lh = b.line_h(13.0);
    let mh = b.line_h(META);
    let (label_fit, _) = b.fit.fit(label, META, 400, label_w - 8.0, 1);
    b.text(
        format!("{key}:k"),
        rect(x, y, label_w - 8.0, lh),
        &label_fit,
        META,
        400,
        pal.secondary,
        TextAlign::MiddleLeading,
    );
    if let Some((glyph, color)) = glyph {
        b.glyph(
            format!("{key}:g"),
            rect(x + label_w, y, 16.0, lh),
            glyph,
            12.0,
            color,
        );
    }
    let (value_fit, lines) = b
        .fit
        .fit(value, 13.0, if emphasis { 700 } else { 400 }, vw, 3);
    b.text(
        format!("{key}:v"),
        rect(vx, y, vw, lines as f32 * lh),
        &value_fit,
        13.0,
        if emphasis { 700 } else { 400 },
        pal.primary,
        TextAlign::TopLeading,
    );
    let mut ny = y + lines as f32 * lh;
    if !note.is_empty() {
        let (note_fit, nl) = b.fit.fit(note, META, 400, vw, 3);
        b.text(
            format!("{key}:n"),
            rect(vx, ny, vw, nl as f32 * mh),
            &note_fit,
            META,
            400,
            pal.secondary,
            TextAlign::TopLeading,
        );
        ny += nl as f32 * mh;
    }
    ny + 8.0
}

pub fn lane_key(lane: &Swimlane) -> String {
    format!("{}/{}", lane.project, lane.model.as_deref().unwrap_or(""))
}

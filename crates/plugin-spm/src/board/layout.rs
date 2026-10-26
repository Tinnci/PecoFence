//! Width- and content-driven layout for the board.
//!
//! Coordinates are logical DIPs (before text scaling). Nodes outside the
//! scrolling region are cropped: fills and hit areas are intersected with it,
//! and a text line is drawn only when it fits completely, so scrolled content
//! never paints over the fixed header and never reflows while it scrolls.

#[path = "timeline.rs"]
pub(super) mod timeline;
pub use timeline::Zoom;

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
const TOP: f32 = 12.0;
const NUMBER: f32 = 13.0;
const TITLE: f32 = 14.0;
const META: f32 = 12.0;

/// Something a control does. Actions index a per-layout table.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    View(BoardView),
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

struct Builder<'a> {
    out: Layout,
    pal: &'a Palette,
    fit: &'a Fitter<'a>,
    lang: Lang,
    clip: Option<RectDip>,
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> RectDip {
    RectDip {
        x,
        y,
        w: w.max(0.0),
        h: h.max(0.0),
    }
}

fn intersect(a: RectDip, b: RectDip) -> Option<RectDip> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let r = (a.x + a.w).min(b.x + b.w);
    let bottom = (a.y + a.h).min(b.y + b.h);
    (r > x && bottom > y).then(|| rect(x, y, r - x, bottom - y))
}

fn style(
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
    fn push(
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

    fn fill(&mut self, key: String, r: RectDip, color: [f32; 4], radius: f32) {
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
    fn text(
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
    fn button(
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

struct CardInfo<'a> {
    card: &'a BoardCard,
    signals: Vec<Signal>,
    column: DisplayColumn,
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

fn count_text(n: usize, complete: bool) -> String {
    if complete {
        n.to_string()
    } else {
        format!("≥{n}")
    }
}

fn lane_name(lane: &Swimlane) -> String {
    lane.model.clone().unwrap_or_else(|| lane.project.clone())
}

fn status_line(b: &mut Builder, input: &Input, r: RectDip) {
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
fn freshness(b: &mut Builder, board: &BoardSnapshot, input: &Input, x: f32, y: f32, w: f32) -> f32 {
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

/// One card. `y` is in final coordinates (already scrolled). Returns its height.
/// Card anatomy: number and lead signal; title (two lines); current assignee;
/// code · build · test bar with the facts; next action.
fn card(b: &mut Builder, info: &CardInfo, x: f32, y: f32, w: f32, selected: bool) -> f32 {
    let pal = *b.pal;
    let lang = b.lang;
    let card = info.card;
    let key = format!("card:{}", card.card_id.0);
    let inner_x = x + 12.0;
    let inner_w = w - 24.0;
    let number = model::label_of(card);
    let h = card_height(b, info, w);
    let r = rect(x, y, w, h);
    if selected {
        b.outline(&format!("{key}:sel"), r, pal.accent, 1.5, 8.0, pal.selected);
    } else if pal.hc {
        b.outline(&format!("{key}:hc"), r, pal.primary, 1.0, 8.0, pal.card);
    } else {
        b.fill(format!("{key}:bg"), r, pal.card, 8.0);
    }
    let meta_lh = b.line_h(META).max(18.0);
    let title_lh = b.line_h(TITLE);

    // Line 1: requirement number; the most severe signal on the right, as glyph and word.
    let lead = info.signals.iter().copied().find(|s| {
        (s.tone() != Tone::Info || *s == Signal::WorkflowLag) && *s != Signal::Unassigned
    });
    let mut flag_w = 0.0;
    if let Some(signal) = lead {
        let color = pal.tone(signal.tone());
        let (label, _) = b
            .fit
            .fit(signal.label(lang), META, 600, inner_w * 0.55 - 16.0, 1);
        flag_w = b.fit.width(&label, META, 600).ceil() + 16.0;
        let fx = inner_x + inner_w - flag_w;
        b.glyph(
            format!("{key}:flag:glyph"),
            rect(fx, y + 10.0, 14.0, 18.0),
            signal.glyph(),
            11.0,
            color,
        );
        b.text(
            format!("{key}:flag"),
            rect(fx + 16.0, y + 10.0, flag_w - 16.0, 18.0),
            &label,
            META,
            600,
            color,
            TextAlign::MiddleLeading,
        );
    }
    let (number_fit, _) = b.fit.fit(&number, NUMBER, 600, inner_w - flag_w - 8.0, 1);
    b.text(
        format!("{key}:number"),
        rect(inner_x, y + 10.0, inner_w - flag_w - 8.0, 18.0),
        &number_fit,
        NUMBER,
        600,
        pal.primary,
        TextAlign::MiddleLeading,
    );
    let mut cy = y + 10.0 + 18.0 + 4.0;

    let (title, title_lines) = b.fit.fit(&card.title, TITLE, 400, inner_w, 2);
    b.text(
        format!("{key}:title"),
        rect(inner_x, cy, inner_w, title_lines as f32 * title_lh),
        &title,
        TITLE,
        400,
        pal.primary,
        TextAlign::TopLeading,
    );
    cy += title_lines as f32 * title_lh + 6.0;

    // Line 3: who holds it now. An empty slot is a warning, with its own glyph.
    let assignee = card.assignee().map(|a| a.person.display_name.clone());
    let done = card.stage.stage == spm_contracts::BoardStage::Done;
    let (who, who_color) = match &assignee {
        Some(name) => (name.clone(), pal.secondary),
        None if !done => (lang.t("Unassigned").to_owned(), pal.warning),
        None => (String::new(), pal.secondary),
    };
    if !who.is_empty() {
        let d = 20.0;
        let ay = cy + (meta_lh - d) / 2.0;
        if assignee.is_some() {
            b.fill(
                format!("{key}:avatar"),
                rect(inner_x, ay, d, d),
                pal.chip_avatar(),
                d / 2.0,
            );
            let initial: String = who.chars().next().into_iter().collect();
            b.text(
                format!("{key}:avatar:t"),
                rect(inner_x, ay, d, d),
                &initial,
                11.0,
                600,
                pal.primary,
                TextAlign::Center,
            );
        } else {
            b.glyph(
                format!("{key}:avatar:g"),
                rect(inner_x, ay, d, d),
                Signal::Unassigned.glyph(),
                12.0,
                who_color,
            );
        }
        let (fitted, _) = b.fit.fit(&who, META, 400, inner_w - d - 6.0, 1);
        b.text(
            format!("{key}:who"),
            rect(inner_x + d + 6.0, cy, inner_w - d - 6.0, meta_lh),
            &fitted,
            META,
            400,
            who_color,
            TextAlign::MiddleLeading,
        );
        cy += meta_lh + 2.0;
    }

    // Line 4: implementation progress, separate from the workflow column.
    let code = &card.progress.code;
    let code_state = if code.linked > 0 && code.merged == code.linked {
        ProgressState::Passed
    } else if code.linked > 0 {
        ProgressState::InProgress
    } else {
        ProgressState::NotStarted
    };
    for (i, state) in [code_state, card.progress.build, card.progress.verification]
        .into_iter()
        .enumerate()
    {
        let color = match state {
            ProgressState::Passed => pal.success,
            ProgressState::Failed => pal.danger,
            ProgressState::InProgress => pal.accent,
            _ => pal.rule,
        };
        b.fill(
            format!("{key}:seg:{i}"),
            rect(
                inner_x + i as f32 * 16.0,
                cy + meta_lh / 2.0 - 2.0,
                14.0,
                4.0,
            ),
            color,
            2.0,
        );
    }
    let facts = model::fact_line(card, lang);
    let (fitted, _) = b.fit.fit(&facts, META, 400, inner_w - 54.0, 1);
    b.text(
        format!("{key}:facts"),
        rect(inner_x + 54.0, cy, inner_w - 54.0, meta_lh),
        &fitted,
        META,
        400,
        pal.secondary,
        TextAlign::MiddleLeading,
    );
    cy += meta_lh;

    let action = next_action_text(card);
    if let Some(action) = &action {
        cy += 2.0;
        let (fitted, _) = b.fit.fit(action, META, 400, inner_w, 1);
        b.text(
            format!("{key}:next"),
            rect(inner_x, cy, inner_w, meta_lh),
            &fitted,
            META,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
    }

    let mut name = format!("{number}, {}, {}", card.title, info.column.label(lang));
    if let Some(a) = &assignee {
        name.push_str(&format!(", {a}"));
    }
    for s in &info.signals {
        name.push_str(&format!(", {}", s.label(lang)));
    }
    if let Some(a) = &action {
        name.push_str(&format!(", {a}"));
    }
    b.hit(
        format!("{key}:hit"),
        r,
        "listitem",
        name,
        Target::Select(card.card_id.clone()),
    );
    h
}

fn next_action_text(card: &BoardCard) -> Option<String> {
    card.next_action.as_ref().map(|a| match &a.owner {
        Some(owner) => format!("→ {}: {}", model::first_name(owner), a.text),
        None => format!("→ {}", a.text),
    })
}

fn card_height(b: &Builder, info: &CardInfo, w: f32) -> f32 {
    let card = info.card;
    let (_, lines) = b.fit.fit(&card.title, TITLE, 400, w - 24.0, 2);
    let meta_lh = b.line_h(META).max(18.0);
    let who = card.assignee().is_some() || card.stage.stage != spm_contracts::BoardStage::Done;
    10.0 + 18.0
        + 4.0
        + lines as f32 * b.line_h(TITLE)
        + 6.0
        + if who { meta_lh + 2.0 } else { 0.0 }
        + meta_lh
        + if card.next_action.is_some() {
            2.0 + meta_lh
        } else {
            0.0
        }
        + 10.0
}

// ---------------------------------------------------------------------------
// Workspace

fn workspace(b: &mut Builder, input: &Input, viewport: RectDip) {
    let lang = b.lang;
    let pal = *b.pal;
    let Some(board) = input.board else {
        loading(b, input, viewport);
        return;
    };
    let dock = viewport.w >= DETAIL_DOCK_MIN;
    let detail_w = (viewport.w * 0.34).clamp(380.0, 460.0);
    let has_detail = input.detail_open
        && input
            .selected
            .is_some_and(|id| model::find(board, id).is_some());
    let board_w = if has_detail && dock {
        viewport.w - detail_w - GAP
    } else {
        viewport.w
    };
    if has_detail && !dock {
        b.out.mode = Mode::Detail;
        // The title, refresh and copy commands live in the host's title row (PanelChrome).
        let top = TOP;
        b.button(
            "back",
            PAD,
            top,
            32.0,
            Some("\u{E72B}"),
            lang.t("Back to board"),
            false,
            Target::CloseDetail,
            240.0,
        );
        let area = rect(
            PAD,
            top + 44.0,
            viewport.w - 2.0 * PAD,
            viewport.h - top - 44.0 - 36.0,
        );
        detail(b, board, input, area);
        status_line(
            b,
            input,
            rect(PAD, viewport.h - 30.0, viewport.w - 2.0 * PAD, 24.0),
        );
        return;
    }
    let fy = TOP;
    filters(b, board, input, PAD, fy, board_w - 2.0 * PAD);
    let fresh_y = fy + 36.0;
    freshness(b, board, input, PAD, fresh_y, board_w - 2.0 * PAD);
    let cards = visible_cards(board, input.filter);
    let columns: Vec<DisplayColumn> = DisplayColumn::ALL
        .iter()
        .copied()
        .filter(|c| {
            *c != DisplayColumn::Unmapped
                || cards.iter().any(|i| i.column == DisplayColumn::Unmapped)
        })
        .collect();
    // A tab that no longer exists (for example Unmapped after a refresh) falls back.
    let tab = if columns.contains(&input.tab) {
        input.tab
    } else if columns.contains(&DisplayColumn::Develop) {
        DisplayColumn::Develop
    } else {
        columns[0]
    };
    let avail = board_w - 2.0 * PAD;
    let n = columns.len() as f32;
    // Rails: Unmapped (rare, needs a policy fix rather than space) and Done (finished
    // work) shrink to a count before any working column gets narrower than a card.
    let roomy = n * 232.0 + (n - 1.0) * GAP <= avail;
    let unmapped_rail = !roomy && columns.contains(&DisplayColumn::Unmapped);
    let fits = |done_rail: bool| {
        let rails = f32::from(u8::from(unmapped_rail) + u8::from(done_rail));
        (n - rails) * CARD_MIN + rails * RAIL + (n - 1.0) * GAP <= avail
    };
    let done_rail = !roomy && !(input.done_open && fits(false)) && fits(true);
    let tabs = !fits(done_rail);
    let is_rail = |c: DisplayColumn| {
        (done_rail && c == DisplayColumn::Done) || (unmapped_rail && c == DisplayColumn::Unmapped)
    };
    let head_y = fresh_y + 30.0;
    let board_top = head_y + 36.0;
    let board_bottom = viewport.h - 36.0;
    let region = rect(0.0, board_top, board_w, (board_bottom - board_top).max(0.0));
    b.out.board = Some(region);
    // Column geometry.
    let mut xs = Vec::new();
    if tabs {
        b.out.mode = Mode::Tabs;
        b.out.tabs = columns.clone();
        let mut x = PAD;
        for column in &columns {
            let count = cards.iter().filter(|i| i.column == *column).count();
            let label = format!(
                "{} {}",
                column.label(lang),
                count_text(count, board.totals_complete)
            );
            let w = b.button(
                &format!("tab:{column:?}"),
                x,
                head_y,
                30.0,
                None,
                &label,
                *column == tab,
                Target::Tab(*column),
                200.0,
            );
            x += w + 6.0;
        }
        xs.push((tab, PAD, avail));
    } else {
        let rails = f32::from(u8::from(unmapped_rail) + u8::from(done_rail));
        let cw = (avail - rails * RAIL - (n - 1.0) * GAP) / (n - rails);
        let mut x = PAD;
        for column in &columns {
            let w = if is_rail(*column) { RAIL } else { cw };
            xs.push((*column, x, w));
            let count = cards.iter().filter(|i| i.column == *column).count();
            let unmapped = *column == DisplayColumn::Unmapped;
            let color = if unmapped { pal.warning } else { pal.primary };
            let label = column.label(lang);
            let mut lx = x + 4.0;
            if unmapped && !is_rail(*column) {
                b.glyph(
                    format!("colhead:{column:?}:glyph"),
                    rect(lx, head_y, 16.0, 28.0),
                    "\u{E7BA}",
                    12.0,
                    pal.warning,
                );
                lx += 20.0;
            }
            if is_rail(*column) {
                let text = format!(
                    "{} {}",
                    if unmapped { "\u{26A0}" } else { "\u{2713}" },
                    count_text(count, board.totals_complete)
                );
                b.text(
                    format!("colhead:{column:?}"),
                    rect(x, head_y, w, 28.0),
                    &text,
                    13.0,
                    700,
                    color,
                    TextAlign::Center,
                );
            } else {
                let lw = b.fit.width(label, 13.0, 700);
                b.text(
                    format!("colhead:{column:?}"),
                    rect(lx, head_y, lw + 2.0, 28.0),
                    label,
                    13.0,
                    700,
                    color,
                    TextAlign::MiddleLeading,
                );
                b.text(
                    format!("colhead:{column:?}:n"),
                    rect(lx + lw + 8.0, head_y, 60.0, 28.0),
                    &count_text(count, board.totals_complete),
                    13.0,
                    400,
                    pal.secondary,
                    TextAlign::MiddleLeading,
                );
            }
            x += w + GAP;
        }
    }
    b.fill(
        "rule:head".into(),
        rect(PAD, board_top - 6.0, avail, 1.0),
        pal.rule,
        0.0,
    );
    // Lanes.
    b.clip = Some(region);
    let mut y = board_top + 6.0 - input.scroll;
    let lanes: Vec<&Swimlane> = board
        .lanes
        .iter()
        .map(|l| &l.swimlane)
        .filter(|l| {
            input
                .filter
                .model
                .as_ref()
                .is_none_or(|m| l.model.as_ref() == Some(m))
        })
        .collect();
    let content_start = y;
    let column_index = |c: DisplayColumn| columns.iter().position(|x| *x == c).unwrap_or(0);
    if cards.is_empty() {
        let message = if input.filter.is_active() {
            lang.t("No cards match the filters.")
        } else {
            lang.t("No requirements in this delivery scope.")
        };
        b.text(
            "empty".into(),
            rect(PAD, y + 24.0, avail, 22.0),
            message,
            TITLE,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
        if input.filter.is_active() {
            b.button(
                "empty:clear",
                PAD,
                y + 56.0,
                32.0,
                None,
                lang.t("Clear filters"),
                false,
                Target::ClearFilters,
                200.0,
            );
        }
        y += 100.0;
    }
    for (lane_index, lane) in lanes.iter().enumerate() {
        let lane_cards: Vec<&CardInfo> =
            cards.iter().filter(|i| &i.card.swimlane == *lane).collect();
        if lane_cards.is_empty() && input.filter.is_active() {
            continue;
        }
        let collapsed = input.collapsed.contains(lane);
        let lkey = format!("lane:{}:{}", lane.project, lane_name(lane));
        let blocked = lane_cards
            .iter()
            .filter(|i| i.signals.contains(&Signal::Blocked))
            .count();
        let attention = lane_cards
            .iter()
            .filter(|i| i.signals.iter().any(|s| s.needs_attention()))
            .count();
        b.glyph(
            format!("{lkey}:chev"),
            rect(PAD, y, 18.0, 28.0),
            if collapsed { "\u{E76C}" } else { "\u{E70D}" },
            11.0,
            pal.secondary,
        );
        let name = lane_name(lane);
        let nw = b.fit.width(&name, 14.0, 700);
        b.text(
            format!("{lkey}:name"),
            rect(PAD + 22.0, y, nw + 2.0, 28.0),
            &name,
            14.0,
            700,
            pal.primary,
            TextAlign::MiddleLeading,
        );
        let mut summary = format!("{} {}", lane_cards.len(), lang.t("cards"));
        if blocked > 0 {
            summary.push_str(&format!(" · {blocked} {}", lang.t("blocked")));
        }
        if attention > 0 {
            summary.push_str(&format!(
                " · {attention} {}",
                lang.t("Needs attention").to_lowercase()
            ));
        }
        b.text(
            format!("{lkey}:summary"),
            rect(PAD + 30.0 + nw, y, 360.0, 28.0),
            &summary,
            META,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
        b.hit(
            format!("{lkey}:hit"),
            rect(PAD, y, nw + 40.0 + b.fit.width(&summary, META, 400), 28.0),
            "button",
            format!("{name}, {summary}"),
            Target::ToggleLane((*lane).clone()),
        );
        y += 32.0;
        if collapsed {
            y += 8.0;
            continue;
        }
        // Column cells; the lane is as tall as its tallest column.
        let mut tallest: f32 = 0.0;
        let mut placed = Vec::new();
        for (column, x, w) in &xs {
            let mut cy = 8.0;
            let in_cell: Vec<&&CardInfo> =
                lane_cards.iter().filter(|i| i.column == *column).collect();
            if is_rail(*column) {
                cy += 28.0;
            } else {
                for info in &in_cell {
                    let h = card_height(b, info, *w - 12.0);
                    placed.push((*info, *column, *x + 6.0, cy, *w - 12.0, h));
                    cy += h + CARD_GAP;
                }
                if in_cell.is_empty() {
                    cy += 28.0;
                }
            }
            tallest = tallest.max(cy);
        }
        let lane_h = tallest + 4.0;
        for (column, x, w) in &xs {
            let ckey = format!("{lkey}:cell:{column:?}");
            b.fill(
                format!("{ckey}:bg"),
                rect(*x, y, *w, lane_h),
                pal.column,
                10.0,
            );
            let count = lane_cards.iter().filter(|i| i.column == *column).count();
            if is_rail(*column) {
                if count == 0 {
                    b.text(
                        format!("{ckey}:none"),
                        rect(*x, y + 10.0, *w, 20.0),
                        "—",
                        META,
                        400,
                        pal.secondary,
                        TextAlign::Center,
                    );
                    continue;
                }
                let tone = if *column == DisplayColumn::Unmapped {
                    pal.warning
                } else {
                    pal.secondary
                };
                b.text(
                    format!("{ckey}:n"),
                    rect(*x, y + 8.0, *w, 22.0),
                    &count.to_string(),
                    14.0,
                    700,
                    tone,
                    TextAlign::Center,
                );
                let label = format!("{} {count}", column.label(lang));
                // Done expands; an unmapped card opens directly, since it needs a decision.
                let target = match lane_cards.iter().find(|i| i.column == *column) {
                    Some(info) if *column == DisplayColumn::Unmapped => {
                        Target::Select(info.card.card_id.clone())
                    }
                    _ => Target::ToggleDone,
                };
                b.hit(
                    format!("{ckey}:hit"),
                    rect(*x, y, *w, lane_h),
                    "button",
                    label,
                    target,
                );
            } else if count == 0 {
                b.text(
                    format!("{ckey}:none"),
                    rect(*x, y + 10.0, *w, 20.0),
                    "—",
                    META,
                    400,
                    pal.secondary,
                    TextAlign::Center,
                );
            }
        }
        for (info, column, x, cy, w, _) in placed {
            let selected = input.selected == Some(&info.card.card_id);
            let h = card(b, info, x, y + cy, w, selected);
            b.out.nav.push(NavCard {
                id: info.card.card_id.clone(),
                column: column_index(column),
                lane: lane_index,
                y: y + cy - content_start,
                h,
            });
        }
        y += lane_h + 16.0;
    }
    b.out.board_content = y - content_start;
    b.clip = None;
    if b.out.board_content > region.h {
        scrollbar(
            b,
            "board",
            region,
            input.scroll,
            b.out.board_content,
            board_w - 6.0,
        );
    }
    status_line(
        b,
        input,
        rect(PAD, viewport.h - 30.0, board_w - 2.0 * PAD, 24.0),
    );
    if has_detail {
        let x = board_w + GAP;
        let area = rect(x, 12.0, viewport.w - x - 12.0, viewport.h - 24.0);
        // On paper the pane is the same sheet, divided by a rule; high contrast keeps a panel.
        if pal.hc {
            b.fill("detail:pane".into(), area, pal.column, 12.0);
        } else {
            b.fill(
                "detail:rule".into(),
                rect(x - GAP / 2.0, 12.0, 1.0, viewport.h - 24.0),
                pal.rule,
                0.0,
            );
        }
        detail(
            b,
            board,
            input,
            rect(area.x + 4.0, area.y + 4.0, area.w - 8.0, area.h - 8.0),
        );
    } else if !b.out.nav.is_empty() && dock && input.selected.is_none() && board_w > 1300.0 {
        let _ = lang.t("Select a card to see its evidence.");
    }
}

fn scrollbar(b: &mut Builder, key: &str, region: RectDip, offset: f32, content: f32, x: f32) {
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

fn filters(b: &mut Builder, board: &BoardSnapshot, input: &Input, x0: f32, y: f32, w: f32) {
    let lang = b.lang;
    let filter = input.filter;
    let models: Vec<String> = board
        .lanes
        .iter()
        .filter_map(|l| l.swimlane.model.clone())
        .collect();
    let degraded = model::degraded_sources(board);
    let attention = board
        .cards
        .iter()
        .filter(|c| {
            model::signals(c, &degraded)
                .iter()
                .any(|s| s.needs_attention())
        })
        .count();
    let attention_label = format!("{} {attention}", lang.t("Needs attention"));
    let query_label = filter.query.as_ref().map(|q| format!("“{q}”"));
    // Measure: when the model chips do not fit, one chip cycles through them.
    let chip_w = |b: &Builder, s: &str| b.fit.width(s, META, 600) + 24.0;
    let tail = chip_w(b, &attention_label)
        + 40.0
        + 8.0
        + query_label
            .as_ref()
            .map_or(chip_w(b, lang.t("Find")) + 22.0, |q| chip_w(b, q) + 60.0);
    let all = lang.t("All models");
    let models_w: f32 = chip_w(b, all) + models.iter().map(|m| chip_w(b, m) + 6.0).sum::<f32>();
    let mut x = x0;
    if models_w + tail <= w {
        x += b.button(
            "model:all",
            x,
            y,
            28.0,
            None,
            all,
            filter.model.is_none(),
            Target::Model(None),
            200.0,
        ) + 6.0;
        for m in &models {
            let on = filter.model.as_ref() == Some(m);
            x += b.button(
                &format!("model:{m}"),
                x,
                y,
                28.0,
                None,
                m,
                on,
                Target::Model(Some(m.clone())),
                200.0,
            ) + 6.0;
        }
    } else {
        let label = filter.model.clone().unwrap_or_else(|| all.to_owned());
        x += b.button(
            "model:cycle",
            x,
            y,
            28.0,
            Some("\u{E71C}"),
            &label,
            filter.model.is_some(),
            Target::CycleModel,
            220.0,
        ) + 6.0;
    }
    x += 10.0;
    x += b.button(
        "attention",
        x,
        y,
        28.0,
        Some("\u{E7BA}"),
        &attention_label,
        filter.attention,
        Target::Attention,
        240.0,
    ) + 6.0;
    match &query_label {
        Some(q) => {
            x += b.button(
                "find",
                x,
                y,
                28.0,
                Some("\u{E721}"),
                q,
                true,
                Target::Find,
                240.0,
            ) + 4.0;
            b.button(
                "find:clear",
                x,
                y,
                28.0,
                Some("\u{E711}"),
                "",
                false,
                Target::ClearQuery,
                28.0,
            );
        }
        None => {
            b.button(
                "find",
                x,
                y,
                28.0,
                Some("\u{E721}"),
                lang.t("Find"),
                false,
                Target::Find,
                160.0,
            );
        }
    }
}

fn loading(b: &mut Builder, input: &Input, viewport: RectDip) {
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

fn section(b: &mut Builder, key: &str, label: &str, x: f32, y: f32, w: f32) -> f32 {
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
fn row(
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

fn detail(b: &mut Builder, board: &BoardSnapshot, input: &Input, area: RectDip) {
    let lang = b.lang;
    let pal = *b.pal;
    let Some(card) = input.selected.and_then(|id| model::find(board, id)) else {
        return;
    };
    let tz = board.project_timezone.as_str();
    let degraded = model::degraded_sources(board);
    let signals = model::signals(card, &degraded);
    let x = area.x + 16.0;
    let w = area.w - 32.0;
    b.out.detail = Some(area);
    let close_w = 32.0;
    // Fixed header: requirement number, title, identity and source actions.
    let number = model::label_of(card);
    b.text(
        "d:number".into(),
        rect(x, area.y + 12.0, w - close_w - 8.0, 30.0),
        &number,
        22.0,
        700,
        pal.primary,
        TextAlign::MiddleLeading,
    );
    if b.out.mode != Mode::Detail {
        b.button(
            "d:close",
            x + w - close_w,
            area.y + 12.0,
            close_w,
            Some("\u{E711}"),
            "",
            false,
            Target::CloseDetail,
            close_w,
        );
    }
    let serif = b.fit.with_font(FontRole::Serif);
    let (title, lines) = serif.fit(&card.title, 20.0, 600, w, 3);
    let title_lh = serif.line_height(20.0).ceil();
    let mut y = area.y + 46.0;
    b.serif(
        "d:title".into(),
        rect(x, y, w, lines as f32 * title_lh),
        &title,
        20.0,
        pal.primary,
    );
    y += lines as f32 * title_lh + 6.0;
    let identity = format!(
        "{} · {} {} {} · {}",
        model::lane_label(&card.swimlane),
        model::system_label(card.source.system),
        card.source.record_kind,
        card.source.record_id,
        lang.f("rev {}", &[&board.revision.0.to_string()])
    );
    let (identity_fit, _) = b.fit.fit(&identity, META, 400, w, 1);
    b.text(
        "d:identity".into(),
        rect(x, y, w, 18.0),
        &identity_fit,
        META,
        400,
        pal.secondary,
        TextAlign::MiddleLeading,
    );
    y += 26.0;
    let system = model::system_label(card.source.system);
    let open_label = lang.f("Open in {}", &[system]);
    let used = b.button(
        "d:open",
        x,
        y,
        30.0,
        Some("\u{E8A7}"),
        &open_label,
        false,
        Target::OpenSource(card.source.clone()),
        220.0,
    );
    b.button(
        "d:copy",
        x + used + 8.0,
        y,
        30.0,
        Some("\u{E8C8}"),
        lang.t("Copy ID"),
        false,
        Target::Copy(card.card_id.0.clone()),
        180.0,
    );
    y += 40.0;
    let region = rect(area.x, y, area.w, area.y + area.h - y);
    b.clip = Some(region);
    let start = y;
    y -= input.detail_scroll;
    // Status: the workflow, then code, build, test and acceptance as separate facts.
    y = section(b, "d:status", lang.t("Status"), x, y, w);
    let (workflow, since) = model::workflow_text(card, tz, lang);
    y = row(
        b,
        "d:wf",
        lang.t("Workflow"),
        None,
        &workflow,
        &since,
        x,
        y,
        w,
        true,
    );
    let code_state = model::code_state(card);
    y = row(
        b,
        "d:code",
        lang.t("Code"),
        Some((model::progress_glyph(code_state), pal.progress(code_state))),
        &model::code_text(card, lang),
        "",
        x,
        y,
        w,
        false,
    );
    let build = card.progress.build;
    y = row(
        b,
        "d:build",
        lang.t("Build"),
        Some((model::progress_glyph(build), pal.progress(build))),
        model::progress_word(build, lang),
        "",
        x,
        y,
        w,
        false,
    );
    let test = card.progress.verification;
    y = row(
        b,
        "d:test",
        lang.t("Test"),
        Some((model::progress_glyph(test), pal.progress(test))),
        model::progress_word(test, lang),
        "",
        x,
        y,
        w,
        false,
    );
    let accepted = card
        .related(spm_contracts::RelationshipKind::Accepted)
        .next()
        .is_some()
        || card.stage.stage == spm_contracts::BoardStage::Done;
    let accept_state = if accepted {
        ProgressState::Passed
    } else {
        ProgressState::NotStarted
    };
    y = row(
        b,
        "d:accept",
        lang.t("Acceptance"),
        Some((
            model::progress_glyph(accept_state),
            pal.progress(accept_state),
        )),
        &model::acceptance_text(card, tz, lang),
        &card.stage.explanation,
        x,
        y,
        w,
        false,
    );
    // Signals with their reasons.
    let shown: Vec<Signal> = signals.to_vec();
    if !shown.is_empty() {
        y = section(b, "d:signals", lang.t("Signals"), x, y, w);
        for (i, signal) in shown.iter().enumerate() {
            let detail = match signal {
                Signal::Blocked => card
                    .blockers
                    .iter()
                    .map(|blk| {
                        let mut text = blk.reason.clone();
                        if let Some(contact) = &blk.contact {
                            text.push_str(&format!(
                                " · {}: {}",
                                lang.t("Contact"),
                                contact.display_name
                            ));
                        }
                        if let Some(since) = blk.since {
                            text.push_str(&format!(
                                " · {}",
                                lang.f("since {}", &[&short_time(since, tz)])
                            ));
                        }
                        text
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                Signal::Conflict => card
                    .conflicts
                    .iter()
                    .map(|c| c.explanation.clone())
                    .collect::<Vec<_>>()
                    .join("\n"),
                Signal::SourceNotRefreshed => degraded
                    .iter()
                    .filter(|s| card.observed_through.contains_key(s))
                    .map(|s| {
                        let at = card.observed_through[s];
                        format!(
                            "{} · {}",
                            lang.f("{} did not refresh", &[model::system_label(*s)]),
                            lang.f("observed {}", &[&short_time(at, tz)])
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                Signal::NotInTracker => card
                    .assignee()
                    .map(|a| basis_note(a, tz, lang))
                    .unwrap_or_default(),
                Signal::Unmapped
                | Signal::WorkflowLag
                | Signal::TestFailed
                | Signal::BuildFailed => card.stage.explanation.clone(),
                Signal::Unassigned => card
                    .next_action
                    .as_ref()
                    .map(|n| n.text.clone())
                    .unwrap_or_default(),
            };
            y = row(
                b,
                &format!("d:signal:{i}"),
                signal.label(lang),
                Some((signal.glyph(), pal.tone(signal.tone()))),
                &detail,
                "",
                x,
                y,
                w,
                false,
            );
        }
    }
    if let Some(action) = &card.next_action {
        y = section(b, "d:next", lang.t("Next action"), x, y, w);
        let owner = action.owner.as_ref().map_or_else(
            || lang.t("Unassigned").to_owned(),
            |o| o.display_name.clone(),
        );
        y = row(
            b,
            "d:next:row",
            &owner,
            Some(("\u{E72A}", pal.accent_ink)),
            &action.text,
            "",
            x,
            y,
            w,
            true,
        );
    }
    y = section(b, "d:people", lang.t("People"), x, y, w);
    for (i, person) in model::people_rows(card, tz, lang).iter().enumerate() {
        y = row(
            b,
            &format!("d:person:{i}"),
            person.role,
            None,
            &person.name,
            &person.detail,
            x,
            y,
            w,
            person.emphasis,
        );
    }
    // Evidence timeline, loaded on selection.
    let count = match input.evidence {
        EvidenceView::Loaded(e) => e.items.len() as u32,
        _ => card.evidence_count,
    };
    y = section(
        b,
        "d:evidence",
        &format!("{} ({count})", lang.t("Evidence")),
        x,
        y,
        w,
    );
    match input.evidence {
        EvidenceView::None | EvidenceView::Loading => {
            b.text(
                "d:ev:loading".into(),
                rect(x, y, w, 20.0),
                lang.t("Loading evidence…"),
                META,
                400,
                pal.secondary,
                TextAlign::MiddleLeading,
            );
            y += 24.0;
            for i in 0..3 {
                b.fill(
                    format!("d:ev:skel:{i}"),
                    rect(x, y, w * (0.9 - i as f32 * 0.15), 12.0),
                    pal.skeleton,
                    3.0,
                );
                y += 22.0;
            }
        }
        EvidenceView::Failed(error) => {
            let mh = b.line_h(META);
            b.glyph(
                "d:ev:failed:g".into(),
                rect(x, y, 16.0, 20.0),
                "\u{E783}",
                12.0,
                pal.danger,
            );
            b.text(
                "d:ev:failed".into(),
                rect(x + 20.0, y, w - 20.0, 20.0),
                lang.t("Evidence did not load."),
                13.0,
                700,
                pal.danger,
                TextAlign::MiddleLeading,
            );
            let (reason, lines) = b.fit.fit(error, META, 400, w - 20.0, 2);
            b.text(
                "d:ev:reason".into(),
                rect(x + 20.0, y + 22.0, w - 20.0, lines as f32 * mh),
                &reason,
                META,
                400,
                pal.secondary,
                TextAlign::TopLeading,
            );
            y += 28.0 + lines as f32 * mh;
            b.button(
                "d:ev:retry",
                x,
                y,
                28.0,
                Some("\u{E72C}"),
                lang.t("Try again"),
                false,
                Target::RetryEvidence,
                200.0,
            );
            y += 36.0;
        }
        EvidenceView::Loaded(evidence) => {
            y = timeline(
                b,
                card,
                &evidence.items,
                input.selected_evidence,
                tz,
                x,
                y,
                w,
            );
        }
    }
    b.clip = None;
    b.out.detail_content = y + input.detail_scroll - start + 12.0;
    if b.out.detail_content > region.h {
        scrollbar(
            b,
            "detail",
            region,
            input.detail_scroll,
            b.out.detail_content,
            area.x + area.w - 6.0,
        );
    }
}

fn basis_note(r: &spm_contracts::Relationship, tz: &str, lang: Lang) -> String {
    match &r.statement {
        Some(s) => lang.f(
            "{} named {} in {} · {}",
            &[
                &s.author.display_name,
                &r.person.display_name,
                "Lark",
                &r.source_time.map(|t| short_time(t, tz)).unwrap_or_default(),
            ],
        ),
        None => String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn timeline(
    b: &mut Builder,
    card: &BoardCard,
    items: &[EvidenceItem],
    selected: Option<&str>,
    tz: &str,
    x: f32,
    mut y: f32,
    w: f32,
) -> f32 {
    let lang = b.lang;
    let pal = *b.pal;
    let mh = b.line_h(META);
    let lh = b.line_h(13.0);
    let current = card.current_assignee.as_deref();
    let time_w = 84.0;
    let rail_x = x + time_w + 4.0;
    let tx = rail_x + 16.0;
    let tw = x + w - tx;
    for (i, item) in items.iter().enumerate() {
        let key = format!("d:ev:{}", item.id);
        let at = item.source_time.unwrap_or(item.observed_at);
        let on = selected == Some(item.id.as_str());
        let supports_current =
            current.is_some_and(|c| item.relationship_ids.iter().any(|r| r == c));
        let system = model::system_label(item.source.system);
        let (summary, lines) = b
            .fit
            .fit(&item.summary, 13.0, 400, tw, if on { 6 } else { 3 });
        let mut h = mh + lines as f32 * lh + 8.0;
        let mut kinds: Vec<String> = item
            .relationship_ids
            .iter()
            .filter_map(|id| card.relationship(id))
            .map(|r| {
                let mut kind = format!(
                    "{} {}",
                    model::kind_label(r.kind, lang),
                    r.person.display_name
                );
                if let Some(status) = model::status_label(r.status, lang) {
                    kind.push_str(&format!(" ({status})"));
                }
                if Some(r.id.as_str()) == current {
                    kind = format!("{} ← {}", kind, lang.t("Responsible now"));
                }
                kind
            })
            .collect();
        kinds.dedup();
        let roles = kinds.join(" · ");
        if !roles.is_empty() {
            h += mh;
        }
        if on {
            h += mh + 40.0;
        }
        let r = rect(x - 8.0, y - 4.0, w + 16.0, h);
        if on {
            b.fill(format!("{key}:sel"), r, pal.chip, 8.0);
        }
        // Rail: a dot per item, filled for the item that set the current assignee.
        if i + 1 < items.len() {
            b.fill(
                format!("{key}:rail"),
                rect(rail_x + 5.0, y + 10.0, 1.0, h),
                pal.rule,
                0.0,
            );
        }
        let dot = if supports_current {
            pal.accent_ink
        } else {
            pal.secondary
        };
        b.glyph(
            format!("{key}:dot"),
            rect(rail_x - 2.0, y, 16.0, mh),
            if supports_current {
                "\u{EA3B}"
            } else {
                "\u{EA3A}"
            },
            9.0,
            dot,
        );
        b.text(
            format!("{key}:time"),
            rect(x, y, time_w, mh),
            &short_time(at, tz),
            META,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
        // The source name is a link to the record itself, one click from the timeline.
        let link_w = (b.fit.width(system, META, 700).ceil() + 18.0).min(tw);
        b.text(
            format!("{key}:system"),
            rect(tx, y, link_w - 16.0, mh),
            system,
            META,
            700,
            pal.accent_ink,
            TextAlign::MiddleLeading,
        );
        b.glyph(
            format!("{key}:system:go"),
            rect(tx + link_w - 15.0, y, 14.0, mh),
            "\u{E8A7}",
            10.0,
            pal.accent_ink,
        );
        let mut cy = y + mh;
        b.text(
            format!("{key}:summary"),
            rect(tx, cy, tw, lines as f32 * lh),
            &summary,
            13.0,
            400,
            pal.primary,
            TextAlign::TopLeading,
        );
        cy += lines as f32 * lh;
        if !roles.is_empty() {
            let (fitted, _) = b.fit.fit(&roles, META, 400, tw, 1);
            b.text(
                format!("{key}:roles"),
                rect(tx, cy, tw, mh),
                &fitted,
                META,
                400,
                if supports_current {
                    pal.accent_ink
                } else {
                    pal.secondary
                },
                TextAlign::MiddleLeading,
            );
            cy += mh;
        }
        b.hit(
            format!("{key}:hit"),
            r,
            "listitem",
            format!("{} {system}: {}", short_time(at, tz), item.summary),
            Target::SelectEvidence(item.id.clone()),
        );
        b.hit(
            format!("{key}:link"),
            rect(tx - 2.0, y, link_w + 4.0, mh),
            "link",
            lang.f("Open in {}", &[system]),
            Target::OpenSource(item.source.clone()),
        );
        if on {
            let detail = format!(
                "{} · {} · {}",
                item.field_locator.as_deref().unwrap_or("—"),
                lang.f("observed {}", &[&short_time(item.observed_at, tz)]),
                item.id
            );
            let (fitted, _) = b.fit.fit(&detail, META, 400, tw, 1);
            b.text(
                format!("{key}:locator"),
                rect(tx, cy, tw, mh),
                &fitted,
                META,
                400,
                pal.secondary,
                TextAlign::MiddleLeading,
            );
            cy += mh + 6.0;
            let used = b.button(
                &format!("{key}:open"),
                tx,
                cy,
                28.0,
                Some("\u{E8A7}"),
                lang.t("Open source"),
                false,
                Target::OpenSource(item.source.clone()),
                200.0,
            );
            let reference = format!(
                "{}:{}/{}/{}/{}",
                format!("{:?}", item.source.system).to_lowercase(),
                item.source.tenant,
                item.source.project,
                item.source.record_kind,
                item.source.record_id
            );
            b.button(
                &format!("{key}:copy"),
                tx + used + 8.0,
                cy,
                28.0,
                Some("\u{E8C8}"),
                lang.t("Copy reference"),
                false,
                Target::Copy(reference),
                220.0,
            );
        }
        y += h + 4.0;
    }
    y
}

// ---------------------------------------------------------------------------
// Compact and capsule

fn compact(b: &mut Builder, input: &Input, viewport: RectDip) {
    let lang = b.lang;
    let pal = *b.pal;
    let Some(board) = input.board else {
        b.out.mode = Mode::Compact;
        loading(b, input, viewport);
        return;
    };
    let tz = board.project_timezone.as_str();
    let cards = visible_cards(board, input.filter);
    if input.detail_open
        && let Some(card) = input
            .selected
            .and_then(|id| cards.iter().find(|i| &i.card.card_id == id))
    {
        b.out.mode = Mode::CompactDetail;
        b.button(
            "back",
            12.0,
            10.0,
            28.0,
            Some("\u{E72B}"),
            lang.t("Back to board"),
            false,
            Target::CloseDetail,
            200.0,
        );
        let x = 16.0;
        let w = viewport.w - 32.0;
        let mut y = 48.0;
        b.text(
            "cd:number".into(),
            rect(x, y, w, 26.0),
            &model::label_of(card.card),
            18.0,
            700,
            pal.primary,
            TextAlign::MiddleLeading,
        );
        y += 28.0;
        let lh = b.line_h(14.0);
        let (title, lines) = b.fit.fit(&card.card.title, 14.0, 600, w, 2);
        b.text(
            "cd:title".into(),
            rect(x, y, w, lines as f32 * lh),
            &title,
            14.0,
            600,
            pal.primary,
            TextAlign::TopLeading,
        );
        y += lines as f32 * lh + 8.0;
        let region = rect(0.0, y, viewport.w, viewport.h - y - 8.0);
        b.clip = Some(region);
        let (workflow, since) = model::workflow_text(card.card, tz, lang);
        y = row(
            b,
            "cd:wf",
            lang.t("Workflow"),
            None,
            &workflow,
            &since,
            x,
            y,
            w,
            true,
        );
        let who = card.card.assignee().map_or_else(
            || lang.t("Unassigned").to_owned(),
            |a| a.person.display_name.clone(),
        );
        y = row(
            b,
            "cd:who",
            lang.t("Responsible now"),
            None,
            &who,
            "",
            x,
            y,
            w,
            false,
        );
        y = row(
            b,
            "cd:code",
            lang.t("Code"),
            None,
            &model::code_text(card.card, lang),
            "",
            x,
            y,
            w,
            false,
        );
        if let Some(signal) = card.signals.first() {
            y = row(
                b,
                "cd:signal",
                signal.label(lang),
                Some((signal.glyph(), pal.tone(signal.tone()))),
                &model::signal_text(card.card, *signal, lang),
                "",
                x,
                y,
                w,
                false,
            );
        }
        if let Some(action) = &card.card.next_action {
            let owner = action.owner.as_ref().map_or_else(
                || lang.t("Unassigned").to_owned(),
                |o| o.display_name.clone(),
            );
            row(
                b,
                "cd:next",
                &owner,
                Some(("\u{E72A}", pal.accent_ink)),
                &action.text,
                "",
                x,
                y,
                w,
                true,
            );
        }
        b.clip = None;
        return;
    }
    b.out.mode = Mode::Compact;
    let x = 12.0;
    let w = viewport.w - 24.0;
    // Title, refresh and source status are in the host's title row (PanelChrome).
    let counts: Vec<String> = DisplayColumn::ALL
        .iter()
        .filter_map(|c| {
            let n = cards.iter().filter(|i| i.column == *c).count();
            (n > 0).then(|| format!("{} {}", c.label(lang), count_text(n, board.totals_complete)))
        })
        .collect();
    let (fitted, _) = b.fit.fit(&counts.join(" · "), META, 400, w - 8.0, 1);
    b.text(
        "c:counts".into(),
        rect(x + 4.0, 8.0, w - 8.0, 18.0),
        &fitted,
        META,
        400,
        pal.secondary,
        TextAlign::MiddleLeading,
    );
    // Ranked list: what needs a person first.
    let mut ranked: Vec<&CardInfo> = cards
        .iter()
        .filter(|i| i.column != DisplayColumn::Done)
        .collect();
    ranked.sort_by_key(|i| model::attention_rank(i.card, &i.signals));
    let row_h = 52.0;
    let top = 32.0;
    let room = ((viewport.h - top - 28.0) / (row_h + 4.0)).floor().max(1.0) as usize;
    let shown = ranked.len().min(room);
    for (i, info) in ranked.iter().take(shown).enumerate() {
        let y = top + i as f32 * (row_h + 4.0);
        let key = format!("c:row:{}", info.card.card_id.0);
        let r = rect(x, y, w, row_h);
        let selected = input.selected == Some(&info.card.card_id);
        if selected {
            b.outline(&key, r, pal.accent, 1.5, 8.0, pal.selected);
        } else {
            b.fill(format!("{key}:bg"), r, pal.card, 8.0);
        }
        let lead = info.signals.iter().copied().find(|s| s.needs_attention());
        let glyph_color = lead.map_or(pal.secondary, |s| pal.tone(s.tone()));
        if let Some(signal) = lead {
            b.glyph(
                format!("{key}:glyph"),
                rect(x + 12.0, y + 6.0, 18.0, 20.0),
                signal.glyph(),
                12.0,
                glyph_color,
            );
        }
        let number = model::label_of(info.card);
        let who = info.card.assignee().map_or_else(
            || lang.t("Unassigned").to_owned(),
            |a| a.person.display_name.clone(),
        );
        let (who_fit, _) = b.fit.fit(&who, META, 400, w * 0.35, 1);
        let who_w = b.fit.width(&who_fit, META, 400) + 4.0;
        let nx = x + 34.0;
        let nw = b.fit.width(&number, NUMBER, 700) + 2.0;
        b.text(
            format!("{key}:number"),
            rect(nx, y + 6.0, nw, 20.0),
            &number,
            NUMBER,
            700,
            pal.primary,
            TextAlign::MiddleLeading,
        );
        let title_w = (x + w - 12.0 - who_w - 8.0 - (nx + nw + 8.0)).max(0.0);
        let (title, _) = b.fit.fit(&info.card.title, 13.0, 400, title_w, 1);
        b.text(
            format!("{key}:title"),
            rect(nx + nw + 8.0, y + 6.0, title_w, 20.0),
            &title,
            13.0,
            400,
            pal.primary,
            TextAlign::MiddleLeading,
        );
        b.text(
            format!("{key}:who"),
            rect(x + w - 12.0 - who_w, y + 6.0, who_w, 20.0),
            &who_fit,
            META,
            400,
            if info.card.assignee().is_some() {
                pal.secondary
            } else {
                pal.warning
            },
            TextAlign::MiddleLeading,
        );
        let second = match (lead, &info.card.next_action) {
            (Some(signal), _) if signal.tone() == Tone::Danger => {
                model::signal_text(info.card, signal, lang)
            }
            (_, Some(action)) => format!(
                "→ {}: {}",
                action
                    .owner
                    .as_ref()
                    .map_or("—", |o| o.display_name.as_str()),
                action.text
            ),
            (Some(signal), None) => model::signal_text(info.card, signal, lang),
            (None, None) => model::fact_line(info.card, lang),
        };
        let (fitted, _) = b.fit.fit(&second, META, 400, w - 46.0, 1);
        b.text(
            format!("{key}:second"),
            rect(nx, y + 27.0, w - 46.0, 18.0),
            &fitted,
            META,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
        b.hit(
            format!("{key}:hit"),
            r,
            "listitem",
            format!("{number}, {}, {who}, {second}", info.card.title),
            Target::Select(info.card.card_id.clone()),
        );
        b.out.nav.push(NavCard {
            id: info.card.card_id.clone(),
            column: 0,
            lane: 0,
            y,
            h: row_h,
        });
    }
    let hidden = ranked.len() - shown;
    if let Some((text, tone)) = input.status {
        let color = tone.map_or(pal.secondary, |t| pal.tone(t));
        let (fitted, _) = b.fit.fit(text, META, 400, w, 1);
        b.text(
            "status".into(),
            rect(x + 4.0, viewport.h - 24.0, w - 8.0, 20.0),
            &fitted,
            META,
            400,
            color,
            TextAlign::MiddleLeading,
        );
    } else if hidden > 0 {
        b.text(
            "c:more".into(),
            rect(x + 4.0, viewport.h - 24.0, w - 8.0, 20.0),
            &lang.f("{} more", &[&hidden.to_string()]),
            META,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
    }
}

fn capsule(b: &mut Builder, input: &Input, viewport: RectDip) {
    let lang = b.lang;
    let pal = *b.pal;
    let Some(board) = input.board else {
        b.text(
            "cap:loading".into(),
            rect(12.0, 0.0, viewport.w - 24.0, viewport.h),
            lang.t("Loading the board…"),
            META,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
        return;
    };
    let cards = visible_cards(board, &Filter::default());
    let open: Vec<String> = [
        DisplayColumn::Todo,
        DisplayColumn::Develop,
        DisplayColumn::Verify,
        DisplayColumn::Done,
    ]
    .iter()
    .map(|c| {
        format!(
            "{} {}",
            c.label(lang),
            cards.iter().filter(|i| i.column == *c).count()
        )
    })
    .collect();
    let blocked = cards
        .iter()
        .filter(|i| i.signals.contains(&Signal::Blocked))
        .count();
    let degraded = !model::degraded_sources(board).is_empty();
    let mut x = 12.0;
    let name = board.project_id.0.clone();
    let nw = b.fit.width(&name, 13.0, 700);
    b.text(
        "cap:name".into(),
        rect(x, 0.0, nw + 2.0, viewport.h),
        &name,
        13.0,
        700,
        pal.primary,
        TextAlign::MiddleLeading,
    );
    x += nw + 12.0;
    let line = open.join(" · ");
    let right = 60.0;
    let (fitted, _) = b.fit.fit(&line, META, 400, viewport.w - x - right, 1);
    let lw = b.fit.width(&fitted, META, 400) + 2.0;
    b.text(
        "cap:counts".into(),
        rect(x, 0.0, lw, viewport.h),
        &fitted,
        META,
        400,
        pal.secondary,
        TextAlign::MiddleLeading,
    );
    x += lw + 10.0;
    if blocked > 0 {
        b.glyph(
            "cap:blocked:glyph".into(),
            rect(x, 0.0, 16.0, viewport.h),
            Signal::Blocked.glyph(),
            12.0,
            pal.danger,
        );
        b.text(
            "cap:blocked".into(),
            rect(x + 18.0, 0.0, 24.0, viewport.h),
            &blocked.to_string(),
            META,
            700,
            pal.danger,
            TextAlign::MiddleLeading,
        );
    }
    if degraded {
        b.glyph(
            "cap:stale".into(),
            rect(viewport.w - 28.0, 0.0, 16.0, viewport.h),
            "\u{E823}",
            12.0,
            pal.warning,
        );
    }
}

pub fn lane_key(lane: &Swimlane) -> String {
    format!("{}/{}", lane.project, lane.model.as_deref().unwrap_or(""))
}

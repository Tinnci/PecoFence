//! Diagram widget: Mermaid text drawn natively as scene nodes (Pele-style), so the
//! result is crisp at any DPI, themed by the paper palette and readable by UIA.

use super::mermaid::{
    self, Diagram, Estimate, FlowLayout, Flowchart, Gantt, ParseError, Shape, Stroke, TextMeasure,
};
use super::{
    CAPSULE_HEIGHT, CONTENT_TOP, Colors, Control, FRAME_INTERVAL, HEADER_META, HeaderAction, Tween,
    WidgetElement, WidgetFrame, build_capsule, build_panel_header, fade, glyph, paper,
};
use chrono::{Datelike, Local, NaiveDate};
use pecofence_plugin_api::design;
use pecofence_plugin_api::design::motion::{self, Curve};
use pecofence_plugin_api::*;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const DIAGRAM_PROVIDER: &str = "pecofence.diagram";
const EDIT: u64 = 1;
const UNDO: u64 = 2;
const MAX_SOURCE_UNITS: usize = 16 * 1024;

// Scene ids: fixed chrome below 100, diagram parts in their own 32-bit blocks.
const STAGE: u64 = 4;
const EMPTY_TITLE: u64 = 9;
const EMPTY_HINT: u64 = 10;
const BANNER: u64 = 11;
const BANNER_TITLE: u64 = 12;
const BANNER_LINE: u64 = 13;
const EDGE: u64 = 5 << 32;
const ARROW: u64 = 6 << 32;
const EDGE_LABEL: u64 = 7 << 32;
const NODE: u64 = 8 << 32;
const NODE_OUTLINE: u64 = 9 << 32;
const AXIS_LABEL: u64 = 10 << 32;
const TASK_LABEL: u64 = 11 << 32;
const TASK_BAR: u64 = 12 << 32;
const TODAY: u64 = 13 << 32;
const CLUSTER: u64 = 14 << 32;
const CLUSTER_OUTLINE: u64 = 15 << 32;
const CLUSTER_TITLE: u64 = 16 << 32;

/// Starting point offered when a new diagram opens its editor.
pub fn diagram_template(chinese: bool) -> &'static str {
    if chinese {
        "flowchart LR\n  A[需求冻结] --> SYS[SYS 开发]\n  A --> VND[VND 开发]\n  SYS --> I[集成]\n  VND --> I\n  I --> V[验证]\n"
    } else {
        "flowchart LR\n  A[Freeze scope] --> SYS[SYS build]\n  A --> VND[Vendor build]\n  SYS --> I[Integrate]\n  VND --> I\n  I --> V[Verify]\n"
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct DiagramConfig {
    source: String,
}

fn validate_source(source: &str) -> Result<()> {
    if source.encode_utf16().count() > MAX_SOURCE_UNITS || source.contains('\0') {
        return Err(Error::Invalid(
            "diagram source exceeds 16384 UTF-16 units or contains NUL".into(),
        ));
    }
    Ok(())
}

fn parse_config(config: &PanelConfig) -> Result<DiagramConfig> {
    if config.version != 1 || config.bytes.len() > 96 * 1024 {
        return Err(Error::Invalid(
            "unsupported or oversized diagram configuration".into(),
        ));
    }
    let value: DiagramConfig =
        serde_json::from_slice(&config.bytes).map_err(|error| Error::Invalid(error.to_string()))?;
    validate_source(&value.source)?;
    Ok(value)
}

/// Parsed source plus the layout derived from it, rebuilt only when the text changes.
struct Rendered {
    diagram: Diagram,
    flow: Option<FlowLayout>,
    /// Gantt: width of the longest task name at the caption size.
    names_width: f32,
}

impl Rendered {
    fn new(diagram: Diagram, measure: &dyn TextMeasure) -> Self {
        let (flow, names_width) = match &diagram {
            Diagram::Flowchart(chart) => (Some(mermaid::layout_flowchart(chart, measure)), 0.0),
            Diagram::Gantt(chart) => (
                None,
                chart
                    .tasks
                    .iter()
                    .map(|t| measure.line_width(&t.name, design::CAPTION))
                    .fold(0.0, f32::max),
            ),
        };
        Self {
            diagram,
            flow,
            names_width,
        }
    }
}

/// DirectWrite widths from the host's render service, in the paper body face the
/// diagram draws with; the estimate stands in when the service is unavailable.
struct HostMeasure<'a>(&'a Capability<dyn RenderService>);

impl TextMeasure for HostMeasure<'_> {
    fn line_width(&self, text: &str, size: f32) -> f32 {
        let spec = TextSpec {
            text: text.into(),
            size_dip: size,
            weight: 400,
            font: FontRole::PaperBody,
            align: TextAlign::TopLeading,
        };
        self.0
            .with(|render| render.measure(&spec, 10_000.0))
            .map_or_else(|_| Estimate.line_width(text, size), |m| m.width)
    }
}

pub struct DiagramPlugin;

impl PanelProvider for DiagramPlugin {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: DIAGRAM_PROVIDER,
            api_major: 1,
            config_major: 1,
            required_services: &["theme"],
        }
    }

    fn validate(&self, config: &PanelConfig) -> Result<()> {
        parse_config(config).map(|_| ())
    }

    fn create(&self, ctx: PluginContext, input: CreatePanel) -> Result<Box<dyn PanelInstance>> {
        let config = parse_config(&input.config)?;
        let mut widget = DiagramWidget {
            theme: ctx.theme.with(|service| service.snapshot()).ok(),
            ctx,
            key: input.key,
            mount: None,
            config: DiagramConfig::default(),
            rendered: None,
            error: None,
            undo: None,
            appear: None,
            chinese: true,
            revision: 0,
            frame_revision: 0,
            layout_tracker: design::LayoutTracker::default(),
            stopped: false,
            field: None,
        };
        widget.load(config);
        Ok(Box::new(widget))
    }
}

struct DiagramWidget {
    ctx: PluginContext,
    key: InstanceKey,
    mount: Option<MountKey>,
    config: DiagramConfig,
    /// Last source that parsed; kept on screen (dimmed) while the text has an error.
    rendered: Option<Rendered>,
    error: Option<ParseError>,
    undo: Option<String>,
    appear: Option<Tween>,
    chinese: bool,
    theme: Option<Arc<ThemeSnapshot>>,
    revision: u64,
    frame_revision: u64,
    layout_tracker: design::LayoutTracker,
    stopped: bool,
    /// Source editor field from the last frame: the stage, as a code page.
    field: Option<TextInputStyle>,
}

/// The stage the diagram is drawn on (also the source editor's field).
fn stage_rect(v: RectDip) -> RectDip {
    RectDip {
        x: v.x + design::CONTENT_INSET,
        y: v.y + CONTENT_TOP,
        w: (v.w - design::CONTENT_INSET * 2.0).max(0.0),
        h: (v.h - CONTENT_TOP - design::CONTENT_INSET).max(0.0),
    }
}

/// Mermaid is code: edit it in place over the stage, in a monospace face on the
/// lighter "page" surface the diagram's shapes use, padded like the diagram area.
fn source_field(frame: &WidgetFrame, colors: Colors) -> TextInputStyle {
    let v = frame.viewport;
    let field = if v.h < CAPSULE_HEIGHT {
        RectDip {
            x: v.x + 8.0,
            y: v.y + 6.0,
            w: (v.w - 16.0).max(1.0),
            h: (v.h - 12.0).max(1.0),
        }
    } else {
        stage_rect(v)
    };
    let page = colors.input_surface();
    super::input_field(
        frame.scale,
        field,
        [14.0, 12.0, 14.0, 12.0],
        colors,
        page,
        TextInputFont::Mono,
        design::CARD_RADIUS,
        None,
    )
}

impl DiagramWidget {
    fn load(&mut self, config: DiagramConfig) {
        if config.source.trim().is_empty() {
            self.rendered = None;
            self.error = None;
        } else {
            match mermaid::parse(&config.source) {
                Ok(diagram) => {
                    let measure = HostMeasure(&self.ctx.render);
                    self.rendered = Some(Rendered::new(diagram, &measure));
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        self.config = config;
    }

    fn redraw(&mut self) -> PanelUpdate {
        self.revision = self.revision.saturating_add(1);
        PanelUpdate {
            commands: vec![HostCommand::Invalidate],
            relayout: false,
        }
    }

    fn change(&mut self, source: String) -> Result<PanelUpdate> {
        validate_source(&source)?;
        if source == self.config.source {
            return Ok(PanelUpdate::default());
        }
        let config = DiagramConfig { source };
        let bytes: Arc<[u8]> = serde_json::to_vec(&config)
            .map_err(|error| Error::Backend(error.to_string()))?
            .into();
        parse_config(&PanelConfig {
            version: 1,
            bytes: bytes.clone(),
        })?;
        let previous = self.config.source.clone();
        self.load(config);
        self.undo = Some(previous);
        let animations = self.theme.as_ref().is_none_or(|theme| theme.animations);
        self.appear = Tween::start(motion::NORMAL, Curve::Decelerate, animations);
        let mut update = self.redraw();
        update.commands.push(HostCommand::PersistConfig(bytes));
        Ok(update)
    }
}

impl PanelInstance for DiagramWidget {
    fn initial_presentation(&self) -> Presentation {
        Presentation::Compact
    }

    fn mount(&mut self, ctx: MountContext) -> Result<()> {
        if ctx.key.instance != self.key {
            return Err(Error::Invalid("mount belongs to another diagram".into()));
        }
        self.mount = Some(ctx.key);
        Ok(())
    }

    fn event(&mut self, event: PanelEvent) -> Result<PanelUpdate> {
        if self.stopped {
            return Err(Error::Closed);
        }
        match event {
            PanelEvent::ThemeChanged { .. } => {
                self.theme = self.ctx.theme.with(|service| service.snapshot()).ok();
                Ok(self.redraw())
            }
            PanelEvent::Wake => Ok(self.redraw()),
            PanelEvent::Invoke { action: EDIT, .. } => Ok(PanelUpdate {
                commands: vec![HostCommand::RequestTextInput {
                    action: EDIT,
                    initial: if self.config.source.trim().is_empty() {
                        diagram_template(self.chinese).to_string()
                    } else {
                        self.config.source.clone()
                    },
                    multiline: true,
                    style: self.field,
                }],
                relayout: false,
            }),
            PanelEvent::TextSubmitted { action: EDIT, text } => {
                self.change(text.replace("\r\n", "\n").replace('\r', "\n"))
            }
            PanelEvent::Invoke { action: UNDO, .. } => match self.undo.clone() {
                Some(previous) => {
                    let update = self.change(previous)?;
                    self.undo = None;
                    Ok(update)
                }
                None => Ok(PanelUpdate::default()),
            },
            _ => Ok(PanelUpdate::default()),
        }
    }

    fn prepare_frame(&mut self, input: FrameInput) -> Result<Rc<dyn PreparedFrame>> {
        let mount = self.mount.ok_or(Error::Closed)?;
        self.frame_revision = self.frame_revision.checked_add(1).ok_or(Error::Exhausted)?;
        self.chinese = input.locale.starts_with("zh");
        let now = Instant::now();
        if self.appear.is_some_and(|tween| tween.done(now)) {
            self.appear = None;
        }
        let mut frame = WidgetFrame::new(mount, self.frame_revision, self.revision, input);
        self.field = Some(source_field(&frame, Colors::paper(self.theme.as_deref())));
        build_diagram(
            &mut frame,
            &View {
                empty: self.config.source.trim().is_empty(),
                rendered: self.rendered.as_ref(),
                error: self.error.as_ref(),
                can_undo: self.undo.is_some(),
                appear: self.appear.map_or(1.0, |tween| tween.progress(now)),
                today: Local::now().date_naive(),
            },
            Colors::paper(self.theme.as_deref()),
            self.chinese,
        );
        let frame = frame.finish(&mut self.layout_tracker)?;
        Ok(Rc::new(frame))
    }

    fn unmount(&mut self, key: MountKey) {
        if self.mount == Some(key) {
            self.mount = None;
        }
    }

    fn begin_stop(&mut self, _: StopReason) {
        self.stopped = true;
        self.mount = None;
    }

    fn next_wake_after(&self) -> Option<Duration> {
        (!self.stopped && self.appear.is_some()).then_some(FRAME_INTERVAL)
    }
}

struct View<'a> {
    empty: bool,
    rendered: Option<&'a Rendered>,
    error: Option<&'a ParseError>,
    can_undo: bool,
    /// Entrance progress after an edit, 0..=1.
    appear: f32,
    today: NaiveDate,
}

fn short_date(date: NaiveDate, chinese: bool) -> String {
    if chinese {
        format!("{}月{}日", date.month(), date.day())
    } else {
        date.format("%b %-d").to_string()
    }
}

fn summary(diagram: &Diagram, chinese: bool) -> String {
    match (diagram, chinese) {
        (Diagram::Flowchart(chart), true) => format!("流程图 · {} 个节点", chart.nodes.len()),
        (Diagram::Flowchart(chart), false) => format!("Flowchart · {} nodes", chart.nodes.len()),
        (Diagram::Gantt(chart), _) => {
            let kind = if chinese { "甘特图" } else { "Gantt" };
            match chart.span() {
                Some((start, end)) => format!(
                    "{kind} · {} – {}",
                    short_date(start, chinese),
                    short_date(end.pred_opt().unwrap_or(end), chinese)
                ),
                None if chinese => format!("{kind} · 暂无任务"),
                None => format!("{kind} · no tasks"),
            }
        }
    }
}

fn build_diagram(frame: &mut WidgetFrame, view: &View, colors: Colors, chinese: bool) {
    frame.ground(colors);
    let diagram = view.rendered.map(|r| &r.diagram);
    if frame.viewport.h < CAPSULE_HEIGHT {
        let text = match (view.error, diagram) {
            (Some(_), _) if chinese => "图表源码有误".to_string(),
            (Some(_), _) => "Diagram source has an error".to_string(),
            (None, Some(diagram)) => match diagram.title() {
                Some(title) => format!("{title} · {}", summary(diagram, chinese)),
                None => summary(diagram, chinese),
            },
            (None, None) if chinese => "空白图表".to_string(),
            (None, None) => "Empty diagram".to_string(),
        };
        build_capsule(frame, text, Some(EDIT), Some(colors.subtle), colors);
        return;
    }
    let narrow = frame.viewport.w < 300.0;
    let mut actions = Vec::new();
    if view.can_undo {
        actions.push(HeaderAction {
            id: 6,
            content: Control::Icon(
                glyph::UNDO,
                if chinese {
                    "撤销上一步"
                } else {
                    "Undo last"
                },
            ),
            kind: design::ButtonKind::Flat,
            action: Some(UNDO),
            width: design::CONTROL_HEIGHT,
        });
    }
    actions.push(HeaderAction {
        id: 2,
        content: if narrow {
            Control::Icon(
                glyph::EDIT,
                if chinese {
                    "编辑源码"
                } else {
                    "Edit source"
                },
            )
        } else {
            Control::Text(if chinese { "编辑" } else { "Edit" })
        },
        kind: design::ButtonKind::Accent,
        action: Some(EDIT),
        width: if narrow { design::CONTROL_HEIGHT } else { 64.0 },
    });
    let title = diagram
        .and_then(Diagram::title)
        .map(str::to_string)
        .unwrap_or_else(|| if chinese { "图表" } else { "Diagram" }.to_string());
    let meta = match (view.error, diagram, view.empty) {
        (Some(_), Some(_), _) if chinese => "源码有误 · 显示上次成功的图".to_string(),
        (Some(_), Some(_), _) => "Source error · showing the last good diagram".to_string(),
        (Some(_), None, _) if chinese => "源码有误".to_string(),
        (Some(_), None, _) => "Source error".to_string(),
        (None, Some(diagram), _) => summary(diagram, chinese),
        (None, None, _) if chinese => "空白图表".to_string(),
        (None, None, _) => "Empty diagram".to_string(),
    };
    build_panel_header(frame, title, meta, &actions, colors);
    if view.error.is_some()
        && !colors.high_contrast
        && let Some(node) = frame.nodes.iter_mut().find(|n| n.id == HEADER_META)
    {
        node.style.foreground = colors.paper_ink(paper::WARNING_INK);
    }
    let v = frame.viewport;
    let stage = stage_rect(v);
    let stage_fill = if colors.high_contrast {
        colors.subtle
    } else {
        colors.tone_surface(paper::STAGE)
    };
    frame.add(
        WidgetElement::text(STAGE, stage, "", design::BODY, 400, colors.primary)
            .role("button")
            .fill(Some(stage_fill), design::CARD_RADIUS)
            .action(Some(EDIT)),
    );
    frame.set_last_accessible_name(
        if chinese {
            "编辑图表源码"
        } else {
            "Edit diagram source"
        }
        .into(),
    );
    let ink = Ink::new(
        colors,
        if view.error.is_some() {
            0.4
        } else {
            view.appear
        },
    );
    let area = RectDip {
        x: stage.x + 12.0,
        y: stage.y + 12.0,
        w: (stage.w - 24.0).max(0.0),
        h: (stage.h - 24.0).max(0.0),
    };
    match view.rendered {
        Some(Rendered {
            diagram: Diagram::Flowchart(chart),
            flow: Some(layout),
            ..
        }) => draw_flowchart(frame, chart, layout, area, ink),
        Some(Rendered {
            diagram: Diagram::Gantt(chart),
            names_width,
            ..
        }) => draw_gantt(frame, chart, *names_width, area, ink, view.today, chinese),
        _ if view.error.is_none() => {
            let middle = area.y + area.h / 2.0;
            frame.add(
                WidgetElement::text(
                    EMPTY_TITLE,
                    RectDip {
                        y: middle - 24.0,
                        h: 24.0,
                        ..area
                    },
                    if chinese {
                        "用 Mermaid 写一张图"
                    } else {
                        "Draw a diagram with Mermaid"
                    },
                    design::SUBTITLE,
                    600,
                    colors.secondary,
                )
                .parent(STAGE)
                .align(TextAlign::Center),
            );
            frame.add(
                WidgetElement::text(
                    EMPTY_HINT,
                    RectDip {
                        y: middle + 4.0,
                        h: 20.0,
                        ..area
                    },
                    if chinese {
                        "点击编辑 · 支持 flowchart 与 gantt"
                    } else {
                        "Click to edit · flowchart and gantt"
                    },
                    design::CAPTION,
                    400,
                    colors.secondary,
                )
                .parent(STAGE)
                .align(TextAlign::Center),
            );
        }
        _ => {}
    }
    if let Some(error) = view.error {
        draw_error(frame, error, area, colors, chinese, view.rendered.is_some());
    }
}

/// Colors for diagram parts, faded together for the entrance and the error state.
#[derive(Clone, Copy)]
struct Ink {
    colors: Colors,
    opacity: f32,
}

impl Ink {
    fn new(colors: Colors, opacity: f32) -> Self {
        Self {
            colors,
            opacity: opacity.clamp(0.0, 1.0),
        }
    }
    fn surface(&self, paper: [f32; 4]) -> [f32; 4] {
        let fill = if self.colors.high_contrast {
            self.colors.subtle
        } else if paper == paper::TERRACOTTA {
            self.colors.accent
        } else {
            self.colors.tone_surface(paper)
        };
        fade(fill, self.opacity)
    }
    fn line(&self, paper: [f32; 4], system: [f32; 4]) -> [f32; 4] {
        fade(
            if self.colors.high_contrast {
                system
            } else {
                self.colors.paper_ink(paper)
            },
            self.opacity,
        )
    }
    fn text(&self) -> [f32; 4] {
        fade(self.colors.primary, self.opacity)
    }
    fn secondary(&self) -> [f32; 4] {
        fade(self.colors.secondary, self.opacity)
    }
}

/// Rounded-rectangle outline as a closed polyline (four points per corner).
fn rounded_outline(b: RectDip, radius: f32) -> Vec<(f32, f32)> {
    let r = radius.min(b.w / 2.0).min(b.h / 2.0).max(0.0);
    let corners = [
        (b.x + b.w - r, b.y + r, -90.0f32),
        (b.x + b.w - r, b.y + b.h - r, 0.0),
        (b.x + r, b.y + b.h - r, 90.0),
        (b.x + r, b.y + r, 180.0),
    ];
    let mut points = Vec::with_capacity(20);
    for (cx, cy, start) in corners {
        for step in 0..=4 {
            let angle = (start + step as f32 * 22.5).to_radians();
            points.push((cx + r * angle.cos(), cy + r * angle.sin()));
        }
    }
    points
}

fn bounds(points: &[(f32, f32)]) -> RectDip {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in points {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    RectDip {
        x: x0,
        y: y0,
        w: (x1 - x0).max(0.0),
        h: (y1 - y0).max(0.0),
    }
}

fn arrow_head(tip: (f32, f32), from: (f32, f32), size: f32) -> Vec<(f32, f32)> {
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
    let (ux, uy) = (dx / len, dy / len);
    let back = (tip.0 - ux * size, tip.1 - uy * size);
    let half = size * 0.55;
    vec![
        (back.0 - uy * half, back.1 + ux * half),
        tip,
        (back.0 + uy * half, back.1 - ux * half),
    ]
}

fn draw_flowchart(
    frame: &mut WidgetFrame,
    chart: &Flowchart,
    layout: &FlowLayout,
    area: RectDip,
    ink: Ink,
) {
    if chart.nodes.is_empty() || area.w <= 0.0 || area.h <= 0.0 {
        return;
    }
    // Fit to the stage, never enlarging; very large charts clip at 40%.
    let scale = (area.w / layout.width.max(1.0))
        .min(area.h / layout.height.max(1.0))
        .clamp(0.4, 1.0);
    let ox = area.x + ((area.w - layout.width * scale) / 2.0).max(0.0);
    let oy = area.y + ((area.h - layout.height * scale) / 2.0).max(0.0);
    let map = |(x, y): (f32, f32)| (ox + x * scale, oy + y * scale);
    let colors = ink.colors;
    let rect = |b: mermaid::Box2| {
        let (x, y) = map((b.x, b.y));
        RectDip {
            x,
            y,
            w: b.w * scale,
            h: b.h * scale,
        }
    };
    // Subgraph borders, parents first so nested ones paint on top.
    for (k, (cluster, border)) in chart.clusters.iter().zip(&layout.clusters).enumerate() {
        let Some(border) = border else { continue };
        let r = rect(*border);
        let id = k as u64;
        frame.add(
            WidgetElement::text(CLUSTER | id, r, "", design::CAPTION, 400, ink.text())
                .parent(STAGE)
                .role("group")
                .fill(Some(ink.surface(paper::CARD)), 8.0 * scale),
        );
        frame.set_last_accessible_name(cluster.title.clone());
        frame.add(
            WidgetElement::line(
                CLUSTER_OUTLINE | id,
                r,
                rounded_outline(r, 8.0 * scale),
                true,
                ink.line(paper::SHAPE_LINE, colors.secondary),
                1.0,
            )
            .parent(STAGE),
        );
        frame.add(
            WidgetElement::text(
                CLUSTER_TITLE | id,
                RectDip {
                    x: r.x + 8.0 * scale,
                    y: r.y + 2.0 * scale,
                    w: (r.w - 16.0 * scale).max(0.0),
                    h: mermaid::CLUSTER_TITLE * scale,
                },
                &cluster.title,
                (design::CAPTION * scale).max(9.0),
                600,
                ink.secondary(),
            )
            .parent(STAGE)
            .align(TextAlign::MiddleLeading),
        );
    }
    let link = ink.line(paper::LINK, colors.secondary);
    for (i, edge) in layout.edges.iter().enumerate() {
        let points: Vec<(f32, f32)> = edge.points.iter().copied().map(map).collect();
        let (rgba, width) = match edge.stroke {
            Stroke::Solid => (link, 1.5),
            Stroke::Thick => (ink.text(), 2.5),
            Stroke::Dotted => (fade(link, 0.6), 1.0),
        };
        frame.add(
            WidgetElement::line(
                EDGE | i as u64,
                bounds(&points),
                points.clone(),
                false,
                rgba,
                width,
            )
            .parent(STAGE),
        );
        if edge.arrow && points.len() >= 2 {
            let tip = points[points.len() - 1];
            let head = arrow_head(tip, points[points.len() - 2], 7.0 * scale.max(0.7));
            frame.add(
                WidgetElement::line(ARROW | i as u64, bounds(&head), head, true, rgba, width)
                    .parent(STAGE),
            );
        }
        if let Some((text, b)) = &edge.label {
            let r = rect(*b);
            frame.add(
                WidgetElement::text(
                    EDGE_LABEL | i as u64,
                    r,
                    text,
                    (mermaid::EDGE_LABEL_SIZE * scale).max(9.0),
                    400,
                    ink.secondary(),
                )
                .parent(STAGE)
                .fill(Some(ink.surface(paper::STAGE)), r.h.min(r.w) / 2.0)
                .align(TextAlign::Center),
            );
        }
    }
    for (i, node) in chart.nodes.iter().enumerate() {
        let rect = rect(layout.boxes[i]);
        let radius = match node.shape {
            Shape::Rect => 6.0,
            Shape::Round => 12.0,
            Shape::Stadium | Shape::Circle => rect.h / 2.0,
            Shape::Diamond => 0.0,
        } * if node.shape == Shape::Circle || node.shape == Shape::Stadium {
            1.0
        } else {
            scale
        };
        let outline = if node.shape == Shape::Diamond {
            vec![
                (rect.x + rect.w / 2.0, rect.y),
                (rect.x + rect.w, rect.y + rect.h / 2.0),
                (rect.x + rect.w / 2.0, rect.y + rect.h),
                (rect.x, rect.y + rect.h / 2.0),
            ]
        } else {
            rounded_outline(rect, radius)
        };
        let fill = (node.shape != Shape::Diamond).then(|| ink.surface(paper::SHAPE));
        frame.add(
            WidgetElement::text(
                NODE | i as u64,
                rect,
                &layout.labels[i],
                (mermaid::LABEL_SIZE * scale).max(9.0),
                400,
                ink.text(),
            )
            .parent(STAGE)
            .fill(fill, radius)
            .align(TextAlign::Center),
        );
        frame.add(
            WidgetElement::line(
                NODE_OUTLINE | i as u64,
                rect,
                outline,
                true,
                ink.line(paper::SHAPE_LINE, colors.primary),
                1.0,
            )
            .parent(STAGE),
        );
    }
}

fn draw_gantt(
    frame: &mut WidgetFrame,
    chart: &Gantt,
    names_width: f32,
    area: RectDip,
    ink: Ink,
    today: NaiveDate,
    chinese: bool,
) {
    let Some((start, end)) = chart.span() else {
        frame.add(
            WidgetElement::text(
                EMPTY_TITLE,
                area,
                if chinese {
                    "还没有任务"
                } else {
                    "No tasks yet"
                },
                design::BODY,
                400,
                ink.secondary(),
            )
            .parent(STAGE)
            .align(TextAlign::Center),
        );
        return;
    };
    let colors = ink.colors;
    let days = (end - start).num_days().max(1) as f32;
    let label_w = names_width.clamp(48.0, (area.w * 0.32).max(48.0)) + 8.0;
    let track = RectDip {
        x: area.x + label_w,
        y: area.y + 22.0,
        w: (area.w - label_w).max(0.0),
        h: (area.h - 22.0).max(0.0),
    };
    let x_of = |date: NaiveDate| track.x + (date - start).num_days() as f32 / days * track.w;
    // Ticks: Mondays for spans up to 12 weeks, else the first of each month.
    let weekly = days <= 84.0;
    let mut tick = start;
    let mut ticks = Vec::new();
    while tick < end && ticks.len() < 24 {
        let is_tick = if weekly {
            tick.weekday() == chrono::Weekday::Mon
        } else {
            tick.day() == 1
        };
        if is_tick || tick == start {
            ticks.push(tick);
        }
        tick = tick.succ_opt().unwrap_or(end);
    }
    let stride = ticks.len().div_ceil(8).max(1);
    let ticks: Vec<NaiveDate> = ticks.into_iter().step_by(stride).collect();
    for (i, &date) in ticks.iter().enumerate() {
        let x = x_of(date);
        let next = ticks.get(i + 1).map_or(track.x + track.w, |d| x_of(*d));
        let text = if weekly {
            format!("W{}", date.iso_week().week())
        } else if chinese {
            format!("{}月", date.month())
        } else {
            date.format("%b").to_string()
        };
        frame.add(
            WidgetElement::text(
                AXIS_LABEL | i as u64,
                RectDip {
                    x,
                    y: area.y,
                    w: (next - x).max(0.0),
                    h: 18.0,
                },
                text,
                design::CAPTION,
                400,
                ink.secondary(),
            )
            .parent(STAGE)
            .align(TextAlign::Center),
        );
    }
    let row_h = (track.h / chart.tasks.len().max(1) as f32).clamp(14.0, 32.0);
    let bar_h = (row_h - 8.0).max(8.0);
    for (i, task) in chart.tasks.iter().enumerate() {
        let y = track.y + i as f32 * row_h;
        if y + row_h > track.y + track.h + 0.5 {
            break;
        }
        frame.add(
            WidgetElement::text(
                TASK_LABEL | i as u64,
                RectDip {
                    x: area.x,
                    y,
                    w: label_w - 8.0,
                    h: row_h,
                },
                &task.name,
                design::CAPTION,
                if task.critical { 600 } else { 400 },
                ink.text(),
            )
            .parent(STAGE)
            .align(TextAlign::MiddleLeading),
        );
        let x0 = x_of(task.start);
        let range = format!(
            "{}/{} – {}/{}",
            task.start.month(),
            task.start.day(),
            task.end.pred_opt().unwrap_or(task.end).month(),
            task.end.pred_opt().unwrap_or(task.end).day()
        );
        if task.milestone {
            let (cx, cy, r) = (x0, y + row_h / 2.0, bar_h / 2.0);
            let diamond = vec![(cx, cy - r), (cx + r, cy), (cx, cy + r), (cx - r, cy)];
            frame.add(
                WidgetElement::line(
                    TASK_BAR | i as u64,
                    bounds(&diamond),
                    diamond,
                    true,
                    ink.line(paper::TERRACOTTA, colors.primary),
                    2.0,
                )
                .parent(STAGE),
            );
            frame.set_last_accessible_name(format!("{} {}", task.name, range));
            continue;
        }
        let w = (x_of(task.end) - x0).max(4.0);
        let (fill, text_ink) = if task.critical {
            (
                ink.surface(paper::TERRACOTTA),
                fade(colors.panel, ink.opacity),
            )
        } else if task.active {
            (
                ink.surface(colors.accent_wash()),
                fade(colors.accent_ink, ink.opacity),
            )
        } else if task.done {
            (ink.surface(paper::CARD), ink.secondary())
        } else {
            (ink.surface(paper::BAR), ink.text())
        };
        let show_range = Estimate.line_width(&range, 11.0) + 14.0 <= w && bar_h >= 14.0;
        frame.add(
            WidgetElement::text(
                TASK_BAR | i as u64,
                RectDip {
                    x: x0,
                    y: y + (row_h - bar_h) / 2.0,
                    w,
                    h: bar_h,
                },
                if show_range {
                    range.clone()
                } else {
                    String::new()
                },
                11.0,
                400,
                text_ink,
            )
            .parent(STAGE)
            .fill(Some(fill), bar_h / 2.0)
            .align(TextAlign::Center),
        );
        frame.set_last_accessible_name(format!("{} {}", task.name, range));
    }
    if (start..end).contains(&today) {
        let x = x_of(today);
        let points = vec![(x, track.y - 4.0), (x, track.y + track.h)];
        frame.add(
            WidgetElement::line(
                TODAY,
                bounds(&points),
                points,
                false,
                ink.line(paper::TERRACOTTA, colors.primary),
                1.5,
            )
            .parent(STAGE),
        );
        frame.set_last_accessible_name(if chinese { "今天" } else { "Today" }.into());
    }
}

fn draw_error(
    frame: &mut WidgetFrame,
    error: &ParseError,
    area: RectDip,
    colors: Colors,
    chinese: bool,
    has_previous: bool,
) {
    let h = 76.0f32.min(area.h);
    let banner = RectDip {
        x: area.x,
        y: if has_previous {
            area.y + area.h - h
        } else {
            area.y + (area.h - h) / 2.0
        },
        w: area.w,
        h,
    };
    let (wash, warning) = if colors.high_contrast {
        (colors.subtle, colors.primary)
    } else {
        (
            colors.tone_surface(paper::WARNING_WASH),
            colors.paper_ink(paper::WARNING_INK),
        )
    };
    frame.add(
        WidgetElement::text(BANNER, banner, "", design::BODY, 400, warning)
            .parent(STAGE)
            .role("alert")
            .fill(Some(wash), design::CARD_RADIUS),
    );
    let headline = match (error.line, chinese) {
        (0, _) => error.message(chinese),
        (line, true) => format!("第 {line} 行：{}", error.message(true)),
        (line, false) => format!("Line {line}: {}", error.message(false)),
    };
    frame.set_last_accessible_name(headline.clone());
    frame.add(
        WidgetElement::text(
            BANNER_TITLE,
            RectDip {
                x: banner.x + 14.0,
                y: banner.y + 10.0,
                w: (banner.w - 28.0).max(0.0),
                h: 22.0,
            },
            headline,
            design::BODY,
            600,
            warning,
        )
        .parent(BANNER)
        .align(TextAlign::MiddleLeading),
    );
    if error.line > 0 {
        frame.add(
            WidgetElement::text(
                BANNER_LINE,
                RectDip {
                    x: banner.x + 14.0,
                    y: banner.y + 38.0,
                    w: (banner.w - 28.0).max(0.0),
                    h: 22.0,
                },
                format!("{} │ {}", error.line, error.text),
                design::CAPTION,
                400,
                colors.secondary,
            )
            .parent(BANNER)
            .align(TextAlign::MiddleLeading),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecofence_plugin_api::scene::Scene;

    fn scene(source: &str, w: f32, h: f32, chinese: bool) -> Scene {
        let mut frame = WidgetFrame::new(
            MountKey {
                instance: InstanceKey {
                    id: 1,
                    activation: 1,
                },
                generation: 1,
            },
            1,
            1,
            FrameInput {
                viewport: RectDip {
                    x: 0.0,
                    y: 0.0,
                    w,
                    h,
                },
                locale: if chinese { "zh-CN" } else { "en" },
                presentation: Presentation::Compact,
                grouping: Grouping::Single,
                exposure: Exposure::Desktop,
                active: true,
                text_scale: 1.0,
                theme_epoch: 1,
                device_epoch: 1,
            },
        );
        // The widget never parses an empty source; mirror `DiagramWidget::load`.
        let parsed = (!source.trim().is_empty()).then(|| mermaid::parse(source));
        let rendered = parsed
            .as_ref()
            .and_then(|p| p.as_ref().ok())
            .cloned()
            .map(|diagram| Rendered::new(diagram, &Estimate));
        let error = parsed.and_then(|p| p.err());
        build_diagram(
            &mut frame,
            &View {
                empty: source.trim().is_empty(),
                rendered: rendered.as_ref(),
                error: error.as_ref(),
                can_undo: false,
                appear: 1.0,
                today: NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(),
            },
            Colors::paper(None),
            chinese,
        );
        frame.finish(&mut design::LayoutTracker::default()).unwrap()
    }

    #[test]
    fn template_flowchart_fits_the_stage_with_labelled_nodes_and_arrows() {
        let scene = scene(diagram_template(true), 560.0, 400.0, true);
        let stage = scene.nodes().iter().find(|n| n.id == STAGE).unwrap().rect;
        let nodes: Vec<_> = scene
            .nodes()
            .iter()
            .filter(|n| n.id >> 32 == NODE >> 32)
            .collect();
        assert_eq!(nodes.len(), 5);
        assert!(nodes.iter().any(|n| n.name == "集成"));
        for node in &nodes {
            assert!(node.rect.x >= stage.x && node.rect.x + node.rect.w <= stage.x + stage.w);
            assert!(node.rect.y >= stage.y && node.rect.y + node.rect.h <= stage.y + stage.h);
            assert_eq!(node.action, None);
        }
        let arrows = scene
            .nodes()
            .iter()
            .filter(|n| n.id >> 32 == ARROW >> 32)
            .count();
        assert_eq!(arrows, 5);
        assert!(
            scene
                .nodes()
                .iter()
                .filter(|n| n.style.path.is_some())
                .all(|n| n.style.stroke.is_some() && n.action.is_none())
        );
        let meta = scene.nodes().iter().find(|n| n.id == HEADER_META).unwrap();
        assert_eq!(meta.text, "流程图 · 5 个节点");
        // The whole stage edits the source; header controls stay separate.
        assert!(
            scene
                .hit_tree()
                .iter()
                .any(|h| h.id == STAGE && h.action == EDIT)
        );
        let mut canvas = pecofence_render::HeadlessCanvas::default();
        scene.paint(&mut canvas).unwrap();
        assert!(canvas.is_balanced());
        assert!(
            canvas
                .commands()
                .iter()
                .any(|c| matches!(c, pecofence_render::DrawCommand::StrokePath(_, _)))
        );
    }

    #[test]
    fn gantt_draws_axis_bars_today_and_milestones() {
        let source = "gantt\n title 版本计划\n dateFormat YYYY-MM-DD\n SYS 开发 :sys, 2026-10-19, 18d\n VND 开发 :vnd, 2026-10-19, 3w\n 集成 :crit, int, after sys vnd, 11d\n 发布 :milestone, after int, 0d\n";
        let scene = scene(source, 560.0, 400.0, true);
        let title = scene.nodes().iter().find(|n| n.id == 1).unwrap();
        assert_eq!(title.text, "版本计划");
        let meta = scene.nodes().iter().find(|n| n.id == HEADER_META).unwrap();
        assert_eq!(meta.text, "甘特图 · 10月19日 – 11月19日");
        let bars: Vec<_> = scene
            .nodes()
            .iter()
            .filter(|n| n.id >> 32 == TASK_BAR >> 32)
            .collect();
        assert_eq!(bars.len(), 4);
        assert_eq!(bars[0].rect.x, bars[1].rect.x);
        assert!(bars[1].rect.w > bars[0].rect.w);
        assert!(bars[2].rect.x >= bars[1].rect.x + bars[1].rect.w - 0.5);
        assert_eq!(bars[2].style.fill, Some(paper::TERRACOTTA));
        assert!(bars[3].style.path.is_some());
        assert!(scene.nodes().iter().any(|n| n.id == TODAY));
        assert!(scene.nodes().iter().any(|n| n.text == "W43"));
    }

    #[test]
    fn errors_show_line_and_text_and_keep_controls_reachable() {
        let scene = scene("flowchart LR\n  A --> B\n  SYS -->\n", 560.0, 400.0, false);
        let banner = scene.nodes().iter().find(|n| n.id == BANNER).unwrap();
        assert_eq!(banner.role, "alert");
        assert_eq!(banner.name, "Line 3: An arrow is missing its target");
        let line = scene.nodes().iter().find(|n| n.id == BANNER_LINE).unwrap();
        assert_eq!(line.text, "3 │ SYS -->");
        assert!(scene.hit_tree().iter().any(|h| h.action == EDIT));
        crate::assert_controls_fit(&scene);
    }

    #[test]
    fn empty_and_capsule_states_invite_editing() {
        let empty = scene("", 560.0, 400.0, true);
        assert!(
            empty
                .nodes()
                .iter()
                .any(|n| n.text == "用 Mermaid 写一张图")
        );
        let capsule = scene(diagram_template(false), 360.0, 48.0, false);
        assert!(
            capsule
                .nodes()
                .iter()
                .any(|n| n.text == "Flowchart · 5 nodes")
        );
        assert_eq!(capsule.hit_tree().len(), 1);
    }

    #[test]
    fn config_round_trips_and_rejects_nul() {
        let config = DiagramConfig {
            source: diagram_template(true).into(),
        };
        let bytes: Arc<[u8]> = serde_json::to_vec(&config).unwrap().into();
        assert_eq!(
            parse_config(&PanelConfig { version: 1, bytes }).unwrap(),
            config
        );
        assert!(validate_source("a\0b").is_err());
        assert!(validate_source(&"x".repeat(MAX_SOURCE_UNITS + 1)).is_err());
    }

    #[test]
    fn straight_links_and_the_today_line_are_painted() {
        // A horizontal link (I --> V) and Gantt's vertical today marker have empty
        // bounds; both must still reach the canvas.
        for (source, id) in [
            (diagram_template(true), EDGE | 4),
            ("gantt\n dateFormat YYYY-MM-DD\n A :2026-10-19, 3w\n", TODAY),
        ] {
            let scene = scene(source, 560.0, 400.0, true);
            let node = scene.nodes().iter().find(|n| n.id == id).unwrap();
            assert!(node.rect.w > 0.0 && node.rect.h > 0.0);
            let path = node.style.path.as_ref().unwrap();
            let mut canvas = pecofence_render::HeadlessCanvas::default();
            scene.paint(&mut canvas).unwrap();
            assert!(canvas.commands().iter().any(|c| matches!(
                c,
                pecofence_render::DrawCommand::StrokePath(p, _) if p == path
            )));
        }
    }

    #[test]
    fn subgraph_borders_and_link_labels_reach_the_scene() {
        let source = "flowchart LR\n  A[需求] -->|评审通过| B\n  subgraph dev [开发阶段]\n    B[SYS] --> C[VND]\n  end\n  C --> D[发布]\n";
        let scene = scene(source, 560.0, 400.0, true);
        let find = |id: u64| scene.nodes().iter().find(|n| n.id == id).unwrap();
        let border = find(CLUSTER);
        assert_eq!(
            (border.role.as_str(), border.name.as_str()),
            ("group", "开发阶段")
        );
        assert_eq!(find(CLUSTER_TITLE).text, "开发阶段");
        assert!(find(CLUSTER_OUTLINE).style.path.is_some());
        let label = find(EDGE_LABEL).rect;
        let nodes: Vec<_> = scene
            .nodes()
            .iter()
            .filter(|n| n.id >> 32 == NODE >> 32)
            .collect();
        for node in &nodes {
            let r = node.rect;
            let apart = label.x + label.w <= r.x
                || r.x + r.w <= label.x
                || label.y + label.h <= r.y
                || r.y + r.h <= label.y;
            assert!(apart, "label {label:?} overlaps {:?}", node.name);
        }
        let inside = |r: RectDip| r.x > border.rect.x && r.x + r.w < border.rect.x + border.rect.w;
        assert_eq!(nodes.iter().filter(|n| inside(n.rect)).count(), 2);
    }

    #[test]
    fn source_editor_is_a_monospace_page_over_the_stage() {
        let frame = WidgetFrame::new(
            MountKey {
                instance: InstanceKey {
                    id: 1,
                    activation: 1,
                },
                generation: 1,
            },
            1,
            1,
            FrameInput {
                viewport: RectDip {
                    x: 0.0,
                    y: 0.0,
                    w: 560.0,
                    h: 384.0,
                },
                locale: "en",
                presentation: Presentation::Compact,
                grouping: Grouping::Single,
                exposure: Exposure::Desktop,
                active: true,
                text_scale: 1.0,
                theme_epoch: 1,
                device_epoch: 1,
            },
        );
        let field = source_field(&frame, Colors::paper(None));
        assert_eq!(field.bounds, stage_rect(frame.viewport));
        assert_eq!(field.font, TextInputFont::Mono);
        assert_eq!(field.background, paper::SHAPE);
        assert_eq!(field.padding, [14.0, 12.0, 14.0, 12.0]);
        assert_eq!(field.accent, Some(paper::TERRACOTTA));
    }
}

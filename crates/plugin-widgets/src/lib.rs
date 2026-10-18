//! First-party local-time widgets. State, date math and layout stay independent of Win32;
//! the host supplies the timer, theme and Direct2D canvas.

use chrono::{Datelike, Local, NaiveDate, Timelike};
use pecofence_plugin_api::design::motion::{self, Curve};
use pecofence_plugin_api::design::{self, ContentScale, LayoutTracker};
use pecofence_plugin_api::scene::{NodeStyle, Scene, SceneNode};
use pecofence_plugin_api::*;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod lunar;
mod note;
mod todo;
pub use note::{NOTE_PROVIDER, NotePlugin};
pub use todo::{TODO_PROVIDER, TodoPlugin};

pub const CLOCK_PROVIDER: &str = "pecofence.clock";
pub const CALENDAR_PROVIDER: &str = "pecofence.calendar";
const PREVIOUS: u64 = 1;
const NEXT: u64 = 2;
const TODAY: u64 = 3;
const DAY_BASE: u64 = 1_000;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WidgetConfig {
    pub show_seconds: bool,
    pub use_12_hour: bool,
    pub week_starts_on_sunday: bool,
}

#[derive(Clone, Copy)]
enum Kind {
    Clock,
    Calendar,
}

pub struct ClockPlugin;
pub struct CalendarPlugin;

fn validate(config: &PanelConfig) -> Result<WidgetConfig> {
    if config.version != 1 {
        return Err(Error::Invalid("unsupported widget config version".into()));
    }
    serde_json::from_slice(&config.bytes).map_err(|error| Error::Invalid(error.to_string()))
}

fn create(kind: Kind, ctx: PluginContext, input: CreatePanel) -> Result<Box<dyn PanelInstance>> {
    let config = validate(&input.config)?;
    let theme = ctx.theme.with(|service| service.snapshot()).ok();
    Ok(Box::new(TimeWidget {
        ctx,
        key: input.key,
        mount: None,
        kind,
        config,
        month_offset: 0,
        selected_day: None,
        month_motion: None,
        theme,
        frame_revision: 0,
        layout_revision: 0,
        layout_tracker: LayoutTracker::default(),
        stopped: false,
    }))
}

impl PanelProvider for ClockPlugin {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: CLOCK_PROVIDER,
            api_major: 1,
            config_major: 1,
            required_services: &["theme"],
        }
    }

    fn validate(&self, config: &PanelConfig) -> Result<()> {
        validate(config).map(|_| ())
    }

    fn create(&self, ctx: PluginContext, input: CreatePanel) -> Result<Box<dyn PanelInstance>> {
        create(Kind::Clock, ctx, input)
    }
}

impl PanelProvider for CalendarPlugin {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: CALENDAR_PROVIDER,
            api_major: 1,
            config_major: 1,
            required_services: &["theme"],
        }
    }

    fn validate(&self, config: &PanelConfig) -> Result<()> {
        validate(config).map(|_| ())
    }

    fn create(&self, ctx: PluginContext, input: CreatePanel) -> Result<Box<dyn PanelInstance>> {
        create(Kind::Calendar, ctx, input)
    }
}

struct TimeWidget {
    ctx: PluginContext,
    key: InstanceKey,
    mount: Option<MountKey>,
    kind: Kind,
    config: WidgetConfig,
    month_offset: i32,
    selected_day: Option<u32>,
    /// Month change in flight and its direction (-1 previous, 1 next).
    month_motion: Option<(Tween, f32)>,
    theme: Option<Arc<ThemeSnapshot>>,
    frame_revision: u64,
    layout_revision: u64,
    layout_tracker: LayoutTracker,
    stopped: bool,
}

impl TimeWidget {
    fn animate(&self) -> bool {
        self.theme.as_ref().is_none_or(|theme| theme.animations)
    }

    fn next_clock_wake(&self) -> Duration {
        let now = Local::now();
        let within_second = now.timestamp_subsec_millis() as u64;
        let millis = if self.config.show_seconds {
            1_000 - within_second
        } else {
            60_000 - now.second() as u64 * 1_000 - within_second
        };
        Duration::from_millis(millis + 25)
    }

    fn next_calendar_wake(&self) -> Duration {
        let now = Local::now();
        let next_day = now.date_naive().succ_opt();
        let millis = next_day
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .and_then(|midnight| {
                use chrono::TimeZone;
                Local.from_local_datetime(&midnight).earliest()
            })
            .map(|midnight| (midnight - now).num_milliseconds().max(0) as u64 + 25)
            .unwrap_or(3_600_000);
        // Recheck after resume and unusual timezone transitions even without a broadcast.
        Duration::from_millis(millis.min(3_600_000))
    }
}

impl PanelInstance for TimeWidget {
    fn initial_presentation(&self) -> Presentation {
        match self.kind {
            Kind::Clock => Presentation::Capsule,
            Kind::Calendar => Presentation::Compact,
        }
    }

    fn mount(&mut self, ctx: MountContext) -> Result<()> {
        if ctx.key.instance != self.key {
            return Err(Error::Invalid("mount belongs to another instance".into()));
        }
        self.mount = Some(ctx.key);
        Ok(())
    }

    fn event(&mut self, event: PanelEvent) -> Result<PanelUpdate> {
        if self.stopped {
            return Err(Error::Closed);
        }
        match event {
            PanelEvent::Wake => Ok(invalidate()),
            PanelEvent::ThemeChanged { .. } => {
                self.theme = self.ctx.theme.with(|service| service.snapshot()).ok();
                Ok(invalidate())
            }
            PanelEvent::Invoke { action, .. } if matches!(self.kind, Kind::Calendar) => {
                let before = self.month_offset;
                let changed = match action {
                    PREVIOUS if self.month_offset > -1_200 => {
                        self.month_offset -= 1;
                        self.selected_day = None;
                        true
                    }
                    NEXT if self.month_offset < 1_200 => {
                        self.month_offset += 1;
                        self.selected_day = None;
                        true
                    }
                    TODAY => {
                        self.month_offset = 0;
                        self.selected_day = Some(Local::now().day());
                        true
                    }
                    day if (DAY_BASE + 1..=DAY_BASE + 31).contains(&day) => {
                        self.selected_day = Some((day - DAY_BASE) as u32);
                        true
                    }
                    _ => false,
                };
                if changed {
                    if self.month_offset != before {
                        let direction = (self.month_offset - before).signum() as f32;
                        self.month_motion =
                            Tween::start(motion::FAST, Curve::Decelerate, self.animate())
                                .map(|tween| (tween, direction));
                    }
                    self.layout_revision = self.layout_revision.saturating_add(1);
                    Ok(invalidate())
                } else {
                    Ok(PanelUpdate::default())
                }
            }
            _ => Ok(PanelUpdate::default()),
        }
    }

    fn prepare_frame(&mut self, input: FrameInput) -> Result<Rc<dyn PreparedFrame>> {
        let mount_key = self.mount.ok_or(Error::Closed)?;
        self.frame_revision = self.frame_revision.checked_add(1).ok_or(Error::Exhausted)?;
        let mut frame =
            WidgetFrame::new(mount_key, self.frame_revision, self.layout_revision, input);
        let colors = match self.kind {
            Kind::Clock => Colors::from_theme(self.theme.as_deref()),
            Kind::Calendar => Colors::paper(self.theme.as_deref()),
        };
        let chinese = input.locale.starts_with("zh");
        match self.kind {
            Kind::Clock => build_clock(&mut frame, self.config, colors, chinese),
            Kind::Calendar => {
                let now = Instant::now();
                if self.month_motion.is_some_and(|(tween, _)| tween.done(now)) {
                    self.month_motion = None;
                }
                build_calendar(
                    &mut frame,
                    self.config,
                    self.month_offset,
                    self.selected_day,
                    colors,
                    chinese,
                    Local::now().date_naive(),
                    self.month_motion
                        .map_or(MonthMotion::REST, |(tween, direction)| MonthMotion {
                            progress: tween.progress(now),
                            direction,
                        }),
                )
            }
        }
        let frame = frame.finish(&mut self.layout_tracker)?;
        Ok(Rc::new(frame))
    }

    fn unmount(&mut self, key: MountKey) {
        if self.mount == Some(key) {
            self.mount = None;
        }
    }

    fn begin_stop(&mut self, _reason: StopReason) {
        self.stopped = true;
        self.mount = None;
    }

    fn next_wake_after(&self) -> Option<Duration> {
        if self.stopped {
            return None;
        }
        if self.month_motion.is_some() {
            return Some(FRAME_INTERVAL);
        }
        Some(match self.kind {
            Kind::Clock => self.next_clock_wake(),
            Kind::Calendar => self.next_calendar_wake(),
        })
    }
}

fn invalidate() -> PanelUpdate {
    PanelUpdate {
        commands: vec![HostCommand::Invalidate],
        relayout: false,
    }
}

/// Wake interval while a tween runs; the host clamps wakes to at least 10 ms.
pub(crate) const FRAME_INTERVAL: Duration = Duration::from_millis(16);

/// One-shot Fluent tween driven by provider wakes. Providers start one in response to
/// input and request [`FRAME_INTERVAL`] wakes until it is done; no tween is created when
/// the user has turned animation effects off, so the end state is drawn directly.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tween {
    start: Instant,
    duration: Duration,
    curve: Curve,
}

impl Tween {
    pub(crate) fn start(duration: Duration, curve: Curve, enabled: bool) -> Option<Self> {
        enabled.then(|| Self {
            start: Instant::now(),
            duration,
            curve,
        })
    }

    /// Eased progress in `0..=1`.
    pub(crate) fn progress(&self, now: Instant) -> f32 {
        let t = now.saturating_duration_since(self.start).as_secs_f32()
            / self.duration.as_secs_f32().max(f32::EPSILON);
        motion::ease(self.curve, t)
    }

    pub(crate) fn done(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= self.duration
    }
}

pub(crate) fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

pub(crate) fn lerp_color(from: [f32; 4], to: [f32; 4], t: f32) -> [f32; 4] {
    std::array::from_fn(|i| lerp(from[i], to[i], t).clamp(0.0, 1.0))
}

pub(crate) fn fade(color: [f32; 4], opacity: f32) -> [f32; 4] {
    [
        color[0],
        color[1],
        color[2],
        (color[3] * opacity).clamp(0.0, 1.0),
    ]
}

const fn srgb(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

/// Warm paper palette for the note, TODO and calendar widgets. Every text pair
/// meets 4.5:1 on both the ground and the card; terracotta fills carry cream ink.
pub(crate) mod paper {
    use super::srgb;
    pub(crate) const GROUND: [f32; 4] = srgb(0xF6, 0xF1, 0xE7);
    pub(crate) const CARD: [f32; 4] = srgb(0xEC, 0xE4, 0xD6);
    pub(crate) const INK: [f32; 4] = srgb(0x2B, 0x27, 0x22);
    pub(crate) const INK_SECONDARY: [f32; 4] = srgb(0x68, 0x5F, 0x55);
    pub(crate) const INK_FAINT: [f32; 4] = srgb(0x9A, 0x8F, 0x80);
    pub(crate) const TERRACOTTA: [f32; 4] = srgb(0xA9, 0x50, 0x2D);
    pub(crate) const TERRACOTTA_INK: [f32; 4] = srgb(0x94, 0x45, 0x26);
    pub(crate) const CREAM: [f32; 4] = srgb(0xFF, 0xF8, 0xEE);
}

#[derive(Clone, Copy)]
pub(crate) struct Colors {
    pub(crate) high_contrast: bool,
    pub(crate) primary: [f32; 4],
    pub(crate) secondary: [f32; 4],
    /// Solid emphasis fills (today, progress, checked boxes).
    pub(crate) accent: [f32; 4],
    /// Accent used as text, darker where the fill would not reach 4.5:1.
    pub(crate) accent_ink: [f32; 4],
    pub(crate) subtle: [f32; 4],
    /// Ink on solid accent fills.
    pub(crate) panel: [f32; 4],
    faint_ink: [f32; 4],
    /// Widget-owned background; `None` draws on the fence's own material.
    pub(crate) ground: Option<[f32; 4]>,
}

impl Colors {
    /// Accent wash for emphasis fills; high contrast keeps the opaque subtle surface.
    pub(crate) fn accent_wash(&self) -> [f32; 4] {
        if self.high_contrast {
            self.subtle
        } else {
            let alpha = if self.ground.is_some() { 0.14 } else { 0.22 };
            [self.accent[0], self.accent[1], self.accent[2], alpha]
        }
    }

    /// Ink for content that is present but out of focus, such as adjacent-month
    /// dates. High contrast keeps the opaque system color.
    pub(crate) fn faint(&self) -> [f32; 4] {
        self.faint_ink
    }

    /// Foreground, fill and pill radius for a button of height `h` (logical DIPs).
    pub(crate) fn button(
        &self,
        kind: design::ButtonKind,
        h: f32,
    ) -> ([f32; 4], Option<[f32; 4]>, f32) {
        use design::ButtonKind::*;
        let (foreground, fill) = match kind {
            Accent => (self.accent_ink, Some(self.accent_wash())),
            Subtle => (self.primary, Some(self.subtle)),
            Flat => (self.primary, None),
        };
        (foreground, fill, design::pill_radius(h))
    }

    /// Host theme colors: the clock draws directly on the fence material.
    pub(crate) fn from_theme(theme: Option<&ThemeSnapshot>) -> Self {
        let fallback = ThemeSnapshot::dark();
        let theme = theme.unwrap_or(&fallback);
        let secondary = theme.text_secondary;
        Self {
            high_contrast: theme.high_contrast,
            primary: theme.text_primary,
            secondary,
            accent: theme.accent,
            accent_ink: theme.accent,
            subtle: theme.surface_subtle,
            panel: theme.surface_panel,
            faint_ink: if theme.high_contrast {
                secondary
            } else {
                fade(secondary, 0.5)
            },
            ground: None,
        }
    }

    /// Warm paper colors for the note, TODO and calendar widgets. High contrast
    /// keeps the system palette so user-chosen contrast colors always win.
    pub(crate) fn paper(theme: Option<&ThemeSnapshot>) -> Self {
        if theme.is_some_and(|theme| theme.high_contrast) {
            return Self::from_theme(theme);
        }
        Self {
            high_contrast: false,
            primary: paper::INK,
            secondary: paper::INK_SECONDARY,
            accent: paper::TERRACOTTA,
            accent_ink: paper::TERRACOTTA_INK,
            subtle: paper::CARD,
            panel: paper::CREAM,
            faint_ink: paper::INK_FAINT,
            ground: Some(paper::GROUND),
        }
    }
}

/// Segoe Fluent Icons code points used by the local widgets.
pub(crate) mod glyph {
    pub(crate) const ADD: char = '\u{E710}';
    pub(crate) const BROOM: char = '\u{EA99}';
    pub(crate) const CALENDAR_DAY: char = '\u{E8BF}';
    pub(crate) const COLOR: char = '\u{E790}';
    pub(crate) const EDIT: char = '\u{E70F}';
    pub(crate) const UNDO: char = '\u{E7A7}';
    pub(crate) const CHEVRON_LEFT: char = '\u{E76B}';
    pub(crate) const CHEVRON_RIGHT: char = '\u{E76C}';
    pub(crate) const CHECKBOX: char = '\u{E739}';
    pub(crate) const CHECKBOX_CHECKED: char = '\u{E73D}';
    pub(crate) const CHECKLIST: char = '\u{E9D5}';
    pub(crate) const QUICK_NOTE: char = '\u{E70B}';
}

/// Node id of the paper ground; clear of control, task (bit 63) and date (bits 32–33) ids.
const BACKGROUND: u64 = 1 << 62;

/// Node id of the header summary; clear of every widget's control ids.
pub(crate) const HEADER_META: u64 = 20;

/// Below this logical height a widget draws its one-line capsule summary
/// (DeskBox capsules) instead of a clipped full layout.
pub(crate) const CAPSULE_HEIGHT: f32 = 120.0;

/// Single-line capsule: an optional leading control and a summary that fills the rest.
pub(crate) fn build_capsule(
    frame: &mut WidgetFrame,
    summary: String,
    action: Option<u64>,
    surface: Option<[f32; 4]>,
    colors: Colors,
) {
    let v = frame.viewport;
    let rect = RectDip {
        x: v.x + 8.0,
        y: v.y + 6.0,
        w: (v.w - 16.0).max(0.0),
        h: (v.h - 12.0).max(0.0),
    };
    let mut element = WidgetElement::text(1, rect, "", design::BODY, 400, colors.primary).fill(
        surface,
        design::pill_radius(rect.h).min(design::CARD_RADIUS * 2.0),
    );
    if action.is_some() {
        element = element.role("button").action(action);
    }
    frame.add(element);
    frame.set_last_accessible_name(summary.clone());
    frame.add(
        WidgetElement::text(
            2,
            RectDip {
                x: rect.x + 12.0,
                w: (rect.w - 24.0).max(0.0),
                ..rect
            },
            summary,
            design::BODY,
            400,
            colors.primary,
        )
        .parent(1)
        .align(TextAlign::MiddleLeading),
    );
}

/// Top of the content area below the shared title/meta header.
pub(crate) const CONTENT_TOP: f32 = 66.0;

/// Named component input keeps layout, appearance and action intent explicit.
/// The caller declares stable identity; this preparation-time adapter applies content scaling.
pub(crate) struct WidgetElement<'a> {
    id: u64,
    parent: Option<u64>,
    rect: RectDip,
    text: String,
    size: f32,
    weight: u16,
    foreground: [f32; 4],
    fill: Option<[f32; 4]>,
    radius: f32,
    action: Option<u64>,
    role: &'a str,
    font: Option<FontRole>,
    align: TextAlign,
}

impl<'a> WidgetElement<'a> {
    /// Plain, non-interactive text; the builder methods add surface and intent.
    pub(crate) fn text(
        id: u64,
        rect: RectDip,
        text: impl Into<String>,
        size: f32,
        weight: u16,
        foreground: [f32; 4],
    ) -> Self {
        Self {
            id,
            parent: None,
            rect,
            text: text.into(),
            size,
            weight,
            foreground,
            fill: None,
            radius: 0.0,
            action: None,
            role: "text",
            font: None,
            align: TextAlign::TopLeading,
        }
    }

    pub(crate) fn role(mut self, role: &'a str) -> Self {
        self.role = role;
        self
    }

    pub(crate) fn parent(mut self, parent: u64) -> Self {
        self.parent = Some(parent);
        self
    }

    pub(crate) fn fill(mut self, fill: Option<[f32; 4]>, radius: f32) -> Self {
        self.fill = fill;
        self.radius = radius;
        self
    }

    pub(crate) fn action(mut self, action: Option<u64>) -> Self {
        self.action = action;
        self
    }

    pub(crate) fn align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// Render the text with the system icon font instead of the text ramp.
    pub(crate) fn icon(mut self) -> Self {
        self.font = Some(FontRole::Icons);
        self
    }

    /// Render the text with the serif title face.
    pub(crate) fn serif(mut self) -> Self {
        self.font = Some(FontRole::Serif);
        self
    }
}

pub(crate) struct WidgetFrame {
    identity: FrameIdentity,
    pub(crate) viewport: RectDip,
    clip: RectDip,
    scale: ContentScale,
    nodes: Vec<SceneNode>,
}

impl WidgetFrame {
    pub(crate) fn new(
        mount_key: MountKey,
        frame_revision: u64,
        layout_revision: u64,
        input: FrameInput,
    ) -> Self {
        Self {
            identity: FrameIdentity {
                mount_key,
                frame_revision,
                layout_revision,
                theme_epoch: input.theme_epoch,
                device_epoch: input.device_epoch,
            },
            viewport: ContentScale::new(input.viewport, input.text_scale).logical_viewport(),
            clip: input.viewport,
            scale: ContentScale::new(input.viewport, input.text_scale),
            nodes: Vec::new(),
        }
    }

    fn finish(self, tracker: &mut LayoutTracker) -> Result<Scene> {
        let scene = Scene::new(self.identity, self.clip, self.nodes)?;
        let revision = tracker.update(scene.hit_tree(), self.identity.layout_revision)?;
        Ok(scene.with_layout_revision(revision))
    }

    pub(crate) fn set_last_accessible_name(&mut self, name: String) {
        if let Some(node) = self.nodes.last_mut() {
            node.name = name;
        }
    }

    pub(crate) fn add(&mut self, element: WidgetElement<'_>) {
        let WidgetElement {
            id,
            parent,
            rect,
            text,
            size,
            weight,
            foreground,
            fill,
            radius,
            action,
            role,
            font,
            align,
        } = element;
        let font = font.unwrap_or(if size < design::BODY {
            FontRole::Small
        } else if size >= 28.0 {
            FontRole::Display
        } else {
            FontRole::Text
        });
        let rect = self.scale.rect(rect);
        let size = self.scale.value(size);
        let radius = self.scale.value(radius);
        self.nodes.push(SceneNode {
            id,
            parent,
            rect,
            role: role.into(),
            name: text.clone(),
            text,
            action,
            style: NodeStyle {
                stroke: None,
                size,
                font,
                weight,
                foreground,
                fill,
                radius,
                text_inset: [0.0, 0.0],
                align,
            },
        });
    }

    /// Paint the widget's own ground behind everything else, when its palette has one.
    pub(crate) fn ground(&mut self, colors: Colors) {
        let Some(ground) = colors.ground else {
            return;
        };
        let v = self.viewport;
        self.add(
            WidgetElement::text(BACKGROUND, v, "", design::BODY, 400, colors.primary)
                .role("presentation")
                .fill(Some(ground), design::CARD_RADIUS),
        );
    }

    /// Pill button with centered content. A `None` action renders it disabled.
    pub(crate) fn button(
        &mut self,
        id: u64,
        rect: RectDip,
        content: Control<'_>,
        kind: design::ButtonKind,
        action: Option<u64>,
        colors: Colors,
    ) {
        let (mut foreground, mut fill, radius) = colors.button(kind, rect.h);
        if action.is_none() {
            foreground = colors.faint();
            fill = fill.map(|_| colors.subtle);
        }
        let element = match content {
            Control::Text(label) => WidgetElement::text(
                id,
                rect,
                label,
                design::BODY,
                if kind == design::ButtonKind::Accent {
                    600
                } else {
                    400
                },
                foreground,
            ),
            Control::Icon(glyph, _) => WidgetElement::text(
                id,
                rect,
                glyph,
                (rect.h * 0.5).clamp(12.0, 16.0),
                400,
                foreground,
            )
            .icon(),
        };
        self.add(
            element
                .role("button")
                .fill(fill, radius)
                .action(action)
                .align(TextAlign::Center),
        );
        if let Control::Icon(_, name) = content {
            self.set_last_accessible_name(name.into());
        }
    }
}

/// Visible content of a button: a text label, or an icon glyph with its accessible name.
#[derive(Clone, Copy)]
pub(crate) enum Control<'a> {
    Text(&'a str),
    Icon(char, &'a str),
}

pub(crate) struct HeaderAction<'a> {
    pub(crate) id: u64,
    pub(crate) content: Control<'a>,
    pub(crate) kind: design::ButtonKind,
    pub(crate) action: Option<u64>,
    pub(crate) width: f32,
}

/// Shared header for local widgets: title with a one-line summary below it, and
/// right-aligned actions laid out left to right in the given order.
pub(crate) fn build_panel_header(
    frame: &mut WidgetFrame,
    title: impl Into<String>,
    meta: impl Into<String>,
    actions: &[HeaderAction<'_>],
    colors: Colors,
) {
    const GAP: f32 = 4.0;
    let v = frame.viewport;
    let right = v.x + v.w - design::CONTENT_INSET;
    let actions_width =
        actions.iter().map(|a| a.width).sum::<f32>() + GAP * actions.len().saturating_sub(1) as f32;
    let text_width = (right - actions_width - 8.0 - (v.x + design::CONTENT_INSET)).max(0.0);
    frame.add(
        WidgetElement::text(
            1,
            RectDip {
                x: v.x + design::CONTENT_INSET,
                y: v.y + 10.0,
                w: text_width,
                h: 28.0,
            },
            title,
            design::TITLE,
            600,
            colors.primary,
        )
        .serif()
        .role("heading")
        .align(TextAlign::MiddleLeading),
    );
    frame.add(
        WidgetElement::text(
            HEADER_META,
            RectDip {
                x: v.x + design::CONTENT_INSET,
                y: v.y + 38.0,
                w: text_width,
                h: 20.0,
            },
            meta,
            design::CAPTION,
            400,
            colors.secondary,
        )
        .align(TextAlign::MiddleLeading),
    );
    let mut x = (right - actions_width).max(v.x + design::CONTENT_INSET);
    for action in actions {
        frame.button(
            action.id,
            RectDip {
                x,
                y: v.y + 14.0,
                w: action.width,
                h: design::CONTROL_HEIGHT,
            },
            action.content,
            action.kind,
            action.action,
            colors,
        );
        x += action.width + GAP;
    }
}

fn build_clock(frame: &mut WidgetFrame, config: WidgetConfig, colors: Colors, chinese: bool) {
    let now = Local::now();
    let v = frame.viewport;
    let inset = 20.0;
    let width = (v.w - inset * 2.0).max(0.0);
    let format = match (config.use_12_hour, config.show_seconds) {
        (true, true) => "%I:%M:%S %p",
        (true, false) => "%I:%M %p",
        (false, true) => "%H:%M:%S",
        (false, false) => "%H:%M",
    };
    let time = now.format(format).to_string();
    let date = if chinese {
        let weekday = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
            [now.weekday().num_days_from_monday() as usize];
        format!(
            "{}年{}月{}日  {weekday}",
            now.year(),
            now.month(),
            now.day()
        )
    } else {
        now.format("%Y-%m-%d  %a").to_string()
    };
    // Capsule height is a real layout, not a clipped version of the large clock.
    // Text scaling reduces this logical viewport before we choose the layout.
    if v.h < 120.0 {
        let show_date = v.w >= 400.0;
        frame.add(WidgetElement::text(
            1,
            RectDip {
                x: v.x + design::CONTENT_INSET,
                y: v.y,
                w: if show_date { width * 0.5 } else { width },
                h: v.h,
            },
            time,
            if v.h >= 44.0 { 28.0 } else { design::BODY },
            600,
            colors.primary,
        ));
        if show_date {
            frame.add(WidgetElement::text(
                2,
                RectDip {
                    x: v.x + v.w * 0.5,
                    y: v.y,
                    w: (v.w * 0.5 - design::CONTENT_INSET).max(0.0),
                    h: v.h,
                },
                date,
                design::BODY,
                400,
                colors.secondary,
            ));
        }
        return;
    }
    let time_size = if v.w < 260.0 { 37.0 } else { 52.0 };
    let top = v.y + ((v.h - 115.0) / 2.0).max(12.0);
    frame.add(WidgetElement::text(
        1,
        RectDip {
            x: v.x + inset,
            y: top,
            w: width,
            h: 72.0,
        },
        time,
        time_size,
        600,
        colors.primary,
    ));
    frame.add(WidgetElement::text(
        2,
        RectDip {
            x: v.x + inset + 2.0,
            y: top + 76.0,
            w: width,
            h: 30.0,
        },
        date,
        17.0,
        400,
        colors.secondary,
    ));
}

const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn describe_day(date: NaiveDate, chinese: bool) -> String {
    if chinese {
        let weekday = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
            [date.weekday().num_days_from_monday() as usize];
        format!("{}月{}日 {weekday}", date.month(), date.day())
    } else {
        date.format("%a, %b %-d").to_string()
    }
}

/// Month-change entrance: the grid fades in while the title slides from the
/// direction of travel (Fluent FAST, decelerate).
#[derive(Clone, Copy)]
struct MonthMotion {
    progress: f32,
    direction: f32,
}

impl MonthMotion {
    const REST: Self = Self {
        progress: 1.0,
        direction: 0.0,
    };
}

#[allow(clippy::too_many_arguments)]
fn build_calendar(
    frame: &mut WidgetFrame,
    config: WidgetConfig,
    offset: i32,
    selected_day: Option<u32>,
    colors: Colors,
    chinese: bool,
    today: NaiveDate,
    motion: MonthMotion,
) {
    frame.ground(colors);
    if frame.viewport.h < CAPSULE_HEIGHT {
        let summary = match (chinese, lunar::describe(today)) {
            (true, Some(lunar)) => format!("{} · {lunar}", describe_day(today, true)),
            (true, None) => describe_day(today, true),
            (false, _) => describe_day(today, false),
        };
        build_capsule(frame, summary, Some(TODAY), None, colors);
        return;
    }
    let (year, month) = month_for(today, offset);
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return;
    };
    let selected = selected_day.and_then(|day| first.with_day(day));
    // Below this width the month title would be squeezed by text buttons.
    let narrow = frame.viewport.w < 300.0;
    let meta = match selected {
        Some(date) if date != today => {
            if chinese {
                match lunar::describe(date) {
                    Some(lunar) => format!("已选 {} · {lunar}", describe_day(date, true)),
                    None => format!("已选 {}", describe_day(date, true)),
                }
            } else {
                format!("Selected · {}", describe_day(date, false))
            }
        }
        _ if chinese => match lunar::describe(today) {
            Some(lunar) => format!("今天 {} · {lunar}", describe_day(today, true)),
            None => format!("今天 {}", describe_day(today, true)),
        },
        _ => format!("Today · {}", describe_day(today, false)),
    };
    build_panel_header(
        frame,
        if chinese {
            format!("{year}年{month}月")
        } else {
            format!("{} {year}", MONTHS_EN[month as usize - 1])
        },
        meta,
        &[
            HeaderAction {
                id: 2 + PREVIOUS,
                content: Control::Icon(
                    glyph::CHEVRON_LEFT,
                    if chinese {
                        "上个月"
                    } else {
                        "Previous month"
                    },
                ),
                kind: design::ButtonKind::Flat,
                action: Some(PREVIOUS),
                width: design::CONTROL_HEIGHT,
            },
            HeaderAction {
                id: 2 + TODAY,
                content: if narrow {
                    Control::Icon(glyph::CALENDAR_DAY, if chinese { "今天" } else { "Today" })
                } else {
                    Control::Text(if chinese { "今天" } else { "Today" })
                },
                kind: design::ButtonKind::Accent,
                action: Some(TODAY),
                width: if narrow {
                    design::CONTROL_HEIGHT
                } else if chinese {
                    56.0
                } else {
                    64.0
                },
            },
            HeaderAction {
                id: 2 + NEXT,
                content: Control::Icon(
                    glyph::CHEVRON_RIGHT,
                    if chinese { "下个月" } else { "Next month" },
                ),
                kind: design::ButtonKind::Flat,
                action: Some(NEXT),
                width: design::CONTROL_HEIGHT,
            },
        ],
        colors,
    );
    let opacity = lerp(0.15, 1.0, motion.progress);
    let slide = frame
        .scale
        .value(motion.direction * 16.0 * (1.0 - motion.progress));
    if let Some(title) = frame.nodes.iter_mut().find(|node| node.id == 1) {
        title.rect.x += slide;
        title.style.foreground = fade(title.style.foreground, opacity);
    }
    let v = frame.viewport;
    let pad = 12.0;
    let width = (v.w - pad * 2.0).max(0.0);
    let cell_w = width / 7.0;
    let labels = if chinese && config.week_starts_on_sunday {
        ["日", "一", "二", "三", "四", "五", "六"]
    } else if chinese {
        ["一", "二", "三", "四", "五", "六", "日"]
    } else if config.week_starts_on_sunday {
        ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
    } else {
        ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
    };
    let grid_y = v.y + CONTENT_TOP;
    for (col, label) in labels.iter().enumerate() {
        let weekday = if config.week_starts_on_sunday {
            (col + 6) % 7
        } else {
            col
        };
        frame.add(
            WidgetElement::text(
                10 + weekday as u64,
                RectDip {
                    x: v.x + pad + col as f32 * cell_w,
                    y: grid_y,
                    w: cell_w,
                    h: 24.0,
                },
                *label,
                design::CAPTION,
                600,
                colors.secondary,
            )
            .role("columnheader")
            .align(TextAlign::Center),
        );
    }
    let lead = if config.week_starts_on_sunday {
        first.weekday().num_days_from_sunday() as usize
    } else {
        first.weekday().num_days_from_monday() as usize
    };
    // A fixed six-week grid keeps every month at the same geometry; days from the
    // adjacent months fill the gaps as quiet, non-interactive context.
    let days_y = grid_y + 26.0;
    let row_h = ((v.y + v.h - 10.0 - days_y).max(0.0) / 6.0).min(48.0);
    // Chinese locales add the lunar day, solar term or festival under each date
    // (DeskBox "Glance"); the cell becomes a rounded tile to fit two lines.
    let lunar_cells = chinese && row_h >= 38.0 && cell_w >= 34.0;
    let diameter = (cell_w.min(row_h) - 4.0).clamp(0.0, 40.0);
    for slot in 0..42usize {
        let Some(date) =
            first.checked_add_signed(chrono::Duration::days(slot as i64 - lead as i64))
        else {
            continue;
        };
        let (col, row) = (slot % 7, slot / 7);
        let rect = if lunar_cells {
            RectDip {
                x: v.x + pad + col as f32 * cell_w + 2.0,
                y: days_y + row as f32 * row_h + 2.0,
                w: cell_w - 4.0,
                h: row_h - 4.0,
            }
        } else {
            RectDip {
                x: v.x + pad + col as f32 * cell_w + (cell_w - diameter) / 2.0,
                y: days_y + row as f32 * row_h + (row_h - diameter) / 2.0,
                w: diameter,
                h: diameter,
            }
        };
        let radius = if lunar_cells { 10.0 } else { diameter / 2.0 };
        let name = format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day());
        let in_month = date.year() == year && date.month() == month;
        let is_today = in_month && date == today;
        let is_selected = in_month && selected == Some(date);
        let (foreground, fill) = if !in_month {
            (colors.faint(), None)
        } else if is_today && !colors.high_contrast {
            (colors.panel, Some(colors.accent))
        } else if is_today || is_selected {
            (colors.accent_ink, Some(colors.accent_wash()))
        } else {
            (colors.primary, None)
        };
        let (foreground, fill) = (fade(foreground, opacity), fill.map(|c| fade(c, opacity)));
        let weight = if is_today || is_selected { 600 } else { 400 };
        let id = calendar_day_id(date);
        let number = date.day().to_string();
        let element = if lunar_cells {
            WidgetElement::text(id, rect, "", design::BODY, weight, foreground)
        } else {
            WidgetElement::text(id, rect, &number, design::BODY, weight, foreground)
                .align(TextAlign::Center)
        };
        if in_month {
            frame.add(
                element
                    .role("button")
                    .fill(fill, radius)
                    .action(Some(DAY_BASE + date.day() as u64)),
            );
        } else {
            frame.add(element);
        }
        frame.set_last_accessible_name(name);
        if is_selected
            && colors.high_contrast
            && let Some(node) = frame.nodes.last_mut()
        {
            node.style.stroke = Some(pecofence_plugin_api::StrokeStyle {
                rgba: colors.primary,
                width: 2.0,
            });
        }
        if !lunar_cells {
            continue;
        }
        let split = rect.h * 0.56;
        frame.add(
            WidgetElement::text(
                (2 << 32) | (id & 0xFFFF_FFFF),
                RectDip {
                    y: rect.y + 2.0,
                    h: split - 2.0,
                    ..rect
                },
                number,
                design::BODY,
                weight,
                foreground,
            )
            .parent(id)
            .role("presentation")
            .align(TextAlign::Center),
        );
        if let Some(note) = lunar::day_note(date) {
            let ink = if is_today && !colors.high_contrast {
                fade(colors.panel, 0.85)
            } else if note.highlight && in_month {
                colors.accent_ink
            } else {
                colors.faint()
            };
            frame.add(
                WidgetElement::text(
                    (3 << 32) | (id & 0xFFFF_FFFF),
                    RectDip {
                        y: rect.y + split - 2.0,
                        h: rect.h - split,
                        ..rect
                    },
                    note.text,
                    10.0,
                    400,
                    fade(ink, opacity),
                )
                .parent(id)
                .align(TextAlign::Center),
            );
        }
    }
}

/// Every interactive control stays inside the panel and no two controls share pixels.
#[cfg(test)]
pub(crate) fn assert_controls_fit(scene: &Scene) {
    let viewport = scene.viewport();
    let hits = scene.hit_tree();
    for (i, a) in hits.iter().enumerate() {
        assert!(
            design::control_visible(a.rect, viewport),
            "control {} at {:?} leaves {:?}",
            a.id,
            a.rect,
            viewport
        );
        for b in &hits[i + 1..] {
            let overlap = a.rect.x < b.rect.x + b.rect.w - 0.01
                && b.rect.x < a.rect.x + a.rect.w - 0.01
                && a.rect.y < b.rect.y + b.rect.h - 0.01
                && b.rect.y < a.rect.y + a.rect.h - 0.01;
            assert!(!overlap, "controls {} and {} overlap", a.id, b.id);
        }
    }
}

// Chrono's supported civil dates fit in i32 days; preserve all signed bits.
fn calendar_day_id(date: NaiveDate) -> u64 {
    (1 << 32) | u64::from(date.num_days_from_ce() as u32)
}

fn month_for(today: NaiveDate, offset: i32) -> (i32, u32) {
    let index = today.year() * 12 + today.month0() as i32 + offset;
    (index.div_euclid(12), (index.rem_euclid(12) + 1) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecofence_render::HeadlessCanvas;

    #[test]
    fn contrast_calendar_selection_has_a_border_without_an_extra_hit_target() {
        let input = FrameInput {
            viewport: RectDip {
                x: 0.0,
                y: 0.0,
                w: 420.0,
                h: 330.0,
            },
            locale: "zh-CN",
            presentation: Presentation::Compact,
            grouping: Grouping::Single,
            exposure: Exposure::Desktop,
            active: true,
            text_scale: 1.5,
            theme_epoch: 1,
            device_epoch: 1,
        };
        let mut frame = WidgetFrame::new(
            MountKey {
                instance: InstanceKey {
                    id: 1,
                    activation: 1,
                },
                generation: 1,
            },
            1,
            0,
            input,
        );
        let mut theme = ThemeSnapshot::dark();
        theme.high_contrast = true;
        theme.surface_subtle = [0.0; 4];
        build_calendar(
            &mut frame,
            WidgetConfig::default(),
            0,
            Some(20),
            Colors::from_theme(Some(&theme)),
            true,
            NaiveDate::from_ymd_opt(2024, 2, 15).unwrap(),
            MonthMotion::REST,
        );
        let scene = frame.finish(&mut LayoutTracker::default()).unwrap();
        let selected = scene
            .nodes()
            .iter()
            .find(|node| node.action == Some(DAY_BASE + 20))
            .unwrap();
        assert_eq!(selected.style.stroke.unwrap().rgba, theme.text_primary);
        assert_eq!(selected.style.fill.unwrap()[3], 0.0);
        assert_eq!(
            scene
                .nodes()
                .iter()
                .filter(|node| node.style.stroke.is_some())
                .count(),
            1
        );
        let hit = scene
            .hit_tree()
            .iter()
            .find(|hit| hit.action == DAY_BASE + 20)
            .unwrap();
        let semantic = scene
            .semantics()
            .iter()
            .find(|node| node.id == selected.id)
            .unwrap();
        assert_eq!(hit.rect, selected.rect);
        assert_eq!(semantic.rect, hit.rect);
        assert_eq!(scene.hit_tree().len(), 32);
        let mut canvas = pecofence_render::HeadlessCanvas::default();
        scene.paint(&mut canvas).unwrap();
        assert!(canvas.is_balanced());
        assert!(
            canvas
                .commands()
                .iter()
                .any(|command| matches!(command, pecofence_render::DrawCommand::DrawRect(_, _)))
        );
    }

    #[test]
    fn capsule_clock_uses_the_available_height_instead_of_clipping_a_large_layout() {
        for factor in [1.0, 1.5, 2.0, 2.25] {
            let input = FrameInput {
                viewport: RectDip {
                    x: 0.0,
                    y: 0.0,
                    w: 360.0,
                    h: 48.0,
                },
                locale: "en",
                presentation: Presentation::Capsule,
                grouping: Grouping::Single,
                exposure: Exposure::Desktop,
                active: true,
                text_scale: factor,
                theme_epoch: 1,
                device_epoch: 1,
            };
            let mut frame = WidgetFrame::new(
                MountKey {
                    instance: InstanceKey {
                        id: 1,
                        activation: 1,
                    },
                    generation: 1,
                },
                1,
                0,
                input,
            );
            build_clock(
                &mut frame,
                WidgetConfig::default(),
                Colors::from_theme(None),
                false,
            );
            assert_eq!(frame.nodes.len(), 1);
            assert!(frame.nodes[0].rect.y >= 0.0);
            assert!(frame.nodes[0].rect.y + frame.nodes[0].rect.h <= 48.0);
            assert!(frame.nodes[0].style.size <= 32.0);
        }
    }

    #[test]
    fn widget_text_scaling_reflows_and_keeps_paint_hit_and_semantic_rects_together() {
        let mut tracker = LayoutTracker::default();
        let mut previous_revision = 0;
        for factor in [1.0, 1.25, 1.5, 2.0, 2.25] {
            let input = FrameInput {
                viewport: RectDip {
                    x: 9.0,
                    y: 13.0,
                    w: 600.0,
                    h: 500.0,
                },
                locale: "zh-CN",
                presentation: Presentation::Compact,
                grouping: Grouping::Single,
                exposure: Exposure::Desktop,
                active: true,
                text_scale: factor,
                theme_epoch: 1,
                device_epoch: 1,
            };
            let mut frame = WidgetFrame::new(
                MountKey {
                    instance: InstanceKey {
                        id: 1,
                        activation: 1,
                    },
                    generation: 1,
                },
                1,
                0,
                input,
            );
            build_panel_header(
                &mut frame,
                "便笺",
                "正文",
                &[HeaderAction {
                    id: 2,
                    content: Control::Text("编辑"),
                    kind: design::ButtonKind::Accent,
                    action: Some(1),
                    width: 64.0,
                }],
                Colors::from_theme(None),
            );
            let frame = frame.finish(&mut tracker).unwrap();
            let hit = &frame.hit_tree()[0];
            let semantic = frame
                .semantics()
                .iter()
                .find(|node| node.action == Some(hit.action))
                .unwrap();
            let node = frame
                .nodes()
                .iter()
                .find(|node| node.rect == hit.rect)
                .unwrap();
            assert_eq!(hit.rect, semantic.rect);
            assert_eq!(hit.rect, node.rect);
            assert!(hit.rect.x + hit.rect.w <= input.viewport.x + input.viewport.w);
            assert_eq!(node.style.size, design::BODY * factor);
            assert!(frame.identity().layout_revision > previous_revision);
            previous_revision = frame.identity().layout_revision;
            let mut canvas = pecofence_render::HeadlessCanvas::default();
            frame.paint(&mut canvas).unwrap();
            assert!(canvas.is_balanced());
        }
    }

    #[test]
    fn month_offsets_cross_year_boundaries() {
        assert_eq!(
            month_for(NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(), -1),
            (2025, 12)
        );
        assert_eq!(
            month_for(NaiveDate::from_ymd_opt(2026, 12, 15).unwrap(), 1),
            (2027, 1)
        );
    }

    #[test]
    fn leap_month_paints_dates_and_matching_hit_targets() {
        let mut frame = WidgetFrame {
            identity: FrameIdentity {
                mount_key: MountKey {
                    instance: InstanceKey {
                        id: 1,
                        activation: 1,
                    },
                    generation: 1,
                },
                frame_revision: 1,
                layout_revision: 1,
                theme_epoch: 1,
                device_epoch: 1,
            },
            viewport: RectDip {
                x: 0.0,
                y: 0.0,
                w: 420.0,
                h: 330.0,
            },
            clip: RectDip {
                x: 0.0,
                y: 0.0,
                w: 420.0,
                h: 330.0,
            },
            scale: ContentScale::new(
                RectDip {
                    x: 0.0,
                    y: 0.0,
                    w: 420.0,
                    h: 330.0,
                },
                1.0,
            ),
            nodes: Vec::new(),
        };
        build_calendar(
            &mut frame,
            WidgetConfig::default(),
            0,
            None,
            Colors::from_theme(None),
            true,
            NaiveDate::from_ymd_opt(2024, 2, 15).unwrap(),
            MonthMotion::REST,
        );
        let frame = frame.finish(&mut LayoutTracker::default()).unwrap();
        assert_eq!(frame.hit_tree().len(), 32); // month controls + 29 dates
        assert!(
            frame
                .hit_tree()
                .iter()
                .any(|hit| hit.action == DAY_BASE + 29)
        );
        assert!(
            !frame
                .hit_tree()
                .iter()
                .any(|hit| hit.action == DAY_BASE + 30)
        );
        assert_eq!(
            frame.hit_tree().len(),
            frame
                .semantics()
                .iter()
                .filter(|node| node.action.is_some())
                .count()
        );
        let mut canvas = HeadlessCanvas::default();
        frame.paint(&mut canvas).unwrap();
        assert!(canvas.is_balanced());
    }
    fn identity_frame(width: f32, height: f32, text_scale: f32) -> WidgetFrame {
        WidgetFrame::new(
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
                    w: width,
                    h: height,
                },
                locale: "en",
                presentation: Presentation::Compact,
                grouping: Grouping::Single,
                exposure: Exposure::Desktop,
                active: true,
                text_scale,
                theme_epoch: 1,
                device_epoch: 1,
            },
        )
    }

    fn calendar_scene(offset: i32, sunday: bool, scale: f32) -> Scene {
        let mut frame = identity_frame(420.0, 330.0, scale);
        build_calendar(
            &mut frame,
            WidgetConfig {
                week_starts_on_sunday: sunday,
                ..Default::default()
            },
            offset,
            None,
            Colors::from_theme(None),
            false,
            NaiveDate::from_ymd_opt(2024, 2, 15).unwrap(),
            MonthMotion::REST,
        );
        frame.finish(&mut LayoutTracker::default()).unwrap()
    }

    #[test]
    fn calendar_identity_tracks_civil_dates_and_weekdays_instead_of_slots() {
        let february = calendar_scene(0, false, 1.0);
        let march = calendar_scene(1, false, 1.0);
        let feb_day = february
            .semantics()
            .iter()
            .find(|n| n.name == "2024-02-01")
            .unwrap();
        let march_day = march
            .semantics()
            .iter()
            .find(|n| n.name == "2024-03-01")
            .unwrap();
        assert_ne!(feb_day.id, march_day.id);
        assert_eq!(feb_day.action, march_day.action); // existing day action numbering is unchanged
        let mut interaction = design::ControlInteraction {
            focused: Some(feb_day.id),
            ..Default::default()
        };
        interaction.reconcile(march.hit_tree());
        assert_eq!(
            interaction.focused, None,
            "focus cannot jump to the next month's same day number"
        );
        let sunday = calendar_scene(0, true, 1.5);
        let scaled_day = sunday
            .semantics()
            .iter()
            .find(|n| n.name == "2024-02-01")
            .unwrap();
        assert_eq!(scaled_day.id, feb_day.id);
        for label in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
            let before = february
                .semantics()
                .iter()
                .find(|n| n.name == label)
                .unwrap();
            let after = sunday.semantics().iter().find(|n| n.name == label).unwrap();
            assert_eq!(before.id, after.id);
        }
        for action in [PREVIOUS, NEXT, TODAY] {
            assert_eq!(
                february
                    .hit_tree()
                    .iter()
                    .find(|n| n.action == action)
                    .unwrap()
                    .id,
                march
                    .hit_tree()
                    .iter()
                    .find(|n| n.action == action)
                    .unwrap()
                    .id
            );
        }
        assert_ne!(
            calendar_day_id(NaiveDate::from_ymd_opt(-1, 1, 1).unwrap()),
            calendar_day_id(NaiveDate::from_ymd_opt(1, 1, 1).unwrap())
        );
    }

    #[test]
    fn clock_identity_survives_optional_date_and_layout_changes() {
        let mut scenes = Vec::new();
        for (width, height, scale) in [(250.0, 60.0, 1.0), (500.0, 60.0, 1.0), (500.0, 330.0, 1.5)]
        {
            let mut frame = identity_frame(width, height, scale);
            build_clock(
                &mut frame,
                WidgetConfig::default(),
                Colors::from_theme(None),
                false,
            );
            scenes.push(frame.finish(&mut LayoutTracker::default()).unwrap());
        }
        assert_eq!(scenes[0].semantics().len(), 1);
        assert_eq!(scenes[1].semantics().len(), 2);
        assert_eq!(scenes[2].semantics().len(), 2);
        for scene in &scenes {
            assert_eq!(scene.semantics()[0].id, scenes[0].semantics()[0].id);
        }
        assert_eq!(scenes[1].semantics()[1].id, scenes[2].semantics()[1].id);
    }

    #[test]
    fn calendar_fills_six_weeks_and_marks_today_with_a_solid_accent() {
        let scene = calendar_scene(0, false, 1.0);
        let days: Vec<_> = scene
            .nodes()
            .iter()
            .filter(|node| node.id >> 32 == 1)
            .collect();
        assert_eq!(days.len(), 42);
        assert_eq!(days.iter().filter(|node| node.action.is_some()).count(), 29);
        let theme = ThemeSnapshot::dark();
        let today = days.iter().find(|node| node.name == "2024-02-15").unwrap();
        assert_eq!(today.style.fill, Some(theme.accent));
        assert_eq!(today.style.foreground, theme.surface_panel);
        assert_eq!(today.rect.w, today.rect.h);
        assert_eq!(today.style.align, TextAlign::Center);
        let adjacent = days.iter().find(|node| node.name == "2024-01-29").unwrap();
        assert_eq!(adjacent.action, None);
        assert!(adjacent.style.foreground[3] < theme.text_secondary[3]);
    }

    #[test]
    fn calendar_controls_fit_at_every_text_scale() {
        for scale in [1.0, 1.25, 1.5, 2.0, 2.25] {
            for sunday in [false, true] {
                assert_controls_fit(&calendar_scene(0, sunday, scale));
            }
        }
    }

    #[test]
    fn short_panels_draw_a_capsule_summary_with_lunar_context() {
        let mut frame = identity_frame(360.0, 48.0, 1.0);
        build_calendar(
            &mut frame,
            WidgetConfig::default(),
            0,
            None,
            Colors::from_theme(None),
            true,
            NaiveDate::from_ymd_opt(2024, 9, 17).unwrap(),
            MonthMotion::REST,
        );
        let scene = frame.finish(&mut LayoutTracker::default()).unwrap();
        assert_eq!(scene.nodes().len(), 2);
        assert_eq!(scene.hit_tree()[0].action, TODAY);
        assert_eq!(
            scene.nodes()[1].text,
            "9月17日 周二 · 农历八月十五 · 中秋节"
        );
        assert_controls_fit(&scene);
    }

    #[test]
    fn month_change_fades_the_grid_and_slides_the_title() {
        let rest = calendar_scene(0, false, 1.0);
        let mut frame = identity_frame(420.0, 330.0, 1.0);
        build_calendar(
            &mut frame,
            WidgetConfig::default(),
            0,
            None,
            Colors::from_theme(None),
            false,
            NaiveDate::from_ymd_opt(2024, 2, 15).unwrap(),
            MonthMotion {
                progress: 0.0,
                direction: 1.0,
            },
        );
        let start = frame.finish(&mut LayoutTracker::default()).unwrap();
        let title = |scene: &Scene| scene.nodes().iter().find(|n| n.id == 1).unwrap().clone();
        assert_eq!(title(&start).rect.x, title(&rest).rect.x + 16.0);
        let day = |scene: &Scene| {
            scene
                .nodes()
                .iter()
                .find(|n| n.name == "2024-02-15")
                .unwrap()
                .clone()
        };
        assert!(day(&start).style.fill.unwrap()[3] < day(&rest).style.fill.unwrap()[3]);
        // Hit geometry of the dates does not move while they fade.
        assert_eq!(day(&start).rect, day(&rest).rect);
    }

    #[test]
    fn tweens_are_skipped_when_animation_effects_are_off() {
        assert!(Tween::start(motion::FAST, Curve::Decelerate, false).is_none());
        let tween = Tween::start(motion::FAST, Curve::Decelerate, true).unwrap();
        let end = tween.start + motion::FAST;
        assert_eq!(tween.progress(end), 1.0);
        assert!(tween.done(end));
        assert_eq!(tween.progress(tween.start), 0.0);
    }

    fn contrast(a: [f32; 4], b: [f32; 4]) -> f32 {
        let luminance = |c: [f32; 4]| {
            let channel = |v: f32| {
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(c[0]) + 0.7152 * channel(c[1]) + 0.0722 * channel(c[2])
        };
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn paper_palette_text_meets_wcag_aa_on_every_surface() {
        for surface in [paper::GROUND, paper::CARD] {
            for ink in [paper::INK, paper::INK_SECONDARY, paper::TERRACOTTA_INK] {
                assert!(contrast(ink, surface) >= 4.5, "{ink:?} on {surface:?}");
            }
        }
        assert!(contrast(paper::CREAM, paper::TERRACOTTA) >= 4.5);
        // Accent button text sits on a 14% terracotta wash over the ground.
        let wash = lerp_color(paper::GROUND, paper::TERRACOTTA, 0.14);
        assert!(contrast(paper::TERRACOTTA_INK, wash) >= 4.5);
    }

    #[test]
    fn paper_widgets_draw_their_own_ground_and_serif_titles() {
        let mut frame = identity_frame(420.0, 330.0, 1.0);
        build_calendar(
            &mut frame,
            WidgetConfig::default(),
            0,
            None,
            Colors::paper(None),
            false,
            NaiveDate::from_ymd_opt(2024, 2, 15).unwrap(),
            MonthMotion::REST,
        );
        let scene = frame.finish(&mut LayoutTracker::default()).unwrap();
        let ground = &scene.nodes()[0];
        assert_eq!(ground.id, BACKGROUND);
        assert_eq!(ground.style.fill, Some(paper::GROUND));
        assert_eq!(ground.action, None);
        let title = scene.nodes().iter().find(|n| n.id == 1).unwrap();
        assert_eq!(title.style.font, FontRole::Serif);
        let today = scene
            .nodes()
            .iter()
            .find(|n| n.name == "2024-02-15")
            .unwrap();
        assert_eq!(today.style.fill, Some(paper::TERRACOTTA));
        assert_eq!(today.style.foreground, paper::CREAM);
        assert_controls_fit(&scene);

        let mut contrast_theme = ThemeSnapshot::dark();
        contrast_theme.high_contrast = true;
        let system = Colors::paper(Some(&contrast_theme));
        assert_eq!(system.ground, None);
        assert_eq!(system.primary, contrast_theme.text_primary);
    }
}

//! First-party local-time widgets. State, date math and layout stay independent of Win32;
//! the host supplies the timer, theme and Direct2D canvas.

use chrono::{Datelike, Local, NaiveDate, Timelike};
use pecofence_plugin_api::*;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

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
        theme,
        frame_revision: 0,
        layout_revision: 0,
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
    theme: Option<Arc<ThemeSnapshot>>,
    frame_revision: u64,
    layout_revision: u64,
    stopped: bool,
}

impl TimeWidget {
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
        let colors = Colors::from_theme(self.theme.as_deref());
        let chinese = input.locale.starts_with("zh");
        match self.kind {
            Kind::Clock => build_clock(&mut frame, self.config, colors, chinese),
            Kind::Calendar => build_calendar(
                &mut frame,
                self.config,
                self.month_offset,
                self.selected_day,
                colors,
                chinese,
                Local::now().date_naive(),
            ),
        }
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

#[derive(Clone, Copy)]
pub(crate) struct Colors {
    pub(crate) primary: [f32; 4],
    pub(crate) secondary: [f32; 4],
    pub(crate) accent: [f32; 4],
    pub(crate) subtle: [f32; 4],
}

impl Colors {
    pub(crate) fn from_theme(theme: Option<&ThemeSnapshot>) -> Self {
        if let Some(theme) = theme {
            Self {
                primary: theme.text_primary,
                secondary: theme.text_secondary,
                accent: theme.accent,
                subtle: theme.surface_subtle,
            }
        } else {
            Self {
                primary: [0.95, 0.96, 0.98, 1.0],
                secondary: [0.72, 0.77, 0.84, 1.0],
                accent: [0.55, 0.77, 1.0, 1.0],
                subtle: [0.17, 0.20, 0.25, 1.0],
            }
        }
    }
}

struct Node {
    rect: RectDip,
    text: String,
    size: f32,
    weight: u16,
    foreground: [f32; 4],
    fill: Option<[f32; 4]>,
    radius: f32,
    clip_text: bool,
}

pub(crate) struct WidgetFrame {
    identity: FrameIdentity,
    pub(crate) viewport: RectDip,
    nodes: Vec<Node>,
    hits: Vec<HitNode>,
    semantics: Vec<SemanticNode>,
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
            viewport: input.viewport,
            nodes: Vec::new(),
            hits: Vec::new(),
            semantics: Vec::new(),
        }
    }

    pub(crate) fn add(
        &mut self,
        rect: RectDip,
        text: impl Into<String>,
        size: f32,
        weight: u16,
        foreground: [f32; 4],
        fill: Option<[f32; 4]>,
        radius: f32,
        action: Option<u64>,
        role: &str,
    ) {
        let text = text.into();
        let id = self.nodes.len() as u64 + 1;
        if let Some(action) = action {
            self.hits.push(HitNode { id, rect, action });
        }
        self.semantics.push(SemanticNode {
            id,
            parent: None,
            role: role.into(),
            name: text.clone(),
            rect,
            action,
        });
        self.nodes.push(Node {
            rect,
            text,
            size,
            weight,
            foreground,
            fill,
            radius,
            clip_text: false,
        });
    }
}

impl PreparedFrame for WidgetFrame {
    fn identity(&self) -> FrameIdentity {
        self.identity
    }
    fn hit_tree(&self) -> &[HitNode] {
        &self.hits
    }
    fn semantics(&self) -> &[SemanticNode] {
        &self.semantics
    }
    fn paint(&self, canvas: &mut dyn Canvas) -> Result<()> {
        canvas.push_clip(self.viewport)?;
        let result = (|| {
            for node in &self.nodes {
                if let Some(fill) = node.fill {
                    canvas.fill_rounded_rect(node.rect, node.radius, fill)?;
                }
                if !node.text.is_empty() {
                    if node.clip_text {
                        canvas.push_clip(node.rect)?;
                    }
                    let result = canvas.draw_text(
                        node.rect,
                        &TextSpec {
                            text: node.text.clone(),
                            size_dip: node.size,
                            weight: node.weight,
                            font: FontRole::Text,
                        },
                        node.foreground,
                    );
                    if node.clip_text {
                        result.and(canvas.pop_clip())?;
                    } else {
                        result?;
                    }
                }
            }
            Ok(())
        })();
        result.and(canvas.pop_clip())
    }
}

/// Shared header geometry for local notes and TODO lists.
fn build_panel_header(
    frame: &mut WidgetFrame,
    title: impl Into<String>,
    action_label: &str,
    action: Option<u64>,
    hint: &str,
    colors: Colors,
) {
    let v = frame.viewport;
    let width = (v.w - 28.0).max(0.0);
    frame.add(
        RectDip {
            x: v.x + 14.0,
            y: v.y + 12.0,
            w: (width - 74.0).max(0.0),
            h: 34.0,
        },
        title,
        21.0,
        600,
        colors.primary,
        None,
        0.0,
        None,
        "heading",
    );
    frame.add(
        RectDip {
            x: v.x + v.w - 75.0,
            y: v.y + 10.0,
            w: 60.0,
            h: 34.0,
        },
        action_label,
        14.0,
        600,
        colors.accent,
        Some(colors.subtle),
        6.0,
        action,
        "button",
    );
    frame.add(
        RectDip {
            x: v.x + 14.0,
            y: v.y + 44.0,
            w: width,
            h: 20.0,
        },
        hint,
        11.0,
        400,
        colors.secondary,
        None,
        0.0,
        None,
        "text",
    );
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
    let time_size = if v.w < 260.0 { 37.0 } else { 52.0 };
    let top = v.y + ((v.h - 115.0) / 2.0).max(12.0);
    frame.add(
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
        None,
        0.0,
        None,
        "text",
    );
    frame.add(
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
        None,
        0.0,
        None,
        "text",
    );
}

fn build_calendar(
    frame: &mut WidgetFrame,
    config: WidgetConfig,
    offset: i32,
    selected_day: Option<u32>,
    colors: Colors,
    chinese: bool,
    today: NaiveDate,
) {
    let (year, month) = month_for(today, offset);
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return;
    };
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let Some(next_first) = NaiveDate::from_ymd_opt(next_year, next_month, 1) else {
        return;
    };
    let days = (next_first - first).num_days() as u32;
    let v = frame.viewport;
    let pad = 14.0;
    let width = (v.w - pad * 2.0).max(0.0);
    let cell_w = width / 7.0;
    let row_h = ((v.h - 112.0).max(0.0) / 6.0).min(46.0);
    let header_y = v.y + 12.0;
    frame.add(
        RectDip {
            x: v.x + pad,
            y: header_y,
            w: (width - 130.0).max(0.0),
            h: 36.0,
        },
        if chinese {
            format!("{year}年 {month}月")
        } else {
            format!("{year}-{month:02}")
        },
        22.0,
        600,
        colors.primary,
        None,
        0.0,
        None,
        "heading",
    );
    for (action, x, label) in [
        (PREVIOUS, v.x + v.w - 120.0, "‹"),
        (TODAY, v.x + v.w - 85.0, if chinese { "今" } else { "T" }),
        (NEXT, v.x + v.w - 45.0, "›"),
    ] {
        frame.add(
            RectDip {
                x,
                y: header_y,
                w: 32.0,
                h: 31.0,
            },
            label,
            21.0,
            600,
            colors.primary,
            Some(colors.subtle),
            6.0,
            Some(action),
            "button",
        );
    }
    let labels = if chinese && config.week_starts_on_sunday {
        ["日", "一", "二", "三", "四", "五", "六"]
    } else if chinese {
        ["一", "二", "三", "四", "五", "六", "日"]
    } else if config.week_starts_on_sunday {
        ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
    } else {
        ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
    };
    let grid_y = header_y + 54.0;
    for (col, label) in labels.iter().enumerate() {
        frame.add(
            RectDip {
                x: v.x + pad + col as f32 * cell_w + 5.0,
                y: grid_y,
                w: (cell_w - 5.0).max(0.0),
                h: 25.0,
            },
            *label,
            14.0,
            600,
            colors.secondary,
            None,
            0.0,
            None,
            "columnheader",
        );
    }
    let lead = if config.week_starts_on_sunday {
        first.weekday().num_days_from_sunday() as usize
    } else {
        first.weekday().num_days_from_monday() as usize
    };
    for day in 1..=days {
        let slot = lead + day as usize - 1;
        let col = slot % 7;
        let row = slot / 7;
        let rect = RectDip {
            x: v.x + pad + col as f32 * cell_w,
            y: grid_y + 28.0 + row as f32 * row_h,
            w: (cell_w - 4.0).max(0.0),
            h: (row_h - 3.0).max(0.0),
        };
        let is_today = offset == 0 && day == today.day();
        let selected = selected_day == Some(day);
        frame.add(
            rect,
            day.to_string(),
            16.0,
            if is_today || selected { 700 } else { 400 },
            if is_today {
                colors.accent
            } else {
                colors.primary
            },
            (is_today || selected).then_some(colors.subtle),
            6.0,
            Some(DAY_BASE + day as u64),
            "button",
        );
    }
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
            nodes: Vec::new(),
            hits: Vec::new(),
            semantics: Vec::new(),
        };
        build_calendar(
            &mut frame,
            WidgetConfig::default(),
            0,
            None,
            Colors::from_theme(None),
            true,
            NaiveDate::from_ymd_opt(2024, 2, 15).unwrap(),
        );
        assert_eq!(frame.hits.len(), 32); // month controls + 29 dates
        assert!(frame.hits.iter().any(|hit| hit.action == DAY_BASE + 29));
        assert!(!frame.hits.iter().any(|hit| hit.action == DAY_BASE + 30));
        assert_eq!(
            frame.hits.len(),
            frame
                .semantics
                .iter()
                .filter(|node| node.action.is_some())
                .count()
        );
        let mut canvas = HeadlessCanvas::default();
        frame.paint(&mut canvas).unwrap();
        assert!(canvas.is_balanced());
    }
}

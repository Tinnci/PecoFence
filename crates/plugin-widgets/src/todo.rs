//! Local TODO panel. Tasks live in the provider-owned JSON saved with its fence.

use super::{
    CAPSULE_HEIGHT, CONTENT_TOP, Colors, Control, HeaderAction, WidgetFrame, build_capsule,
    build_panel_header, glyph,
};
use super::{FRAME_INTERVAL, Tween, WidgetElement, fade, lerp};
use pecofence_plugin_api::design;
use pecofence_plugin_api::design::motion::{self, Curve};
use pecofence_plugin_api::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

pub const TODO_PROVIDER: &str = "pecofence.todo";
const ADD: u64 = 1;
const CLEAR_DONE: u64 = 2;
const PREVIOUS_PAGE: u64 = 3;
const NEXT_PAGE: u64 = 4;
const UNDO: u64 = 5;
const TASK_BASE: u64 = 1_000;
const EDIT_BASE: u64 = 2_000;
const MAX_TASKS: usize = 128;
// Upper bit separates task controls from fixed scene controls; two low bits identify row parts.
const MAX_TASK_ID: u64 = (1 << 61) - 1;
const MAX_TITLE_CHARS: usize = 160;
const MAX_CONFIG_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TodoConfig {
    pub next_id: u64,
    pub tasks: Vec<TodoTask>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoTask {
    #[serde(default)]
    pub id: u64,
    pub title: String,
    pub done: bool,
}

impl Default for TodoConfig {
    fn default() -> Self {
        Self {
            next_id: 1,
            tasks: Vec::new(),
        }
    }
}

fn normalize_ids(config: &mut TodoConfig) -> Result<()> {
    let mut used = std::collections::HashSet::new();
    let mut next = config.next_id.max(1);
    for task in &config.tasks {
        if task.id > MAX_TASK_ID || (task.id != 0 && !used.insert(task.id)) {
            return Err(Error::Invalid(
                "invalid or duplicate TODO task identity".into(),
            ));
        }
        next = next.max(task.id + 1);
    }
    for task in &mut config.tasks {
        if task.id == 0 {
            if next > MAX_TASK_ID {
                return Err(Error::Exhausted);
            }
            task.id = next;
            next += 1;
        }
    }
    if next > MAX_TASK_ID + 1 {
        return Err(Error::Exhausted);
    }
    config.next_id = next;
    Ok(())
}

fn task_node(id: u64, part: u64) -> u64 {
    (1 << 63) | (id << 2) | part
}

fn parse(config: &PanelConfig) -> Result<TodoConfig> {
    if config.version != 1 {
        return Err(Error::Invalid("unsupported TODO config version".into()));
    }
    if config.bytes.len() > MAX_CONFIG_BYTES {
        return Err(Error::Invalid(
            "TODO list exceeds configuration size limit".into(),
        ));
    }
    let mut parsed: TodoConfig =
        serde_json::from_slice(&config.bytes).map_err(|e| Error::Invalid(e.to_string()))?;
    if parsed.tasks.len() > MAX_TASKS
        || parsed.tasks.iter().any(|task| {
            task.title.trim().is_empty() || task.title.chars().count() > MAX_TITLE_CHARS
        })
    {
        return Err(Error::Invalid(
            "TODO list exceeds task or title limits".into(),
        ));
    }
    normalize_ids(&mut parsed)?;
    Ok(parsed)
}

pub struct TodoPlugin;

impl PanelProvider for TodoPlugin {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: TODO_PROVIDER,
            api_major: 1,
            config_major: 1,
            required_services: &["theme"],
        }
    }

    fn validate(&self, config: &PanelConfig) -> Result<()> {
        parse(config).map(|_| ())
    }

    fn create(&self, ctx: PluginContext, input: CreatePanel) -> Result<Box<dyn PanelInstance>> {
        Ok(Box::new(TodoWidget {
            theme: ctx.theme.with(|service| service.snapshot()).ok(),
            ctx,
            key: input.key,
            mount: None,
            config: parse(&input.config)?,
            undo: None,
            motion: None,
            page: 0,
            frame_revision: 0,
            layout_tracker: design::LayoutTracker::default(),
            layout_revision: 0,
            stopped: false,
            fields: None,
        }))
    }
}

struct TodoWidget {
    ctx: PluginContext,
    key: InstanceKey,
    mount: Option<MountKey>,
    config: TodoConfig,
    undo: Option<TodoConfig>,
    motion: Option<ListMotion>,
    page: usize,
    theme: Option<Arc<ThemeSnapshot>>,
    frame_revision: u64,
    layout_tracker: design::LayoutTracker,
    layout_revision: u64,
    stopped: bool,
    /// Geometry of the last frame, so text fields open on the row they edit.
    fields: Option<FieldLayout>,
}

/// What a todo text field needs from the last laid-out frame.
#[derive(Clone, Copy)]
struct FieldLayout {
    scale: design::ContentScale,
    viewport: RectDip,
    colors: Colors,
    chinese: bool,
    page: usize,
    capacity: usize,
}

/// Field a todo editor opens: one task's row, or a new row for an added task.
#[derive(Clone, Copy)]
enum Field {
    Edit(usize),
    Add,
}

/// A task row's card in logical DIPs (`build_todo` lays rows out the same way).
fn row_rect(v: RectDip, slot: usize) -> RectDip {
    RectDip {
        x: v.x + design::CONTENT_INSET,
        y: v.y + LIST_TOP + slot as f32 * ROW_PITCH,
        w: (v.w - design::CONTENT_INSET * 2.0).max(1.0),
        h: ROW_HEIGHT,
    }
}

/// Editing happens in place: the field covers the task's row card with the text where
/// the title is drawn; a new task gets the next empty row on the page (or the first
/// row when the page is full) with a placeholder.
fn field_style(config: &TodoConfig, layout: FieldLayout, field: Field) -> Option<TextInputStyle> {
    let v = layout.viewport;
    let colors = layout.colors;
    let hint = if layout.chinese {
        "新任务…"
    } else {
        "New task…"
    };
    if v.h < CAPSULE_HEIGHT {
        let pill = RectDip {
            x: v.x + 8.0,
            y: v.y + 6.0,
            w: (v.w - 16.0).max(1.0),
            h: (v.h - 12.0).max(1.0),
        };
        return Some(super::input_field(
            layout.scale,
            pill,
            [12.0, 0.0, 12.0, 2.0],
            colors,
            colors.subtle,
            TextInputFont::Body,
            design::CARD_RADIUS,
            matches!(field, Field::Add).then_some(hint),
        ));
    }
    let start = layout.page.saturating_mul(layout.capacity);
    let shown = config
        .tasks
        .len()
        .saturating_sub(start)
        .min(layout.capacity);
    // A light text box on the page surface reads as "being typed into" on any row.
    let page = if colors.high_contrast {
        colors.subtle
    } else {
        super::paper::SHAPE
    };
    match field {
        // Editing keeps the row and its checkbox: the box starts just after the
        // checkbox and its text lands exactly on the title.
        Field::Edit(index) => {
            let position = display_order(config)
                .iter()
                .position(|(i, _)| *i == index)?;
            let slot = position
                .checked_sub(start)
                .filter(|s| *s < layout.capacity)?;
            let row = row_rect(v, slot);
            Some(super::input_field(
                layout.scale,
                RectDip {
                    x: row.x + 36.0,
                    y: row.y + 4.0,
                    w: (row.w - 40.0).max(1.0),
                    h: (row.h - 8.0).max(1.0),
                },
                [4.0, 0.0, 6.0, 2.0],
                colors,
                page,
                TextInputFont::Body,
                design::CONTROL_RADIUS,
                None,
            ))
        }
        // A new task is a new, blank row: the next empty one on the page, or the
        // first row when the page is full.
        Field::Add => Some(super::input_field(
            layout.scale,
            row_rect(v, if shown < layout.capacity { shown } else { 0 }),
            [40.0, 0.0, 8.0, 2.0],
            colors,
            page,
            TextInputFont::Body,
            design::CARD_RADIUS,
            Some(hint),
        )),
    }
}

impl TodoWidget {
    fn field_style(&self, field: Field) -> Option<TextInputStyle> {
        field_style(&self.config, self.fields?, field)
    }

    fn redraw(&mut self) -> PanelUpdate {
        self.layout_revision = self.layout_revision.saturating_add(1);
        PanelUpdate {
            commands: vec![HostCommand::Invalidate],
            relayout: false,
        }
    }

    fn change(&mut self, next: TodoConfig) -> Result<PanelUpdate> {
        self.change_with(next, Emphasis::None)
    }

    /// Apply a list change and, unless animation effects are off, let the rows glide
    /// from where they were (FLIP) while the progress bar eases to its new value.
    fn change_with(&mut self, next: TodoConfig, emphasis: Emphasis) -> Result<PanelUpdate> {
        let from_slots = display_order(&self.config)
            .iter()
            .enumerate()
            .map(|(slot, (_, task))| (task.id, slot as f32))
            .collect();
        let from_ratio = done_ratio(&self.config);
        let Some(bytes) = apply_change(&mut self.config, &mut self.undo, next)? else {
            return Ok(PanelUpdate::default());
        };
        let animations = self.theme.as_ref().is_none_or(|theme| theme.animations);
        self.motion =
            Tween::start(motion::NORMAL, Curve::PointToPoint, animations).map(|tween| ListMotion {
                tween,
                from_slots,
                from_ratio,
                emphasis,
            });
        let mut update = self.redraw();
        update.commands.push(HostCommand::PersistConfig(bytes));
        Ok(update)
    }
}

impl PanelInstance for TodoWidget {
    fn initial_presentation(&self) -> Presentation {
        Presentation::Compact
    }

    fn mount(&mut self, ctx: MountContext) -> Result<()> {
        if ctx.key.instance != self.key {
            return Err(Error::Invalid(
                "mount belongs to another TODO instance".into(),
            ));
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
            PanelEvent::Invoke { action: ADD, .. } if self.config.tasks.len() < MAX_TASKS => {
                Ok(PanelUpdate {
                    commands: vec![HostCommand::RequestTextInput {
                        action: ADD,
                        initial: String::new(),
                        multiline: false,
                        style: self.field_style(Field::Add),
                    }],
                    relayout: false,
                })
            }
            PanelEvent::Invoke { action: UNDO, .. } => {
                if let Some(previous) = self.undo.clone() {
                    let update = self.change(previous)?;
                    self.undo = None;
                    Ok(update)
                } else {
                    Ok(PanelUpdate::default())
                }
            }
            PanelEvent::Invoke {
                action: CLEAR_DONE, ..
            } => {
                let mut next = self.config.clone();
                next.tasks.retain(|task| !task.done);
                self.change(next)
            }
            PanelEvent::Invoke {
                action: PREVIOUS_PAGE,
                ..
            } if self.page > 0 => {
                self.page -= 1;
                Ok(self.redraw())
            }
            PanelEvent::Invoke {
                action: NEXT_PAGE, ..
            } => {
                self.page = self.page.saturating_add(1);
                Ok(self.redraw())
            }
            PanelEvent::Invoke { action, .. }
                if (TASK_BASE..TASK_BASE + MAX_TASKS as u64).contains(&action) =>
            {
                let mut next = self.config.clone();
                let mut emphasis = Emphasis::None;
                if let Some(task) = next.tasks.get_mut((action - TASK_BASE) as usize) {
                    task.done = !task.done;
                    emphasis = Emphasis::Toggled(task.id);
                }
                self.change_with(next, emphasis)
            }
            PanelEvent::Invoke { action, .. }
                if (EDIT_BASE..EDIT_BASE + MAX_TASKS as u64).contains(&action) =>
            {
                if let Some(task) = self.config.tasks.get((action - EDIT_BASE) as usize) {
                    Ok(PanelUpdate {
                        commands: vec![HostCommand::RequestTextInput {
                            action,
                            initial: task.title.clone(),
                            multiline: false,
                            style: self.field_style(Field::Edit((action - EDIT_BASE) as usize)),
                        }],
                        relayout: false,
                    })
                } else {
                    Ok(PanelUpdate::default())
                }
            }
            PanelEvent::TextSubmitted { action, text }
                if action == ADD || (EDIT_BASE..EDIT_BASE + MAX_TASKS as u64).contains(&action) =>
            {
                let title = text.trim();
                if title.is_empty() {
                    return Ok(PanelUpdate::default());
                }
                if title.chars().count() > MAX_TITLE_CHARS {
                    return Err(Error::Invalid(
                        "TODO title must contain at most 160 characters".into(),
                    ));
                }
                let mut next = self.config.clone();
                let mut emphasis = Emphasis::None;
                if action == ADD {
                    if next.tasks.len() >= MAX_TASKS {
                        return Err(Error::Invalid(
                            "TODO list is full (128 tasks); clear completed tasks first".into(),
                        ));
                    }
                    if next.next_id > MAX_TASK_ID {
                        return Err(Error::Exhausted);
                    }
                    let id = next.next_id;
                    emphasis = Emphasis::Added(id);
                    next.next_id += 1;
                    next.tasks.push(TodoTask {
                        id,
                        title: title.into(),
                        done: false,
                    });
                } else if let Some(task) = next.tasks.get_mut((action - EDIT_BASE) as usize) {
                    task.title = title.into();
                }
                let changed = next != self.config;
                let update = self.change_with(next, emphasis)?;
                if action == ADD && changed {
                    self.page = usize::MAX; // clamped to the last page by prepare_frame
                }
                Ok(update)
            }
            _ => Ok(PanelUpdate::default()),
        }
    }

    fn prepare_frame(&mut self, input: FrameInput) -> Result<Rc<dyn PreparedFrame>> {
        let mount = self.mount.ok_or(Error::Closed)?;
        self.frame_revision = self.frame_revision.checked_add(1).ok_or(Error::Exhausted)?;
        let capacity = page_capacity(
            design::ContentScale::new(input.viewport, input.text_scale)
                .logical_viewport()
                .h,
        );
        let pages = self.config.tasks.len().max(1).div_ceil(capacity);
        self.page = self.page.min(pages - 1);
        let now = Instant::now();
        if self.motion.as_ref().is_some_and(|m| m.tween.done(now)) {
            self.motion = None;
        }
        let mut frame = WidgetFrame::new(mount, self.frame_revision, self.layout_revision, input);
        let colors = Colors::paper(self.theme.as_deref());
        let chinese = input.locale.starts_with("zh");
        self.fields = Some(FieldLayout {
            scale: frame.scale,
            viewport: frame.viewport,
            colors,
            chinese,
            page: self.page,
            capacity,
        });
        build_todo(
            &mut frame,
            &self.config,
            self.page,
            capacity,
            self.undo.is_some(),
            colors,
            chinese,
            self.motion.as_ref().map(|m| (m, m.tween.progress(now))),
        );
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

    fn next_wake_after(&self) -> Option<std::time::Duration> {
        (!self.stopped && self.motion.is_some()).then_some(FRAME_INTERVAL)
    }
}

/// Which row a list change is about, for its accent motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Emphasis {
    None,
    /// Checkbox pops and the row card fades in or out.
    Toggled(u64),
    /// New row rises into place while fading in.
    Added(u64),
}

struct ListMotion {
    tween: Tween,
    /// Display slot of every task before the change, by task id.
    from_slots: HashMap<u64, f32>,
    from_ratio: f32,
    emphasis: Emphasis,
}

fn done_ratio(config: &TodoConfig) -> f32 {
    let total = config.tasks.len();
    if total == 0 {
        return 0.0;
    }
    config.tasks.iter().filter(|task| task.done).count() as f32 / total as f32
}

// Serialize and validate before changing either the live list or its undo snapshot.
// The host remains responsible for acknowledging durable configuration saves.
fn apply_change(
    config: &mut TodoConfig,
    undo: &mut Option<TodoConfig>,
    mut next: TodoConfig,
) -> Result<Option<Arc<[u8]>>> {
    // Undo restores tasks but must not rewind the identity allocator.
    next.next_id = next.next_id.max(config.next_id);
    normalize_ids(&mut next)?;
    if next == *config {
        return Ok(None);
    }
    let bytes: Arc<[u8]> = serde_json::to_vec(&next)
        .map_err(|e| Error::Backend(e.to_string()))?
        .into();
    parse(&PanelConfig {
        version: 1,
        bytes: bytes.clone(),
    })?;
    *undo = Some(std::mem::replace(config, next));
    Ok(Some(bytes))
}

const LIST_TOP: f32 = 78.0;
const ROW_HEIGHT: f32 = 40.0;
const ROW_PITCH: f32 = 44.0;
const FOOTER_HEIGHT: f32 = 28.0;
const FOOTER_BOTTOM: f32 = 12.0;

fn footer_y(height: f32) -> f32 {
    height - FOOTER_BOTTOM - FOOTER_HEIGHT
}

fn page_capacity(height: f32) -> usize {
    let list = footer_y(height) - 6.0 - LIST_TOP;
    ((list + ROW_PITCH - ROW_HEIGHT) / ROW_PITCH)
        .floor()
        .max(1.0) as usize
}

/// Display order: pending tasks keep their order, completed ones sink below them.
/// Actions still address the stored index, so reordering never changes commands.
fn display_order(config: &TodoConfig) -> Vec<(usize, &TodoTask)> {
    let mut order: Vec<_> = config.tasks.iter().enumerate().collect();
    order.sort_by_key(|(_, task)| task.done);
    order
}

#[allow(clippy::too_many_arguments)]
fn build_todo(
    frame: &mut WidgetFrame,
    config: &TodoConfig,
    page: usize,
    capacity: usize,
    can_undo: bool,
    colors: Colors,
    chinese: bool,
    motion: Option<(&ListMotion, f32)>,
) {
    frame.ground(colors);
    let total = config.tasks.len();
    let done = config.tasks.iter().filter(|task| task.done).count();
    let pending = total - done;
    let full = total >= MAX_TASKS;
    if frame.viewport.h < CAPSULE_HEIGHT {
        // The capsule names the next open task; clicking it adds a new one.
        let next = config.tasks.iter().find(|task| !task.done);
        let summary = match (next, chinese) {
            (Some(task), true) => format!("待办 {pending} · {}", task.title),
            (Some(task), false) => format!("{pending} to do · {}", task.title),
            (None, true) if total > 0 => "待办已全部完成".to_string(),
            (None, false) if total > 0 => "All tasks done".to_string(),
            (None, true) => "暂无任务".to_string(),
            (None, false) => "Nothing to do".to_string(),
        };
        build_capsule(
            frame,
            summary,
            (!full).then_some(ADD),
            Some(colors.subtle),
            colors,
        );
        return;
    }
    // Narrow panels (small fences or large text scales) collapse labels to icons.
    let narrow = frame.viewport.w - design::CONTENT_INSET * 2.0 < 300.0;
    let meta = match (chinese, total, pending) {
        (true, 0, _) => "没有任务".to_string(),
        (false, 0, _) => "No tasks".to_string(),
        (true, _, 0) => format!("{total} 项已全部完成"),
        (false, _, 0) => format!("All {total} done"),
        (true, _, _) => format!("{pending} 项待办 · {done} 项已完成"),
        (false, _, _) => format!("{pending} to do · {done} done"),
    };
    build_panel_header(
        frame,
        if chinese { "待办" } else { "To do" },
        meta,
        &[HeaderAction {
            id: 2,
            content: match (full, chinese, narrow) {
                (true, true, _) => Control::Text("已满"),
                (true, false, _) => Control::Text("Full"),
                (false, true, true) => Control::Icon(glyph::ADD, "添加"),
                (false, false, true) => Control::Icon(glyph::ADD, "Add"),
                (false, true, false) => Control::Text("＋ 添加"),
                (false, false, false) => Control::Text("+ Add"),
            },
            kind: design::ButtonKind::Accent,
            action: (!full).then_some(ADD),
            width: if narrow && !full {
                design::CONTROL_HEIGHT
            } else {
                76.0
            },
        }],
        colors,
    );
    let v = frame.viewport;
    let width = (v.w - design::CONTENT_INSET * 2.0).max(0.0);
    let left = v.x + design::CONTENT_INSET;
    if total > 0 {
        let track = RectDip {
            x: left,
            y: v.y + CONTENT_TOP,
            w: width,
            h: 4.0,
        };
        frame.add(
            WidgetElement::text(10, track, "", design::CAPTION, 400, colors.secondary)
                .role("progressbar")
                .fill(Some(colors.subtle), 2.0),
        );
        frame.set_last_accessible_name(if chinese {
            format!("已完成 {done}/{total}")
        } else {
            format!("{done} of {total} done")
        });
        let ratio = done as f32 / total as f32;
        let ratio = motion.map_or(ratio, |(m, t)| lerp(m.from_ratio, ratio, t));
        if ratio > 0.0 {
            frame.add(
                WidgetElement::text(
                    11,
                    RectDip {
                        w: track.w * ratio,
                        ..track
                    },
                    "",
                    design::CAPTION,
                    400,
                    colors.accent,
                )
                .parent(10)
                .role("presentation")
                .fill(Some(colors.accent), 2.0),
            );
        }
    } else {
        let list_bottom = v.y + footer_y(v.h) - 6.0;
        let middle = (v.y + LIST_TOP + list_bottom) / 2.0;
        frame.add(
            WidgetElement::text(
                12,
                RectDip {
                    x: left,
                    y: middle - 52.0,
                    w: width,
                    h: 36.0,
                },
                glyph::CHECKLIST,
                28.0,
                400,
                colors.faint(),
            )
            .role("presentation")
            .icon()
            .align(TextAlign::Center),
        );
        frame.set_last_accessible_name(String::new());
        frame.add(
            WidgetElement::text(
                9,
                RectDip {
                    x: left,
                    y: middle - 12.0,
                    w: width,
                    h: 24.0,
                },
                if chinese {
                    "暂无任务"
                } else {
                    "Nothing to do"
                },
                design::SUBTITLE,
                600,
                colors.secondary,
            )
            .align(TextAlign::Center),
        );
        frame.add(
            WidgetElement::text(
                13,
                RectDip {
                    x: left,
                    y: middle + 14.0,
                    w: width,
                    h: 20.0,
                },
                if chinese {
                    "点击“添加”创建第一项"
                } else {
                    "Use Add to create your first task"
                },
                design::CAPTION,
                400,
                colors.secondary,
            )
            .align(TextAlign::Center),
        );
    }
    let start = page.saturating_mul(capacity);
    for (slot, (index, task)) in display_order(config)
        .into_iter()
        .skip(start)
        .take(capacity)
        .enumerate()
    {
        // FLIP: rows start from their previous slot and glide to the new one;
        // a new row rises 8 DIP while fading in.
        let to = slot as f32;
        let (from, opacity, rise) = match motion {
            Some((m, t)) if m.emphasis == Emphasis::Added(task.id) => (to, t, 8.0 * (1.0 - t)),
            Some((m, t)) => {
                let from = m
                    .from_slots
                    .get(&task.id)
                    .map(|from| from - start as f32)
                    .filter(|from| (-1.0..=capacity as f32).contains(from));
                match from {
                    Some(from) => (lerp(from, to, t), 1.0, 0.0),
                    None => (to, t, 0.0),
                }
            }
            None => (to, 1.0, 0.0),
        };
        let row = RectDip {
            x: left,
            y: v.y + LIST_TOP + from * ROW_PITCH + rise,
            w: width,
            h: ROW_HEIGHT,
        };
        let ink = fade(
            if task.done {
                colors.secondary
            } else {
                colors.primary
            },
            opacity,
        );
        // Completed rows drop their card so pending work stays visually dominant;
        // a toggled row's card fades rather than vanishing.
        let toggle = motion
            .filter(|(m, _)| m.emphasis == Emphasis::Toggled(task.id))
            .map(|(_, t)| t);
        let card = match (task.done, toggle) {
            (true, Some(t)) => 1.0 - t,
            (true, None) => 0.0,
            (false, Some(t)) => t,
            (false, None) => 1.0,
        } * opacity;
        frame.add(
            WidgetElement::text(task_node(task.id, 0), row, "", design::BODY, 400, ink)
                .role("group")
                .fill(
                    (card > 0.0).then(|| fade(colors.subtle, card)),
                    design::CARD_RADIUS,
                ),
        );
        frame.add(
            WidgetElement::text(
                task_node(task.id, 1),
                RectDip { w: 40.0, ..row },
                if task.done {
                    glyph::CHECKBOX_CHECKED
                } else {
                    glyph::CHECKBOX
                },
                // The checkbox pops a quarter larger at the middle of its toggle.
                18.0 * (1.0 + 0.25 * (std::f32::consts::PI * toggle.unwrap_or(1.0)).sin()),
                400,
                if task.done {
                    fade(colors.accent, opacity)
                } else {
                    ink
                },
            )
            .parent(task_node(task.id, 0))
            .role("checkbox")
            .action(Some(TASK_BASE + index as u64))
            .icon()
            .align(TextAlign::Center),
        );
        frame.set_last_accessible_name(if chinese {
            format!(
                "{}：{}",
                if task.done {
                    "已完成，点击恢复"
                } else {
                    "未完成，点击完成"
                },
                task.title
            )
        } else {
            format!(
                "{}: {}",
                if task.done {
                    "Completed, mark pending"
                } else {
                    "Pending, mark complete"
                },
                task.title
            )
        });
        // Keep the complete title in the text/semantic node. The renderer clips
        // to this rectangle; opening the editor reveals the complete text.
        frame.add(
            WidgetElement::text(
                task_node(task.id, 2),
                RectDip {
                    x: row.x + 40.0,
                    w: (row.w - 48.0).max(0.0),
                    ..row
                },
                &task.title,
                design::BODY,
                400,
                ink,
            )
            .parent(task_node(task.id, 0))
            .role("button")
            .action(Some(EDIT_BASE + index as u64))
            .align(TextAlign::MiddleLeading),
        );
    }
    let footer = RectDip {
        x: left,
        y: v.y + footer_y(v.h),
        w: 0.0,
        h: FOOTER_HEIGHT,
    };
    let mut x = left;
    if done > 0 {
        let label = match (chinese, narrow) {
            (true, false) => format!("清除已完成 · {done}"),
            (false, false) => format!("Clear done · {done}"),
            (true, true) => format!("清除 {done} 项已完成"),
            (false, true) => format!("Clear {done} done"),
        };
        let w = if narrow { FOOTER_HEIGHT } else { 128.0 };
        frame.button(
            4,
            RectDip { x, w, ..footer },
            if narrow {
                Control::Icon(glyph::BROOM, &label)
            } else {
                Control::Text(&label)
            },
            design::ButtonKind::Subtle,
            Some(CLEAR_DONE),
            colors,
        );
        x += w + 6.0;
    }
    if can_undo {
        frame.button(
            5,
            RectDip {
                x,
                w: FOOTER_HEIGHT,
                ..footer
            },
            Control::Icon(
                glyph::UNDO,
                if chinese {
                    "撤销上一步"
                } else {
                    "Undo last"
                },
            ),
            design::ButtonKind::Flat,
            Some(UNDO),
            colors,
        );
        x += FOOTER_HEIGHT + 6.0;
    }
    let pages = total.max(1).div_ceil(capacity);
    if pages > 1 {
        let right = left + width;
        let label_w = (right - x - FOOTER_HEIGHT * 2.0).clamp(0.0, 48.0);
        let label_w = if label_w < 24.0 { 0.0 } else { label_w };
        frame.button(
            6,
            RectDip {
                x: right - FOOTER_HEIGHT * 2.0 - label_w,
                w: FOOTER_HEIGHT,
                ..footer
            },
            Control::Icon(
                glyph::CHEVRON_LEFT,
                if chinese {
                    "上一页"
                } else {
                    "Previous page"
                },
            ),
            design::ButtonKind::Flat,
            (page > 0).then_some(PREVIOUS_PAGE),
            colors,
        );
        if label_w > 0.0 {
            frame.add(
                WidgetElement::text(
                    7,
                    RectDip {
                        x: right - FOOTER_HEIGHT - label_w,
                        w: label_w,
                        ..footer
                    },
                    format!("{}/{}", page + 1, pages),
                    design::CAPTION,
                    400,
                    colors.secondary,
                )
                .align(TextAlign::Center),
            );
        }
        frame.button(
            8,
            RectDip {
                x: right - FOOTER_HEIGHT,
                w: FOOTER_HEIGHT,
                ..footer
            },
            Control::Icon(
                glyph::CHEVRON_RIGHT,
                if chinese { "下一页" } else { "Next page" },
            ),
            design::ButtonKind::Flat,
            (page + 1 < pages).then_some(NEXT_PAGE),
            colors,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecofence_render::HeadlessCanvas;

    #[test]
    fn todo_config_round_trips_and_rejects_empty_titles() {
        let config = TodoConfig {
            next_id: 2,
            tasks: vec![TodoTask {
                id: 1,
                title: "Buy milk".into(),
                done: false,
            }],
        };
        let bytes: Arc<[u8]> = serde_json::to_vec(&config).unwrap().into();
        assert_eq!(parse(&PanelConfig { version: 1, bytes }).unwrap(), config);
        let invalid: Arc<[u8]> = br#"{"tasks":[{"title":" ","done":false}]}"#.as_slice().into();
        assert!(
            parse(&PanelConfig {
                version: 1,
                bytes: invalid
            })
            .is_err()
        );
        let oversized = TodoConfig {
            next_id: 2,
            tasks: (0..MAX_TASKS)
                .map(|_| TodoTask {
                    id: 1,
                    title: "📝".repeat(MAX_TITLE_CHARS),
                    done: false,
                })
                .collect(),
        };
        let bytes: Arc<[u8]> = serde_json::to_vec(&oversized).unwrap().into();
        assert!(bytes.len() > MAX_CONFIG_BYTES);
        assert!(parse(&PanelConfig { version: 1, bytes }).is_err());
    }

    #[test]
    fn rejected_changes_preserve_tasks_and_previous_undo() {
        let original = TodoConfig {
            next_id: 2,
            tasks: vec![TodoTask {
                id: 1,
                title: "Original".into(),
                done: true,
            }],
        };
        let mut config = original.clone();
        let previous = TodoConfig::default();
        let mut undo = Some(previous.clone());
        let mut invalid = original.clone();
        invalid.tasks[0].title = "x".repeat(MAX_TITLE_CHARS + 1);
        assert!(apply_change(&mut config, &mut undo, invalid).is_err());
        assert_eq!(config, original);
        assert_eq!(undo, Some(previous.clone()));
        let unchanged = config.clone();
        assert!(
            apply_change(&mut config, &mut undo, unchanged)
                .unwrap()
                .is_none()
        );
        assert_eq!(undo, Some(previous));
    }

    #[test]
    fn edited_and_cleared_tasks_can_be_restored_without_losing_completion() {
        let original = TodoConfig {
            next_id: 2,
            tasks: vec![TodoTask {
                id: 1,
                title: "原始标题".into(),
                done: true,
            }],
        };
        let mut config = original.clone();
        let mut undo = None;
        let mut edited = original.clone();
        edited.tasks[0].title = "修正后的标题".into();
        let saved = apply_change(&mut config, &mut undo, edited.clone())
            .unwrap()
            .unwrap();
        assert_eq!(
            parse(&PanelConfig {
                version: 1,
                bytes: saved
            })
            .unwrap(),
            edited
        );
        assert_eq!(undo, Some(original));
        apply_change(&mut config, &mut undo, TodoConfig::default()).unwrap();
        assert!(config.tasks.is_empty());
        let restore = undo.clone().unwrap();
        let saved = apply_change(&mut config, &mut undo, restore)
            .unwrap()
            .unwrap();
        assert_eq!(
            parse(&PanelConfig {
                version: 1,
                bytes: saved
            })
            .unwrap(),
            edited
        );
    }

    #[test]
    fn todo_frame_paginates_visible_rows() {
        let input = FrameInput {
            viewport: RectDip {
                x: 0.0,
                y: 0.0,
                w: 420.0,
                h: 330.0,
            },
            locale: "en",
            presentation: Presentation::Compact,
            grouping: Grouping::Single,
            exposure: Exposure::Desktop,
            active: true,
            text_scale: 1.0,
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
            1,
            input,
        );
        let config = TodoConfig {
            next_id: 21,
            tasks: (0..20)
                .map(|n| TodoTask {
                    id: n + 1,
                    title: format!("Task {n}"),
                    done: false,
                })
                .collect(),
        };
        let capacity = page_capacity(
            design::ContentScale::new(input.viewport, input.text_scale)
                .logical_viewport()
                .h,
        );
        build_todo(
            &mut frame,
            &config,
            1,
            capacity,
            true,
            Colors::from_theme(None),
            false,
            None,
        );
        let frame = frame.finish(&mut design::LayoutTracker::default()).unwrap();
        assert_eq!(
            frame
                .hit_tree()
                .iter()
                .filter(|n| (TASK_BASE..EDIT_BASE).contains(&n.action))
                .count(),
            capacity
        );
        assert!(frame.hit_tree().iter().any(|node| node.action == UNDO));
        for slot in 0..capacity {
            let index = capacity + slot;
            let toggle = frame
                .hit_tree()
                .iter()
                .find(|n| n.action == TASK_BASE + index as u64)
                .unwrap();
            let edit = frame
                .hit_tree()
                .iter()
                .find(|n| n.action == EDIT_BASE + index as u64)
                .unwrap();
            assert!(toggle.rect.x + toggle.rect.w <= edit.rect.x);
            assert!(!toggle.rect.contains(edit.rect.x + 1.0, edit.rect.y + 1.0));
            assert_eq!(
                frame
                    .semantics()
                    .iter()
                    .find(|n| n.action == Some(EDIT_BASE + index as u64))
                    .unwrap()
                    .name,
                config.tasks[index].title
            );
        }
        let mut canvas = HeadlessCanvas::default();
        frame.paint(&mut canvas).unwrap();
        assert!(canvas.is_balanced());
    }
    fn read_config(json: &str) -> Result<TodoConfig> {
        parse(&PanelConfig {
            version: 1,
            bytes: json.as_bytes().to_vec().into(),
        })
    }

    #[test]
    fn legacy_tasks_receive_unique_persistable_ids_without_changing_titles() {
        let config = read_config(
            r#"{"tasks":[{"title":"同名","done":false},{"title":"同名","done":true}]}"#,
        )
        .unwrap();
        assert_eq!(
            config.tasks.iter().map(|task| task.id).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(config.next_id, 3);
        assert_eq!(
            read_config(&serde_json::to_string(&config).unwrap()).unwrap(),
            config
        );
        assert!(
            read_config(
                r#"{"tasks":[{"id":1,"title":"A","done":false},{"id":1,"title":"B","done":false}]}"#
            )
            .is_err()
        );
        assert!(
            read_config(&format!(
                r#"{{"tasks":[{{"id":{},"title":"A","done":false}}]}}"#,
                MAX_TASK_ID + 1
            ))
            .is_err()
        );
    }

    #[test]
    fn clear_undo_and_restart_do_not_reuse_task_identity() {
        let mut config =
            read_config(r#"{"tasks":[{"title":"A","done":true},{"title":"B","done":false}]}"#)
                .unwrap();
        let mut undo = None;
        apply_change(&mut config, &mut undo, TodoConfig::default()).unwrap();
        assert_eq!(config.next_id, 3);
        let mut added = config.clone();
        added.tasks.push(TodoTask {
            id: 0,
            title: "C".into(),
            done: false,
        });
        apply_change(&mut config, &mut undo, added).unwrap();
        assert_eq!(config.tasks[0].id, 3);
        let restore = undo.clone().unwrap();
        apply_change(&mut config, &mut undo, restore).unwrap();
        assert!(config.tasks.is_empty());
        assert_eq!(config.next_id, 4);
        let mut restarted = read_config(&serde_json::to_string(&config).unwrap()).unwrap();
        let mut added = restarted.clone();
        added.tasks.push(TodoTask {
            id: 0,
            title: "D".into(),
            done: false,
        });
        apply_change(&mut restarted, &mut None, added).unwrap();
        assert_eq!(restarted.tasks[0].id, 4);
    }

    fn scene_for(
        config: &TodoConfig,
        page: usize,
        capacity: usize,
        undo: bool,
    ) -> pecofence_plugin_api::scene::Scene {
        let input = FrameInput {
            viewport: RectDip {
                x: 0.0,
                y: 0.0,
                w: 420.0,
                h: 330.0,
            },
            locale: "en",
            presentation: Presentation::Compact,
            grouping: Grouping::Single,
            exposure: Exposure::Desktop,
            active: true,
            text_scale: 1.0,
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
            1,
            input,
        );
        build_todo(
            &mut frame,
            config,
            page,
            capacity,
            undo,
            Colors::from_theme(None),
            false,
            None,
        );
        frame.finish(&mut design::LayoutTracker::default()).unwrap()
    }

    #[test]
    fn task_controls_keep_identity_and_parent_across_clear_paging_and_footer_changes() {
        let mut config = read_config(r#"{"tasks":[{"title":"Remove","done":true},{"title":"Keep","done":false},{"title":"Later","done":false}]}"#).unwrap();
        let before = scene_for(&config, 0, 5, false);
        let original = before
            .semantics()
            .iter()
            .find(|n| n.name == "Keep")
            .unwrap();
        let original_id = original.id;
        let original_parent = original.parent;
        let original_action = original.action;
        config.tasks.retain(|task| !task.done);
        let after = scene_for(&config, 0, 5, true);
        let kept = after.semantics().iter().find(|n| n.name == "Keep").unwrap();
        assert_eq!(kept.id, original_id);
        assert_eq!(kept.parent, original_parent);
        assert_ne!(kept.action, original_action); // the existing layout revision still guards index-based commands
        let toggle = after
            .semantics()
            .iter()
            .find(|n| n.role == "checkbox" && n.name.ends_with("Keep"))
            .unwrap();
        assert_eq!(toggle.parent, original_parent);
        assert_ne!(toggle.id, kept.id);
        let page = scene_for(&config, 1, 1, true);
        let later_paged = page.semantics().iter().find(|n| n.name == "Later").unwrap();
        let later_all = after
            .semantics()
            .iter()
            .find(|n| n.name == "Later")
            .unwrap();
        assert_eq!(later_paged.id, later_all.id);
        assert_eq!(later_paged.parent, later_all.parent);
        assert!(!after.semantics().iter().any(|n| n.name == "Remove"));
        assert_eq!(task_node(MAX_TASK_ID, 2) >> 63, 1);
    }

    #[test]
    fn completed_tasks_sink_below_pending_ones_but_keep_their_actions() {
        let config = read_config(
            r#"{"tasks":[{"title":"Done first","done":true},{"title":"Open","done":false}]}"#,
        )
        .unwrap();
        let scene = scene_for(&config, 0, 5, true);
        let open = scene.semantics().iter().find(|n| n.name == "Open").unwrap();
        let done = scene
            .semantics()
            .iter()
            .find(|n| n.name == "Done first")
            .unwrap();
        assert!(open.rect.y < done.rect.y);
        assert_eq!(open.action, Some(EDIT_BASE + 1));
        assert_eq!(done.action, Some(EDIT_BASE));
        assert!(
            scene
                .semantics()
                .iter()
                .any(|n| n.role == "progressbar" && n.name == "1 of 2 done")
        );
    }

    #[test]
    fn todo_controls_fit_at_every_text_scale() {
        let config = TodoConfig {
            next_id: 21,
            tasks: (0..20)
                .map(|n| TodoTask {
                    id: n + 1,
                    title: format!("Task {n}"),
                    done: n % 3 == 0,
                })
                .collect(),
        };
        for scale in [1.0, 1.25, 1.5, 2.0, 2.25] {
            for chinese in [false, true] {
                let input = FrameInput {
                    viewport: RectDip {
                        x: 0.0,
                        y: 0.0,
                        w: 420.0,
                        h: 370.0,
                    },
                    locale: if chinese { "zh-CN" } else { "en" },
                    presentation: Presentation::Compact,
                    grouping: Grouping::Single,
                    exposure: Exposure::Desktop,
                    active: true,
                    text_scale: scale,
                    theme_epoch: 1,
                    device_epoch: 1,
                };
                let logical = design::ContentScale::new(input.viewport, scale).logical_viewport();
                let capacity = page_capacity(logical.h);
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
                    input,
                );
                build_todo(
                    &mut frame,
                    &config,
                    1,
                    capacity,
                    true,
                    Colors::from_theme(None),
                    chinese,
                    None,
                );
                let scene = frame.finish(&mut design::LayoutTracker::default()).unwrap();
                crate::assert_controls_fit(&scene);
            }
        }
    }

    #[test]
    fn toggled_row_glides_to_its_new_slot_and_its_checkbox_pops() {
        let config =
            read_config(r#"{"tasks":[{"title":"A","done":true},{"title":"B","done":false}]}"#)
                .unwrap();
        // "A" was just completed: it was in slot 0 and now sorts below "B".
        let motion = ListMotion {
            tween: Tween::start(motion::NORMAL, Curve::PointToPoint, true).unwrap(),
            from_slots: HashMap::from([(1, 0.0), (2, 1.0)]),
            from_ratio: 0.0,
            emphasis: Emphasis::Toggled(1),
        };
        let row_y = |t: f32| {
            let input = FrameInput {
                viewport: RectDip {
                    x: 0.0,
                    y: 0.0,
                    w: 420.0,
                    h: 330.0,
                },
                locale: "en",
                presentation: Presentation::Compact,
                grouping: Grouping::Single,
                exposure: Exposure::Desktop,
                active: true,
                text_scale: 1.0,
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
                1,
                input,
            );
            build_todo(
                &mut frame,
                &config,
                0,
                5,
                false,
                Colors::from_theme(None),
                false,
                Some((&motion, t)),
            );
            let scene = frame.finish(&mut design::LayoutTracker::default()).unwrap();
            let row = scene
                .nodes()
                .iter()
                .find(|n| n.id == task_node(1, 0))
                .unwrap();
            let check = scene
                .nodes()
                .iter()
                .find(|n| n.id == task_node(1, 1))
                .unwrap();
            let bar = scene.nodes().iter().find(|n| n.id == 11).map(|n| n.rect.w);
            (row.rect.y, check.style.size, bar)
        };
        let (start_y, start_size, start_bar) = row_y(0.0);
        let (mid_y, mid_size, _) = row_y(0.5);
        let (end_y, end_size, end_bar) = row_y(1.0);
        assert_eq!(start_y, LIST_TOP);
        assert_eq!(end_y, LIST_TOP + ROW_PITCH);
        assert!(start_y < mid_y && mid_y < end_y);
        assert!(mid_size > start_size && (end_size - start_size).abs() < 0.01);
        assert_eq!(start_bar, None);
        assert!(end_bar.unwrap() > 0.0);
    }

    #[test]
    fn todo_fields_open_on_the_row_they_edit_or_the_next_empty_row() {
        let config = read_config(
            r#"{"tasks":[{"title":"Done","done":true},{"title":"Open","done":false}]}"#,
        )
        .unwrap();
        let viewport = RectDip {
            x: 0.0,
            y: 0.0,
            w: 400.0,
            h: 348.0,
        };
        let widget_fields = FieldLayout {
            scale: design::ContentScale::new(viewport, 1.0),
            viewport,
            colors: Colors::paper(None),
            chinese: true,
            page: 0,
            capacity: page_capacity(viewport.h),
        };
        let style = |field| field_style(&config, widget_fields, field).unwrap();
        // "Open" is pending, so it is drawn first; its field covers row 0 and the
        // text starts where the title is drawn, after the checkbox.
        let scene = scene_for(&config, 0, 5, false);
        let title = scene.nodes().iter().find(|n| n.name == "Open").unwrap();
        let edit = style(Field::Edit(1));
        assert_eq!(edit.bounds.y, title.rect.y + 4.0);
        assert_eq!(edit.bounds.x + edit.padding[0], title.rect.x);
        // The checkbox stays visible beside the field.
        let checkbox = scene
            .nodes()
            .iter()
            .find(|n| n.role == "checkbox" && n.name.ends_with("Open"))
            .unwrap();
        let glyph_right = checkbox.rect.x + (checkbox.rect.w + checkbox.style.size) / 2.0;
        assert!(glyph_right <= edit.bounds.x);
        assert_eq!(edit.background, crate::paper::SHAPE);
        assert_eq!(edit.accent, Some(crate::paper::TERRACOTTA));
        // A new task gets the empty third row, with a hint.
        let add = style(Field::Add);
        assert_eq!(add.bounds.y, LIST_TOP + 2.0 * ROW_PITCH);
        assert_eq!(add.placeholder, Some("新任务…"));
    }
}

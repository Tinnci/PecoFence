//! Local TODO panel. Tasks live in the provider-owned JSON saved with its fence.

use super::WidgetElement;
use super::{Colors, WidgetFrame, build_panel_header};
use pecofence_plugin_api::design;
use pecofence_plugin_api::*;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::sync::Arc;

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
            page: 0,
            frame_revision: 0,
            layout_tracker: design::LayoutTracker::default(),
            layout_revision: 0,
            stopped: false,
        }))
    }
}

struct TodoWidget {
    ctx: PluginContext,
    key: InstanceKey,
    mount: Option<MountKey>,
    config: TodoConfig,
    undo: Option<TodoConfig>,
    page: usize,
    theme: Option<Arc<ThemeSnapshot>>,
    frame_revision: u64,
    layout_tracker: design::LayoutTracker,
    layout_revision: u64,
    stopped: bool,
}

impl TodoWidget {
    fn redraw(&mut self) -> PanelUpdate {
        self.layout_revision = self.layout_revision.saturating_add(1);
        PanelUpdate {
            commands: vec![HostCommand::Invalidate],
            relayout: false,
        }
    }

    fn change(&mut self, next: TodoConfig) -> Result<PanelUpdate> {
        let Some(bytes) = apply_change(&mut self.config, &mut self.undo, next)? else {
            return Ok(PanelUpdate::default());
        };
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
            PanelEvent::Invoke { action: ADD, .. } if self.config.tasks.len() < MAX_TASKS => {
                Ok(PanelUpdate {
                    commands: vec![HostCommand::RequestTextInput {
                        action: ADD,
                        initial: String::new(),
                        multiline: false,
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
                if let Some(task) = next.tasks.get_mut((action - TASK_BASE) as usize) {
                    task.done = !task.done;
                }
                self.change(next)
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
                let update = self.change(next)?;
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
        let mut frame = WidgetFrame::new(mount, self.frame_revision, self.layout_revision, input);
        build_todo(
            &mut frame,
            &self.config,
            self.page,
            capacity,
            self.undo.is_some(),
            Colors::from_theme(self.theme.as_deref()),
            input.locale.starts_with("zh"),
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

fn page_capacity(height: f32) -> usize {
    ((height - 112.0) / 42.0).floor().max(1.0) as usize
}

fn build_todo(
    frame: &mut WidgetFrame,
    config: &TodoConfig,
    page: usize,
    capacity: usize,
    can_undo: bool,
    colors: Colors,
    chinese: bool,
) {
    let v = frame.viewport;
    let width = (v.w - design::CONTENT_INSET * 2.0).max(0.0);
    let pending = config.tasks.iter().filter(|task| !task.done).count();
    build_panel_header(
        frame,
        if chinese {
            format!("待办 · {pending} 项")
        } else {
            format!("To do · {pending}")
        },
        if config.tasks.len() >= MAX_TASKS {
            if chinese { "已满" } else { "Full" }
        } else if chinese {
            "＋ 添加"
        } else {
            "+ Add"
        },
        (config.tasks.len() < MAX_TASKS).then_some(ADD),
        if chinese {
            "点击标题编辑 · 勾选框完成或恢复"
        } else {
            "Click a title to edit; check to complete or restore"
        },
        colors,
    );
    if config.tasks.is_empty() {
        frame.add(WidgetElement {
            id: 9,
            parent: None,
            rect: RectDip {
                x: v.x + 18.0,
                y: v.y + 80.0,
                w: width - 8.0,
                h: 32.0,
            },
            text: (if chinese {
                "暂无任务，点击右上角添加"
            } else {
                "No tasks yet. Add one above."
            })
            .into(),
            size: design::BODY,
            weight: 400,
            foreground: colors.secondary,
            fill: None,
            radius: 0.0,
            action: None,
            role: "text",
        });
    }
    let start = page.saturating_mul(capacity);
    for (slot, (index, task)) in config
        .tasks
        .iter()
        .enumerate()
        .skip(start)
        .take(capacity)
        .enumerate()
    {
        let y = v.y + 68.0 + slot as f32 * 42.0;
        frame.add(WidgetElement {
            id: task_node(task.id, 0),
            parent: None,
            rect: RectDip {
                x: v.x + design::CONTENT_INSET,
                y,
                w: width,
                h: 37.0,
            },
            text: ("").into(),
            size: design::BODY,
            weight: 400,
            foreground: if task.done {
                colors.secondary
            } else {
                colors.primary
            },
            fill: Some(colors.subtle),
            radius: design::CONTROL_RADIUS,
            action: None,
            role: "group",
        });
        frame.add(WidgetElement {
            id: task_node(task.id, 1),
            parent: Some(task_node(task.id, 0)),
            rect: RectDip {
                x: v.x + design::CONTENT_INSET,
                y,
                w: 36.0,
                h: 37.0,
            },
            text: (if task.done { "☑" } else { "□" }).into(),
            size: 19.0,
            weight: 400,
            foreground: if task.done {
                colors.accent
            } else {
                colors.primary
            },
            fill: None,
            radius: 0.0,
            action: Some(TASK_BASE + index as u64),
            role: "checkbox",
        });
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
        frame.add(WidgetElement {
            id: task_node(task.id, 2),
            parent: Some(task_node(task.id, 0)),
            rect: RectDip {
                x: v.x + design::CONTENT_INSET + 36.0,
                y,
                w: (width - 36.0).max(0.0),
                h: 37.0,
            },
            text: (&task.title).into(),
            size: design::BODY,
            weight: 400,
            foreground: if task.done {
                colors.secondary
            } else {
                colors.primary
            },
            fill: None,
            radius: 0.0,
            action: Some(EDIT_BASE + index as u64),
            role: "button",
        });
    }
    let footer_y = v.y + v.h - 37.0;
    if config.tasks.iter().any(|task| task.done) {
        frame.add(WidgetElement {
            id: 4,
            parent: None,
            rect: RectDip {
                x: v.x + design::CONTENT_INSET,
                y: footer_y,
                w: 110.0,
                h: 26.0,
            },
            text: (if chinese {
                "清除已完成"
            } else {
                "Clear done"
            })
            .into(),
            size: 12.0,
            weight: 400,
            foreground: colors.secondary,
            fill: None,
            radius: 0.0,
            action: Some(CLEAR_DONE),
            role: "button",
        });
    }
    if can_undo {
        frame.add(WidgetElement {
            id: 5,
            parent: None,
            rect: RectDip {
                x: v.x + 130.0,
                y: footer_y,
                w: 115.0,
                h: 26.0,
            },
            text: (if chinese {
                "撤销上一步"
            } else {
                "Undo last"
            })
            .into(),
            size: 12.0,
            weight: 600,
            foreground: colors.accent,
            fill: None,
            radius: 0.0,
            action: Some(UNDO),
            role: "button",
        });
    }
    let pages = config.tasks.len().max(1).div_ceil(capacity);
    if pages > 1 {
        frame.add(WidgetElement {
            id: 6,
            parent: None,
            rect: RectDip {
                x: v.x + v.w - 112.0,
                y: footer_y,
                w: 30.0,
                h: 26.0,
            },
            text: ("‹").into(),
            size: 20.0,
            weight: 600,
            foreground: colors.primary,
            fill: None,
            radius: 0.0,
            action: (page > 0).then_some(PREVIOUS_PAGE),
            role: "button",
        });
        frame.add(WidgetElement {
            id: 7,
            parent: None,
            rect: RectDip {
                x: v.x + v.w - 79.0,
                y: footer_y,
                w: 42.0,
                h: 26.0,
            },
            text: format!("{}/{}", page + 1, pages),
            size: 12.0,
            weight: 400,
            foreground: colors.secondary,
            fill: None,
            radius: 0.0,
            action: None,
            role: "text",
        });
        frame.add(WidgetElement {
            id: 8,
            parent: None,
            rect: RectDip {
                x: v.x + v.w - 36.0,
                y: footer_y,
                w: 30.0,
                h: 26.0,
            },
            text: ("›").into(),
            size: 20.0,
            weight: 600,
            foreground: colors.primary,
            fill: None,
            radius: 0.0,
            action: (page + 1 < pages).then_some(NEXT_PAGE),
            role: "button",
        });
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
}

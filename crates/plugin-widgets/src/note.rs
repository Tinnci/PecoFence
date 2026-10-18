//! Local plain-text notes; the host owns editing, lifecycle and durable saves.

use super::WidgetElement;
use super::{CONTENT_TOP, Colors, Control, HeaderAction, WidgetFrame, build_panel_header, glyph};
use pecofence_plugin_api::design;
use pecofence_plugin_api::*;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::sync::Arc;

pub const NOTE_PROVIDER: &str = "pecofence.note";
const EDIT: u64 = 1;
const UNDO: u64 = 2;
const MAX_TEXT_UNITS: usize = 8192;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct NoteConfig {
    text: String,
}

fn parse(config: &PanelConfig) -> Result<NoteConfig> {
    if config.version != 1 || config.bytes.len() > 64 * 1024 {
        return Err(Error::Invalid(
            "unsupported or oversized note configuration".into(),
        ));
    }
    let value: NoteConfig =
        serde_json::from_slice(&config.bytes).map_err(|error| Error::Invalid(error.to_string()))?;
    validate_text(&value.text)?;
    Ok(value)
}

fn validate_text(text: &str) -> Result<()> {
    if text.encode_utf16().count() > MAX_TEXT_UNITS || text.contains('\0') {
        return Err(Error::Invalid(
            "note exceeds 8192 UTF-16 units or contains NUL".into(),
        ));
    }
    Ok(())
}

fn submitted_text(text: &str) -> Result<String> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    validate_text(&normalized)?;
    Ok(normalized)
}

pub struct NotePlugin;

impl PanelProvider for NotePlugin {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: NOTE_PROVIDER,
            api_major: 1,
            config_major: 1,
            required_services: &["theme"],
        }
    }

    fn validate(&self, config: &PanelConfig) -> Result<()> {
        parse(config).map(|_| ())
    }

    fn create(&self, ctx: PluginContext, input: CreatePanel) -> Result<Box<dyn PanelInstance>> {
        Ok(Box::new(NoteWidget {
            config: parse(&input.config)?,
            theme: ctx.theme.with(|service| service.snapshot()).ok(),
            ctx,
            key: input.key,
            mount: None,
            undo: None,
            revision: 0,
            frame_revision: 0,
            layout_tracker: design::LayoutTracker::default(),
            stopped: false,
        }))
    }
}

struct NoteWidget {
    ctx: PluginContext,
    key: InstanceKey,
    mount: Option<MountKey>,
    config: NoteConfig,
    undo: Option<String>,
    theme: Option<Arc<ThemeSnapshot>>,
    revision: u64,
    frame_revision: u64,
    layout_tracker: design::LayoutTracker,
    stopped: bool,
}

impl NoteWidget {
    fn redraw(&mut self) -> PanelUpdate {
        self.revision = self.revision.saturating_add(1);
        PanelUpdate {
            commands: vec![HostCommand::Invalidate],
            relayout: false,
        }
    }

    fn change(&mut self, text: String) -> Result<PanelUpdate> {
        validate_text(&text)?;
        if text == self.config.text {
            return Ok(PanelUpdate::default());
        }
        let config = NoteConfig { text };
        let bytes: Arc<[u8]> = serde_json::to_vec(&config)
            .map_err(|error| Error::Backend(error.to_string()))?
            .into();
        parse(&PanelConfig {
            version: 1,
            bytes: bytes.clone(),
        })?;
        self.undo = Some(std::mem::replace(&mut self.config, config).text);
        let mut update = self.redraw();
        update.commands.push(HostCommand::PersistConfig(bytes));
        Ok(update)
    }
}

impl PanelInstance for NoteWidget {
    fn initial_presentation(&self) -> Presentation {
        Presentation::Compact
    }

    fn mount(&mut self, ctx: MountContext) -> Result<()> {
        if ctx.key.instance != self.key {
            return Err(Error::Invalid("mount belongs to another note".into()));
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
            PanelEvent::Invoke { action: EDIT, .. } => Ok(PanelUpdate {
                commands: vec![HostCommand::RequestTextInput {
                    action: EDIT,
                    initial: self.config.text.clone(),
                    multiline: true,
                }],
                relayout: false,
            }),
            PanelEvent::TextSubmitted { action: EDIT, text } => self.change(submitted_text(&text)?),
            PanelEvent::Invoke { action: UNDO, .. } => {
                if let Some(text) = self.undo.clone() {
                    let update = self.change(text)?;
                    self.undo = None;
                    Ok(update)
                } else {
                    Ok(PanelUpdate::default())
                }
            }
            _ => Ok(PanelUpdate::default()),
        }
    }

    fn prepare_frame(&mut self, input: FrameInput) -> Result<Rc<dyn PreparedFrame>> {
        let mount = self.mount.ok_or(Error::Closed)?;
        self.frame_revision = self.frame_revision.checked_add(1).ok_or(Error::Exhausted)?;
        let mut frame = WidgetFrame::new(mount, self.frame_revision, self.revision, input);
        build_note(
            &mut frame,
            &self.config,
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

    fn begin_stop(&mut self, _: StopReason) {
        self.stopped = true;
        self.mount = None;
    }
}

fn note_summary(text: &str, chinese: bool) -> String {
    if text.trim().is_empty() {
        return if chinese {
            "空白便笺"
        } else {
            "Empty note"
        }
        .into();
    }
    let chars = text.chars().filter(|c| !c.is_whitespace()).count();
    let lines = text.lines().count().max(1);
    match (chinese, lines) {
        (true, 1) => format!("{chars} 字"),
        (true, _) => format!("{chars} 字 · {lines} 行"),
        (false, 1) => format!("{chars} characters"),
        (false, _) => format!("{chars} characters · {lines} lines"),
    }
}

fn build_note(
    frame: &mut WidgetFrame,
    config: &NoteConfig,
    can_undo: bool,
    colors: Colors,
    chinese: bool,
) {
    let narrow = frame.viewport.w < 300.0;
    let edit = HeaderAction {
        id: 2,
        content: if narrow {
            Control::Icon(glyph::EDIT, if chinese { "编辑" } else { "Edit" })
        } else {
            Control::Text(if chinese { "编辑" } else { "Edit" })
        },
        kind: design::ButtonKind::Accent,
        action: Some(EDIT),
        width: if narrow { design::CONTROL_HEIGHT } else { 64.0 },
    };
    let undo = HeaderAction {
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
    };
    let actions = if can_undo {
        vec![undo, edit]
    } else {
        vec![edit]
    };
    build_panel_header(
        frame,
        if chinese { "便笺" } else { "Note" },
        note_summary(&config.text, chinese),
        &actions,
        colors,
    );
    let v = frame.viewport;
    // The body card is the main edit target and fills the rest of the panel.
    let body = RectDip {
        x: v.x + design::CONTENT_INSET,
        y: v.y + CONTENT_TOP,
        w: (v.w - design::CONTENT_INSET * 2.0).max(0.0),
        h: (v.h - CONTENT_TOP - design::CONTENT_INSET).max(0.0),
    };
    frame.add(
        WidgetElement::text(4, body, "", design::BODY, 400, colors.primary)
            .role("button")
            .fill(Some(colors.subtle), design::CARD_RADIUS)
            .action(Some(EDIT)),
    );
    frame.set_last_accessible_name(if chinese { "编辑便笺" } else { "Edit note" }.into());
    let inner = RectDip {
        x: body.x + 14.0,
        y: body.y + 12.0,
        w: (body.w - 28.0).max(0.0),
        h: (body.h - 24.0).max(0.0),
    };
    if !config.text.is_empty() {
        frame.add(
            WidgetElement::text(5, inner, &config.text, design::BODY, 400, colors.primary)
                .parent(4),
        );
        return;
    }
    // Empty state: a centered invitation that also teaches the editor keys.
    let middle = inner.y + inner.h / 2.0;
    frame.add(
        WidgetElement::text(
            8,
            RectDip {
                x: inner.x,
                y: middle - 52.0,
                w: inner.w,
                h: 36.0,
            },
            glyph::QUICK_NOTE,
            28.0,
            400,
            colors.faint(),
        )
        .parent(4)
        .role("presentation")
        .icon()
        .align(TextAlign::Center),
    );
    frame.set_last_accessible_name(String::new());
    frame.add(
        WidgetElement::text(
            5,
            RectDip {
                x: inner.x,
                y: middle - 12.0,
                w: inner.w,
                h: 24.0,
            },
            if chinese {
                "从这里开始记录…"
            } else {
                "Start a note here…"
            },
            design::SUBTITLE,
            600,
            colors.secondary,
        )
        .parent(4)
        .align(TextAlign::Center),
    );
    frame.add(
        WidgetElement::text(
            7,
            RectDip {
                x: inner.x,
                y: middle + 14.0,
                w: inner.w,
                h: 20.0,
            },
            if chinese {
                "Ctrl+Enter 或失焦保存 · Esc 取消"
            } else {
                "Ctrl+Enter / blur saves; Esc cancels"
            },
            design::CAPTION,
            400,
            colors.secondary,
        )
        .parent(4)
        .align(TextAlign::Center),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_multiline_input_preserves_whitespace_and_normalizes_line_endings() {
        assert_eq!(
            submitted_text("  first\r\n第二行\r\n\r\n").unwrap(),
            "  first\n第二行\n\n"
        );
        assert_eq!(submitted_text("").unwrap(), "");
        assert!(submitted_text("invalid\0text").is_err());
        assert!(submitted_text(&"📝".repeat(4097)).is_err());
        assert!(submitted_text(&"📝".repeat(4096)).is_ok());
    }

    #[test]
    fn empty_and_multiline_configuration_round_trip() {
        let empty: Arc<[u8]> = b"{}".as_slice().into();
        assert_eq!(
            parse(&PanelConfig {
                version: 1,
                bytes: empty
            })
            .unwrap(),
            NoteConfig::default()
        );
        let config = NoteConfig {
            text: "项目记录\n- first\n- second".into(),
        };
        let bytes = serde_json::to_vec(&config).unwrap().into();
        assert_eq!(parse(&PanelConfig { version: 1, bytes }).unwrap(), config);
    }
    #[test]
    fn summary_counts_visible_characters_and_lines() {
        assert_eq!(note_summary("  \n", false), "Empty note");
        assert_eq!(note_summary("项目 记录", true), "4 字");
        assert_eq!(note_summary("a\nbc\n", false), "3 characters · 2 lines");
    }

    #[test]
    fn note_header_and_body_identity_do_not_depend_on_undo_visibility() {
        let mut scenes = Vec::new();
        for undo in [false, true] {
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
                },
            );
            build_note(
                &mut frame,
                &NoteConfig {
                    text: "Keep this text".into(),
                },
                undo,
                Colors::from_theme(None),
                false,
            );
            scenes.push(frame.finish(&mut design::LayoutTracker::default()).unwrap());
        }
        let before = &scenes[0];
        let after = &scenes[1];
        for name in ["Keep this text", "Edit note", "Note", "12 characters"] {
            let a = before.semantics().iter().find(|n| n.name == name).unwrap();
            let b = after.semantics().iter().find(|n| n.name == name).unwrap();
            assert_eq!(a.id, b.id);
        }
        let text = after
            .semantics()
            .iter()
            .find(|n| n.name == "Keep this text")
            .unwrap();
        let body = after
            .semantics()
            .iter()
            .find(|n| n.name == "Edit note")
            .unwrap();
        assert_eq!(text.parent, Some(body.id));
        let undo = after
            .semantics()
            .iter()
            .find(|n| n.action == Some(UNDO))
            .unwrap();
        assert!(!before.semantics().iter().any(|n| n.id == undo.id));
    }

    #[test]
    fn note_controls_fit_and_empty_state_teaches_editor_keys() {
        for scale in [1.0, 1.5, 2.25] {
            for (text, undo) in [("", false), ("Body", true)] {
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
                            w: 420.0,
                            h: 370.0,
                        },
                        locale: "en",
                        presentation: Presentation::Compact,
                        grouping: Grouping::Single,
                        exposure: Exposure::Desktop,
                        active: true,
                        text_scale: scale,
                        theme_epoch: 1,
                        device_epoch: 1,
                    },
                );
                build_note(
                    &mut frame,
                    &NoteConfig { text: text.into() },
                    undo,
                    Colors::from_theme(None),
                    false,
                );
                let scene = frame.finish(&mut design::LayoutTracker::default()).unwrap();
                // The body card intentionally contains its text; only check the header controls.
                let body = scene
                    .hit_tree()
                    .iter()
                    .find(|hit| hit.id == 4)
                    .unwrap()
                    .rect;
                for hit in scene.hit_tree().iter().filter(|hit| hit.id != 4) {
                    assert!(hit.rect.y + hit.rect.h <= body.y);
                }
                crate::assert_controls_fit(&scene);
                let hint = scene
                    .semantics()
                    .iter()
                    .any(|n| n.name == "Ctrl+Enter / blur saves; Esc cancels");
                assert_eq!(hint, text.is_empty());
            }
        }
    }
}

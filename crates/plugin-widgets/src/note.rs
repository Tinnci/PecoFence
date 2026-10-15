//! Local plain-text notes; the host owns editing, lifecycle and durable saves.

use super::{Colors, WidgetFrame, build_panel_header};
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

fn build_note(
    frame: &mut WidgetFrame,
    config: &NoteConfig,
    can_undo: bool,
    colors: Colors,
    chinese: bool,
) {
    build_panel_header(
        frame,
        if chinese { "便笺" } else { "Note" },
        if chinese { "编辑" } else { "Edit" },
        Some(EDIT),
        if chinese {
            "记录想法与临时信息 · 点击正文编辑"
        } else {
            "Capture a thought; click the text to edit"
        },
        colors,
    );
    let v = frame.viewport;
    let body = RectDip {
        x: v.x + 14.0,
        y: v.y + 68.0,
        w: (v.w - 28.0).max(0.0),
        h: (v.h - 116.0).max(0.0),
    };
    frame.add(
        body,
        "",
        15.0,
        400,
        colors.primary,
        Some(colors.subtle),
        6.0,
        Some(EDIT),
        "button",
    );
    frame.add(
        RectDip {
            x: body.x + 12.0,
            y: body.y + 10.0,
            w: (body.w - 24.0).max(0.0),
            h: (body.h - 20.0).max(0.0),
        },
        if config.text.is_empty() {
            if chinese {
                "从这里开始记录…"
            } else {
                "Start a note here…"
            }
        } else {
            &config.text
        },
        15.0,
        400,
        if config.text.is_empty() {
            colors.secondary
        } else {
            colors.primary
        },
        None,
        0.0,
        None,
        "text",
    );
    if let Some(node) = frame.nodes.last_mut() {
        node.clip_text = true;
    }
    if can_undo {
        frame.add(
            RectDip {
                x: v.x + 14.0,
                y: v.y + v.h - 37.0,
                w: 110.0,
                h: 26.0,
            },
            if chinese {
                "撤销上一步"
            } else {
                "Undo last"
            },
            12.0,
            600,
            colors.accent,
            None,
            0.0,
            Some(UNDO),
            "button",
        );
    }
    frame.add(
        RectDip {
            x: v.x + 130.0,
            y: v.y + v.h - 37.0,
            w: (v.w - 144.0).max(0.0),
            h: 26.0,
        },
        if chinese {
            "Ctrl+Enter 或失焦保存 · Esc 取消"
        } else {
            "Ctrl+Enter / blur saves; Esc cancels"
        },
        11.0,
        400,
        colors.secondary,
        None,
        0.0,
        None,
        "text",
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
}

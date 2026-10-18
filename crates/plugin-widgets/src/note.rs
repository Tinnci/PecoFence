//! Local plain-text notes; the host owns editing, lifecycle and durable saves.

use super::{
    CAPSULE_HEIGHT, CONTENT_TOP, Colors, Control, HeaderAction, WidgetFrame, build_capsule,
    build_panel_header, glyph,
};
use super::{FRAME_INTERVAL, Tween, WidgetElement, fade, lerp_color};
use pecofence_plugin_api::design;
use pecofence_plugin_api::design::motion::{self, Curve};
use pecofence_plugin_api::*;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const NOTE_PROVIDER: &str = "pecofence.note";
const EDIT: u64 = 1;
const UNDO: u64 = 2;
const TINT: u64 = 3;
const MAX_TEXT_UNITS: usize = 8192;
/// How long the header confirms an accepted edit before returning to the summary.
const UPDATED_HOLD: Duration = Duration::from_millis(1_200);

/// Paper color of the note body (DeskBox paper styles, Sticky Notes palette).
/// `Default` keeps the theme surface; high contrast always does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Tint {
    #[default]
    Default,
    Yellow,
    Green,
    Pink,
    Purple,
    Blue,
    Gray,
}

impl Tint {
    const ORDER: [Tint; 7] = [
        Tint::Default,
        Tint::Yellow,
        Tint::Green,
        Tint::Pink,
        Tint::Purple,
        Tint::Blue,
        Tint::Gray,
    ];

    fn is_default(&self) -> bool {
        *self == Tint::Default
    }

    fn next(self) -> Self {
        let index = Self::ORDER.iter().position(|t| *t == self).unwrap_or(0);
        Self::ORDER[(index + 1) % Self::ORDER.len()]
    }

    fn rgb(self) -> Option<[f32; 3]> {
        Some(match self {
            Tint::Default => return None,
            Tint::Yellow => [1.0, 0.84, 0.32],
            Tint::Green => [0.52, 0.85, 0.48],
            Tint::Pink => [1.0, 0.58, 0.74],
            Tint::Purple => [0.74, 0.6, 1.0],
            Tint::Blue => [0.48, 0.74, 1.0],
            Tint::Gray => [0.62, 0.64, 0.68],
        })
    }

    fn name(self, chinese: bool) -> &'static str {
        match (self, chinese) {
            (Tint::Default, true) => "默认",
            (Tint::Yellow, true) => "黄色",
            (Tint::Green, true) => "绿色",
            (Tint::Pink, true) => "粉色",
            (Tint::Purple, true) => "紫色",
            (Tint::Blue, true) => "蓝色",
            (Tint::Gray, true) => "灰色",
            (Tint::Default, false) => "Default",
            (Tint::Yellow, false) => "Yellow",
            (Tint::Green, false) => "Green",
            (Tint::Pink, false) => "Pink",
            (Tint::Purple, false) => "Purple",
            (Tint::Blue, false) => "Blue",
            (Tint::Gray, false) => "Gray",
        }
    }

    /// Body fill: a translucent wash of the paper color over the panel surface.
    fn surface(self, colors: Colors) -> [f32; 4] {
        match self.rgb() {
            Some([r, g, b]) if !colors.high_contrast => [r, g, b, 0.24],
            _ => colors.subtle,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct NoteConfig {
    text: String,
    #[serde(skip_serializing_if = "Tint::is_default")]
    tint: Tint,
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
            tint_motion: None,
            updated_at: None,
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
    /// Paper color cross-fade: the fill it started from.
    tint_motion: Option<(Tween, [f32; 4])>,
    /// When the last edit was accepted, for the header confirmation.
    updated_at: Option<Instant>,
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

    fn animations(&self) -> bool {
        self.theme.as_ref().is_none_or(|theme| theme.animations)
    }

    fn persist(&mut self, config: NoteConfig) -> Result<PanelUpdate> {
        let bytes: Arc<[u8]> = serde_json::to_vec(&config)
            .map_err(|error| Error::Backend(error.to_string()))?
            .into();
        parse(&PanelConfig {
            version: 1,
            bytes: bytes.clone(),
        })?;
        self.config = config;
        let mut update = self.redraw();
        update.commands.push(HostCommand::PersistConfig(bytes));
        Ok(update)
    }

    fn change(&mut self, text: String) -> Result<PanelUpdate> {
        validate_text(&text)?;
        if text == self.config.text {
            return Ok(PanelUpdate::default());
        }
        let previous = self.config.text.clone();
        let update = self.persist(NoteConfig {
            text,
            tint: self.config.tint,
        })?;
        self.undo = Some(previous);
        self.updated_at = Some(Instant::now());
        Ok(update)
    }

    fn cycle_tint(&mut self) -> Result<PanelUpdate> {
        let colors = Colors::from_theme(self.theme.as_deref());
        let from = self.config.tint.surface(colors);
        let update = self.persist(NoteConfig {
            text: self.config.text.clone(),
            tint: self.config.tint.next(),
        })?;
        self.tint_motion =
            Tween::start(motion::FAST, Curve::Decelerate, self.animations()).map(|t| (t, from));
        Ok(update)
    }

    /// Opacity of the "updated" confirmation: fades in, holds, fades out.
    fn updated_opacity(&self, now: Instant) -> Option<f32> {
        let elapsed = now.saturating_duration_since(self.updated_at?);
        let fade_time = motion::FAST;
        if elapsed >= UPDATED_HOLD + fade_time {
            return None;
        }
        if !self.animations() {
            return Some(1.0);
        }
        let fade_in = motion::ease(
            Curve::Decelerate,
            elapsed.as_secs_f32() / fade_time.as_secs_f32(),
        );
        let fade_out = 1.0
            - motion::ease(
                Curve::Accelerate,
                elapsed.saturating_sub(UPDATED_HOLD).as_secs_f32() / fade_time.as_secs_f32(),
            );
        Some(fade_in.min(fade_out))
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
            PanelEvent::Invoke { action: TINT, .. } => self.cycle_tint(),
            PanelEvent::Wake => Ok(self.redraw()),
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
        let now = Instant::now();
        if self.tint_motion.is_some_and(|(tween, _)| tween.done(now)) {
            self.tint_motion = None;
        }
        let updated = self.updated_opacity(now);
        if updated.is_none() {
            self.updated_at = None;
        }
        let colors = Colors::from_theme(self.theme.as_deref());
        let to = self.config.tint.surface(colors);
        let surface = self.tint_motion.map_or(to, |(tween, from)| {
            lerp_color(from, to, tween.progress(now))
        });
        build_note(
            &mut frame,
            &self.config,
            self.undo.is_some(),
            colors,
            input.locale.starts_with("zh"),
            NoteLook { surface, updated },
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
        if self.stopped {
            return None;
        }
        if self.tint_motion.is_some() {
            return Some(FRAME_INTERVAL);
        }
        let elapsed = self.updated_at?.elapsed();
        let end = UPDATED_HOLD + motion::FAST;
        // Frames only while the confirmation fades; a single wake covers the hold.
        Some(
            if !self.animations() {
                end.saturating_sub(elapsed)
            } else if (motion::FAST..UPDATED_HOLD).contains(&elapsed) {
                UPDATED_HOLD - elapsed
            } else {
                FRAME_INTERVAL
            }
            .max(FRAME_INTERVAL),
        )
    }
}

#[cfg(test)]
const REST: NoteLook = NoteLook {
    surface: [0.165, 0.2, 0.251, 1.0],
    updated: None,
};

/// Animated appearance inputs, resolved by the widget before layout.
#[derive(Clone, Copy)]
struct NoteLook {
    surface: [f32; 4],
    /// Opacity of the "updated" confirmation in the header, while it shows.
    updated: Option<f32>,
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
    look: NoteLook,
) {
    if frame.viewport.h < CAPSULE_HEIGHT {
        let first = config
            .text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty());
        let summary = match (first, chinese) {
            (Some(line), _) => line.to_string(),
            (None, true) => "从这里开始记录…".to_string(),
            (None, false) => "Start a note here…".to_string(),
        };
        build_capsule(frame, summary, Some(EDIT), Some(look.surface), colors);
        return;
    }
    let narrow = frame.viewport.w < 300.0;
    let tint = HeaderAction {
        id: 9,
        content: Control::Icon(glyph::COLOR, config.tint.name(chinese)),
        kind: design::ButtonKind::Flat,
        action: Some(TINT),
        width: design::CONTROL_HEIGHT,
    };
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
        vec![tint, undo, edit]
    } else {
        vec![tint, edit]
    };
    let updated = look.updated.filter(|opacity| *opacity > 0.0);
    build_panel_header(
        frame,
        if chinese { "便笺" } else { "Note" },
        match updated {
            Some(_) if chinese => "✓ 已更新".to_string(),
            Some(_) => "✓ Updated".to_string(),
            None => note_summary(&config.text, chinese),
        },
        &actions,
        colors,
    );
    if let Some(opacity) = updated
        && let Some(meta) = frame
            .nodes
            .iter_mut()
            .find(|node| node.id == super::HEADER_META)
    {
        meta.style.foreground = fade(colors.accent, opacity);
    }
    // The paper-color button names the current color for screen readers.
    if let Some(node) = frame.nodes.iter_mut().find(|node| node.id == 9) {
        node.name = if chinese {
            format!("便笺颜色：{}", config.tint.name(true))
        } else {
            format!("Note color: {}", config.tint.name(false))
        };
        if let Some([r, g, b]) = config.tint.rgb().filter(|_| !colors.high_contrast) {
            node.style.foreground = [r, g, b, 1.0];
        }
    }
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
            .fill(Some(look.surface), design::CARD_RADIUS)
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
            tint: Tint::Yellow,
        };
        let bytes: Arc<[u8]> = serde_json::to_vec(&config).unwrap().into();
        assert!(
            std::str::from_utf8(&bytes)
                .unwrap()
                .contains(r#""tint":"yellow""#)
        );
        assert_eq!(parse(&PanelConfig { version: 1, bytes }).unwrap(), config);
        // Untinted notes keep the original version-1 JSON shape.
        let plain = NoteConfig {
            text: "a".into(),
            ..Default::default()
        };
        assert_eq!(serde_json::to_string(&plain).unwrap(), r#"{"text":"a"}"#);
        assert_eq!(Tint::Gray.next(), Tint::Default);
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
                    ..Default::default()
                },
                undo,
                Colors::from_theme(None),
                false,
                REST,
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
                    &NoteConfig {
                        text: text.into(),
                        ..Default::default()
                    },
                    undo,
                    Colors::from_theme(None),
                    false,
                    REST,
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

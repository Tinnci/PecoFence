//! One frame's title controls, shared by painting, input and semantics.
use crate::fence_chrome::{CaptionButton, TabDraw, TitleCommandDraw, TitleDeco, TitleState};
use pecofence_plugin_api::RectDip;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleControlId {
    Tab(u128),
    Up,
    Chevron,
    Command(usize),
    Caption(CaptionButton),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleControlKind {
    Tab,
    Icon,
    Chevron,
    Caption,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ControlState {
    pub hover: f32,
    pub pressed: bool,
}

#[derive(Clone, Debug)]
pub struct TitleControl {
    pub id: TitleControlId,
    pub kind: TitleControlKind,
    pub rect: RectDip,
    pub glyph: char,
    pub label: std::borrow::Cow<'static, str>,
    pub enabled: bool,
    pub state: ControlState,
}

impl TitleControl {
    #[allow(clippy::too_many_arguments)]
    pub fn layout(
        width: f32,
        height: f32,
        deco: TitleDeco,
        tabs: &[TabDraw<'_>],
        commands: &[TitleCommandDraw],
        labels: &[&str],
        rolled: bool,
        state: TitleState,
    ) -> Vec<Self> {
        let rect = |r: crate::Rect| RectDip {
            x: r.left,
            y: r.top,
            w: r.width(),
            h: r.height(),
        };
        let mut controls = Vec::new();
        let mut add = |id,
                       kind,
                       rect,
                       glyph,
                       label: std::borrow::Cow<'static, str>,
                       enabled,
                       hover,
                       pressed| {
            controls.push(Self {
                id,
                kind,
                rect,
                glyph,
                label,
                enabled,
                state: ControlState { hover, pressed },
            })
        };
        for tab in tabs
            .iter()
            .filter(|t| tabs.len() > 1 && !t.dragging)
            .chain(tabs.iter().filter(|t| tabs.len() > 1 && t.dragging))
        {
            add(
                TitleControlId::Tab(tab.key),
                TitleControlKind::Tab,
                RectDip {
                    x: tab.x,
                    y: 0.0,
                    w: tab.w,
                    h: height,
                },
                '\0',
                tab.title.to_owned().into(),
                true,
                tab.hover,
                tab.pressed,
            );
        }
        if deco.up_button && tabs.len() <= 1 {
            add(
                TitleControlId::Up,
                TitleControlKind::Icon,
                rect(TitleDeco::up_box(height)),
                '\u{E74A}',
                std::borrow::Cow::Borrowed("向上"),
                true,
                state.up_hover,
                state.up_pressed,
            );
        }
        if tabs.len() <= 1 && !rolled {
            for (i, label) in labels.iter().enumerate() {
                let command = commands.get(i);
                add(
                    TitleControlId::Command(i),
                    TitleControlKind::Icon,
                    rect(deco.command_box(width, height, i)),
                    command.map_or('\0', |c| c.glyph),
                    (*label).to_owned().into(),
                    command.is_none_or(|c| c.enabled),
                    command.map_or(0.0, |c| c.hover),
                    command.is_some_and(|c| c.pressed),
                );
            }
        }
        if deco.captions {
            for button in CaptionButton::ALL {
                let (glyph, label) = match button {
                    CaptionButton::Minimize => ('\u{E921}', "最小化"),
                    CaptionButton::Maximize if state.maximized => ('\u{E923}', "向下还原"),
                    CaptionButton::Maximize => ('\u{E922}', "最大化"),
                    CaptionButton::Close => ('\u{E8BB}', "关闭"),
                };
                add(
                    TitleControlId::Caption(button),
                    TitleControlKind::Caption,
                    rect(TitleDeco::caption_box(width, height, button)),
                    glyph,
                    std::borrow::Cow::Borrowed(label),
                    true,
                    (state.caption_hover == Some(button)) as u8 as f32,
                    state.caption_pressed == Some(button),
                );
            }
        } else {
            add(
                TitleControlId::Chevron,
                TitleControlKind::Chevron,
                rect(TitleDeco::chevron_box(width, height)),
                '\u{E70E}',
                std::borrow::Cow::Borrowed(if rolled { "展开" } else { "卷起" }),
                true,
                state.chevron_hover,
                state.chevron_pressed,
            );
        }
        controls
    }
    pub fn draw_rect(&self) -> crate::Rect {
        crate::Rect::from_xywh(self.rect.x, self.rect.y, self.rect.w, self.rect.h)
    }
}

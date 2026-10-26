//! Colors for the board and the summary panel: warm paper like the desktop
//! widgets, or the system palette in high contrast. Layout code only picks roles.

use super::model::Tone;
use pecofence_plugin_api::{ThemeSnapshot, paper};
use spm_contracts::{BoardStage, ProgressState};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub hc: bool,
    pub dark: bool,
    pub primary: [f32; 4],
    pub secondary: [f32; 4],
    /// Solid emphasis: selection outlines, progress fills.
    pub accent: [f32; 4],
    /// The accent as text and glyphs.
    pub accent_ink: [f32; 4],
    pub danger: [f32; 4],
    pub warning: [f32; 4],
    pub success: [f32; 4],
    /// The panel's own ground; `None` draws on the fence material (high contrast).
    pub ground: Option<[f32; 4]>,
    pub base: [f32; 4],
    pub column: [f32; 4],
    pub card: [f32; 4],
    pub selected: [f32; 4],
    pub chip: [f32; 4],
    pub chip_on: [f32; 4],
    pub chip_on_text: [f32; 4],
    pub rule: [f32; 4],
    pub skeleton: [f32; 4],
}

fn over(top: [f32; 4], alpha: f32, under: [f32; 4]) -> [f32; 4] {
    let a = (top[3] * alpha).clamp(0.0, 1.0);
    [
        top[0] * a + under[0] * (1.0 - a),
        top[1] * a + under[1] * (1.0 - a),
        top[2] * a + under[2] * (1.0 - a),
        1.0,
    ]
}

impl Palette {
    /// Warm paper, like the note, TODO and calendar widgets. High contrast keeps the
    /// system palette so user-chosen contrast colors win.
    pub fn from_theme(theme: &ThemeSnapshot) -> Self {
        if theme.high_contrast {
            return Self::high_contrast(theme);
        }
        let p = paper::resolve(Some(theme));
        let on = if p.dark { p.accent_ink } else { p.accent };
        Self {
            hc: false,
            dark: p.dark,
            primary: p.ink,
            secondary: p.ink_secondary,
            accent: p.accent,
            accent_ink: p.accent_ink,
            danger: p.tone(paper::ERROR.0, paper::ERROR.1),
            warning: p.tone(paper::WARNING.0, paper::WARNING.1),
            success: p.tone(paper::SUCCESS.0, paper::SUCCESS.1),
            ground: Some(p.ground),
            base: p.ground,
            column: p.ground,
            card: p.card,
            selected: p.wash(on, if p.dark { 0.14 } else { 0.09 }),
            chip: p.card,
            chip_on: p.wash(on, if p.dark { 0.18 } else { 0.14 }),
            chip_on_text: p.accent_ink,
            rule: paper::mix(p.card, p.ink, 0.1),
            skeleton: paper::mix(p.card, p.ink, 0.06),
        }
    }

    /// Timeline's three terracotta steps use the same resolved paper accent.
    pub(super) fn stage(&self, stage: BoardStage) -> [f32; 4] {
        if self.hc {
            return self.base;
        }
        match stage {
            BoardStage::NotStarted | BoardStage::Unmapped => self.skeleton,
            BoardStage::Implementation | BoardStage::Review => {
                paper::mix(self.accent, self.base, 0.35)
            }
            BoardStage::Done => paper::mix(self.accent, self.primary, 0.35),
            _ => self.accent,
        }
    }

    pub(super) fn weekend(&self) -> [f32; 4] {
        if self.hc {
            self.base
        } else {
            paper::mix(self.base, self.primary, 0.035)
        }
    }

    /// Avatar circles: a neutral step off the card.
    pub(super) fn chip_avatar(&self) -> [f32; 4] {
        if self.hc {
            self.base
        } else {
            paper::mix(self.card, self.primary, 0.09)
        }
    }

    fn high_contrast(theme: &ThemeSnapshot) -> Self {
        let p = theme.text_primary;
        let base = [
            theme.surface_panel[0],
            theme.surface_panel[1],
            theme.surface_panel[2],
            1.0,
        ];
        Self {
            hc: true,
            dark: 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2] > 0.5,
            primary: p,
            secondary: theme.text_secondary,
            accent: theme.accent,
            accent_ink: theme.accent,
            danger: theme.danger,
            warning: theme.warning,
            success: theme.success,
            ground: None,
            base,
            column: base,
            card: base,
            selected: base,
            chip: base,
            chip_on: p,
            chip_on_text: base,
            rule: over(theme.stroke, 1.0, base),
            skeleton: over(p, 0.10, base),
        }
    }

    pub fn tone(&self, tone: Tone) -> [f32; 4] {
        match tone {
            Tone::Danger => self.danger,
            Tone::Warning => self.warning,
            Tone::Info => self.accent_ink,
        }
    }

    pub(super) fn progress(&self, state: ProgressState) -> [f32; 4] {
        match state {
            ProgressState::Passed => self.success,
            ProgressState::Failed => self.danger,
            ProgressState::InProgress => self.accent_ink,
            _ => self.secondary,
        }
    }
}

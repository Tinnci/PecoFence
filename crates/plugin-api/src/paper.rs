//! Warm paper surfaces shared by the desktop widgets and the SPM panels.
//!
//! `resolve` turns the user's paper preference into a ground, a card and inks
//! that meet 4.5:1 on both. Providers that draw on paper own their background
//! and paint `ground` themselves. High contrast never comes through here: callers
//! keep the system palette so user-chosen contrast colors win.

use crate::{ChromeSeverity, PaperStyle, ThemeSnapshot};

pub const fn srgb(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

pub const GROUND: [f32; 4] = srgb(0xF6, 0xF1, 0xE7);
pub const CARD: [f32; 4] = srgb(0xEC, 0xE4, 0xD6);
pub const INK: [f32; 4] = srgb(0x2B, 0x27, 0x22);
pub const INK_SECONDARY: [f32; 4] = srgb(0x68, 0x5F, 0x55);
pub const INK_FAINT: [f32; 4] = srgb(0x9A, 0x8F, 0x80);
pub const TERRACOTTA: [f32; 4] = srgb(0xA9, 0x50, 0x2D);
pub const TERRACOTTA_INK: [f32; 4] = srgb(0x94, 0x45, 0x26);
pub const CREAM: [f32; 4] = srgb(0xFF, 0xF8, 0xEE);

/// Status inks on paper as (light paper, dark paper), one per WinUI InfoBar severity.
pub const SUCCESS: ([f32; 4], [f32; 4]) = (srgb(0x4E, 0x6B, 0x2E), srgb(0xA9, 0xC4, 0x7F));
/// CI success ink, separate from the general InfoBar success role.
pub const GOOD: ([f32; 4], [f32; 4]) = (srgb(0x3D, 0x6B, 0x4B), srgb(0x9C, 0xCB, 0x9F));
pub const WARNING: ([f32; 4], [f32; 4]) = (srgb(0x7D, 0x5D, 0x0E), srgb(0xE5, 0xBC, 0x66));
pub const ERROR: ([f32; 4], [f32; 4]) = (srgb(0x9E, 0x2F, 0x3B), srgb(0xF2, 0x95, 0x8A));

/// The ink for a status line of `severity`; `None` keeps the secondary ink.
pub fn status_ink(severity: ChromeSeverity, dark: bool) -> Option<[f32; 4]> {
    let pick = |(light, on_dark): ([f32; 4], [f32; 4])| if dark { on_dark } else { light };
    match severity {
        ChromeSeverity::Informational => None,
        ChromeSeverity::Success => Some(pick(SUCCESS)),
        ChromeSeverity::Warning => Some(pick(WARNING)),
        ChromeSeverity::Error => Some(pick(ERROR)),
    }
}

pub fn mix(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// WCAG relative luminance.
pub fn luminance(c: [f32; 4]) -> f32 {
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(c[0]) + 0.7152 * linear(c[1]) + 0.0722 * linear(c[2])
}

pub fn contrast(a: [f32; 4], b: [f32; 4]) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `ink`, moved toward black or white until it reaches `minimum` on both surfaces.
pub fn readable(ink: [f32; 4], ground: [f32; 4], card: [f32; 4], minimum: f32) -> [f32; 4] {
    let black = srgb(0, 0, 0);
    let white = srgb(255, 255, 255);
    let score = |c| contrast(c, ground).min(contrast(c, card));
    if score(ink) >= minimum {
        return ink;
    }
    let target = if score(black) >= score(white) {
        black
    } else {
        white
    };
    for step in 1..=100 {
        let c = mix(ink, target, step as f32 / 100.0);
        if score(c) >= minimum {
            return c;
        }
    }
    target
}

/// The resolved paper for one theme snapshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paper {
    pub dark: bool,
    pub ground: [f32; 4],
    pub card: [f32; 4],
    pub ink: [f32; 4],
    pub ink_secondary: [f32; 4],
    /// Low-emphasis ink; meets 4.5:1 only on light paper at full brightness.
    pub ink_faint: [f32; 4],
    /// Solid emphasis fills; carries `CREAM` text.
    pub accent: [f32; 4],
    /// The accent as text.
    pub accent_ink: [f32; 4],
}

impl Paper {
    /// A status color as text on this paper: `light` on light paper, `dark` on dark paper.
    pub fn tone(&self, light: [f32; 4], dark: [f32; 4]) -> [f32; 4] {
        readable(
            if self.dark { dark } else { light },
            self.ground,
            self.card,
            4.5,
        )
    }

    /// `color` laid over the ground at `alpha`, as an opaque fill.
    pub fn wash(&self, color: [f32; 4], alpha: f32) -> [f32; 4] {
        let mut c = mix(self.ground, color, alpha);
        c[3] = 1.0;
        c
    }
}

pub fn resolve(theme: Option<&ThemeSnapshot>) -> Paper {
    let env = theme.map(|t| t.paper).unwrap_or_default();
    let (ground, card) = match env.style {
        PaperStyle::Light => (GROUND, CARD),
        PaperStyle::Soft => (srgb(207, 197, 181), srgb(199, 188, 171)),
        PaperStyle::Dark => (srgb(36, 34, 31), srgb(48, 45, 41)),
        PaperStyle::Adaptive => {
            let light = env
                .wallpaper_luminance
                .filter(|v| v.is_finite())
                .unwrap_or_else(|| {
                    if theme.is_some_and(|t| t.text_primary[0] > 0.6) {
                        0.1
                    } else {
                        0.8
                    }
                })
                .clamp(0.0, 1.0);
            // Avoid bright panels on dark desktops; use a stable monitor-wide sample.
            if light < 0.35 {
                let t = (light / 0.35).powi(2);
                (
                    mix(srgb(36, 34, 31), srgb(92, 85, 75), t),
                    mix(srgb(48, 45, 41), srgb(97, 90, 80), t),
                )
            } else if light < 0.65 {
                (srgb(207, 197, 181), srgb(199, 188, 171))
            } else {
                (GROUND, CARD)
            }
        }
    };
    let brightness = if env.brightness.is_finite() {
        env.brightness.clamp(0.6, 1.0)
    } else {
        1.0
    };
    let dim = |mut c: [f32; 4]| {
        for v in &mut c[..3] {
            *v *= brightness;
        }
        c
    };
    let ground = dim(ground);
    let mut card = dim(card);
    // Both surfaces must admit the same readable ink, even near the midtone crossover.
    for _ in 0..8 {
        let black = srgb(0, 0, 0);
        let white = srgb(255, 255, 255);
        if contrast(black, ground).min(contrast(black, card)) >= 4.5
            || contrast(white, ground).min(contrast(white, card)) >= 4.5
        {
            break;
        }
        card = mix(card, ground, 0.5);
    }
    let dark = luminance(ground) < 0.18;
    let ink = if dark { CREAM } else { INK };
    let secondary = if dark {
        srgb(199, 186, 169)
    } else {
        INK_SECONDARY
    };
    let accent_ink = if dark {
        srgb(234, 162, 127)
    } else {
        TERRACOTTA_INK
    };
    let secondary = readable(secondary, ground, card, 4.5);
    Paper {
        dark,
        ground,
        card,
        ink: readable(ink, ground, card, 4.5),
        ink_secondary: secondary,
        ink_faint: if env.style == PaperStyle::Light && brightness == 1.0 {
            INK_FAINT
        } else {
            secondary
        },
        accent: TERRACOTTA,
        accent_ink: readable(accent_ink, ground, card, 4.5),
    }
}

//! Paper remains warm across brightness levels; readable ink is resolved after surfaces.
use super::{Colors, paper, srgb};
use pecofence_plugin_api::{PaperStyle, ThemeSnapshot};

fn mix(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
fn luminance(c: [f32; 4]) -> f32 {
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(c[0]) + 0.7152 * linear(c[1]) + 0.0722 * linear(c[2])
}
fn contrast(a: [f32; 4], b: [f32; 4]) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn readable(ink: [f32; 4], ground: [f32; 4], card: [f32; 4]) -> [f32; 4] {
    let black = srgb(0, 0, 0);
    let white = srgb(255, 255, 255);
    let score = |c| contrast(c, ground).min(contrast(c, card));
    if score(ink) >= 4.5 {
        return ink;
    }
    let target = if score(black) >= score(white) {
        black
    } else {
        white
    };
    for step in 1..=100 {
        let c = mix(ink, target, step as f32 / 100.0);
        if score(c) >= 4.5 {
            return c;
        }
    }
    target
}

pub(super) fn resolve(theme: Option<&ThemeSnapshot>) -> Colors {
    let env = theme.map(|t| t.paper).unwrap_or_default();
    let (ground, card) = match env.style {
        PaperStyle::Light => (paper::GROUND, paper::CARD),
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
                (paper::GROUND, paper::CARD)
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
    let primary = if dark { paper::CREAM } else { paper::INK };
    let secondary = if dark {
        srgb(199, 186, 169)
    } else {
        paper::INK_SECONDARY
    };
    let accent_ink = if dark {
        srgb(234, 162, 127)
    } else {
        paper::TERRACOTTA_INK
    };
    Colors {
        high_contrast: false,
        primary: readable(primary, ground, card),
        secondary: readable(secondary, ground, card),
        accent: paper::TERRACOTTA,
        accent_ink: readable(accent_ink, ground, card),
        subtle: card,
        panel: paper::CREAM,
        faint_ink: if env.style == PaperStyle::Light && brightness == 1.0 {
            paper::INK_FAINT
        } else {
            readable(secondary, ground, card)
        },
        ground: Some(ground),
    }
}
impl Colors {
    pub(crate) fn tone_surface(self, color: [f32; 4]) -> [f32; 4] {
        let Some(ground) = self.ground else {
            return self.subtle;
        };
        if ground == paper::GROUND {
            return color;
        }
        let mut surface = if luminance(ground) < 0.18 {
            mix(self.subtle, color, 0.08)
        } else {
            let ratio = ground[0] / paper::GROUND[0];
            [
                color[0] * ratio,
                color[1] * ratio,
                color[2] * ratio,
                color[3],
            ]
        };
        for _ in 0..12 {
            if contrast(self.primary, surface) >= 4.5 {
                break;
            }
            surface = mix(surface, self.subtle, 0.5);
        }
        surface[3] = color[3];
        surface
    }
    pub(crate) fn paper_ink(self, color: [f32; 4]) -> [f32; 4] {
        if self.high_contrast {
            self.primary
        } else if self.ground == Some(paper::GROUND) {
            color
        } else {
            readable(color, self.ground.unwrap_or(self.subtle), self.subtle)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paper_preferences_keep_text_readable_and_fixed_modes_ignore_wallpaper() {
        for style in [
            PaperStyle::Adaptive,
            PaperStyle::Light,
            PaperStyle::Soft,
            PaperStyle::Dark,
        ] {
            for brightness in [0.6, 0.8, 1.0] {
                for wallpaper in [0.0, 0.1, 0.3, 0.5, 0.8, 1.0] {
                    let mut t = ThemeSnapshot::dark();
                    t.paper = pecofence_plugin_api::PaperEnvironment {
                        style,
                        brightness,
                        wallpaper_luminance: Some(wallpaper),
                    };
                    let c = Colors::paper(Some(&t));
                    for surface in [c.ground.unwrap(), c.subtle] {
                        for ink in [c.primary, c.secondary, c.accent_ink] {
                            assert!(
                                contrast(ink, surface) >= 4.49,
                                "{style:?} {brightness} {wallpaper}"
                            );
                        }
                    }
                    assert!(contrast(c.panel, c.accent) >= 4.5);
                    if style == PaperStyle::Adaptive && wallpaper == 0.0 {
                        assert!(luminance(c.ground.unwrap()) < 0.03);
                    }
                    if style != PaperStyle::Adaptive {
                        t.paper.wallpaper_luminance = Some(1.0 - wallpaper);
                        assert_eq!(c.ground, Colors::paper(Some(&t)).ground);
                    }
                    for tint in [srgb(255, 244, 184), srgb(199, 229, 255)] {
                        let surface = c.tone_surface(tint);
                        assert!(contrast(c.primary, surface) >= 4.49);
                    }
                    t.high_contrast = true;
                    let hc = Colors::paper(Some(&t));
                    assert_eq!(hc.ground, None);
                    assert_eq!(hc.primary, t.text_primary);
                }
            }
        }
    }
}

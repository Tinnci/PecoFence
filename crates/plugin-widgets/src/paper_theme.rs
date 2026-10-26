//! Paper remains warm across brightness levels; readable ink is resolved after surfaces.
use super::{Colors, paper};
use pecofence_plugin_api::ThemeSnapshot;
pub(crate) use pecofence_plugin_api::paper::{contrast, readable};
use pecofence_plugin_api::paper::{luminance, mix};

pub(super) fn resolve(theme: Option<&ThemeSnapshot>) -> Colors {
    let p = pecofence_plugin_api::paper::resolve(theme);
    Colors {
        high_contrast: false,
        primary: p.ink,
        secondary: p.ink_secondary,
        accent: p.accent,
        accent_ink: p.accent_ink,
        subtle: p.card,
        panel: paper::CREAM,
        faint_ink: p.ink_faint,
        ground: Some(p.ground),
    }
}
impl Colors {
    /// Semantic editing surface, shared by TODO and diagram fields. Never use a
    /// raw light-paper swatch for a field in an already resolved dark palette.
    pub(crate) fn input_surface(self) -> [f32; 4] {
        self.tone_surface(paper::SHAPE)
    }

    /// Resolve the final native field as a pair. EDIT paints opaque RGB, so
    /// flatten alpha first and check ink against this exact surface, rather than
    /// against the page/card palette. This is the common boundary for all widgets.
    pub(crate) fn input_palette(self, surface: [f32; 4]) -> ([f32; 4], [f32; 4]) {
        if self.high_contrast {
            return (self.primary, self.subtle);
        }
        let ground = self.ground.unwrap_or(self.subtle);
        let alpha = if surface[3].is_finite() {
            surface[3].clamp(0.0, 1.0)
        } else {
            1.0
        };
        let mut background = ground;
        for i in 0..3 {
            let channel = if surface[i].is_finite() {
                surface[i].clamp(0.0, 1.0)
            } else {
                ground[i]
            };
            background[i] = ground[i] + (channel - ground[i]) * alpha;
        }
        background[3] = 1.0;
        // Leave margin for COLORREF's 8-bit channel rounding.
        (
            readable(self.primary, background, background, 4.6),
            background,
        )
    }

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
            readable(color, self.ground.unwrap_or(self.subtle), self.subtle, 4.5)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecofence_plugin_api::PaperStyle;
    use pecofence_plugin_api::paper::srgb;
    #[test]
    fn native_input_boundary_checks_actual_surface_including_alpha_and_raw_swatches() {
        for style in [
            PaperStyle::Adaptive,
            PaperStyle::Light,
            PaperStyle::Soft,
            PaperStyle::Dark,
        ] {
            for brightness in [0.6, 0.8, 1.0] {
                for wallpaper in [0.0, 0.34, 0.35, 0.64, 0.65, 1.0] {
                    let mut theme = ThemeSnapshot::dark();
                    theme.paper = pecofence_plugin_api::PaperEnvironment {
                        style,
                        brightness,
                        wallpaper_luminance: Some(wallpaper),
                    };
                    let colors = Colors::paper(Some(&theme));
                    for surface in [
                        colors.input_surface(),
                        colors.subtle,
                        paper::SHAPE,
                        srgb(255, 244, 184),
                        [0.9, 0.8, 0.7, 0.2],
                        [f32::NAN; 4],
                    ] {
                        let (ink, background) = colors.input_palette(surface);
                        assert!(background.iter().all(|v| v.is_finite()));
                        assert_eq!(background[3], 1.0);
                        assert!(contrast(ink, background) >= 4.5);
                        // Native EDIT consumes COLORREF, not float colors.
                        let quantize = |c: [f32; 4]| c.map(|v| (v * 255.0).round() / 255.0);
                        assert!(contrast(quantize(ink), quantize(background)) >= 4.5);
                    }
                }
            }
        }
    }

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

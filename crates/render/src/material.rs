//! Material policies. Theme owns colours; the selected strategy owns their use.
use crate::{ColorF, Image, Rect, Theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialKind {
    Fluent,
    LiquidGlass,
    Paper,
    HighContrast,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackdropSource {
    Host,
    Lens,
    Sheet,
    Solid,
}

pub trait Material: Sync {
    fn backdrop_source(&self) -> BackdropSource;
    fn gpu_surface(&self, _theme: &Theme, _seed: f32) -> crate::gpu_glass::Surface {
        crate::gpu_glass::Surface::Glass
    }
    fn corner_radius(&self, theme: &Theme) -> f32 {
        theme.corner_radius
    }
    fn title_fill(&self, theme: &Theme) -> ColorF {
        theme.title_fill
    }
    fn hover_fill(&self, theme: &Theme) -> ColorF {
        theme.subtle_hover
    }
    fn pressed_fill(&self, theme: &Theme) -> ColorF {
        theme.subtle_pressed
    }
    fn control_radius(&self, _theme: &Theme, _rect: Rect) -> f32 {
        4.0
    }
    fn tab_padding(&self) -> f32 {
        16.0
    }
    fn outline_feedback(&self) -> bool {
        false
    }
    fn uses_lens(&self) -> bool {
        false
    }
    fn is_sheet(&self) -> bool {
        false
    }
    fn ink(
        &self,
        theme: Theme,
        _image: &Image,
        _opacity: f32,
        _tint: Option<ColorF>,
        _previous: Option<bool>,
    ) -> Theme {
        theme
    }
}

pub struct Fluent;
pub struct LiquidGlass;
pub struct Paper;
pub struct HighContrast;
impl Material for Fluent {
    fn backdrop_source(&self) -> BackdropSource {
        BackdropSource::Host
    }
}
impl Material for LiquidGlass {
    fn backdrop_source(&self) -> BackdropSource {
        BackdropSource::Lens
    }
    fn uses_lens(&self) -> bool {
        true
    }
    fn tab_padding(&self) -> f32 {
        24.0
    }
    fn control_radius(&self, theme: &Theme, rect: Rect) -> f32 {
        (theme.corner_radius - 4.0)
            .max(0.0)
            .min(rect.width().min(rect.height()) * 0.5)
    }
    fn ink(
        &self,
        theme: Theme,
        image: &Image,
        opacity: f32,
        tint: Option<ColorF>,
        previous: Option<bool>,
    ) -> Theme {
        theme.sample_glass_ink(image, opacity, tint, previous)
    }
}
impl Material for Paper {
    fn gpu_surface(&self, theme: &Theme, seed: f32) -> crate::gpu_glass::Surface {
        crate::gpu_glass::Surface::Paper {
            color: theme.paper_rgb(),
            translucency: theme.paper_translucency(),
            seed,
        }
    }
    fn backdrop_source(&self) -> BackdropSource {
        BackdropSource::Sheet
    }
    fn is_sheet(&self) -> bool {
        true
    }
}
impl Material for HighContrast {
    fn backdrop_source(&self) -> BackdropSource {
        BackdropSource::Solid
    }
    fn outline_feedback(&self) -> bool {
        true
    }
    fn hover_fill(&self, _: &Theme) -> ColorF {
        ColorF::TRANSPARENT
    }
    fn pressed_fill(&self, _: &Theme) -> ColorF {
        ColorF::TRANSPARENT
    }
}
impl MaterialKind {
    pub fn strategy(self) -> &'static dyn Material {
        match self {
            Self::Fluent => &Fluent,
            Self::LiquidGlass => &LiquidGlass,
            Self::Paper => &Paper,
            Self::HighContrast => &HighContrast,
        }
    }
}

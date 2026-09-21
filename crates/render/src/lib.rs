//! Rendering layer: Windows.UI.Composition visual trees with Direct2D-drawn surfaces.
//!
//! The always-resident fence windows are composed by DWM from GPU surfaces; this crate owns
//! the compositor, the shared GPU device and the drawing helpers. Nothing here knows about
//! fences' data model — it draws what the app tells it to.

pub mod canvas;

#[cfg(windows)]
#[allow(
    non_snake_case,
    non_upper_case_globals,
    non_camel_case_types,
    clippy::upper_case_acronyms,
    clippy::missing_transmute_annotations,
    clippy::useless_transmute,
    clippy::too_many_arguments,
    dead_code,
    unused_imports
)]
pub mod comp;

#[cfg(windows)]
pub mod backdrop;
#[cfg(windows)]
pub mod bitmaps;
#[cfg(windows)]
pub mod fence_chrome;
#[cfg(windows)]
#[allow(
    non_snake_case,
    non_upper_case_globals,
    non_camel_case_types,
    clippy::upper_case_acronyms,
    clippy::missing_transmute_annotations,
    clippy::useless_transmute,
    clippy::too_many_arguments,
    dead_code,
    unused_imports
)]
mod gpu_bindings;
#[cfg(windows)]
pub mod gpu_glass;
#[cfg(windows)]
pub mod liquid_glass;
#[cfg(windows)]
pub mod motion;
#[cfg(windows)]
pub mod panel;
#[cfg(windows)]
pub mod stack;
#[cfg(windows)]
pub mod text;
#[cfg(windows)]
pub mod theme;

#[cfg(windows)]
pub use backdrop::{Image, MicaTint, MonitorBackdrop, WallpaperPosition};
#[cfg(windows)]
pub use bitmaps::BitmapCache;
#[cfg(windows)]
pub use canvas::Direct2dCanvas;
pub use canvas::{DrawCommand, HeadlessCanvas};
#[cfg(windows)]
pub use panel::{Clip, Panel};
#[cfg(windows)]
pub use stack::RenderStack;
#[cfg(windows)]
pub use theme::{Theme, ThemeMode};
#[cfg(windows)]
pub use windows_canvas::{
    Bitmap, ColorF, DrawingSession, FontWeight, Matrix3x2, ParagraphAlignment, Rect, RenderTarget,
    RoundedRect, TextAlignment, TextFormat, TextLayout, Vector2, WordWrapping,
};
#[cfg(windows)]
pub use windows_composition::{ContainerVisual, DesktopWindowTarget, SpriteVisual, Visual};
#[cfg(windows)]
pub use windows_core::Result;

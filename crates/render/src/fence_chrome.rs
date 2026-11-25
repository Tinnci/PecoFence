//! Draws the fence "chrome" (backdrop, title bar, divider, stroke) and the icon grid content.
//!
//! Liquid Glass shares its rounded geometry with the compositor clip and header controls.

mod chrome;
mod grid;
mod helpers;
mod rows;
#[cfg(test)]
mod tests;
pub(crate) mod types;
mod typography;

pub use chrome::TitleChrome;
pub use types::{
    Backdrop, BackdropCrop, CaptionButton, ContentDraw, FenceStyle, GLASS_OPACITY, GroupHeaderDraw,
    HeaderColumn, HeaderLabels, ItemCell, ROLLED_COUNT_W, RowCell, RowColumns, RowsDraw,
    ScrollbarDraw, TabDraw, TitleCommandDraw, TitleDeco, TitleState, chevron_angle,
    header_control_radius, header_count_width, tab_text_padding,
};
pub use typography::{DEFAULT_LABEL_LINE_H, FenceChrome, ICON_LABEL_SIDE_INSET};

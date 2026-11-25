use super::types::tab_text_padding;
use crate::theme::{FONT_SMALL, FONT_TEXT, Theme};
use pecofence_plugin_api::design;
use std::cell::RefCell;
use windows_canvas::{
    FontWeight, ParagraphAlignment, TextAlignment, TextFormat, TextLayout, WordWrapping,
};
use windows_core::Result;

/// Icon-label font: the desktop icon-title font (injected from host system parameters)
/// with its measured line height, so the cell grid grows with the text like the desktop's does.
pub(super) struct LabelFont {
    pub(super) format: TextFormat,
    family: String,
    size: f32,
    weight: i32,
    /// Height of one text line in DIPs (16 for the default 12 px).
    pub(super) line_h: f32,
}

/// Line height of the built-in 12 px label font, and the fallback when measuring fails.
pub const DEFAULT_LABEL_LINE_H: f32 = 16.0;
use pecofence_plugin_api::RectDip;
/// Horizontal breathing room inside each icon cell. A wider gutter keeps neighbouring
/// multi-line CJK labels from visually running together in narrow panes.
pub use pecofence_plugin_api::file_geometry::ICON_LABEL_SIDE_INSET;

/// Fence chrome and file-content formats change as one bundle. Geometry reads the measured
/// line heights from this same bundle rather than guessing from the font size.
pub(super) struct FenceTypography {
    pub(super) scale: f32,
    pub(super) row: TextFormat,
    pub(super) titles: [TextFormat; 3],
    pub(super) tabs: [TextFormat; 3],
    pub(super) count: TextFormat,
    pub(super) title_height: f32,
    pub(super) row_right: TextFormat,
    pub(super) group: TextFormat,
    pub(super) empty: TextFormat,
    pub(super) row_line_h: f32,
    pub(super) group_line_h: f32,
    pub(super) empty_line_h: f32,
}

impl FenceTypography {
    fn new(scale: f32) -> Result<Self> {
        let row = TextFormat::new(FONT_SMALL, design::CAPTION * scale)?
            .with_alignment(TextAlignment::Leading)
            .with_paragraph_alignment(ParagraphAlignment::Center)
            .with_word_wrapping(WordWrapping::NoWrap);
        let row_right = TextFormat::new(FONT_SMALL, design::CAPTION * scale)?
            .with_alignment(TextAlignment::Trailing)
            .with_paragraph_alignment(ParagraphAlignment::Center)
            .with_word_wrapping(WordWrapping::NoWrap);
        let group = TextFormat::with_weight(FONT_SMALL, design::CAPTION * scale, FontWeight(600))?
            .with_alignment(TextAlignment::Leading)
            .with_paragraph_alignment(ParagraphAlignment::Center)
            .with_word_wrapping(WordWrapping::NoWrap);
        let empty = TextFormat::new(FONT_TEXT, design::BODY * scale)?
            .with_alignment(TextAlignment::Center)
            .with_paragraph_alignment(ParagraphAlignment::Top)
            .with_word_wrapping(WordWrapping::Wrap);
        let header_format = |face, size, alignment| -> Result<TextFormat> {
            Ok(
                TextFormat::with_weight(face, size * scale, FontWeight(600))?
                    .with_alignment(alignment)
                    .with_paragraph_alignment(ParagraphAlignment::Center)
                    .with_word_wrapping(WordWrapping::NoWrap),
            )
        };
        let header_formats = |alignment| -> Result<[TextFormat; 3]> {
            Ok([
                header_format(FONT_SMALL, design::CAPTION, alignment)?,
                header_format(FONT_TEXT, design::BODY, alignment)?,
                header_format(FONT_TEXT, design::SUBTITLE, alignment)?,
            ])
        };
        let titles = header_formats(TextAlignment::Leading)?;
        let tabs = header_formats(TextAlignment::Center)?;
        let count = TextFormat::new(FONT_SMALL, design::CAPTION * scale)?
            .with_alignment(TextAlignment::Trailing)
            .with_paragraph_alignment(ParagraphAlignment::Center)
            .with_word_wrapping(WordWrapping::NoWrap);
        let line_h = |format: &TextFormat, fallback: f32| {
            TextLayout::new("Ag", format, 10_000.0, 10_000.0)
                .map(|layout| layout.metrics().height.ceil())
                .unwrap_or(fallback * scale)
                .max(1.0)
        };
        let title_height = titles
            .iter()
            .map(|format| line_h(format, 22.0))
            .fold(0.0, f32::max);
        let title_height = ((title_height + 12.0).max(36.0) / 4.0).ceil() * 4.0;
        Ok(Self {
            scale,
            titles,
            tabs,
            count,
            title_height,
            row_line_h: line_h(&row, 16.0),
            group_line_h: line_h(&group, 16.0),
            empty_line_h: line_h(&empty, 20.0),
            row,
            row_right,
            group,
            empty,
        })
    }
}

pub struct FenceChrome {
    pub(super) label_font: RefCell<LabelFont>,
    pub(super) glyph_format: TextFormat,
    pub(super) typography: RefCell<FenceTypography>,
    /// Header sort indicator (8 px chevron).
    pub(super) sort_glyph_format: TextFormat,
    /// 14 px Fluent glyph beside the title (portal folder icon).
    pub(super) title_glyph_format: TextFormat,
    /// 10 px Fluent chrome glyphs of the caption buttons, as Windows draws them.
    pub(super) caption_glyph_format: TextFormat,
}

impl FenceChrome {
    pub fn new() -> Result<Self> {
        Self::with_typography(1.0, None)
    }

    pub fn with_typography(
        text_scale: f32,
        icon_title_font: Option<(&str, f32, FontWeight)>,
    ) -> Result<Self> {
        // WinUI type ramp: Body Strong 14/600 for the title, Caption 12 (Small optical size) for
        // labels and the rolled item count, Body 14 for the empty state. No letter-spacing.
        let label_font = RefCell::new(LabelFont {
            format: Self::build_label_format(FONT_SMALL, design::CAPTION, FontWeight(400))?,
            family: "Segoe UI".to_string(),
            size: 12.0,
            weight: 400,
            line_h: DEFAULT_LABEL_LINE_H,
        });
        let glyph_format = TextFormat::new(crate::theme::FONT_ICONS, 12.0)?
            .with_alignment(TextAlignment::Center)
            .with_paragraph_alignment(ParagraphAlignment::Center)
            .with_word_wrapping(WordWrapping::NoWrap);
        let sort_glyph_format = TextFormat::new(crate::theme::FONT_ICONS, 8.0)?
            .with_alignment(TextAlignment::Center)
            .with_paragraph_alignment(ParagraphAlignment::Center)
            .with_word_wrapping(WordWrapping::NoWrap);
        let title_glyph_format = TextFormat::new(crate::theme::FONT_ICONS, 14.0)?
            .with_alignment(TextAlignment::Center)
            .with_paragraph_alignment(ParagraphAlignment::Center)
            .with_word_wrapping(WordWrapping::NoWrap);
        let caption_glyph_format = TextFormat::new(crate::theme::FONT_ICONS, 10.0)?
            .with_alignment(TextAlignment::Center)
            .with_paragraph_alignment(ParagraphAlignment::Center)
            .with_word_wrapping(WordWrapping::NoWrap);
        let chrome = Self {
            label_font,
            glyph_format,
            typography: RefCell::new(FenceTypography::new(
                pecofence_plugin_api::design::ContentScale::new(RectDip::default(), text_scale)
                    .factor(),
            )?),
            sort_glyph_format,
            title_glyph_format,
            caption_glyph_format,
        };
        // Labels follow the desktop's icon-title font from the start (12 px / 16 stays the
        // fallback when none is supplied).
        if let Some((family, size_dip, weight)) = icon_title_font {
            let _ = chrome.set_icon_title_font(family, size_dip, weight);
        }
        Ok(chrome)
    }

    fn build_label_format(family: &str, size: f32, weight: FontWeight) -> Result<TextFormat> {
        Ok(TextFormat::with_weight(family, size, weight)?
            .with_alignment(TextAlignment::Center)
            .with_paragraph_alignment(ParagraphAlignment::Top)
            .with_word_wrapping(WordWrapping::Wrap))
    }

    /// Adopts the system icon-title font (`family` / `size` DIPs / `weight`) for labels and
    /// re-measures the line height. "Segoe UI" maps to the Small optical size of Segoe UI
    /// Variable so the default keeps its Fluent look; any other family is used verbatim.
    /// Returns whether anything changed (the caller then re-fits labels and re-lays out).
    pub fn set_icon_title_font(&self, family: &str, size: f32, weight: FontWeight) -> Result<bool> {
        let size = size.clamp(8.0, 40.0);
        {
            let cur = self.label_font.borrow();
            if cur.family == family && cur.size == size && cur.weight == weight.0 {
                return Ok(false);
            }
        }
        let face = if family.eq_ignore_ascii_case("Segoe UI") {
            FONT_SMALL
        } else {
            family
        };
        let format = Self::build_label_format(face, size, weight)?;
        let line_h = TextLayout::new("Ag", &format, 10_000.0, 10_000.0)
            .map(|l| l.metrics().height.ceil())
            .unwrap_or(DEFAULT_LABEL_LINE_H)
            .max(1.0);
        *self.label_font.borrow_mut() = LabelFont {
            format,
            family: family.to_string(),
            size,
            weight: weight.0,
            line_h,
        };
        Ok(true)
    }

    /// Label text format (a cheap COM clone; borrow it for one fit / draw).
    pub fn label_format(&self) -> TextFormat {
        self.label_font.borrow().format.clone()
    }

    /// One label line in DIPs for the current icon-title font (16 at the default 12 px).
    pub fn label_line_h(&self) -> f32 {
        self.label_font.borrow().line_h
    }

    pub fn title_format(&self, size: u8) -> TextFormat {
        let index = match size {
            0 => 0,
            2 => 2,
            _ => 1,
        };
        self.typography.borrow().titles[index].clone()
    }

    /// Layout, drawing and tooltip fitting all use the same title size.
    pub fn tab_format(&self, size: u8) -> TextFormat {
        self.typography.borrow().tabs[usize::from(size.min(2))].clone()
    }

    pub fn title_height(&self) -> f32 {
        self.typography.borrow().title_height
    }

    /// Updates chrome and file-content font roles atomically after successful construction.
    pub fn set_text_scale(&self, requested: f32) -> Result<bool> {
        let scale = design::ContentScale::new(RectDip::default(), requested).factor();
        if self.typography.borrow().scale == scale {
            return Ok(false);
        }
        let typography = FenceTypography::new(scale)?;
        *self.typography.borrow_mut() = typography;
        Ok(true)
    }

    pub fn row_format(&self) -> TextFormat {
        self.typography.borrow().row.clone()
    }

    pub fn row_line_h(&self) -> f32 {
        self.typography.borrow().row_line_h
    }

    pub fn group_line_h(&self) -> f32 {
        self.typography.borrow().group_line_h
    }

    /// Does a tab caption fit its pill of width `w` without ellipsis (tooltip decision)?
    pub fn tab_title_fits(&self, title: &str, w: f32, size: u8, theme: &Theme) -> bool {
        crate::text::measure_width(title, &self.tab_format(size))
            <= (w - tab_text_padding(theme)).max(0.0)
    }
}

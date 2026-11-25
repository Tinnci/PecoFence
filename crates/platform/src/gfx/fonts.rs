//! Private DirectWrite font collection for fonts shipped in `fonts/` beside the EXE.
//!
//! windows-canvas only creates text formats against the system collection, so the
//! bundled warm-paper widget fonts (subset Noto Serif SC / Noto Sans SC) are loaded here
//! with an `IDWriteFontSetBuilder1` and never installed system-wide. Formats made from
//! the collection draw and measure through the raw Direct2D / DirectWrite calls below.
//! Characters outside the subset still resolve through DirectWrite's system font
//! fallback; when the folder or a family is missing, callers use system fonts instead.

use crate::bindings::{
    D2D_RECT_F, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_CONDENSED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_METRICS, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, ID2D1Brush, ID2D1RenderTarget,
    IDWriteFactory5, IDWriteFontCollection, IDWriteFontCollection1, IDWriteTextFormat,
    IDWriteTextLayout,
};
use crate::wide::to_wide;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use windows_core::{BOOL, Interface, PCWSTR, Result};

/// Folder beside `pecofence.exe` that packaging fills from `third_party/noto-cjk`.
pub const BUNDLED_FONT_DIR: &str = "fonts";
/// Font files loaded from [`BUNDLED_FONT_DIR`]; any missing file is skipped.
pub const BUNDLED_FONT_FILES: [&str; 2] =
    ["PecoNotoSansSC-Regular.otf", "PecoNotoSerifSC-SemiBold.otf"];

/// GDI faces made available to native controls (the panel text editor) by privately
/// registering the bundled files with GDI for this process only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GdiBundled {
    pub sans: bool,
    pub serif: bool,
}

/// Registers the bundled fonts with GDI (`FR_PRIVATE`: this process only, nothing is
/// installed), once. Native EDIT controls draw through GDI, which cannot see the
/// private DirectWrite collection above.
pub fn register_bundled_for_gdi() -> GdiBundled {
    static REGISTERED: OnceLock<GdiBundled> = OnceLock::new();
    *REGISTERED.get_or_init(|| {
        let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join(BUNDLED_FONT_DIR)))
        else {
            return GdiBundled::default();
        };
        let register = |name: &str| {
            let path = dir.join(name);
            if !path.is_file() {
                return false;
            }
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            // SAFETY: NUL-terminated path that outlives the call; private to this process.
            unsafe {
                crate::bindings::AddFontResourceExW(
                    PCWSTR(wide.as_ptr()),
                    crate::bindings::FR_PRIVATE as u32,
                    None,
                ) > 0
            }
        };
        GdiBundled {
            sans: register(BUNDLED_FONT_FILES[0]),
            serif: register(BUNDLED_FONT_FILES[1]),
        }
    })
}

/// A font collection built from private font files.
pub struct PrivateFonts {
    factory: IDWriteFactory5,
    collection: IDWriteFontCollection1,
}

// SAFETY: objects created from a shared DirectWrite factory are free-threaded; the
// collection is immutable after construction. Same reasoning as windows-canvas's
// process-wide factory.
unsafe impl Send for PrivateFonts {}
// SAFETY: see `Send`; no method mutates the factory or collection.
unsafe impl Sync for PrivateFonts {}

/// Fonts from `fonts/` beside the running EXE, loaded once per process. `None` when the
/// folder holds none of [`BUNDLED_FONT_FILES`] or DirectWrite rejects them.
pub fn bundled() -> Option<&'static PrivateFonts> {
    static BUNDLED: OnceLock<Option<PrivateFonts>> = OnceLock::new();
    BUNDLED
        .get_or_init(|| {
            let dir = std::env::current_exe()
                .ok()?
                .parent()?
                .join(BUNDLED_FONT_DIR);
            let files: Vec<PathBuf> = BUNDLED_FONT_FILES
                .iter()
                .map(|name| dir.join(name))
                .filter(|path| path.is_file())
                .collect();
            if files.is_empty() {
                return None;
            }
            match PrivateFonts::from_files(&files) {
                Ok(fonts) => Some(fonts),
                Err(error) => {
                    tracing::warn!(?dir, %error, "bundled fonts unavailable; using system fonts");
                    None
                }
            }
        })
        .as_ref()
}

impl PrivateFonts {
    /// Builds a collection from font files on disk without registering them anywhere.
    pub fn from_files(paths: &[impl AsRef<Path>]) -> Result<Self> {
        // SAFETY: plain factory creation; the shared factory outlives every object here.
        let factory: IDWriteFactory5 = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };
        // SAFETY: each call passes live interfaces and NUL-terminated paths that outlive it.
        let collection = unsafe {
            let builder = factory.CreateFontSetBuilder()?;
            for path in paths {
                let wide: Vec<u16> = path
                    .as_ref()
                    .as_os_str()
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                let file = factory.CreateFontFileReference(PCWSTR(wide.as_ptr()), None)?;
                builder.AddFontFile(&file).ok()?;
            }
            let set = builder.CreateFontSet()?;
            factory.CreateFontCollectionFromFontSet(&set)?
        };
        Ok(Self {
            factory,
            collection,
        })
    }

    /// Whether the collection holds `family` (typographic family name, e.g. "Noto Sans SC").
    pub fn has_family(&self, family: &str) -> bool {
        let wide = to_wide(family);
        let mut index = 0u32;
        let mut exists = BOOL(0);
        // SAFETY: out-pointers are valid locals; the name is NUL-terminated.
        let hr = unsafe {
            self.collection
                .FindFamilyName(PCWSTR(wide.as_ptr()), &mut index, &mut exists)
        };
        hr.is_ok() && exists.as_bool()
    }

    /// A text format for `family` at `size` DIPs and `weight` (100–900), bound to this
    /// collection. `locale` steers fallback glyph choice for characters outside the files.
    pub fn text_format(
        &self,
        family: &str,
        size: f32,
        weight: i32,
        locale: &str,
        alignment: PrivateTextAlignment,
    ) -> Result<PrivateTextFormat> {
        // SAFETY: strings are NUL-terminated and outlive the call; the collection is live.
        let raw = unsafe {
            let family = to_wide(family);
            let locale = to_wide(locale);
            self.factory.CreateTextFormat(
                PCWSTR(family.as_ptr()),
                &self.collection,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                PCWSTR(locale.as_ptr()),
            )?
        };
        finish_format(self.factory.clone(), raw, alignment)
    }
}

/// A condensed-width format for a system font family (e.g. Bahnschrift), drawn
/// through the same raw path as the bundled fonts. DirectWrite names widths as a
/// stretch of one family ("Bahnschrift"), not as GDI's "Bahnschrift Condensed",
/// and windows-canvas only creates normal-width formats.
pub fn system_condensed_format(
    family: &str,
    size: f32,
    weight: i32,
    locale: &str,
    alignment: PrivateTextAlignment,
) -> Result<PrivateTextFormat> {
    // SAFETY: shared factory; strings are NUL-terminated and outlive the call; a null
    // collection selects the system font collection.
    let (factory, raw) = unsafe {
        let factory: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let family = to_wide(family);
        let locale = to_wide(locale);
        let raw = factory.CreateTextFormat(
            PCWSTR(family.as_ptr()),
            None::<&IDWriteFontCollection>,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_CONDENSED,
            size,
            PCWSTR(locale.as_ptr()),
        )?;
        (factory, raw)
    };
    finish_format(factory, raw, alignment)
}

fn finish_format(
    factory: IDWriteFactory5,
    raw: IDWriteTextFormat,
    alignment: PrivateTextAlignment,
) -> Result<PrivateTextFormat> {
    // SAFETY: setters on a format this function owns; defaults are leading/top.
    unsafe {
        match alignment {
            PrivateTextAlignment::TopLeading => {}
            PrivateTextAlignment::MiddleLeading => {
                raw.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
                    .ok()?;
            }
            PrivateTextAlignment::Center => {
                raw.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER).ok()?;
                raw.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
                    .ok()?;
            }
        }
    }
    if alignment != PrivateTextAlignment::TopLeading {
        single_line_ellipsis(&raw)?;
    }
    Ok(PrivateTextFormat { factory, raw })
}

/// Keep control labels on one line and let DirectWrite render a real ellipsis.
/// Full text remains in the immutable scene for accessibility and model state.
pub fn single_line_ellipsis(format: &impl Interface) -> Result<()> {
    let raw: IDWriteTextFormat = format.cast()?;
    // SAFETY: the shared factory and format are live COM interfaces; the format
    // retains the inline omission sign after SetTrimming returns.
    unsafe {
        let factory: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        raw.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP).ok()?;
        let ellipsis = factory.CreateEllipsisTrimmingSign(&raw)?;
        raw.SetTrimming(
            &DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                ..Default::default()
            },
            &ellipsis,
        )
        .ok()?;
    }
    Ok(())
}

/// Placement of text in its layout box; mirrors the plugin API's `TextAlign`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateTextAlignment {
    TopLeading,
    MiddleLeading,
    Center,
}

/// An `IDWriteTextFormat` bound to a [`PrivateFonts`] collection.
#[derive(Clone)]
pub struct PrivateTextFormat {
    factory: IDWriteFactory5,
    raw: IDWriteTextFormat,
}

impl PrivateTextFormat {
    /// Draws `text` into the layout rectangle on `target` (any `ID2D1RenderTarget`-derived
    /// interface, e.g. the device context behind a windows-canvas session) with `brush`
    /// (any `ID2D1Brush`-derived interface). Matches windows-canvas `draw_text`.
    pub fn draw(
        &self,
        target: &impl Interface,
        text: &str,
        rect: [f32; 4],
        brush: &impl Interface,
    ) -> Result<()> {
        let target: ID2D1RenderTarget = target.cast()?;
        let brush: ID2D1Brush = brush.cast()?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        let [left, top, right, bottom] = rect;
        let rect = D2D_RECT_F {
            left,
            top,
            right,
            bottom,
        };
        // SAFETY: the caller is inside BeginDraw/EndDraw on `target`; every pointer is a
        // live local for the duration of the call.
        unsafe {
            target.DrawText(
                &wide,
                &self.raw,
                &rect,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
        Ok(())
    }

    /// Lays out `text` in a `max_width` × `max_height` box and returns
    /// (width including trailing whitespace, height) in DIPs.
    pub fn measure(&self, text: &str, max_width: f32, max_height: f32) -> Result<(f32, f32)> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut metrics = DWRITE_TEXT_METRICS::default();
        // SAFETY: the layout is created and queried locally; `metrics` is a valid out-pointer.
        unsafe {
            let layout = self
                .factory
                .CreateTextLayout(&wide, &self.raw, max_width, max_height)?;
            layout.GetMetrics(&mut metrics).ok()?;
        }
        Ok((metrics.widthIncludingTrailingWhitespace, metrics.height))
    }

    /// Shapes `text` once in a `max_width` × `max_height` box. The layout can be measured
    /// and then drawn, so both see the same line breaks.
    pub fn layout(&self, text: &str, max_width: f32, max_height: f32) -> Result<PrivateTextLayout> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        // SAFETY: `wide` and the format outlive the call.
        let raw = unsafe {
            self.factory
                .CreateTextLayout(&wide, &self.raw, max_width, max_height)?
        };
        Ok(PrivateTextLayout { raw })
    }
}

/// A shaped `IDWriteTextLayout` over a [`PrivateTextFormat`].
#[derive(Clone)]
pub struct PrivateTextLayout {
    raw: IDWriteTextLayout,
}

impl PrivateTextLayout {
    /// (width including trailing whitespace, height) in DIPs.
    pub fn size(&self) -> Result<(f32, f32)> {
        let mut metrics = DWRITE_TEXT_METRICS::default();
        // SAFETY: `metrics` is a valid out-pointer.
        unsafe { self.raw.GetMetrics(&mut metrics).ok()? };
        Ok((metrics.widthIncludingTrailingWhitespace, metrics.height))
    }

    /// Resizes the layout box. Unchanged width keeps the line breaks; height only moves
    /// vertically aligned text.
    pub fn set_max_size(&self, max_width: f32, max_height: f32) {
        // SAFETY: plain setters on a live layout.
        unsafe {
            let _ = self.raw.SetMaxWidth(max_width);
            let _ = self.raw.SetMaxHeight(max_height);
        }
    }

    /// Draws the layout with its box's top-left at `origin` on `target` (any
    /// `ID2D1RenderTarget`-derived interface) with `brush`. Matches windows-canvas
    /// `draw_text_layout`.
    pub fn draw(
        &self,
        target: &impl Interface,
        origin: [f32; 2],
        brush: &impl Interface,
    ) -> Result<()> {
        let target: ID2D1RenderTarget = target.cast()?;
        let brush: ID2D1Brush = brush.cast()?;
        // SAFETY: the caller is inside BeginDraw/EndDraw on `target`.
        unsafe {
            target.DrawTextLayout(
                windows_numerics::Vector2::new(origin[0], origin[1]),
                &self.raw,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font_files() -> Vec<PathBuf> {
        let dir = std::env::var_os("PECOFENCE_TEST_FONT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/noto-cjk")
            });
        BUNDLED_FONT_FILES
            .iter()
            .map(|name| dir.join(name))
            .collect()
    }

    #[test]
    fn bundled_collection_exposes_the_configured_families() {
        let fonts = PrivateFonts::from_files(&font_files()).unwrap();
        for family in ["Noto Sans SC", "Noto Serif SC"] {
            assert!(fonts.has_family(family), "missing bundled family {family}");
        }
        assert!(!fonts.has_family("PecoFence nonexistent font"));
    }

    #[test]
    fn centered_cjk_headers_stay_single_line_while_note_bodies_wrap() {
        let fonts = PrivateFonts::from_files(&font_files()).unwrap();
        let text = "今天 10月18日 周日 · 农历九月初九 · 重阳节";
        let label = fonts
            .text_format(
                "Noto Sans SC",
                12.0,
                400,
                "zh-cn",
                PrivateTextAlignment::MiddleLeading,
            )
            .unwrap();
        assert_eq!(
            unsafe { label.raw.GetWordWrapping() },
            DWRITE_WORD_WRAPPING_NO_WRAP
        );
        for width in [80.0, 208.0, 368.0] {
            let (_, height) = label.measure(text, width, 20.0).unwrap();
            assert!(height <= 20.0, "single-line caption needs {height} DIPs");
        }
        let title = fonts
            .text_format(
                "Noto Serif SC",
                22.0,
                600,
                "zh-cn",
                PrivateTextAlignment::MiddleLeading,
            )
            .unwrap();
        assert!(title.measure("2026年10月", 200.0, 34.0).unwrap().1 <= 34.0);
        let body = fonts
            .text_format(
                "Noto Sans SC",
                12.0,
                400,
                "zh-cn",
                PrivateTextAlignment::TopLeading,
            )
            .unwrap();
        assert!(body.measure(text, 80.0, 200.0).unwrap().1 > 20.0);
    }

    #[test]
    fn bundled_formats_measure_cjk_text_and_fallback_glyphs() {
        let fonts = PrivateFonts::from_files(&font_files()).unwrap();
        for (family, weight) in [("Noto Sans SC", 400), ("Noto Serif SC", 600)] {
            let format = fonts
                .text_format(
                    family,
                    18.0,
                    weight,
                    "zh-cn",
                    PrivateTextAlignment::TopLeading,
                )
                .unwrap();
            let (width, height) = format.measure("中文 ABC 龘", 1000.0, 1000.0).unwrap();
            assert!(width.is_finite() && width > 0.0);
            assert!(height.is_finite() && height > 0.0);
        }
    }

    #[test]
    fn a_reused_layout_measures_like_a_fresh_one_and_reflows_when_narrowed() {
        let fonts = PrivateFonts::from_files(&font_files()).unwrap();
        let format = fonts
            .text_format(
                "Noto Sans SC",
                14.0,
                400,
                "zh-cn",
                PrivateTextAlignment::TopLeading,
            )
            .unwrap();
        let text = "流程图节点 Node label 中文";
        let layout = format.layout(text, 1000.0, 1000.0).unwrap();
        assert_eq!(
            layout.size().unwrap(),
            format.measure(text, 1000.0, 1000.0).unwrap()
        );
        let (one_line_width, one_line) = layout.size().unwrap();
        layout.set_max_size(one_line_width / 2.0, 1000.0);
        let (width, wrapped) = layout.size().unwrap();
        assert!(
            width <= one_line_width / 2.0 + 0.5,
            "{width} vs {one_line_width}"
        );
        assert!(wrapped > one_line * 1.5, "narrowing wraps onto more lines");
    }

    /// Whether DirectWrite's system collection resolves `family` by name.
    fn system_has_family(family: &str) -> bool {
        // SAFETY: shared factory; out-pointers are valid locals; NUL-terminated name.
        unsafe {
            let factory: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).unwrap();
            let mut collection = None;
            factory
                .GetSystemFontCollection(false, &mut collection, false)
                .ok()
                .unwrap();
            let collection = collection.unwrap();
            let wide = to_wide(family);
            let (mut index, mut exists) = (0u32, BOOL(0));
            collection
                .FindFamilyName(PCWSTR(wide.as_ptr()), &mut index, &mut exists)
                .is_ok()
                && exists.as_bool()
        }
    }

    #[test]
    fn clock_faces_resolve_in_the_system_collection() {
        // DirectWrite names Bahnschrift's widths as stretches of one family: the GDI
        // name "Bahnschrift Condensed" is not a DirectWrite family.
        assert!(system_has_family("Bahnschrift"));
        assert!(!system_has_family("Bahnschrift Condensed"));
        assert!(system_has_family("Segoe UI Variable Display"));
        let condensed = system_condensed_format(
            "Bahnschrift",
            40.0,
            300,
            "en-us",
            PrivateTextAlignment::MiddleLeading,
        )
        .unwrap();
        // SAFETY: shared factory; NUL-terminated strings outlive the call.
        let normal = unsafe {
            let factory: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).unwrap();
            let family = to_wide("Bahnschrift");
            let locale = to_wide("en-us");
            let raw = factory
                .CreateTextFormat(
                    PCWSTR(family.as_ptr()),
                    None::<&IDWriteFontCollection>,
                    300,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    40.0,
                    PCWSTR(locale.as_ptr()),
                )
                .unwrap();
            finish_format(factory, raw, PrivateTextAlignment::MiddleLeading).unwrap()
        };
        let (narrow, _) = condensed.measure("12:34", 1000.0, 100.0).unwrap();
        let (wide, _) = normal.measure("12:34", 1000.0, 100.0).unwrap();
        assert!(
            narrow > 0.0 && narrow < wide * 0.9,
            "condensed {narrow} vs normal {wide}"
        );
    }
}

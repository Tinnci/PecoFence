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
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_METRICS,
    DWriteCreateFactory, ID2D1Brush, ID2D1RenderTarget, IDWriteFactory5, IDWriteFontCollection1,
    IDWriteTextFormat,
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
        let family = to_wide(family);
        let locale = to_wide(locale);
        // SAFETY: strings are NUL-terminated and outlive the call; the collection is live.
        let raw = unsafe {
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
        Ok(PrivateTextFormat {
            factory: self.factory.clone(),
            raw,
        })
    }
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
}

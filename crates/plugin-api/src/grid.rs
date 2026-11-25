//! Widget grid: widgets are sized in spans of the fence icon cell, like iOS/Android
//! widgets occupy home-screen icon slots. Each widget declares its default span and the
//! smallest full span its content needs at [`REFERENCE_CELL`]; the host rounds that
//! content minimum up to whole cells of the live grid.

use crate::Presentation;

/// Outer cell size in DIPs: one fence icon cell (`icon + 32` × `icon + 44`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub w: f32,
    pub h: f32,
}

/// The cell at the default settings (48 px icons, two label lines, 100% text). Widget
/// content minimums are declared against it.
pub const REFERENCE_CELL: Cell = Cell { w: 80.0, h: 96.0 };

const MAX_COLUMNS: u16 = 48;
const MAX_ROWS: u16 = 32;

/// A widget's size in whole cells, title bar included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub columns: u16,
    pub rows: u16,
}

impl Span {
    pub const fn new(columns: u16, rows: u16) -> Self {
        Self { columns, rows }
    }

    /// Outer window size in DIPs.
    pub fn size(self, cell: Cell) -> (f32, f32) {
        (self.columns as f32 * cell.w, self.rows as f32 * cell.h)
    }

    /// One row is the glanceable capsule; anything taller is the full layout.
    pub fn presentation(self) -> Presentation {
        if self.rows <= 1 {
            Presentation::Capsule
        } else {
            Presentation::Compact
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanRules {
    pub default: Span,
    /// Smallest full (two-row-or-taller) span at [`REFERENCE_CELL`] and 100% text.
    pub min_full: Span,
    /// Narrowest one-row capsule at [`REFERENCE_CELL`].
    pub capsule_columns: u16,
}

/// Whole cells needed to hold `dips` (declared at the reference cell) at `text_scale`.
fn cells_for(dips: f32, text_scale: f32, cell: f32) -> u16 {
    let needed = dips * text_scale.max(1.0) / cell.max(1.0);
    // Tolerate float noise so an exact fit is not pushed to the next cell.
    ((needed - 1e-3).ceil().max(1.0) as u16).min(MAX_COLUMNS.max(MAX_ROWS))
}

impl SpanRules {
    /// Smallest full span on the live grid: the content minimum, scaled by the text
    /// size and rounded up to whole cells.
    pub fn min_full(&self, cell: Cell, text_scale: f32) -> Span {
        let (w, h) = self.min_full.size(REFERENCE_CELL);
        Span::new(
            cells_for(w, text_scale, cell.w).min(MAX_COLUMNS),
            cells_for(h, text_scale, cell.h).clamp(2, MAX_ROWS),
        )
    }

    /// Narrowest capsule on the live grid.
    pub fn min_capsule_columns(&self, cell: Cell, text_scale: f32) -> u16 {
        cells_for(
            self.capsule_columns as f32 * REFERENCE_CELL.w,
            text_scale,
            cell.w,
        )
        .min(MAX_COLUMNS)
    }

    /// The nearest span the widget allows: one row is a capsule, heights between the
    /// capsule and the smallest full layout snap to the nearer of the two.
    pub fn clamp(&self, span: Span, cell: Cell, text_scale: f32) -> Span {
        let full = self.min_full(cell, text_scale);
        let rows = span.rows.clamp(1, MAX_ROWS);
        let rows = if rows == 1 || rows >= full.rows {
            rows
        } else if rows * 2 < full.rows + 1 {
            1
        } else {
            full.rows
        };
        let min_columns = if rows == 1 {
            self.min_capsule_columns(cell, text_scale)
        } else {
            full.columns
        };
        Span::new(span.columns.clamp(min_columns, MAX_COLUMNS), rows)
    }

    /// Span for a window size in DIPs (live resize, migration of saved sizes).
    pub fn span_for_size(&self, w: f32, h: f32, cell: Cell, text_scale: f32) -> Span {
        let round = |v: f32, c: f32| (v / c.max(1.0)).round().clamp(1.0, 1000.0) as u16;
        self.clamp(
            Span::new(round(w, cell.w), round(h, cell.h)),
            cell,
            text_scale,
        )
    }

    /// Smallest window the grid allows (the narrowest capsule), for the minimum track size.
    pub fn minimum_size(&self, cell: Cell, text_scale: f32) -> (f32, f32) {
        Span::new(self.min_capsule_columns(cell, text_scale), 1).size(cell)
    }
}

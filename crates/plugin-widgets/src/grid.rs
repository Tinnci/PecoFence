//! Widget grid (docs/WIDGET_GRID.md): widgets are sized in spans of the fence icon
//! cell, like iOS/Android widgets occupy home-screen icon slots. Each widget declares
//! its default span and the smallest full span its content needs at the reference
//! cell; the host rounds that content minimum up to whole cells of the live grid.
//! Platform-free: the host converts DIPs to pixels.

use pecofence_plugin_api::Presentation;

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

/// Grid rules for the first-party widgets; `None` for providers without a grid.
pub fn span_rules(provider: &str) -> Option<SpanRules> {
    let rules = |default: Span, min_full: Span, capsule_columns| SpanRules {
        default,
        min_full,
        capsule_columns,
    };
    Some(match provider {
        crate::CLOCK_PROVIDER => rules(Span::new(4, 2), Span::new(2, 2), 2),
        crate::NOTE_PROVIDER => rules(Span::new(5, 4), Span::new(2, 2), 2),
        crate::TODO_PROVIDER => rules(Span::new(5, 4), Span::new(3, 3), 3),
        crate::CALENDAR_PROVIDER => rules(Span::new(5, 4), Span::new(4, 4), 3),
        crate::DIAGRAM_PROVIDER => rules(Span::new(7, 4), Span::new(4, 3), 3),
        _ => return None,
    })
}

/// Whole cells needed to hold `dips` (declared at the reference cell) at `text_scale`.
fn cells_for(dips: f32, text_scale: f32, cell: f32) -> u16 {
    let needed = dips * text_scale.max(1.0) / cell.max(1.0);
    // Tolerate float noise so an exact fit is not pushed to the next cell.
    ((needed - 1e-3).ceil().max(1.0) as u16).min(MAX_COLUMNS.max(MAX_ROWS))
}

impl SpanRules {
    /// Smallest full span on the live grid: the content minimum, scaled by the text
    /// size and rounded up to whole cells (Android rounds `minWidth` the same way).
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

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [&str; 5] = [
        crate::CLOCK_PROVIDER,
        crate::NOTE_PROVIDER,
        crate::TODO_PROVIDER,
        crate::CALENDAR_PROVIDER,
        crate::DIAGRAM_PROVIDER,
    ];

    #[test]
    fn defaults_map_to_the_documented_sizes_and_are_valid_spans() {
        let sizes: Vec<_> = ALL
            .iter()
            .map(|p| span_rules(p).unwrap().default.size(REFERENCE_CELL))
            .collect();
        assert_eq!(
            sizes,
            [
                (320.0, 192.0),
                (400.0, 384.0),
                (400.0, 384.0),
                (400.0, 384.0),
                (560.0, 384.0)
            ]
        );
        for provider in ALL {
            let rules = span_rules(provider).unwrap();
            assert_eq!(
                rules.clamp(rules.default, REFERENCE_CELL, 1.0),
                rules.default
            );
            assert_eq!(rules.min_full(REFERENCE_CELL, 1.0), rules.min_full);
            // The documented floor: 2 × 1 capsules and 2 × 2 full widgets.
            assert!(rules.capsule_columns >= 2 && rules.min_full.columns >= 2);
            assert!(rules.min_full.rows >= 2);
        }
        assert_eq!(span_rules("pecofence.unknown"), None);
    }

    #[test]
    fn minimums_round_up_with_text_scale_and_small_icons() {
        let todo = span_rules(crate::TODO_PROVIDER).unwrap();
        // 240 × 288 DIP content minimum at 150% needs 360 × 432: 5 × 5 reference cells.
        assert_eq!(todo.min_full(REFERENCE_CELL, 1.5), Span::new(5, 5));
        // 32 px icons give a 64 × 80 cell, so the same content needs more cells.
        let small = Cell { w: 64.0, h: 80.0 };
        assert_eq!(todo.min_full(small, 1.0), Span::new(4, 4));
        assert_eq!(todo.min_capsule_columns(small, 1.0), 4);
        // Text scale below 100% never shrinks the minimum.
        assert_eq!(todo.min_full(REFERENCE_CELL, 0.5), Span::new(3, 3));
    }

    #[test]
    fn heights_between_capsule_and_full_snap_to_the_nearer_one() {
        let calendar = span_rules(crate::CALENDAR_PROVIDER).unwrap(); // full from 4 rows
        let at = |rows| calendar.clamp(Span::new(6, rows), REFERENCE_CELL, 1.0);
        assert_eq!(at(1), Span::new(6, 1));
        assert_eq!(at(2), Span::new(6, 1));
        assert_eq!(at(3), Span::new(6, 4));
        assert_eq!(at(7), Span::new(6, 7));
        // Columns respect the capsule or full minimum for the chosen height.
        assert_eq!(
            calendar.clamp(Span::new(1, 1), REFERENCE_CELL, 1.0),
            Span::new(3, 1)
        );
        assert_eq!(
            calendar.clamp(Span::new(1, 5), REFERENCE_CELL, 1.0),
            Span::new(4, 5)
        );
        assert_eq!(Span::new(3, 1).presentation(), Presentation::Capsule);
        assert_eq!(Span::new(3, 2).presentation(), Presentation::Compact);
    }

    #[test]
    fn saved_pixel_sizes_migrate_to_the_nearest_allowed_span() {
        let note = span_rules(crate::NOTE_PROVIDER).unwrap();
        // The old fixed 420 × 370 note becomes 5 × 4 (400 × 384).
        assert_eq!(
            note.span_for_size(420.0, 370.0, REFERENCE_CELL, 1.0),
            Span::new(5, 4)
        );
        let diagram = span_rules(crate::DIAGRAM_PROVIDER).unwrap();
        assert_eq!(
            diagram.span_for_size(560.0, 400.0, REFERENCE_CELL, 1.0),
            Span::new(7, 4)
        );
        // A tiny or degenerate size still lands on a valid span.
        assert_eq!(
            note.span_for_size(10.0, 0.0, REFERENCE_CELL, 1.0),
            Span::new(2, 1)
        );
        assert_eq!(note.minimum_size(REFERENCE_CELL, 1.0), (160.0, 96.0));
    }
}

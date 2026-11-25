//! Widget grid (docs/WIDGET_GRID.md): widgets are sized in spans of the fence icon
//! cell, like iOS/Android widgets occupy home-screen icon slots. Each widget declares
//! its default span and the smallest full span its content needs at the reference
//! cell; the host rounds that content minimum up to whole cells of the live grid.
//! Platform-free: the host converts DIPs to pixels.

pub use pecofence_plugin_api::grid::{Cell, REFERENCE_CELL, Span, SpanRules};

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

#[cfg(test)]
mod tests {
    use super::*;
    use pecofence_plugin_api::Presentation;

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

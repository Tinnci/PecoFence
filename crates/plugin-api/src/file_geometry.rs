//! File component geometry in surface-local DIPs. Layout, painting and native
//! label editing share these rectangles; monitor DPI is applied only by the host.
use crate::RectDip;

pub const ICON_LABEL_SIDE_INSET: f32 = 8.0;

#[derive(Clone, Copy, Debug)]
pub struct IconMetrics {
    pub size: f32,
    pub top: f32,
    pub label_gap: f32,
    pub line_height: f32,
    pub label_lines: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct FileItemGeometry {
    pub bounds: RectDip,
    pub icon: RectDip,
    pub label: RectDip,
}

impl FileItemGeometry {
    pub fn grid(bounds: RectDip, metrics: IconMetrics) -> Self {
        let icon = RectDip {
            x: bounds.x + (bounds.w - metrics.size) / 2.0,
            y: bounds.y + metrics.top,
            w: metrics.size,
            h: metrics.size,
        };
        let label = RectDip {
            x: bounds.x + ICON_LABEL_SIDE_INSET,
            y: icon.y + metrics.size + metrics.label_gap,
            w: (bounds.w - ICON_LABEL_SIDE_INSET * 2.0).max(1.0),
            h: metrics.line_height * metrics.label_lines,
        };
        Self {
            bounds,
            icon,
            label,
        }
    }

    pub fn row(bounds: RectDip, name_x: f32, name_width: f32, icon_size: f32) -> Self {
        let icon = RectDip {
            x: name_x,
            y: bounds.y + (bounds.h - icon_size) / 2.0,
            w: icon_size,
            h: icon_size,
        };
        let text_x = icon.x + icon_size + 8.0;
        let label = RectDip {
            x: text_x,
            y: bounds.y,
            w: (name_x + name_width - text_x).max(0.0),
            h: bounds.h,
        };
        Self {
            bounds,
            icon,
            label,
        }
    }
}

/// A text band keeps eight DIPs of vertical breathing room, aligned to the
/// existing four-DIP layout grid. Input is a measured DIP line height, not DPI.
pub fn text_band_height(minimum: f32, line_height: f32) -> f32 {
    let line_height = if line_height.is_finite() {
        line_height.max(0.0)
    } else {
        0.0
    };
    ((minimum.max(line_height + 8.0)) / 4.0).ceil() * 4.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_bands_preserve_baselines_and_handle_invalid_measurements() {
        assert_eq!(text_band_height(28.0, 16.0), 28.0);
        assert_eq!(text_band_height(24.0, 16.0), 24.0);
        assert_eq!(text_band_height(28.0, 29.0), 40.0);
        for invalid in [f32::NAN, f32::INFINITY, -1.0] {
            assert_eq!(text_band_height(28.0, invalid), 28.0);
        }
    }

    #[test]
    fn grid_geometry_uses_the_same_label_inset_and_line_height_as_paint() {
        for line_height in [16.0, 24.0, 36.0] {
            let cell = RectDip {
                x: 80.0,
                y: -12.0,
                w: 80.0,
                h: 64.0 + line_height * 2.0,
            };
            let geometry = FileItemGeometry::grid(
                cell,
                IconMetrics {
                    size: 48.0,
                    top: 8.0,
                    label_gap: 4.0,
                    line_height,
                    label_lines: 2.0,
                },
            );
            assert_eq!(geometry.bounds, cell);
            assert_eq!(
                geometry.icon,
                RectDip {
                    x: 96.0,
                    y: -4.0,
                    w: 48.0,
                    h: 48.0
                }
            );
            assert_eq!(
                geometry.label,
                RectDip {
                    x: 88.0,
                    y: 48.0,
                    w: 64.0,
                    h: line_height * 2.0
                }
            );
            assert!(geometry.label.y + geometry.label.h <= cell.y + cell.h);
        }
    }

    #[test]
    fn row_geometry_aligns_the_editor_with_name_text_and_centres_the_icon() {
        for height in [28.0, 40.0, 60.0] {
            let geometry = FileItemGeometry::row(
                RectDip {
                    x: 0.0,
                    y: 12.0,
                    w: 500.0,
                    h: height,
                },
                8.0,
                220.0,
                20.0,
            );
            assert_eq!(geometry.label.x, 36.0);
            assert_eq!(geometry.label.w, 192.0);
            assert_eq!(geometry.label.h, height);
            assert_eq!(geometry.icon.y + geometry.icon.h / 2.0, 12.0 + height / 2.0);
        }
        let narrow = FileItemGeometry::row(
            RectDip {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 28.0,
            },
            8.0,
            10.0,
            20.0,
        );
        assert_eq!(narrow.label.w, 0.0);
    }
}

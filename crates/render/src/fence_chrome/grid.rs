use super::helpers::{
    StateBrushes, draw_drop_wash, draw_group_rule, draw_insert_caret, draw_item_icon,
    draw_item_state, draw_legible_text, draw_scrollbar, empty_text_top,
};
use super::types::{ContentDraw, ItemCell, Px};
use super::typography::{FenceChrome, ICON_LABEL_SIDE_INSET};
use crate::bitmaps::BitmapCache;
use crate::theme::{Theme, with_alpha};
use pecofence_plugin_api::{
    RectDip,
    file_geometry::{FileItemGeometry, IconMetrics},
};
use windows_canvas::{Brush, ColorF, DrawingSession, Rect, RoundedRect};
use windows_core::Result;

impl FenceChrome {
    /// Draws the item grid into a content surface of `width` x `height` DIPs (origin 0,0).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_content(
        &self,
        session: &DrawingSession<'_>,
        theme: &Theme,
        scale: f32,
        bitmaps: &mut BitmapCache,
        width: f32,
        height: f32,
        content: &ContentDraw<'_>,
    ) -> Result<()> {
        let typography = self.typography.borrow();
        session.clear(ColorF::TRANSPARENT);
        let px = Px::new(scale);
        let state = StateBrushes::new(session, theme)?;
        draw_drop_wash(
            session,
            theme,
            &state,
            width,
            height,
            content.drop_highlight,
        );

        let selected = session.create_solid_brush(theme.selection_fill)?;
        let selected_stroke = session.create_solid_brush(theme.selection_stroke)?;
        let label = session.create_solid_brush(theme.text_primary)?;
        let label_dim = session.create_solid_brush(theme.text_secondary)?;

        // "按时间分组" section headers: secondary caption flush with the label inset, then an
        // Explorer-style hairline to the right margin. Static bands (the items glide), painted
        // first so an unfolded label may overdraw them like it overdraws the row below.
        if !content.group_headers.is_empty() {
            let line = session.create_solid_brush(theme.stroke)?;
            // The same local halo the labels get over mixed wallpaper.
            let halo = if theme.material().uses_lens() {
                Some(session.create_solid_brush(if theme.text_secondary.r > 0.5 {
                    ColorF::new(0.0, 0.0, 0.0, 0.28)
                } else {
                    ColorF::new(1.0, 1.0, 1.0, 0.28)
                })?)
            } else {
                None
            };
            let spread = 1.0 / scale.max(1.0);
            let text_x = ICON_LABEL_SIDE_INSET;
            let right = width - 8.0;
            for h in content.group_headers {
                if h.y + h.h <= 0.0 || h.y > height {
                    continue;
                }
                let caption =
                    crate::text::fit_width(h.text, &typography.group, (right - text_x).max(0.0));
                let text_w = crate::text::measure_width(&caption, &typography.group)
                    .min((right - text_x).max(1.0))
                    .max(1.0);
                let rect = Rect::from_xywh(text_x, h.y, text_w, h.h);
                draw_legible_text(
                    session,
                    &caption,
                    &typography.group,
                    &rect,
                    &label_dim,
                    halo.as_ref(),
                    spread,
                );
                draw_group_rule(session, px, h, text_x + text_w, right, &line);
            }
        }

        // The cell whose label unfolds is painted after every other cell: like the desktop's
        // focused icon it overdraws the row below.
        let mut deferred: Option<&ItemCell<'_>> = None;
        for cell in content.items {
            if cell.y + cell.h < 0.0 || cell.y > height {
                continue;
            }
            if cell.full_label.is_some() && deferred.is_none() {
                deferred = Some(cell);
                continue;
            }
            self.draw_cell(
                session, theme, bitmaps, &state, px, content, cell, &label, &label_dim, None,
            )?;
        }
        if let Some(cell) = deferred {
            // The caller measured the unfolded text when it fitted it (`fit_lines_counted`):
            // no second layout per frame here.
            let unfold = cell.full_label.map(|(text, lines)| (text, lines.max(1)));
            self.draw_cell(
                session, theme, bitmaps, &state, px, content, cell, &label, &label_dim, unfold,
            )?;
        }

        if let Some(m) = content.marquee
            && m.right > m.left
            && m.bottom > m.top
        {
            // Plan §6.2: marqueeFill with a 4 px corner radius plus the selection stroke,
            // snapped to whole DIPs so the 1 px stroke stays crisp.
            let m = Rect {
                left: m.left.round(),
                top: m.top.round(),
                right: m.right.round(),
                bottom: m.bottom.round(),
            };
            session.fill_rounded_rect(&RoundedRect::uniform(m, 4.0), &selected);
            session.draw_rounded_rect(&px.ring(m, 4.0), &selected_stroke, px.hair());
        }

        if let Some(r) = content.insert_caret {
            draw_insert_caret(
                session,
                theme,
                &state,
                r,
                f32::MIN,
                content.insert_caret_alpha,
            );
        }

        if let Some(bar) = content.scrollbar {
            draw_scrollbar(session, theme, &self.sort_glyph_format, width, bar)?;
        }

        if let Some(text) = content.empty_text
            && content.items.is_empty()
        {
            let secondary = session.create_solid_brush(theme.text_secondary)?;
            let y0 = empty_text_top(height, typography.empty_line_h);
            session.draw_text(
                text,
                &typography.empty,
                &Rect::from_xywh(12.0, y0, (width - 24.0).max(0.0), (height - y0).max(0.0)),
                &secondary,
            );
        }
        Ok(())
    }

    fn draw_cell(
        &self,
        session: &DrawingSession<'_>,
        theme: &Theme,
        bitmaps: &mut BitmapCache,
        state: &StateBrushes,
        px: Px,
        content: &ContentDraw<'_>,
        cell: &ItemCell<'_>,
        label: &Brush,
        label_dim: &Brush,
        unfold: Option<(&str, u32)>,
    ) -> Result<()> {
        let alpha = cell.alpha.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return Ok(());
        }
        let base_lines = content.label_lines as f32;
        let lines = unfold.map_or(base_lines, |(_, n)| (n as f32).max(base_lines));
        let extra = (lines - base_lines) * content.line_h;
        let rect = Rect::from_xywh(
            cell.x + 2.0,
            cell.y + 2.0,
            cell.w - 4.0,
            cell.h - 4.0 + extra,
        );
        if alpha >= 1.0 {
            draw_item_state(
                session,
                theme,
                state,
                px,
                rect,
                cell.hover,
                cell.selection,
                content.selection_inactive,
                cell.pressed,
                cell.focused,
                cell.drop_target,
            );
        }
        let geometry = FileItemGeometry::grid(
            RectDip {
                x: cell.x,
                y: cell.y,
                w: cell.w,
                h: cell.h,
            },
            IconMetrics {
                size: content.icon_size,
                top: content.icon_top,
                label_gap: content.label_gap,
                line_height: content.line_h,
                label_lines: lines,
            },
        );
        let icon_x = geometry.icon.x;
        let icon_y = geometry.icon.y;
        draw_item_icon(
            session,
            theme,
            state,
            bitmaps,
            cell.icon_key,
            cell.icon,
            cell.failed,
            cell.dimmed,
            alpha,
            cell.icon_alpha,
            icon_x,
            icon_y,
            content.icon_size,
            4.0,
        )?;
        let label_rect = Rect::from_xywh(
            geometry.label.x,
            geometry.label.y,
            geometry.label.w,
            geometry.label.h,
        );
        let text = unfold.map_or(cell.label, |(t, _)| t);
        let resting = if cell.dimmed { label_dim } else { label };
        // A fading cell recolours the scratch brush instead of allocating one per frame.
        let brush = if alpha < 1.0 {
            let c = if cell.dimmed {
                theme.text_secondary
            } else {
                theme.text_primary
            };
            state.a(with_alpha(c, alpha))
        } else {
            resting
        };
        if theme.material().uses_lens() {
            // A local halo protects labels over mixed light/dark wallpaper. It does not
            // frost or darken the glass between icons.
            let halo = if theme.text_primary.r > 0.5 {
                ColorF::new(0.0, 0.0, 0.0, 0.38 * alpha)
            } else {
                ColorF::new(1.0, 1.0, 1.0, 0.55 * alpha)
            };
            let halo = session.create_solid_brush(halo)?;
            for (dx, dy) in [(-0.75, 0.0), (0.75, 0.0), (0.0, -0.75), (0.0, 0.75)] {
                let r = Rect {
                    left: label_rect.left + dx,
                    top: label_rect.top + dy,
                    right: label_rect.right + dx,
                    bottom: label_rect.bottom + dy,
                };
                session.draw_text(text, &self.label_font.borrow().format, &r, &halo);
            }
        }
        session.draw_text(text, &self.label_font.borrow().format, &label_rect, brush);

        Ok(())
    }
}

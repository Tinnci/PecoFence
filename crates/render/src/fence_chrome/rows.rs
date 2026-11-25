use super::helpers::{
    StateBrushes, draw_drop_wash, draw_group_rule, draw_insert_caret, draw_item_icon,
    draw_item_state, draw_legible_text, draw_scrollbar, empty_text_top, header_foreground,
};
use super::types::{Backdrop, HeaderColumn, Px, RowsDraw, header_control_radius};
use super::typography::FenceChrome;
use crate::bitmaps::BitmapCache;
use crate::panel::Clip;
use crate::theme::{Theme, with_alpha};
use pecofence_plugin_api::{RectDip, file_geometry::FileItemGeometry};
use windows_canvas::{ColorF, DrawingSession, Rect, RoundedRect, TextFormat};
use windows_core::Result;

impl FenceChrome {
    /// Draws the List / Details rows into a content surface of `width` x `height` DIPs. The
    /// header (if any) is fixed at the top; `rows` are already offset by the scroll position
    /// and positioned below the header.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_rows(
        &self,
        session: &DrawingSession<'_>,
        clip: Option<&Clip<'_>>,
        theme: &Theme,
        scale: f32,
        bitmaps: &mut BitmapCache,
        width: f32,
        height: f32,
        rows: &RowsDraw<'_>,
        backdrop: Option<(&Backdrop<'_>, f32, Option<ColorF>)>,
    ) -> Result<()> {
        let typography = self.typography.borrow();
        session.clear(ColorF::TRANSPARENT);
        let px = Px::new(scale);
        let state = StateBrushes::new(session, theme)?;
        draw_drop_wash(session, theme, &state, width, height, rows.drop_highlight);
        let selected = session.create_solid_brush(theme.selection_fill)?;
        let selected_stroke = session.create_solid_brush(theme.selection_stroke)?;
        let secondary = session.create_solid_brush(theme.text_secondary)?;
        let cols = rows.columns;
        let header_h = rows.header_h;
        let halo = if theme.material().uses_lens() {
            Some(session.create_solid_brush(ColorF::TRANSPARENT)?)
        } else {
            None
        };
        let spread = 1.0 / scale.max(1.0);
        let paint_text = |text: &str,
                          format: &TextFormat,
                          rect: Rect,
                          key: u128,
                          muted: bool,
                          alpha: f32,
                          right: bool|
         -> Result<()> {
            if rect.width() <= 0.0 || rect.height() <= 0.0 {
                return Ok(());
            }
            let natural_width = crate::text::measure_width(text, format);
            let fitted;
            let (text, measured) = if natural_width <= rect.width() {
                (text, natural_width)
            } else {
                fitted = crate::text::fit_width(text, format, rect.width());
                (fitted.as_str(), crate::text::measure_width(&fitted, format))
            };
            let measured = measured.min(rect.width()).max(1.0);
            let area = Rect::from_xywh(
                if right {
                    rect.right - measured
                } else {
                    rect.left
                },
                rect.top + theme.title_height,
                measured,
                rect.height(),
            );
            let foreground = backdrop.map_or(*theme, |(background, opacity, tint)| {
                header_foreground(background, theme, key, area, scale, opacity, tint)
            });
            let color = if muted {
                foreground.text_secondary
            } else {
                foreground.text_primary
            };
            let brush = state.a(with_alpha(color, alpha));
            if let Some(halo) = &halo {
                halo.set_color(if color.r > 0.5 {
                    ColorF::new(0.0, 0.0, 0.0, 0.28 * alpha)
                } else {
                    ColorF::new(1.0, 1.0, 1.0, 0.28 * alpha)
                });
            }
            let paint = || {
                draw_legible_text(session, text, format, &rect, brush, halo.as_ref(), spread);
                Ok(())
            };
            match clip {
                Some(clip) => clip.scoped(&rect, paint),
                None => paint(),
            }
        };

        let mut paint_rows = || -> Result<()> {
            // Section headers scroll with the rows, under the fixed column header. The caption
            // starts where the 名称 caption does; the hairline runs from its end to the margin.
            if !rows.group_headers.is_empty() {
                let line = session.create_solid_brush(theme.stroke)?;
                let text_x = cols.name_x + 4.0;
                let right = width - 8.0;
                for (i, h) in rows.group_headers.iter().enumerate() {
                    if h.y + h.h <= header_h || h.y > height {
                        continue;
                    }
                    let text_w = crate::text::measure_width(h.text, &typography.group)
                        .min((right - text_x).max(1.0))
                        .max(1.0);
                    paint_text(
                        h.text,
                        &typography.group,
                        Rect::from_xywh(text_x, h.y, text_w, h.h),
                        0x30000 + i as u128,
                        true,
                        1.0,
                        false,
                    )?;
                    draw_group_rule(session, px, h, text_x + text_w, right, &line);
                }
            }
            let mut visible_slot = 0u128;
            for row in rows.rows {
                if row.y + row.h <= header_h || row.y > height {
                    continue;
                }
                let alpha = row.alpha.clamp(0.0, 1.0);
                if alpha <= 0.0 {
                    continue;
                }
                let rect = Rect::from_xywh(4.0, row.y + 1.0, (width - 8.0).max(0.0), row.h - 2.0);
                if alpha >= 1.0 {
                    draw_item_state(
                        session,
                        theme,
                        &state,
                        px,
                        rect,
                        row.hover,
                        row.selection,
                        rows.selection_inactive,
                        row.pressed,
                        row.focused,
                        row.drop_target,
                    );
                }
                let geometry = FileItemGeometry::row(
                    RectDip {
                        x: 0.0,
                        y: row.y,
                        w: width,
                        h: row.h,
                    },
                    cols.name_x,
                    cols.name_w,
                    rows.icon_size,
                );
                let icon_x = geometry.icon.x;
                let icon_y = geometry.icon.y;
                draw_item_icon(
                    session,
                    theme,
                    &state,
                    bitmaps,
                    row.icon_key,
                    row.icon,
                    row.failed,
                    row.dimmed,
                    alpha,
                    row.icon_alpha,
                    icon_x,
                    icon_y,
                    rows.icon_size,
                    3.0,
                )?;
                let text_x = geometry.label.x;
                let name_w = geometry.label.w;
                // Contrast follows each visible text cell, not the average of the whole
                // fence. Stable viewport slots bound the hysteresis cache while scrolling.
                let key = 0x10000 + visible_slot * 8;
                paint_text(
                    row.name,
                    &typography.row,
                    Rect::from_xywh(text_x, row.y, name_w, row.h),
                    key,
                    row.dimmed,
                    alpha,
                    false,
                )?;
                if let Some((x, w)) = cols.date {
                    paint_text(
                        row.date,
                        &typography.row,
                        Rect::from_xywh(x, row.y, w, row.h),
                        key + 1,
                        true,
                        alpha,
                        false,
                    )?;
                }
                if let Some((x, w)) = cols.type_ {
                    paint_text(
                        row.type_name,
                        &typography.row,
                        Rect::from_xywh(x, row.y, w, row.h),
                        key + 2,
                        true,
                        alpha,
                        false,
                    )?;
                }
                if let Some((x, w)) = cols.size {
                    paint_text(
                        row.size,
                        &typography.row_right,
                        Rect::from_xywh(x, row.y, w, row.h),
                        key + 3,
                        true,
                        alpha,
                        true,
                    )?;
                }
                visible_slot += 1;
            }
            Ok(())
        };
        match clip {
            Some(clip) if header_h > 0.0 => {
                // A row scrolled partly under the fixed header must not paint through its
                // captions. The header has no opaque fill (the surface is transparent over the
                // backdrop), so clip the rows to the band below it instead.
                clip.scoped(
                    &Rect::from_xywh(0.0, header_h, width, (height - header_h).max(0.0)),
                    paint_rows,
                )?;
            }
            _ => paint_rows()?,
        }

        // Fixed header over the rows (Details only): column captions (all secondary, like
        // Explorer's header), a full-width hairline underneath and the sort chevron centred at
        // the top of the sorted column.
        if header_h > 0.0 {
            let line = session.create_solid_brush(theme.stroke)?;
            let hair = px.hair();
            // Column dividers (grab handles for resizing) in the gutter left of each column.
            for col in [cols.date, cols.type_, cols.size].into_iter().flatten() {
                let (y0, y1) = (px.snap(6.0), px.snap(header_h - 6.0));
                session.fill_rect(
                    &Rect::from_xywh(px.snap(col.0 - 4.5), y0, hair, (y1 - y0).max(0.0)),
                    &line,
                );
            }
            session.fill_rect(
                &Rect::from_xywh(0.0, px.snap(header_h) - hair, width, hair),
                &line,
            );
            let mut captions: Vec<(f32, f32, &str, HeaderColumn, bool)> = Vec::new();
            captions.push((
                cols.name_x + 4.0,
                cols.name_w - 4.0,
                rows.header_labels.name,
                HeaderColumn::Name,
                false,
            ));
            if let Some((x, w)) = cols.date {
                captions.push((x, w, rows.header_labels.date, HeaderColumn::Date, false));
            }
            if let Some((x, w)) = cols.type_ {
                captions.push((x, w, rows.header_labels.type_, HeaderColumn::Type, false));
            }
            if let Some((x, w)) = cols.size {
                captions.push((x, w, rows.header_labels.size, HeaderColumn::Size, true));
            }
            for (index, (x, w, text, col, right)) in captions.into_iter().enumerate() {
                let cell_rect = Rect::from_xywh(x - 4.0, 2.0, w + 8.0, header_h - 6.0);
                let cell = RoundedRect::uniform(cell_rect, header_control_radius(theme, cell_rect));
                let hover = rows
                    .header_hover
                    .iter()
                    .find(|(c, _)| *c == col)
                    .map_or(0.0, |(_, a)| *a);
                if rows.header_pressed == Some(col) {
                    let b = session.create_solid_brush(theme.material().pressed_fill(theme))?;
                    session.fill_rounded_rect(&cell, &b);
                } else if hover > 0.0 {
                    let b = session.create_solid_brush(with_alpha(
                        theme.material().hover_fill(theme),
                        hover,
                    ))?;
                    session.fill_rounded_rect(&cell, &b);
                }
                let fmt = if right {
                    &typography.row_right
                } else {
                    &typography.row
                };
                paint_text(
                    text,
                    fmt,
                    Rect::from_xywh(x, 0.0, w, header_h - 1.0),
                    0x20000 + index as u128,
                    true,
                    1.0,
                    right,
                )?;
                if rows.sort_column == Some(col) {
                    // 8 px chevron flush with the header top, horizontally centred in the cell
                    // (the Win32 header control's sort arrow), same grey as the captions.
                    let glyph = if rows.sort_descending {
                        "\u{E70D}"
                    } else {
                        "\u{E70E}"
                    };
                    paint_text(
                        glyph,
                        &self.sort_glyph_format,
                        Rect::from_xywh(x + (w - 12.0) / 2.0, 0.0, 12.0, 10.0),
                        0x20010 + index as u128,
                        true,
                        1.0,
                        false,
                    )?;
                }
            }
        }

        if let Some(m) = rows.marquee
            && m.right > m.left
            && m.bottom > m.top
        {
            let m = Rect {
                left: m.left.round(),
                top: m.top.round().max(header_h),
                right: m.right.round(),
                bottom: m.bottom.round(),
            };
            if m.bottom > m.top {
                session.fill_rounded_rect(&RoundedRect::uniform(m, 4.0), &selected);
                session.draw_rounded_rect(&px.ring(m, 4.0), &selected_stroke, px.hair());
            }
        }

        if let Some(r) = rows.insert_caret {
            draw_insert_caret(session, theme, &state, r, header_h, rows.insert_caret_alpha);
        }

        if let Some(bar) = rows.scrollbar {
            draw_scrollbar(session, theme, &self.sort_glyph_format, width, bar)?;
        }

        if let Some(text) = rows.empty_text
            && rows.rows.is_empty()
        {
            let avail = (height - header_h).max(0.0);
            let y0 = empty_text_top(avail, typography.empty_line_h);
            session.draw_text(
                text,
                &typography.empty,
                &Rect::from_xywh(
                    12.0,
                    header_h + y0,
                    (width - 24.0).max(0.0),
                    (avail - y0).max(0.0),
                ),
                &secondary,
            );
        }
        Ok(())
    }
}

use super::types::{
    Backdrop, GroupHeaderDraw, HOT_SELECTED_LIFT, Px, ScrollbarDraw, header_control_radius,
};
use crate::Image;
use crate::bitmaps::BitmapCache;
use crate::theme::{Theme, lerp, with_alpha};
use pecofence_plugin_api::design;
use windows_canvas::{
    Brush, ColorF, DrawingSession, GradientStop, Matrix3x2, ParagraphAlignment, Rect, RoundedRect,
    TextAlignment, TextFormat, Vector2, WordWrapping,
};
use windows_core::Result;

pub(super) fn draw_header_glass(
    session: &DrawingSession<'_>,
    backdrop: &Backdrop<'_>,
    theme: &Theme,
    scale: f32,
    key: u128,
    rect: Rect,
    amount: f32,
    alpha: f32,
) -> Result<()> {
    if alpha <= 0.0 || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return Ok(());
    }
    let radius = header_control_radius(theme, rect);
    if let Backdrop::GpuGlass { material, .. } = backdrop {
        material
            .borrow_mut()
            .draw_control(session, key, rect, scale, radius, amount * alpha)?;
    }
    let white = ColorF::new(1.0, 1.0, 1.0, 1.0);
    // A thin optical wash, not the opaque LayerOnAcrylic fill used by Fluent tabs.
    let wash = session.create_solid_brush(with_alpha(white, (0.018 + 0.075 * amount) * alpha))?;
    session.fill_rounded_rect(&RoundedRect::uniform(rect, radius), &wash);
    let rim = session.create_linear_gradient(
        Vector2::new(rect.left, rect.top),
        Vector2::new(rect.right, rect.bottom),
        &[
            GradientStop::new(0.0, with_alpha(white, (0.18 + 0.43 * amount) * alpha)),
            GradientStop::new(0.4, with_alpha(white, (0.04 + 0.08 * amount) * alpha)),
            GradientStop::new(
                0.65,
                ColorF::new(0.06, 0.12, 0.16, (0.02 + 0.06 * amount) * alpha),
            ),
            GradientStop::new(1.0, with_alpha(white, (0.08 + 0.21 * amount) * alpha)),
        ],
    )?;
    let px = Px::new(scale);
    session.draw_rounded_rect(&px.ring(rect, radius), &rim, px.hair());
    Ok(())
}

pub(super) fn header_foreground(
    backdrop: &Backdrop<'_>,
    theme: &Theme,
    key: u128,
    area: Rect,
    scale: f32,
    opacity: f32,
    tint: Option<ColorF>,
) -> Theme {
    if let Backdrop::GpuGlass {
        material,
        wallpaper,
        rect,
        ..
    } = backdrop
    {
        return material.borrow_mut().control_foreground(
            key,
            *theme,
            wallpaper,
            [
                rect[0] + (area.left * scale).round() as i32,
                rect[1] + (area.top * scale).round() as i32,
                (area.width() * scale).round().max(1.0) as i32,
                (area.height() * scale).round().max(1.0) as i32,
            ],
            opacity,
            tint,
        );
    }
    *theme
}

pub(super) fn draw_legible_text(
    session: &DrawingSession<'_>,
    text: &str,
    format: &TextFormat,
    rect: &Rect,
    brush: &Brush,
    halo: Option<&Brush>,
    spread: f32,
) {
    if let Some(halo) = halo {
        let shifted = Rect {
            top: rect.top + spread,
            bottom: rect.bottom + spread,
            ..*rect
        };
        session.draw_text(text, format, &shifted, halo);
    }
    session.draw_text(text, format, rect, brush);
}

/// The hairline of a "按时间分组" section header: from 8 DIPs after the caption's end
/// (`text_end`) to `right`, one device pixel, vertically centred on the band.
pub(super) fn draw_group_rule(
    session: &DrawingSession<'_>,
    px: Px,
    header: &GroupHeaderDraw<'_>,
    text_end: f32,
    right: f32,
    line: &Brush,
) {
    let x0 = px.snap(text_end + 8.0);
    let x1 = px.snap(right);
    if x1 > x0 {
        let y = px.snap(header.y + header.h / 2.0);
        session.fill_rect(&Rect::from_xywh(x0, y, x1 - x0, px.hair()), line);
    }
}

/// The brushes an item-state pass needs, created once per content draw. The per-item alphas
/// only recolour them (`ID2D1SolidColorBrush::SetColor`, no allocation), so a selection fade
/// over hundreds of items costs no COM objects per frame.
pub(super) struct StateBrushes {
    /// Recoloured per item: subtle (hover / pressed) fill, selection fill and stroke.
    hover: Brush,
    sel_fill: Brush,
    sel_stroke: Brush,
    /// Fixed colours.
    focus_outer: Brush,
    focus_inner: Brush,
    /// Scratch brushes recoloured per use (never two of the same at once): faded label text
    /// (primary / secondary), the icon loading tile, the drop wash + ring and the insert
    /// caret. Every colour that varies per frame or per item goes through these, so 30 items
    /// fading in cost no brush allocations per frame.
    scratch_a: Brush,
    scratch_b: Brush,
}

impl StateBrushes {
    pub(super) fn new(session: &DrawingSession<'_>, theme: &Theme) -> Result<Self> {
        Ok(Self {
            hover: session.create_solid_brush(theme.material().hover_fill(theme))?,
            sel_fill: session.create_solid_brush(theme.selection_fill)?,
            sel_stroke: session.create_solid_brush(theme.selection_stroke)?,
            focus_outer: session.create_solid_brush(theme.focus_outer)?,
            focus_inner: session.create_solid_brush(theme.focus_inner)?,
            scratch_a: session.create_solid_brush(theme.text_primary)?,
            scratch_b: session.create_solid_brush(theme.text_secondary)?,
        })
    }

    /// `scratch_a` recoloured to `c`.
    pub(super) fn a(&self, c: ColorF) -> &Brush {
        self.scratch_a.set_color(c);
        &self.scratch_a
    }

    /// `scratch_b` recoloured to `c`.
    pub(super) fn b(&self, c: ColorF) -> &Brush {
        self.scratch_b.set_color(c);
        &self.scratch_b
    }
}

/// Native controls use the same inside-aligned outline as scene controls.
pub(super) fn draw_contrast_outline(
    session: &DrawingSession<'_>,
    theme: &Theme,
    px: Px,
    rect: Rect,
    strong: bool,
) -> Result<()> {
    let ink = theme.text_primary;
    if let Some((r, stroke)) = design::contrast_outline(
        pecofence_plugin_api::RectDip {
            x: rect.left,
            y: rect.top,
            w: rect.width(),
            h: rect.height(),
        },
        px.grid,
        [ink.r, ink.g, ink.b, ink.a],
        strong,
    ) {
        let brush = session.create_solid_brush(ink)?;
        session.draw_rect(&Rect::from_xywh(r.x, r.y, r.w, r.h), &brush, stroke.width);
    }
    Ok(())
}

/// Paints one item's state layers (selection, hover, pressed, focus ring) under its content.
/// Precedence: a drop target is an accent selection (its alpha fades in over 83 ms and wins
/// over a fading selection); selection and hover blend by alpha so a fading selection still
/// shows the hover beneath it. WinUI ListViewItem states: pressed swaps the subtle layer from
/// SubtleFillColorSecondary to SubtleFillColorTertiary (Pressed *replaces* PointerOver, so the
/// surface recedes rather than darkens); a selected item shows only its selection while
/// pressed (SelectedPressed = Selected); a hovered selected item lifts its selection fill
/// (SelectedPointerOver / LISS_HOTSELECTED), riding the 83 ms hover fade.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_item_state(
    session: &DrawingSession<'_>,
    theme: &Theme,
    brushes: &StateBrushes,
    px: Px,
    rect: Rect,
    hover: f32,
    selection: f32,
    inactive: f32,
    pressed: bool,
    focused: bool,
    drop_target: f32,
) {
    let drop = drop_target.clamp(0.0, 1.0);
    let sel = selection.max(drop);
    if theme.material().outline_feedback() {
        if hover > 0.0 || sel > 0.0 || pressed {
            let ink = theme.text_primary;
            if let Some((r, stroke)) = design::contrast_outline(
                pecofence_plugin_api::RectDip {
                    x: rect.left,
                    y: rect.top,
                    w: rect.width(),
                    h: rect.height(),
                },
                px.grid,
                [ink.r, ink.g, ink.b, ink.a],
                pressed || sel > 0.0,
            ) {
                session.draw_rect(
                    &Rect::from_xywh(r.x, r.y, r.w, r.h),
                    brushes.a(ink),
                    stroke.width,
                );
            }
        }
    } else {
        // One subtle layer: PointerOver = SubtleFillColorSecondary, Pressed = SubtleFillColorTertiary
        // (an instant colour swap of the same layer; its alpha keeps following the hover fade).
        // Kept under a partial selection so the two fades never pop.
        let subtle_a = hover.clamp(0.0, 1.0) * (1.0 - sel);
        if subtle_a > 0.0 {
            let subtle = if pressed {
                theme.material().pressed_fill(theme)
            } else {
                theme.material().hover_fill(theme)
            };
            brushes.hover.set_color(with_alpha(subtle, subtle_a));
            session.fill_rounded_rect(&RoundedRect::uniform(rect, 4.0), &brushes.hover);
        }
        if sel > 0.0 {
            // Accent while this fence is the active window; neutral grey otherwise (drop targets
            // stay accent — drag feedback is not selection).
            let t = inactive * (1.0 - drop);
            let fill = lerp(theme.selection_fill, theme.selection_fill_inactive, t);
            // Hot-selected: the fill strengthens under the pointer (drag hover is already 0).
            let hot = hover.clamp(0.0, 1.0) * selection.clamp(0.0, 1.0);
            let fill = ColorF {
                a: (fill.a + HOT_SELECTED_LIFT * hot).min(1.0),
                ..fill
            };
            let stroke = lerp(theme.selection_stroke, theme.selection_stroke_inactive, t);
            brushes.sel_fill.set_color(with_alpha(fill, sel));
            brushes.sel_stroke.set_color(with_alpha(stroke, sel));
            session.fill_rounded_rect(&RoundedRect::uniform(rect, 4.0), &brushes.sel_fill);
            session.draw_rounded_rect(&px.ring(rect, 4.0), &brushes.sel_stroke, px.hair());
        }
    }
    if focused {
        // FocusVisual: 2 px outer stroke on the cell edge, 1 px inner stroke just inside it.
        let o = Rect::from_xywh(
            rect.left + 1.0,
            rect.top + 1.0,
            (rect.right - rect.left - 2.0).max(0.0),
            (rect.bottom - rect.top - 2.0).max(0.0),
        );
        session.draw_rounded_rect(&RoundedRect::uniform(o, 3.0), &brushes.focus_outer, 2.0);
        let i = Rect::from_xywh(
            rect.left + 2.0,
            rect.top + 2.0,
            (rect.right - rect.left - 4.0).max(0.0),
            (rect.bottom - rect.top - 4.0).max(0.0),
        );
        session.draw_rounded_rect(&px.ring(i, 2.0), &brushes.focus_inner, px.hair());
    }
}

/// Fence-wide drop feedback (a drag hovers the fence itself), faded by `alpha`: the accent
/// wash (plan §6.2 dropWash, accent @ 0x14) filled inside the 1 px rim with rounded bottom
/// corners concentric with the DWM window corner (the top edge abuts the title row, so it stays
/// straight — the rounded top corners are pushed above the surface), plus the 2 px opaque
/// accent ring (dropRing) one hairline inside the rim, again concentric with it. The rim
/// pixels themselves stay untinted.
pub(super) fn draw_drop_wash(
    session: &DrawingSession<'_>,
    theme: &Theme,
    brushes: &StateBrushes,
    width: f32,
    height: f32,
    alpha: f32,
) {
    if alpha <= 0.0 {
        return;
    }
    let alpha = alpha.min(1.0);
    let r = (theme.corner_radius - 1.0).max(0.0);
    session.fill_rounded_rect(
        &RoundedRect::uniform(
            Rect::from_xywh(1.0, -r, (width - 2.0).max(0.0), (height - 1.0 + r).max(0.0)),
            r,
        ),
        brushes.a(with_alpha(theme.drop_highlight, alpha)),
    );
    // 2 px stroke centred 2 DIP inside the edge: it spans 1..3 DIP, so its outer edge touches
    // the rim's inner edge and its centreline radius is the window radius minus 2.
    session.draw_rounded_rect(
        &RoundedRect::uniform(
            Rect::from_xywh(2.0, 2.0, (width - 4.0).max(0.0), (height - 4.0).max(0.0)),
            (theme.corner_radius - 2.0).max(0.0),
        ),
        brushes.b(with_alpha(theme.accent, alpha)),
        2.0,
    );
}

/// Insertion caret (2 DIP accent bar) at `r`, faded by `alpha`; rows clip it to `min_top`.
pub(super) fn draw_insert_caret(
    session: &DrawingSession<'_>,
    theme: &Theme,
    brushes: &StateBrushes,
    r: Rect,
    min_top: f32,
    alpha: f32,
) {
    if alpha <= 0.0 {
        return;
    }
    let r = Rect {
        left: r.left.round(),
        top: r.top.round().max(min_top),
        right: r.right.round(),
        bottom: r.bottom.round(),
    };
    if r.bottom > r.top {
        session.fill_rounded_rect(
            &RoundedRect::uniform(r, 1.0),
            brushes.a(with_alpha(theme.selection_stroke, alpha.min(1.0))),
        );
    }
}

/// Icon box of an item: the bitmap (aspect-preserved inside `size`), cross-faded over the
/// loading tile while `icon_alpha` < 1; a generic document glyph when extraction failed;
/// the tile alone while the icon is still pending. `alpha` fades the whole thing.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_item_icon(
    session: &DrawingSession<'_>,
    theme: &Theme,
    brushes: &StateBrushes,
    bitmaps: &mut BitmapCache,
    icon_key: &str,
    icon: Option<&Image>,
    failed: bool,
    dimmed: bool,
    alpha: f32,
    icon_alpha: f32,
    x: f32,
    y: f32,
    size: f32,
    tile_radius: f32,
) -> Result<()> {
    let tile = |a: f32| {
        if a <= 0.0 {
            return;
        }
        session.fill_rounded_rect(
            &RoundedRect::uniform(Rect::from_xywh(x, y, size, size), tile_radius),
            brushes.a(with_alpha(theme.material().pressed_fill(theme), a)),
        );
    };
    if let Some(img) = icon {
        let icon_alpha = icon_alpha.clamp(0.0, 1.0);
        if icon_alpha < 1.0 && !failed {
            tile(alpha * (1.0 - icon_alpha));
        }
        let bmp = bitmaps.get_or_upload(session, icon_key, img)?;
        // Preserve aspect ratio inside the icon box (thumbnails are not square).
        let (iw, ih) = (img.width as f32, img.height as f32);
        let k = (size / iw).min(size / ih);
        let (dw, dh) = (iw * k, ih * k);
        let dest = Rect::from_xywh(x + (size - dw) / 2.0, y + (size - dh) / 2.0, dw, dh);
        let dim = if dimmed { 0.5 } else { 1.0 };
        session.draw_bitmap(&bmp, &dest, dim * alpha * icon_alpha);
    } else if failed {
        // No icon obtainable (broken shortcut, missing handler): a generic document glyph
        // from Segoe Fluent Icons instead of a permanent grey tile.
        let glyph_format = TextFormat::new(crate::theme::FONT_ICONS, size * 0.72)?
            .with_alignment(TextAlignment::Center)
            .with_paragraph_alignment(ParagraphAlignment::Center)
            .with_word_wrapping(WordWrapping::NoWrap);
        session.draw_text(
            "\u{E7C3}",
            &glyph_format,
            &Rect::from_xywh(x, y, size, size),
            brushes.a(with_alpha(theme.text_secondary, alpha)),
        );
    } else {
        // Placeholder while the icon loads.
        tile(alpha);
    }
    Ok(())
}

/// Overlay scrollbar (Windows 11 / WinUI ScrollBar). Collapsed: a ControlStrong 2 DIP pill.
/// Expanded (`parts` > 0): the 12 DIP track fill (LayerFill stands in for the in-app acrylic)
/// with an arrow RepeatButton at each end — 8 px caret in ControlStrong, TextSecondary when
/// hovered / pressed, SubtleFill hover pill, 0.875 glyph scale while pressed — and the thumb
/// widened to 6 DIP on top. Widths, alphas and the parts reveal come from the caller's tweens.
pub(super) fn draw_scrollbar(
    session: &DrawingSession<'_>,
    theme: &Theme,
    arrow_format: &TextFormat,
    width: f32,
    bar: ScrollbarDraw,
) -> Result<()> {
    if bar.alpha <= 0.0 || bar.width <= 0.0 {
        return Ok(());
    }
    let parts = (bar.parts.clamp(0.0, 1.0) * bar.alpha).clamp(0.0, 1.0);
    if parts > 0.0 && bar.band_h > 0.0 {
        let x = width - ScrollbarDraw::BAR_W;
        let track = Rect::from_xywh(x, bar.band_top, ScrollbarDraw::BAR_W, bar.band_h);
        let fill = session.create_solid_brush(with_alpha(theme.layer_fill, parts))?;
        session.fill_rounded_rect(
            &RoundedRect::uniform(track, ScrollbarDraw::BAR_W / 2.0),
            &fill,
        );
        let boxes = [
            Rect::from_xywh(
                x,
                bar.band_top,
                ScrollbarDraw::BAR_W,
                ScrollbarDraw::BUTTON_H,
            ),
            Rect::from_xywh(
                x,
                bar.band_top + bar.band_h - ScrollbarDraw::BUTTON_H,
                ScrollbarDraw::BAR_W,
                ScrollbarDraw::BUTTON_H,
            ),
        ];
        let glyphs = [ScrollbarDraw::ARROW_UP, ScrollbarDraw::ARROW_DOWN];
        for i in 0..2 {
            let box_r = boxes[i];
            let hover = bar.arrow_hover[i].clamp(0.0, 1.0);
            let pressed = bar.arrow_pressed[i];
            if pressed {
                // WinUI ScrollBarButtonBackgroundPressed is SubtleFillColorTertiary (the plan's
                // ControlStrong is the glyph colour, not the plate).
                let bg = session
                    .create_solid_brush(with_alpha(theme.material().pressed_fill(theme), parts))?;
                session.fill_rounded_rect(&RoundedRect::uniform(box_r, 4.0), &bg);
            } else if hover > 0.0 {
                let bg = session.create_solid_brush(with_alpha(
                    theme.material().hover_fill(theme),
                    hover * parts,
                ))?;
                session.fill_rounded_rect(&RoundedRect::uniform(box_r, 4.0), &bg);
            }
            let hot = if pressed { 1.0 } else { hover };
            let color = lerp(theme.control_strong, theme.text_secondary, hot);
            let glyph = session.create_solid_brush(with_alpha(color, parts))?;
            if pressed {
                let saved = session.transform();
                let centre = Vector2::new(
                    box_r.left + box_r.width() / 2.0,
                    box_r.top + box_r.height() / 2.0,
                );
                let s = ScrollbarDraw::ARROW_SCALE_PRESSED;
                session.set_transform(&(Matrix3x2::scale_around(s, s, centre) * saved));
                session.draw_text(glyphs[i], arrow_format, &box_r, &glyph);
                session.set_transform(&saved);
            } else {
                session.draw_text(glyphs[i], arrow_format, &box_r, &glyph);
            }
        }
    }
    let brush = session.create_solid_brush(with_alpha(theme.control_strong, bar.alpha))?;
    let w = bar
        .width
        .clamp(ScrollbarDraw::REST_WIDTH, ScrollbarDraw::EXPANDED_WIDTH);
    session.fill_rounded_rect(
        &RoundedRect::uniform(
            Rect::from_xywh(
                ScrollbarDraw::thumb_x(width, w),
                bar.thumb_y,
                w,
                bar.thumb_h,
            ),
            w / 2.0,
        ),
        &brush,
    );
    Ok(())
}

/// Top of the empty-state text inside an area `avail` DIPs tall: Explorer places "This folder
/// is empty." in the upper part of the view; very short fences keep it vertically centred
/// (20 DIP = the Body 14 line height).
pub(super) fn empty_text_top(avail: f32, line_height: f32) -> f32 {
    if avail < 96.0 {
        ((avail - line_height) * 0.5).max(0.0)
    } else {
        avail * 0.24
    }
}

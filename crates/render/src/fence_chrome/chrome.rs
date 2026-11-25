use super::helpers::{draw_contrast_outline, draw_header_glass, header_foreground};
use super::types::{
    Backdrop, BackdropCrop, CaptionButton, FenceStyle, NOISE_BITMAP_KEY, Px, TabDraw,
    TitleCommandDraw, TitleDeco, TitleState, chevron_angle, glass_alpha, header_control_radius,
    header_count_width, tab_text_padding,
};
use super::typography::FenceChrome;
use crate::bitmaps::BitmapCache;
use crate::theme::{Theme, lerp, with_alpha};
use crate::title_controls::{TitleControl, TitleControlId};
use windows_canvas::{
    ColorF, DrawingSession, GradientStop, Matrix3x2, Rect, RoundedRect, TextFormat, Vector2,
};
use windows_core::Result;

/// A panel's header drawn by the fence (see `pecofence_plugin_api::PanelChrome`):
/// subtitle after the title, a status line and icon commands before the chevron.
#[derive(Clone, Copy, Debug, Default)]
pub struct TitleChrome<'a> {
    pub subtitle: &'a str,
    pub status: Option<(&'a str, pecofence_plugin_api::ChromeSeverity)>,
    pub commands: &'a [TitleCommandDraw],
    pub controls: &'a [crate::title_controls::TitleControl],
}

impl FenceChrome {
    /// Draws the chrome into a `width` x `height` DIP area whose origin is (0, 0). `scale` is
    /// the DIP→device-pixel factor (hairline snapping); `state` carries the sampled fades.
    /// `rolled_up` is the state the fence is rolling to (or resting in) and `roll_t` the roll
    /// progress 0 (expanded) ..= 1 (rolled) that turns the chevron. `bitmaps` holds the
    /// uploaded wallpaper crop and the acrylic noise tile across frames.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        session: &DrawingSession<'_>,
        bitmaps: &mut BitmapCache,
        theme: &Theme,
        scale: f32,
        width: f32,
        height: f32,
        backdrop: Backdrop<'_>,
        title: &str,
        caption: Option<&str>,
        rolled_up: bool,
        roll_t: f32,
        opacity: f32,
        tabs: &[TabDraw<'_>],
        pill: Option<(f32, f32)>,
        deco: TitleDeco,
        state: TitleState,
        style: FenceStyle,
        chrome: &TitleChrome<'_>,
    ) -> Result<()> {
        let fallback;
        let controls = if chrome.controls.is_empty() {
            let labels = vec![""; chrome.commands.len()];
            fallback = TitleControl::layout(
                width,
                theme.title_height.min(height),
                deco,
                tabs,
                chrome.commands,
                &labels,
                rolled_up,
                state,
            );
            &fallback
        } else {
            chrome.controls
        };
        session.clear(ColorF::TRANSPARENT);
        let backdrop = if theme.material().outline_feedback() {
            Backdrop::Solid
        } else {
            backdrop
        };
        let style = if theme.material().outline_feedback() {
            FenceStyle {
                tint: None,
                title_color: None,
                ..style
            }
        } else {
            style
        };
        let opacity = if theme.material().outline_feedback() {
            1.0
        } else {
            opacity
        };
        let px = Px::new(scale);
        // Per-fence opacity: <1 thins the layer over the glass, >1 adds a solid veil on top.
        let opacity = opacity.clamp(0.4, 1.8);
        let outer = Rect::from_xywh(0.0, 0.0, width, height);
        let radius = theme
            .material()
            .corner_radius(theme)
            .min(width.min(height) * 0.5)
            .max(0.0);
        // The crop may cover more than the surface (height animation): the surface clips it.
        let crop_rect =
            |crop: &BackdropCrop<'_>| Rect::from_xywh(0.0, 0.0, width, crop.height.max(height));

        match backdrop {
            Backdrop::GpuGlass {
                material,
                wallpaper,
                rect,
                scale,
                seed,
            } => {
                let surface = theme.material().gpu_surface(theme, seed);
                let result = material.borrow_mut().draw(
                    session,
                    &mut bitmaps.glass_wallpaper,
                    wallpaper,
                    rect,
                    scale,
                    radius,
                    opacity,
                    state.hover,
                    surface,
                );
                if let Err(error) = result {
                    if windows_canvas::is_device_lost(error.code()) {
                        return Err(error);
                    }
                    let fallback = if theme.material().is_sheet() {
                        theme.solid_fill
                    } else if theme.text_primary.r > 0.5 {
                        Theme::dark().solid_fill
                    } else {
                        Theme::light().solid_fill
                    };
                    let fill = session.create_solid_brush(fallback)?;
                    session.fill_rounded_rect(&RoundedRect::uniform(outer, radius), &fill);
                } else if !theme.material().is_sheet() {
                    let mut c = theme.glass_layer_fill;
                    c.a *= opacity.min(1.0);
                    let layer = session.create_solid_brush(c)?;
                    session.fill_rounded_rect(&RoundedRect::uniform(outer, radius), &layer);
                }
            }
            Backdrop::MicaLike(crop) if crop.image.width > 0 && crop.image.height > 0 => {
                let bitmap = bitmaps.get_or_upload(session, crop.key, crop.image)?;
                session.draw_bitmap(&bitmap, &crop_rect(&crop), glass_alpha(opacity));
                let mut c = theme.layer_fill;
                c.a *= opacity.min(1.0);
                let layer = session.create_solid_brush(c)?;
                session.fill_rounded_rect(&RoundedRect::uniform(outer, radius), &layer);
            }
            Backdrop::Glass(crop, scale) if crop.image.width > 0 && crop.image.height > 0 => {
                let bitmap = if theme.material().uses_lens() {
                    match bitmaps.get(crop.key) {
                        Some(bitmap) => bitmap,
                        None => {
                            let overlay = crate::liquid_glass::refracted_overlay(
                                crop.image,
                                width,
                                crop.height.max(height),
                                radius,
                            );
                            bitmaps.get_or_upload(session, crop.key, &overlay)?
                        }
                    }
                } else {
                    bitmaps.get_or_upload(session, crop.key, crop.image)?
                };
                let alpha = if theme.material().uses_lens() {
                    opacity.min(1.0)
                } else {
                    glass_alpha(opacity)
                };
                session.draw_bitmap(&bitmap, &crop_rect(&crop), alpha);
                let mut c = theme.glass_layer_fill;
                c.a *= opacity.min(1.0);
                let layer = session.create_solid_brush(c)?;
                session.fill_rounded_rect(&RoundedRect::uniform(outer, radius), &layer);
                // Acrylic grain: tile the noise texture at exactly one texel per device pixel.
                if !theme.material().uses_lens() {
                    let tile = crate::backdrop::noise_tile();
                    let noise = bitmaps.get_or_upload(session, NOISE_BITMAP_KEY, tile)?;
                    let step = crate::backdrop::NOISE_TILE_PX as f32 / scale.max(0.5);
                    let mut y = 0.0;
                    while y < height {
                        let mut x = 0.0;
                        while x < width {
                            session.draw_bitmap(&noise, &Rect::from_xywh(x, y, step, step), 1.0);
                            x += step;
                        }
                        y += step;
                    }
                }
            }
            Backdrop::SystemMaterial => {
                let layer = session.create_solid_brush(theme.layer_fill)?;
                session.fill_rounded_rect(&RoundedRect::uniform(outer, radius), &layer);
            }
            _ => {
                let fill = session.create_solid_brush(theme.solid_fill)?;
                session.fill_rounded_rect(&RoundedRect::uniform(outer, radius), &fill);
            }
        }

        if opacity > 1.0 {
            let veil = session.create_solid_brush(theme.opacity_veil(opacity))?;
            session.fill_rounded_rect(&RoundedRect::uniform(outer, radius), &veil);
        }
        if let Some(mut tint) = style.tint {
            // Per-fence colour: a translucent wash so the glass still reads as glass.
            tint.a = 0.22;
            let wash = session.create_solid_brush(tint)?;
            session.fill_rounded_rect(&RoundedRect::uniform(outer, radius), &wash);
        }

        // A quiet, directional reflection gives sampled materials depth without adding a
        // header band. It is static at rest; existing title hover tweens gently lift the edge.
        // Paper takes neither the glass sheen nor the lit glass rim: a plain ink hairline.
        let sampled_material = !theme.material().is_sheet()
            && matches!(
                backdrop,
                Backdrop::Glass(..) | Backdrop::MicaLike(..) | Backdrop::GpuGlass { .. }
            );
        if sampled_material && !theme.material().uses_lens() {
            let sheen = session.create_linear_gradient(
                Vector2::new(0.0, 0.0),
                Vector2::new(width, height.max(1.0)),
                &[
                    GradientStop::new(0.0, with_alpha(theme.glass_rim_top, 0.16)),
                    GradientStop::new(0.48, ColorF::TRANSPARENT),
                    GradientStop::new(1.0, with_alpha(theme.glass_rim_bottom, 0.12)),
                ],
            )?;
            session.fill_rounded_rect(&RoundedRect::uniform(outer, radius), &sheen);
        }

        // Title-row reveal ("title on hover"); a rolled fence always shows its title row.
        let title_a = if rolled_up || theme.material().outline_feedback() {
            1.0
        } else {
            state.title.clamp(0.0, 1.0)
        };
        // No header band, no divider: the title sits directly on the glass. Hovering a
        // single-title fence shows an inset pill (SubtleFill, 83 ms fade), never a full-width
        // strip; a tab strip gets its feedback from the individual tab pills instead.
        let title_h = theme.title_height.min(height);
        let pill_a = state.hover * title_a;
        if pill_a > 0.0 && tabs.len() <= 1 {
            let row = Rect::from_xywh(4.0, 4.0, (width - 8.0).max(0.0), (title_h - 8.0).max(0.0));
            if theme.material().outline_feedback() {
                draw_contrast_outline(session, theme, px, row, false)?;
            } else if theme.material().uses_lens() {
                draw_header_glass(session, &backdrop, theme, scale, 1, row, 0.28, pill_a)?;
            } else {
                let pill = session
                    .create_solid_brush(with_alpha(theme.material().hover_fill(theme), pill_a))?;
                session.fill_rounded_rect(
                    &RoundedRect::uniform(row, header_control_radius(theme, row)),
                    &pill,
                );
            }
        }

        // A single 1 px rim snapped to device pixels at every DPI. Fluent lights it from above,
        // like Windows 11's elevation border: both top corners match, then it falls off to
        // the bottom colour within the corner radius band.
        let rim_rect = px.ring(outer, radius);
        if sampled_material && theme.material().uses_lens() {
            crate::liquid_glass::draw_reflection(
                session, theme, scale, width, height, pill_a, opacity,
            )?;
        } else if sampled_material {
            let band = ((radius * 2.0 + 8.0) / height.max(1.0)).clamp(0.05, 0.5);
            let rim = session.create_linear_gradient(
                Vector2::new(0.0, 0.0),
                Vector2::new(0.0, height.max(1.0)),
                &[
                    GradientStop::new(0.0, with_alpha(theme.glass_rim_top, 1.0 + 0.2 * pill_a)),
                    GradientStop::new(
                        band,
                        lerp(theme.glass_rim_top, theme.glass_rim_bottom, 0.72),
                    ),
                    GradientStop::new(1.0, theme.glass_rim_bottom),
                ],
            )?;
            session.draw_rounded_rect(&rim_rect, &rim, px.hair());
        } else {
            let stroke = session.create_solid_brush(theme.stroke)?;
            session.draw_rounded_rect(&rim_rect, &stroke, px.hair());
        }

        if state.merge_hint > 0.0 {
            // Drag-to-merge target: accent wash over the title row plus the selection ring,
            // fading in over 83 ms and out over 167 ms.
            let a = state.merge_hint.min(1.0);
            let wash = session.create_solid_brush(with_alpha(theme.drop_highlight, a))?;
            let row = Rect::from_xywh(4.0, 4.0, (width - 8.0).max(0.0), (title_h - 8.0).max(0.0));
            let radius = header_control_radius(theme, row);
            if theme.material().uses_lens() {
                draw_header_glass(session, &backdrop, theme, scale, 2, row, 0.5, a)?;
            }
            session.fill_rounded_rect(&RoundedRect::uniform(row, radius), &wash);
            let ring = session.create_solid_brush(with_alpha(theme.selection_stroke, a))?;
            session.draw_rounded_rect(&px.ring(row, radius), &ring, px.hair());
        }
        // A test-only marker lets a lossless screen recording locate the actually
        // presented window independently of GetWindowRect and render submission times.
        #[cfg(debug_assertions)]
        if title == "运动探针"
            && std::env::var_os("PECOFENCE_PRESENT_PROBE")
                .or_else(|| std::env::var_os("OPENFENCE_PRESENT_PROBE"))
                .is_some()
        {
            let marker = session.create_solid_brush(ColorF::new(1.0, 0.0, 1.0, 1.0))?;
            session.fill_rect(
                &Rect::from_xywh(width * 0.5 - 2.0, height * 0.5 + 6.0, 4.0, 4.0),
                &marker,
            );
        }
        if title_a <= 0.0 || title_h < 1.0 {
            // "鼠标悬停时才显示标题栏": the glass and rim stay, the title row stays empty.
            // Frameless widgets have no title row at all: draw nothing there, or the
            // title would be centred on a zero-height strip and show half-cut.
            return Ok(());
        }
        // Title text; leave room for the roll-up chevron on the right, ellipsize the rest.
        let title_width = crate::text::measure_width(title, &self.title_format(style.title_size))
            .min((width - deco.title_x() - 40.0).max(1.0));
        let foreground = header_foreground(
            &backdrop,
            theme,
            5,
            Rect::from_xywh(deco.title_x(), 0.0, title_width, title_h),
            scale,
            opacity,
            style.tint,
        );
        let text = session.create_solid_brush(with_alpha(
            style.title_color.unwrap_or(foreground.text_primary),
            title_a,
        ))?;
        let secondary =
            session.create_solid_brush(with_alpha(foreground.text_secondary, title_a))?;
        let tertiary = session.create_solid_brush(with_alpha(foreground.text_tertiary, title_a))?;
        let title_format = self.title_format(style.title_size);
        // Rolled plate: the item count sits before the chevron so a rolled fence still tells
        // you what is inside (it fades out as the fence expands).
        let count_a = state.count.clamp(0.0, 1.0) * title_a;
        let count_w = if caption.is_some() && (rolled_up || count_a > 0.0) {
            header_count_width(width, tabs.len(), deco.title_x())
        } else {
            0.0
        };
        // The title's ellipsis point moves with the caption fade instead of stepping when
        // the fade ends: the reserve is the caption's width scaled by its alpha (held at the
        // full width while rolling up, so the caption never fades in over the title).
        let title_reserve = if caption.is_some() {
            count_w * count_a.max(if rolled_up { 1.0 } else { 0.0 })
        } else {
            0.0
        };
        // Panel chrome (single-title fences): commands before the chevron, status before them.
        let panel_chrome = tabs.len() <= 1 && !rolled_up;
        let commands_w = if panel_chrome {
            chrome.commands.len() as f32 * TitleDeco::COMMAND_W
        } else {
            0.0
        };
        let small_format = self.typography.borrow().count.clone();
        let status_w = match chrome.status {
            Some((status, _)) if panel_chrome => {
                (crate::text::measure_width(status, &small_format) + 20.0)
                    .min(TitleDeco::STATUS_MAX_W)
                    .min((width * 0.3).max(0.0))
            }
            _ => 0.0,
        };
        let title_reserve = title_reserve + commands_w + status_w;
        if tabs.len() > 1 {
            let inset = if theme.material().uses_lens() {
                4.0
            } else {
                6.0
            };
            let pill_h = (title_h - inset * 2.0).max(0.0);
            let tab_rect = |x, w| Rect::from_xywh(x, inset, w, pill_h);
            let marker = |x: f32, w: f32, color: ColorF, alpha: f32| -> Result<()> {
                let marker_w = (w - 24.0).clamp(0.0, 20.0);
                if marker_w > 0.0 {
                    let brush = session.create_solid_brush(with_alpha(color, alpha))?;
                    session.fill_rounded_rect(
                        &RoundedRect::uniform(
                            Rect::from_xywh(
                                x + (w - marker_w) * 0.5,
                                title_h - inset - 3.0,
                                marker_w,
                                2.0,
                            ),
                            1.0,
                        ),
                        &brush,
                    );
                }
                Ok(())
            };
            // Paint the resting lenses first; the moving selection must stay above them.
            if theme.material().uses_lens() {
                for tab in tabs.iter().filter(|t| !t.dragging) {
                    if tab.active && pill.is_none() {
                        continue;
                    }
                    draw_header_glass(
                        session,
                        &backdrop,
                        theme,
                        scale,
                        tab.key,
                        tab_rect(tab.x, tab.w),
                        0.16 + 0.32 * tab.hover + 0.3 * tab.drop_target,
                        title_a * tab.alpha.clamp(0.0, 1.0),
                    )?;
                }
            }
            if let Some((x, w)) = pill {
                let rect = tab_rect(x, w);
                if theme.material().uses_lens() {
                    draw_header_glass(session, &backdrop, theme, scale, 0, rect, 0.78, title_a)?;
                } else {
                    let fill =
                        session.create_solid_brush(with_alpha(theme.tab_active_fill, title_a))?;
                    session.fill_rounded_rect(
                        &RoundedRect::uniform(rect, header_control_radius(theme, rect)),
                        &fill,
                    );
                }
                let color = if theme.material().outline_feedback() {
                    theme.text_primary
                } else {
                    tabs.iter()
                        .find(|t| t.active)
                        .and_then(|t| t.color)
                        .unwrap_or(theme.accent)
                };
                marker(x, w, color, title_a)?;
            }
            // A dragged tab (lens, marker and caption) is always above its neighbours.
            for control in controls {
                let TitleControlId::Tab(key) = control.id else {
                    continue;
                };
                let Some(tab) = tabs.iter().find(|t| t.key == key) else {
                    continue;
                };
                let ta = title_a * tab.alpha.clamp(0.0, 1.0);
                if ta <= 0.0 {
                    continue;
                }
                let rect = tab_rect(tab.x, tab.w);
                let radius = header_control_radius(theme, rect);
                if pill.is_none() && (tab.active || tab.pressed || tab.dragging) {
                    if theme.material().uses_lens() {
                        let amount = if tab.pressed && !tab.dragging {
                            0.94
                        } else {
                            0.78
                        };
                        draw_header_glass(session, &backdrop, theme, scale, 0, rect, amount, ta)?;
                    } else {
                        let fill =
                            session.create_solid_brush(with_alpha(theme.tab_active_fill, ta))?;
                        session.fill_rounded_rect(&RoundedRect::uniform(rect, radius), &fill);
                    }
                } else if !theme.material().uses_lens() && tab.hover > 0.0 && !tab.active {
                    let fill = session.create_solid_brush(with_alpha(
                        theme.material().hover_fill(theme),
                        tab.hover * ta,
                    ))?;
                    session.fill_rounded_rect(&RoundedRect::uniform(rect, radius), &fill);
                }
                if theme.material().outline_feedback() && (tab.hover > 0.0 || tab.pressed) {
                    draw_contrast_outline(session, theme, px, rect, tab.pressed)?;
                }
                if tab.drop_target > 0.0 {
                    let a = tab.drop_target.min(1.0) * ta;
                    let fill = session.create_solid_brush(with_alpha(theme.selection_fill, a))?;
                    session.fill_rounded_rect(&RoundedRect::uniform(rect, radius), &fill);
                    let ring = session.create_solid_brush(with_alpha(theme.selection_stroke, a))?;
                    session.draw_rounded_rect(&px.ring(rect, radius), &ring, px.hair());
                }
                // A persistent marker identifies the selected tab even on uniform wallpaper.
                if theme.material().outline_feedback() && pill.is_none() && tab.active {
                    marker(tab.x, tab.w, theme.text_primary, ta)?;
                } else if !theme.material().outline_feedback() && pill.is_none() && tab.active {
                    marker(tab.x, tab.w, tab.color.unwrap_or(theme.accent), ta)?;
                } else if !theme.material().outline_feedback()
                    && !tab.active
                    && let Some(color) = tab.color
                {
                    marker(tab.x, tab.w, color, ta * 0.7)?;
                }
                let fmt = self.tab_format(tab.title_size);
                let padding = tab_text_padding(theme);
                let fitted = crate::text::fit_width(tab.title, &fmt, (tab.w - padding).max(0.0));
                let foreground =
                    header_foreground(&backdrop, theme, tab.key, rect, scale, opacity, style.tint);
                let color = if theme.material().outline_feedback() {
                    theme.text_primary
                } else if tab.pressed && !tab.dragging {
                    foreground.text_tertiary
                } else {
                    tab.title_color
                        .unwrap_or(if tab.active || tab.drop_target >= 0.5 {
                            foreground.text_primary
                        } else {
                            foreground.text_secondary
                        })
                };
                let brush = session.create_solid_brush(with_alpha(color, ta))?;
                session.draw_text(
                    &fitted,
                    &fmt,
                    &Rect::from_xywh(
                        tab.x + padding * 0.5,
                        0.0,
                        (tab.w - padding).max(0.0),
                        title_h,
                    ),
                    &brush,
                );
            }
        } else {
            let mut x = 16.0;
            if let Some(control) = controls.iter().find(|c| c.id == TitleControlId::Up) {
                // Up (Segoe Fluent E74A) in a 28×28 box: transparent at rest, SubtleFill
                // fading in on hover, SubtleFillColorTertiary + tertiary glyph while pressed.
                let box_r = control.draw_rect();
                if theme.material().outline_feedback() && (state.up_pressed || state.up_hover > 0.0)
                {
                    draw_contrast_outline(session, theme, px, box_r, state.up_pressed)?;
                } else if theme.material().uses_lens() && (state.up_pressed || state.up_hover > 0.0)
                {
                    draw_header_glass(
                        session,
                        &backdrop,
                        theme,
                        scale,
                        3,
                        box_r,
                        if state.up_pressed { 0.85 } else { 0.45 },
                        title_a
                            * if state.up_pressed {
                                1.0
                            } else {
                                state.up_hover
                            },
                    )?;
                } else if state.up_pressed {
                    let pill = session.create_solid_brush(with_alpha(
                        theme.material().pressed_fill(theme),
                        title_a,
                    ))?;
                    session.fill_rounded_rect(&RoundedRect::uniform(box_r, 4.0), &pill);
                } else if state.up_hover > 0.0 {
                    let pill = session.create_solid_brush(with_alpha(
                        theme.material().hover_fill(theme),
                        state.up_hover * title_a,
                    ))?;
                    session.fill_rounded_rect(&RoundedRect::uniform(box_r, 4.0), &pill);
                }
                session.draw_text(
                    "\u{E74A}",
                    &self.glyph_format,
                    &box_r,
                    if state.up_pressed {
                        &tertiary
                    } else {
                        &secondary
                    },
                );
                x += TitleDeco::UP_W;
            }
            if deco.folder_icon {
                // Folder glyph (E8B7) aligned with the title baseline.
                session.draw_text(
                    "\u{E8B7}",
                    &self.title_glyph_format,
                    &Rect::from_xywh(x - 2.0, 0.0, 20.0, title_h),
                    &secondary,
                );
                x += TitleDeco::ICON_W;
            }
            let controls_w = if deco.captions {
                3.0 * TitleDeco::CAPTION_W + 8.0
            } else {
                40.0
            };
            let title_w = (width - x - controls_w - title_reserve).max(0.0);
            let fitted = crate::text::fit_width(title, &title_format, title_w);
            session.draw_text(
                &fitted,
                &title_format,
                &Rect::from_xywh(x, 0.0, title_w, title_h),
                &text,
            );
            let used = crate::text::measure_width(&fitted, &title_format).min(title_w);
            let sub_w = title_w - used - 10.0;
            if panel_chrome && !chrome.subtitle.is_empty() && sub_w > 40.0 {
                // Leading-aligned caption: the subtitle follows the title, it does not
                // hug the commands like the right-aligned rolled count.
                let subtitle_format = self.typography.borrow().row.clone();
                let fitted = crate::text::fit_width(chrome.subtitle, &subtitle_format, sub_w);
                session.draw_text(
                    &fitted,
                    &subtitle_format,
                    &Rect::from_xywh(x + used + 10.0, 0.0, sub_w, title_h),
                    &secondary,
                );
            }
        }
        if panel_chrome {
            self.draw_panel_chrome(
                session,
                theme,
                px,
                width,
                title_h,
                title_a,
                chrome,
                controls,
                deco,
                status_w,
                &small_format,
                &backdrop,
                scale,
                opacity,
                style,
            )?;
        }
        if count_a > 0.0
            && count_w > 0.0
            && let Some(caption) = caption
        {
            let count_rect = Rect::from_xywh(
                deco.controls_left(width) - count_w,
                0.0,
                count_w - 4.0,
                title_h,
            );
            let foreground =
                header_foreground(&backdrop, theme, 6, count_rect, scale, opacity, style.tint);
            let count =
                session.create_solid_brush(with_alpha(foreground.text_tertiary, count_a))?;
            let count_format = self.typography.borrow().count.clone();
            let caption = crate::text::fit_width(caption, &count_format, count_rect.width());
            session.draw_text(&caption, &count_format, &count_rect, &count);
        }

        // Chevron (Segoe Fluent Icons ChevronUp E70E, turned 180° by the roll progress so it
        // points down once rolled — WinUI Expander rotates its chevron with the height) in a
        // 32×32 button box: revealed with the title-row hover (83 ms), always visible while
        // rolled so the state stays discoverable (tertiary at rest, secondary as the row is
        // hovered); per-control hover pill and pressed fill like a Fluent transparent icon
        // button.
        // `rolled_up` is the target state: while an expand is still rolling (`roll_t` > 0)
        // the chevron stays fully shown until the height has settled, so an expand started
        // with the pointer off the title row (context menu, keyboard) does not pop it to the
        // hover alpha on its first frame.
        if deco.captions {
            self.draw_caption_buttons(session, theme, px, title_a, controls)?;
            return Ok(());
        }
        let chevron_a = if theme.material().outline_feedback() || rolled_up || roll_t > 0.0 {
            1.0
        } else {
            state.hover.clamp(0.0, 1.0)
        } * title_a;
        if chevron_a > 0.0 {
            let Some(control) = controls.iter().find(|c| c.id == TitleControlId::Chevron) else {
                return Ok(());
            };
            let box_r = control.draw_rect();
            let foreground =
                header_foreground(&backdrop, theme, 7, box_r, scale, opacity, style.tint);
            if theme.material().outline_feedback()
                && (state.chevron_pressed || state.chevron_hover > 0.0)
            {
                draw_contrast_outline(session, theme, px, box_r, state.chevron_pressed)?;
            } else if theme.material().uses_lens()
                && (state.chevron_pressed || state.chevron_hover > 0.0)
            {
                draw_header_glass(
                    session,
                    &backdrop,
                    theme,
                    scale,
                    4,
                    box_r,
                    if state.chevron_pressed { 0.85 } else { 0.45 },
                    chevron_a
                        * if state.chevron_pressed {
                            1.0
                        } else {
                            state.chevron_hover
                        },
                )?;
            } else if state.chevron_pressed {
                let pill = session.create_solid_brush(with_alpha(
                    theme.material().pressed_fill(theme),
                    chevron_a,
                ))?;
                session.fill_rounded_rect(&RoundedRect::uniform(box_r, 4.0), &pill);
            } else if state.chevron_hover > 0.0 {
                let pill = session.create_solid_brush(with_alpha(
                    theme.material().hover_fill(theme),
                    state.chevron_hover * chevron_a,
                ))?;
                session.fill_rounded_rect(&RoundedRect::uniform(box_r, 4.0), &pill);
            }
            let color = if state.chevron_pressed {
                foreground.text_tertiary
            } else {
                lerp(
                    foreground.text_tertiary,
                    foreground.text_secondary,
                    state.hover,
                )
            };
            let brush = session.create_solid_brush(with_alpha(color, chevron_a))?;
            let saved = session.transform();
            let centre = Vector2::new(
                box_r.left + box_r.width() / 2.0,
                box_r.top + box_r.height() / 2.0,
            );
            session.set_transform(
                &(Matrix3x2::rotation_around(chevron_angle(roll_t), centre) * saved),
            );
            session.draw_text("\u{E70E}", &self.glyph_format, &box_r, &brush);
            session.set_transform(&saved);
        }
        Ok(())
    }

    /// Windows 11 caption buttons: 46-wide full-height boxes with Segoe Fluent chrome
    /// glyphs. Hover is the subtle wash; Close turns red (#C42B1C) with white ink.
    #[allow(clippy::too_many_arguments)]
    fn draw_caption_buttons(
        &self,
        session: &DrawingSession<'_>,
        theme: &Theme,
        px: Px,
        alpha: f32,
        controls: &[TitleControl],
    ) -> Result<()> {
        let alpha = alpha.max(if theme.material().outline_feedback() {
            1.0
        } else {
            0.0
        });
        if alpha <= 0.0 {
            return Ok(());
        }
        let close_red = ColorF::from_rgba8(0xC4, 0x2B, 0x1C, 0xFF);
        for control in controls {
            let TitleControlId::Caption(button) = control.id else {
                continue;
            };
            let box_r = control.draw_rect();
            let hover = control.state.hover > 0.0;
            let pressed = control.state.pressed;
            let close = button == CaptionButton::Close;
            if theme.material().outline_feedback() && (hover || pressed) {
                draw_contrast_outline(session, theme, px, box_r, pressed)?;
            } else if close && (hover || pressed) {
                let fill = session.create_solid_brush(with_alpha(
                    close_red,
                    alpha * if pressed { 0.9 } else { 1.0 },
                ))?;
                session.fill_rect(&box_r, &fill);
            } else if pressed {
                let fill = session
                    .create_solid_brush(with_alpha(theme.material().pressed_fill(theme), alpha))?;
                session.fill_rect(&box_r, &fill);
            } else if hover {
                let fill = session
                    .create_solid_brush(with_alpha(theme.material().hover_fill(theme), alpha))?;
                session.fill_rect(&box_r, &fill);
            }
            let ink = if close && (hover || pressed) && !theme.material().outline_feedback() {
                ColorF::from_rgba8(0xFF, 0xFF, 0xFF, 0xFF)
            } else if pressed {
                theme.text_secondary
            } else {
                theme.text_primary
            };
            let mut encoded = [0; 4];
            let glyph = control.glyph.encode_utf8(&mut encoded);
            let brush = session.create_solid_brush(with_alpha(ink, alpha))?;
            session.draw_text(glyph, &self.caption_glyph_format, &box_r, &brush);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_panel_chrome(
        &self,
        session: &DrawingSession<'_>,
        theme: &Theme,
        px: Px,
        width: f32,
        title_h: f32,
        alpha: f32,
        chrome: &TitleChrome<'_>,
        controls: &[TitleControl],
        deco: TitleDeco,
        status_w: f32,
        small_format: &TextFormat,
        backdrop: &Backdrop<'_>,
        scale: f32,
        opacity: f32,
        style: FenceStyle,
    ) -> Result<()> {
        if alpha <= 0.0 {
            return Ok(());
        }
        for control in controls {
            let TitleControlId::Command(i) = control.id else {
                continue;
            };
            let box_r = control.draw_rect();
            let command = &control.state;
            let foreground = header_foreground(
                backdrop,
                theme,
                8 + i as u128,
                box_r,
                scale,
                opacity,
                style.tint,
            );
            if theme.material().outline_feedback() && (command.pressed || command.hover > 0.0) {
                draw_contrast_outline(session, theme, px, box_r, command.pressed)?;
            } else if command.pressed {
                let pill = session
                    .create_solid_brush(with_alpha(theme.material().pressed_fill(theme), alpha))?;
                session.fill_rounded_rect(&RoundedRect::uniform(box_r, 4.0), &pill);
            } else if command.hover > 0.0 {
                let pill = session.create_solid_brush(with_alpha(
                    theme.material().hover_fill(theme),
                    command.hover * alpha,
                ))?;
                session.fill_rounded_rect(&RoundedRect::uniform(box_r, 4.0), &pill);
            }
            let color = if !control.enabled || command.pressed {
                foreground.text_tertiary
            } else {
                foreground.text_secondary
            };
            let brush = session.create_solid_brush(with_alpha(color, alpha))?;
            let mut glyph = [0u8; 4];
            session.draw_text(
                control.glyph.encode_utf8(&mut glyph),
                &self.glyph_format,
                &box_r,
                &brush,
            );
        }
        if let Some((status, severity)) = chrome.status
            && status_w > 0.0
        {
            use pecofence_plugin_api::ChromeSeverity;
            let right = deco.controls_left(width)
                - chrome.commands.len() as f32 * TitleDeco::COMMAND_W
                - 6.0;
            let rect = Rect::from_xywh(right - status_w, 0.0, status_w, title_h);
            let foreground =
                header_foreground(backdrop, theme, 7, rect, scale, opacity, style.tint);
            // Light text means a dark title row; pick the matching readable tone.
            let dark = foreground.text_primary.r > 0.5;
            let glyph = match severity {
                ChromeSeverity::Informational => "",
                ChromeSeverity::Success => "\u{E73E} ",
                ChromeSeverity::Warning => "\u{E7BA} ",
                ChromeSeverity::Error => "\u{E783} ",
            };
            let color = if theme.material().outline_feedback() {
                theme.text_primary
            } else {
                pecofence_plugin_api::paper::status_ink(severity, dark).map_or(
                    foreground.text_secondary,
                    |c| {
                        ColorF::from_rgba8(
                            (c[0] * 255.0).round() as u8,
                            (c[1] * 255.0).round() as u8,
                            (c[2] * 255.0).round() as u8,
                            0xFF,
                        )
                    },
                )
            };
            let brush = session.create_solid_brush(with_alpha(color, alpha))?;
            let mut x = rect.left;
            if !glyph.is_empty() {
                session.draw_text(
                    glyph.trim_end(),
                    &self.glyph_format,
                    &Rect::from_xywh(x, 0.0, 16.0, title_h),
                    &brush,
                );
                x += 18.0;
            }
            let fitted = crate::text::fit_width(status, small_format, rect.right - x);
            session.draw_text(
                &fitted,
                small_format,
                &Rect::from_xywh(x, 0.0, rect.right - x, title_h),
                &brush,
            );
        }
        Ok(())
    }
}

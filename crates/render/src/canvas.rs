use pecofence_plugin_api::{
    Canvas, Error, FontRole, ImageId, Path, RectDip, Result, StrokeStyle, TextMetrics, TextSpec,
};

/// Resolve the design-system font family for a plugin text role. Segoe UI
/// Variable is the Windows 11 system font; DirectWrite exposes its optical
/// sizes as separate families, so each role names its family directly.
#[cfg(windows)]
fn text_font_family(role: FontRole) -> &'static str {
    match role {
        FontRole::Text => crate::theme::FONT_TEXT,
        FontRole::Small => crate::theme::FONT_SMALL,
        FontRole::Display => crate::theme::FONT_DISPLAY,
        FontRole::Icons => crate::theme::FONT_ICONS,
        FontRole::Serif => crate::theme::FONT_SERIF,
    }
}

/// Non-Windows (headless test) builds never render real text; any family name
/// works because the headless canvas only records commands.
#[cfg(not(windows))]
#[allow(dead_code)]
fn text_font_family(_role: FontRole) -> &'static str {
    "Segoe UI Variable"
}

#[derive(Clone, Debug, PartialEq)]
pub enum DrawCommand {
    DrawRect(RectDip, StrokeStyle),
    FillRect(RectDip, [f32; 4]),
    FillRoundedRect(RectDip, f32, [f32; 4]),
    StrokePath(Path, StrokeStyle),
    DrawText(RectDip, TextSpec, [f32; 4]),
    DrawImage(ImageId, RectDip),
    PushClip(RectDip),
    PopClip,
}

/// Platform-free command sink used by layout tests and non-Windows CI.
#[derive(Default)]
pub struct HeadlessCanvas {
    commands: Vec<DrawCommand>,
    clip_depth: usize,
}

impl HeadlessCanvas {
    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    pub fn is_balanced(&self) -> bool {
        self.clip_depth == 0
    }
}

impl Canvas for HeadlessCanvas {
    fn draw_rect(&mut self, rect: RectDip, stroke: StrokeStyle) -> Result<()> {
        self.commands.push(DrawCommand::DrawRect(rect, stroke));
        Ok(())
    }

    fn fill_rect(&mut self, rect: RectDip, rgba: [f32; 4]) -> Result<()> {
        self.commands.push(DrawCommand::FillRect(rect, rgba));
        Ok(())
    }

    fn fill_rounded_rect(&mut self, rect: RectDip, radius: f32, rgba: [f32; 4]) -> Result<()> {
        self.commands
            .push(DrawCommand::FillRoundedRect(rect, radius, rgba));
        Ok(())
    }

    fn stroke_path(&mut self, path: &Path, stroke: StrokeStyle) -> Result<()> {
        self.commands
            .push(DrawCommand::StrokePath(path.clone(), stroke));
        Ok(())
    }

    fn draw_text(&mut self, rect: RectDip, text: &TextSpec, rgba: [f32; 4]) -> Result<()> {
        self.commands
            .push(DrawCommand::DrawText(rect, text.clone(), rgba));
        Ok(())
    }

    fn draw_image(&mut self, image: ImageId, destination: RectDip) -> Result<()> {
        self.commands
            .push(DrawCommand::DrawImage(image, destination));
        Ok(())
    }

    fn measure_text(&mut self, text: &TextSpec, width: f32) -> Result<TextMetrics> {
        Ok(TextMetrics {
            width: (text.text.chars().count() as f32 * text.size_dip * 0.55).min(width.max(0.0)),
            height: text.size_dip * 1.45,
        })
    }

    fn push_clip(&mut self, rect: RectDip) -> Result<()> {
        self.clip_depth += 1;
        self.commands.push(DrawCommand::PushClip(rect));
        Ok(())
    }

    fn pop_clip(&mut self) -> Result<()> {
        if self.clip_depth == 0 {
            return Err(Error::Invalid("canvas clip stack underflow".into()));
        }
        self.clip_depth -= 1;
        self.commands.push(DrawCommand::PopClip);
        Ok(())
    }
}

/// Bounded, per-panel DirectWrite formats shared across full paints and pointer
/// feedback. TextFormat is device-independent; DPI is not part of the key.
#[cfg(windows)]
#[derive(Default)]
pub struct TextFormatCache {
    formats: std::collections::HashMap<
        (FontRole, u32, u16, pecofence_plugin_api::TextAlign),
        windows_canvas::TextFormat,
    >,
}

#[cfg(windows)]
impl TextFormatCache {
    fn get(&mut self, text: &TextSpec) -> Result<&windows_canvas::TextFormat> {
        let key = (text.font, text.size_dip.to_bits(), text.weight, text.align);
        if !self.formats.contains_key(&key) {
            // Accessibility sliders can produce many distinct sizes over a session.
            if self.formats.len() >= 128 {
                self.formats.clear();
            }
            let format = windows_canvas::TextFormat::with_weight(
                text_font_family(text.font),
                text.size_dip,
                windows_canvas::FontWeight(i32::from(text.weight)),
            )
            .map_err(|error| Error::Backend(error.to_string()))?;
            use pecofence_plugin_api::TextAlign;
            use windows_canvas::{ParagraphAlignment, TextAlignment};
            let format = match text.align {
                TextAlign::TopLeading => format,
                TextAlign::MiddleLeading => {
                    format.with_paragraph_alignment(ParagraphAlignment::Center)
                }
                TextAlign::Center => format
                    .with_alignment(TextAlignment::Center)
                    .with_paragraph_alignment(ParagraphAlignment::Center),
            };
            self.formats.insert(key, format);
        }
        Ok(self.formats.get(&key).expect("format just inserted"))
    }
}

#[cfg(windows)]
pub struct Direct2dCanvas<'a> {
    session: &'a windows_canvas::DrawingSession<'a>,
    clips: Vec<RectDip>,
    clip_guards: Vec<pecofence_platform::d2d::AxisAlignedClip>,
    images: Option<&'a std::collections::HashMap<ImageId, windows_canvas::Bitmap>>,
    formats: TextFormatCache,
    shared_formats: Option<&'a mut TextFormatCache>,
}

#[cfg(windows)]
impl<'a> Direct2dCanvas<'a> {
    pub fn new(session: &'a windows_canvas::DrawingSession<'a>) -> Self {
        Self {
            session,
            clips: Vec::new(),
            clip_guards: Vec::new(),
            images: None,
            formats: TextFormatCache::default(),
            shared_formats: None,
        }
    }

    pub fn with_text_cache(
        session: &'a windows_canvas::DrawingSession<'a>,
        cache: &'a mut TextFormatCache,
    ) -> Self {
        let mut canvas = Self::new(session);
        canvas.shared_formats = Some(cache);
        canvas
    }

    pub fn with_images(
        session: &'a windows_canvas::DrawingSession<'a>,
        images: &'a std::collections::HashMap<ImageId, windows_canvas::Bitmap>,
    ) -> Self {
        Self {
            session,
            clips: Vec::new(),
            clip_guards: Vec::new(),
            images: Some(images),
            formats: TextFormatCache::default(),
            shared_formats: None,
        }
    }

    fn text_format(&mut self, text: &TextSpec) -> Result<&windows_canvas::TextFormat> {
        self.shared_formats
            .as_deref_mut()
            .unwrap_or(&mut self.formats)
            .get(text)
    }

    fn brush(&self, rgba: [f32; 4]) -> Result<windows_canvas::Brush> {
        self.session
            .create_solid_brush(windows_canvas::ColorF {
                r: rgba[0],
                g: rgba[1],
                b: rgba[2],
                a: rgba[3],
            })
            .map_err(|error| Error::Backend(error.to_string()))
    }

    fn clipped(&self, mut rect: RectDip) -> Option<RectDip> {
        for clip in &self.clips {
            let left = rect.x.max(clip.x);
            let top = rect.y.max(clip.y);
            let right = (rect.x + rect.w).min(clip.x + clip.w);
            let bottom = (rect.y + rect.h).min(clip.y + clip.h);
            rect = RectDip {
                x: left,
                y: top,
                w: (right - left).max(0.0),
                h: (bottom - top).max(0.0),
            };
        }
        (rect.w > 0.0 && rect.h > 0.0).then_some(rect)
    }
}

#[cfg(windows)]
impl Canvas for Direct2dCanvas<'_> {
    fn draw_rect(&mut self, rect: RectDip, stroke: StrokeStyle) -> Result<()> {
        // Clip on the device context, not by changing layout geometry: trimming
        // a text/image rectangle would reflow text or rescale the bitmap.
        if self.clipped(rect).is_none() {
            return Ok(());
        }
        let brush = self.brush(stroke.rgba)?;
        self.session.draw_rect(
            &windows_canvas::Rect::from_xywh(rect.x, rect.y, rect.w, rect.h),
            &brush,
            stroke.width,
        );
        Ok(())
    }

    fn fill_rect(&mut self, rect: RectDip, rgba: [f32; 4]) -> Result<()> {
        // Clip on the device context, not by changing layout geometry: trimming
        // a text/image rectangle would reflow text or rescale the bitmap.
        if self.clipped(rect).is_none() {
            return Ok(());
        }
        let brush = self.brush(rgba)?;
        self.session.fill_rect(
            &windows_canvas::Rect::from_xywh(rect.x, rect.y, rect.w, rect.h),
            &brush,
        );
        Ok(())
    }

    fn fill_rounded_rect(&mut self, rect: RectDip, radius: f32, rgba: [f32; 4]) -> Result<()> {
        // Clip on the device context, not by changing layout geometry: trimming
        // a text/image rectangle would reflow text or rescale the bitmap.
        if self.clipped(rect).is_none() {
            return Ok(());
        }
        let brush = self.brush(rgba)?;
        self.session.fill_rounded_rect(
            &windows_canvas::RoundedRect::uniform(
                windows_canvas::Rect::from_xywh(rect.x, rect.y, rect.w, rect.h),
                radius,
            ),
            &brush,
        );
        Ok(())
    }

    fn stroke_path(&mut self, path: &Path, stroke: StrokeStyle) -> Result<()> {
        let brush = self.brush(stroke.rgba)?;
        for points in path.points.windows(2) {
            self.session.draw_line(
                windows_canvas::Vector2::new(points[0].x, points[0].y),
                windows_canvas::Vector2::new(points[1].x, points[1].y),
                &brush,
                stroke.width,
            );
        }
        if path.closed && path.points.len() > 2 {
            let first = path.points[0];
            let last = path.points[path.points.len() - 1];
            self.session.draw_line(
                windows_canvas::Vector2::new(last.x, last.y),
                windows_canvas::Vector2::new(first.x, first.y),
                &brush,
                stroke.width,
            );
        }
        Ok(())
    }

    fn draw_text(&mut self, rect: RectDip, text: &TextSpec, rgba: [f32; 4]) -> Result<()> {
        // Clip on the device context, not by changing layout geometry: trimming
        // a text/image rectangle would reflow text or rescale the bitmap.
        if self.clipped(rect).is_none() {
            return Ok(());
        }
        let brush = self.brush(rgba)?;
        let session = self.session;
        let format = self.text_format(text)?;
        session.draw_text(
            &text.text,
            format,
            &windows_canvas::Rect::from_xywh(rect.x, rect.y, rect.w, rect.h),
            &brush,
        );
        Ok(())
    }

    fn draw_image(&mut self, image: ImageId, destination: RectDip) -> Result<()> {
        let bitmap = self
            .images
            .and_then(|images| images.get(&image))
            .ok_or_else(|| Error::Invalid(format!("unresolved image resource {}", image.0)))?;
        if self.clipped(destination).is_none() {
            return Ok(());
        }
        self.session.draw_bitmap(
            bitmap,
            &windows_canvas::Rect::from_xywh(
                destination.x,
                destination.y,
                destination.w,
                destination.h,
            ),
            1.0,
        );
        Ok(())
    }

    fn measure_text(&mut self, text: &TextSpec, width: f32) -> Result<TextMetrics> {
        let format = self.text_format(text)?;
        let layout = windows_canvas::TextLayout::new(&text.text, format, width.max(1.0), 10_000.0)
            .map_err(|error| Error::Backend(error.to_string()))?;
        let metrics = layout.metrics();
        Ok(TextMetrics {
            width: metrics.width_including_trailing_whitespace,
            height: metrics.height,
        })
    }

    fn push_clip(&mut self, rect: RectDip) -> Result<()> {
        let guard = pecofence_platform::d2d::push_axis_aligned_clip(
            self.session.raw(),
            rect.x,
            rect.y,
            rect.x + rect.w,
            rect.y + rect.h,
        )
        .map_err(|error| Error::Backend(error.to_string()))?;
        self.clips.push(rect);
        self.clip_guards.push(guard);
        Ok(())
    }

    fn pop_clip(&mut self) -> Result<()> {
        let popped = self
            .clips
            .pop()
            .ok_or_else(|| Error::Invalid("canvas clip stack underflow".into()))?;
        let _ = popped;
        self.clip_guards.pop();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecofence_plugin_api::TextAlign;

    #[test]
    fn component_border_is_inside_the_original_hit_and_text_clip() {
        use pecofence_plugin_api::design::Component;
        let rect = RectDip {
            x: 3.0,
            y: 4.0,
            w: 40.0,
            h: 24.0,
        };
        let mut canvas = HeadlessCanvas::default();
        Component {
            rect,
            text: "Selected",
            size: 14.0,
            weight: 600,
            font: FontRole::Text,
            foreground: [1.0; 4],
            fill: None,
            radius: 4.0,
            text_inset: [0.0; 2],
            align: TextAlign::TopLeading,
            path: None,
            stroke: Some(StrokeStyle {
                rgba: [1.0; 4],
                width: 2.0,
            }),
        }
        .paint(&mut canvas)
        .unwrap();
        assert!(canvas.is_balanced());
        assert!(matches!(canvas.commands(), [DrawCommand::PushClip(clip),
            DrawCommand::DrawRect(border, style), DrawCommand::DrawText(text, _, _), DrawCommand::PopClip]
            if *clip == rect && *text == rect && border.x - style.width / 2.0 == rect.x
            && border.x + border.w + style.width / 2.0 == rect.x + rect.w));
    }

    #[test]
    fn scene_feedback_matches_painted_corners_at_independent_text_and_display_scales() {
        use pecofence_plugin_api::{
            FrameIdentity, InstanceKey, MountKey, PreparedFrame, ThemeSnapshot,
            design::{ControlInteraction, PixelGrid},
            scene::{NodeStyle, Scene, SceneNode},
        };
        for text_scale in [1.0, 1.5, 2.25] {
            let nodes = [0.0, 4.0, 100.0]
                .into_iter()
                .enumerate()
                .map(|(index, radius)| SceneNode {
                    id: index as u64,
                    parent: None,
                    rect: RectDip {
                        x: index as f32 * 90.0,
                        y: 0.0,
                        w: 80.0,
                        h: 32.0 * text_scale,
                    },
                    role: "button".into(),
                    name: "Action".into(),
                    text: "Action".into(),
                    action: Some(7),
                    style: NodeStyle {
                        size: 14.0 * text_scale,
                        weight: 400,
                        font: FontRole::Text,
                        foreground: [1.0; 4],
                        fill: Some([0.2; 4]),
                        stroke: None,
                        radius: radius * text_scale,
                        text_inset: [0.0; 2],
                        align: Default::default(),
                        path: None,
                    },
                })
                .collect();
            let frame = Scene::new(
                FrameIdentity {
                    mount_key: MountKey {
                        instance: InstanceKey {
                            id: 1,
                            activation: 1,
                        },
                        generation: 1,
                    },
                    frame_revision: 1,
                    layout_revision: 1,
                    theme_epoch: 1,
                    device_epoch: 1,
                },
                RectDip {
                    x: 0.0,
                    y: 0.0,
                    w: 300.0,
                    h: 100.0,
                },
                nodes,
            )
            .unwrap();
            let mut base = HeadlessCanvas::default();
            frame.paint(&mut base).unwrap();
            let shapes: Vec<_> = base
                .commands()
                .iter()
                .filter_map(|command| {
                    if let DrawCommand::FillRoundedRect(rect, radius, _) = command {
                        Some((*rect, *radius))
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(shapes.len(), 3);
            for dpi in [96.0, 120.0, 144.0, 192.0] {
                for hit in frame.hit_tree() {
                    let mut canvas = HeadlessCanvas::default();
                    ControlInteraction {
                        hovered: Some(hit.id),
                        ..Default::default()
                    }
                    .paint_with_radii(
                        &mut canvas,
                        frame.hit_tree(),
                        &ThemeSnapshot::dark(),
                        PixelGrid::new(dpi / 96.0),
                        |id| frame.control_corner_radius(id).unwrap(),
                    )
                    .unwrap();
                    let expected = shapes[hit.id as usize];
                    assert!(
                        matches!(canvas.commands(), [DrawCommand::FillRoundedRect(rect, radius, _)]
                        if (*rect, *radius) == expected)
                    );
                }
            }
            assert_eq!(frame.control_corner_radius(99), None);
        }
    }

    #[test]
    fn contrast_feedback_uses_opaque_outlines_without_washing_out_text() {
        use pecofence_plugin_api::{
            HitNode, ThemeSnapshot,
            design::{ControlInteraction, PixelGrid},
        };
        let hits = [HitNode {
            id: 1,
            action: 7,
            rect: RectDip {
                x: 10.0,
                y: 10.0,
                w: 80.0,
                h: 32.0,
            },
        }];
        let mut theme = ThemeSnapshot::dark();
        theme.high_contrast = true;
        for ink in [[1.0; 4], [0.0, 0.0, 0.0, 1.0]] {
            theme.text_primary = ink;
            for pressed in [None, Some(1)] {
                let state = ControlInteraction {
                    hovered: Some(1),
                    pressed,
                    focused: None,
                };
                let mut canvas = HeadlessCanvas::default();
                state
                    .paint(&mut canvas, &hits, &theme, PixelGrid::new(1.5), 4.0)
                    .unwrap();
                assert!(
                    matches!(canvas.commands(), [DrawCommand::DrawRect(_, stroke)]
                    if stroke.rgba == ink && stroke.width > 0.0)
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn native_text_format_cache_reuses_styles_and_bounds_accessibility_size_history() {
        let mut cache = TextFormatCache::default();
        let mut text = TextSpec {
            text: "One".into(),
            size_dip: 14.0,
            weight: 400,
            font: FontRole::Text,
            align: TextAlign::TopLeading,
        };
        cache.get(&text).unwrap();
        text.text = "不同文本，相同样式".into();
        cache.get(&text).unwrap();
        assert_eq!(cache.formats.len(), 1);
        text.size_dip = 21.0;
        cache.get(&text).unwrap();
        assert_eq!(cache.formats.len(), 2);
        text.align = TextAlign::Center;
        cache.get(&text).unwrap();
        assert_eq!(cache.formats.len(), 3);
        for index in 0..160 {
            text.size_dip = 8.0 + index as f32 * 0.1;
            cache.get(&text).unwrap();
            assert!(cache.formats.len() <= 128);
        }
    }

    #[test]
    fn shared_feedback_distinguishes_hover_press_and_keyboard_focus_without_covering_text() {
        use pecofence_plugin_api::{
            HitNode, ThemeSnapshot,
            design::{ControlInteraction, PixelGrid},
        };
        let hits = vec![HitNode {
            id: 7,
            action: 1,
            rect: RectDip {
                x: 12.0,
                y: 8.0,
                w: 80.0,
                h: 32.0,
            },
        }];
        let mut state = ControlInteraction {
            hovered: Some(7),
            ..Default::default()
        };
        let mut canvas = HeadlessCanvas::default();
        state
            .paint(
                &mut canvas,
                &hits,
                &ThemeSnapshot::dark(),
                PixelGrid::new(1.5),
                pecofence_plugin_api::design::CONTROL_RADIUS,
            )
            .unwrap();
        let alpha = match canvas.commands()[0] {
            DrawCommand::FillRoundedRect(_, _, rgba) => rgba[3],
            _ => panic!("hover fill"),
        };
        state.pressed = Some(7);
        state.focused = Some(7);
        let mut pressed = HeadlessCanvas::default();
        state
            .paint(
                &mut pressed,
                &hits,
                &ThemeSnapshot::dark(),
                PixelGrid::new(1.5),
                pecofence_plugin_api::design::CONTROL_RADIUS,
            )
            .unwrap();
        assert!(
            matches!(pressed.commands()[0], DrawCommand::FillRoundedRect(_, _, rgba) if rgba[3] != alpha && rgba[3] <= 0.16)
        );
        assert!(
            pressed
                .commands()
                .iter()
                .any(|command| matches!(command, DrawCommand::DrawRect(_, _)))
        );
        assert!(
            !pressed
                .commands()
                .iter()
                .any(|command| matches!(command, DrawCommand::DrawText(_, _, _)))
        );
        assert!(pressed.is_balanced());
    }

    #[test]
    fn shared_component_keeps_text_inside_its_own_clip_and_preserves_font_role() {
        use pecofence_plugin_api::design::{BODY, Component};
        let rect = RectDip {
            x: 20.0,
            y: 30.0,
            w: 120.0,
            h: 48.0,
        };
        let mut canvas = HeadlessCanvas::default();
        Component {
            stroke: None,
            rect,
            text: "A long filename / 很长的文件名",
            size: BODY * 2.0,
            weight: 400,
            font: FontRole::Text,
            foreground: [1.0; 4],
            fill: Some([0.2; 4]),
            radius: 4.0,
            text_inset: [6.0, 4.0],
            align: TextAlign::TopLeading,
            path: None,
        }
        .paint(&mut canvas)
        .unwrap();
        assert!(canvas.is_balanced());
        assert!(matches!(canvas.commands().first(), Some(DrawCommand::PushClip(r)) if *r == rect));
        let (text_rect, spec) = canvas
            .commands()
            .iter()
            .find_map(|command| match command {
                DrawCommand::DrawText(r, spec, _) => Some((r, spec)),
                _ => None,
            })
            .unwrap();
        assert!(text_rect.x >= rect.x && text_rect.x + text_rect.w <= rect.x + rect.w);
        assert!(text_rect.y >= rect.y && text_rect.y + text_rect.h <= rect.y + rect.h);
        assert_eq!(spec.size_dip, 28.0);
        assert_eq!(spec.font, FontRole::Text);
    }

    #[test]
    fn headless_canvas_records_balanced_commands() {
        let mut canvas = HeadlessCanvas::default();
        canvas
            .push_clip(RectDip {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            })
            .unwrap();
        canvas
            .fill_rounded_rect(
                RectDip {
                    x: 1.0,
                    y: 1.0,
                    w: 8.0,
                    h: 8.0,
                },
                2.0,
                [1.0; 4],
            )
            .unwrap();
        canvas.pop_clip().unwrap();
        assert!(canvas.is_balanced());
        assert_eq!(canvas.commands().len(), 3);
    }

    #[test]
    fn clip_underflow_is_rejected() {
        let mut canvas = HeadlessCanvas::default();
        assert!(matches!(canvas.pop_clip(), Err(Error::Invalid(_))));
    }
}

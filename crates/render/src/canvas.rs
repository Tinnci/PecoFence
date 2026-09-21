use pecofence_plugin_api::{
    Canvas, Error, ImageId, Path, RectDip, Result, StrokeStyle, TextMetrics, TextSpec,
};

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

#[cfg(windows)]
pub struct Direct2dCanvas<'a> {
    session: &'a windows_canvas::DrawingSession<'a>,
    clips: Vec<RectDip>,
    clip_guards: Vec<pecofence_platform::d2d::AxisAlignedClip>,
    images: Option<&'a std::collections::HashMap<ImageId, windows_canvas::Bitmap>>,
}

#[cfg(windows)]
impl<'a> Direct2dCanvas<'a> {
    pub fn new(session: &'a windows_canvas::DrawingSession<'a>) -> Self {
        Self {
            session,
            clips: Vec::new(),
            clip_guards: Vec::new(),
            images: None,
        }
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
        }
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
        let Some(rect) = self.clipped(rect) else {
            return Ok(());
        };
        let brush = self.brush(stroke.rgba)?;
        self.session.draw_rect(
            &windows_canvas::Rect::from_xywh(rect.x, rect.y, rect.w, rect.h),
            &brush,
            stroke.width,
        );
        Ok(())
    }

    fn fill_rect(&mut self, rect: RectDip, rgba: [f32; 4]) -> Result<()> {
        let Some(rect) = self.clipped(rect) else {
            return Ok(());
        };
        let brush = self.brush(rgba)?;
        self.session.fill_rect(
            &windows_canvas::Rect::from_xywh(rect.x, rect.y, rect.w, rect.h),
            &brush,
        );
        Ok(())
    }

    fn fill_rounded_rect(&mut self, rect: RectDip, radius: f32, rgba: [f32; 4]) -> Result<()> {
        let Some(rect) = self.clipped(rect) else {
            return Ok(());
        };
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
        let Some(rect) = self.clipped(rect) else {
            return Ok(());
        };
        let brush = self.brush(rgba)?;
        let format = windows_canvas::TextFormat::new("Segoe UI Variable", text.size_dip)
            .map_err(|error| Error::Backend(error.to_string()))?;
        self.session.draw_text(
            &text.text,
            &format,
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
        let Some(destination) = self.clipped(destination) else {
            return Ok(());
        };
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
        let format = windows_canvas::TextFormat::new("Segoe UI Variable", text.size_dip)
            .map_err(|error| Error::Backend(error.to_string()))?;
        let layout = windows_canvas::TextLayout::new(&text.text, &format, width.max(1.0), 10_000.0)
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

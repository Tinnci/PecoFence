//! Platform-free headless canvas and CPU rasterizer for plugin layout and snapshot tests.

use pecofence_plugin_api::{
    Canvas, Error, ImageId, Path, RectDip, Result, StrokeStyle, TextMetrics, TextSpec,
};

#[cfg(feature = "raster")]
pub mod raster;

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

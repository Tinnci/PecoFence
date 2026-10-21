//! Test-only CPU rasterizer for [`DrawCommand`] lists (feature `raster`).
//!
//! Turns what a scene painted into a [`HeadlessCanvas`](crate::HeadlessCanvas) into pixels
//! with tiny-skia, on any platform, so layouts can be checked against golden images in
//! ordinary `cargo test`. It is not a second renderer for the app: text has no fonts here
//! and is drawn as one placeholder bar per line (its box, alignment and line count are what
//! a layout regression changes), images as grey boxes. Strokes use round joins.

use crate::canvas::DrawCommand;
use pecofence_plugin_api::{Path, RectDip, TextAlign, TextSpec};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform,
};

/// Rasterizes `commands` (device pixels) onto a `width` × `height` image over `background`.
pub fn rasterize(
    commands: &[DrawCommand],
    width: u32,
    height: u32,
    background: [f32; 4],
) -> Pixmap {
    let mut pixmap = Pixmap::new(width.max(1), height.max(1)).expect("non-empty pixmap");
    pixmap.fill(color(background));
    let mut clips: Vec<RectDip> = Vec::new();
    for command in commands {
        let mask = clips
            .last()
            .and_then(|clip| clip_mask(*clip, width, height));
        let mask = mask.as_ref();
        match command {
            DrawCommand::PushClip(rect) => {
                let clip = clips.last().map_or(*rect, |outer| intersect(*outer, *rect));
                clips.push(clip);
            }
            DrawCommand::PopClip => {
                clips.pop();
            }
            DrawCommand::FillRect(rect, rgba) => {
                if let Some(path) = rect_path(*rect) {
                    fill(&mut pixmap, &path, *rgba, mask);
                }
            }
            DrawCommand::DrawRect(rect, stroke) => {
                if let Some(path) = rect_path(*rect) {
                    stroke_path(&mut pixmap, &path, stroke.rgba, stroke.width, mask);
                }
            }
            DrawCommand::FillRoundedRect(rect, radius, rgba) => {
                if let Some(path) = rounded_rect_path(*rect, *radius) {
                    fill(&mut pixmap, &path, *rgba, mask);
                }
            }
            DrawCommand::StrokePath(path, stroke) => {
                if let Some(path) = polyline(path) {
                    stroke_path(&mut pixmap, &path, stroke.rgba, stroke.width, mask);
                }
            }
            DrawCommand::DrawText(rect, text, rgba) => {
                text_placeholder(&mut pixmap, *rect, text, *rgba, mask)
            }
            DrawCommand::DrawImage(_, rect) => {
                if let Some(path) = rect_path(*rect) {
                    fill(&mut pixmap, &path, [0.5, 0.5, 0.5, 0.6], mask);
                }
            }
        }
    }
    pixmap
}

fn color([r, g, b, a]: [f32; 4]) -> Color {
    Color::from_rgba(
        r.clamp(0.0, 1.0),
        g.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
        a.clamp(0.0, 1.0),
    )
    .unwrap_or(Color::TRANSPARENT)
}

fn paint(rgba: [f32; 4]) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(color(rgba));
    paint.anti_alias = true;
    paint
}

fn fill(pixmap: &mut Pixmap, path: &tiny_skia::Path, rgba: [f32; 4], mask: Option<&Mask>) {
    pixmap.fill_path(
        path,
        &paint(rgba),
        FillRule::Winding,
        Transform::identity(),
        mask,
    );
}

fn stroke_path(
    pixmap: &mut Pixmap,
    path: &tiny_skia::Path,
    rgba: [f32; 4],
    width: f32,
    mask: Option<&Mask>,
) {
    let stroke = Stroke {
        width: width.max(0.1),
        line_join: LineJoin::Round,
        line_cap: LineCap::Round,
        ..Stroke::default()
    };
    pixmap.stroke_path(path, &paint(rgba), &stroke, Transform::identity(), mask);
}

fn rect_path(r: RectDip) -> Option<tiny_skia::Path> {
    Some(PathBuilder::from_rect(Rect::from_xywh(r.x, r.y, r.w, r.h)?))
}

fn rounded_rect_path(r: RectDip, radius: f32) -> Option<tiny_skia::Path> {
    let radius = radius.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    if radius <= 0.0 {
        return rect_path(r);
    }
    // Cubic quarter circles (kappa = 0.5523), clockwise from the top edge.
    let k = radius * 0.552_284_8;
    let (l, t, rt, b) = (r.x, r.y, r.x + r.w, r.y + r.h);
    let mut pb = PathBuilder::new();
    pb.move_to(l + radius, t);
    pb.line_to(rt - radius, t);
    pb.cubic_to(rt - radius + k, t, rt, t + radius - k, rt, t + radius);
    pb.line_to(rt, b - radius);
    pb.cubic_to(rt, b - radius + k, rt - radius + k, b, rt - radius, b);
    pb.line_to(l + radius, b);
    pb.cubic_to(l + radius - k, b, l, b - radius + k, l, b - radius);
    pb.line_to(l, t + radius);
    pb.cubic_to(l, t + radius - k, l + radius - k, t, l + radius, t);
    pb.close();
    pb.finish()
}

fn polyline(path: &Path) -> Option<tiny_skia::Path> {
    let (first, rest) = path.points.split_first()?;
    let mut pb = PathBuilder::new();
    pb.move_to(first.x, first.y);
    for point in rest {
        pb.line_to(point.x, point.y);
    }
    if path.closed {
        pb.close();
    }
    pb.finish()
}

fn intersect(a: RectDip, b: RectDip) -> RectDip {
    let (l, t) = (a.x.max(b.x), a.y.max(b.y));
    let (r, bottom) = ((a.x + a.w).min(b.x + b.w), (a.y + a.h).min(b.y + b.h));
    RectDip {
        x: l,
        y: t,
        w: (r - l).max(0.0),
        h: (bottom - t).max(0.0),
    }
}

/// Everything outside `clip` masked off; `None` for an empty clip (nothing draws).
fn clip_mask(clip: RectDip, width: u32, height: u32) -> Option<Mask> {
    let mut mask = Mask::new(width.max(1), height.max(1))?;
    if let Some(path) = rect_path(clip) {
        mask.fill_path(&path, FillRule::Winding, false, Transform::identity());
    }
    Some(mask)
}

/// A faint box for the text's rectangle and a bar per line, as wide as an estimate of the
/// line (1 em for wide characters, 0.55 em otherwise) and placed by the text's alignment.
fn text_placeholder(
    pixmap: &mut Pixmap,
    rect: RectDip,
    text: &TextSpec,
    rgba: [f32; 4],
    mask: Option<&Mask>,
) {
    let lines: Vec<&str> = text
        .text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.is_empty() || rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    let size = text.size_dip.max(1.0);
    let line_h = size * 1.3;
    let block = line_h * lines.len() as f32;
    let top = match text.align {
        TextAlign::TopLeading => rect.y,
        TextAlign::MiddleLeading | TextAlign::Center => rect.y + (rect.h - block) / 2.0,
    };
    let [r, g, b, a] = rgba;
    for (i, line) in lines.iter().enumerate() {
        let estimate: f32 = line
            .chars()
            .map(|c| if (c as u32) < 0x2e80 { 0.55 } else { 1.0 })
            .sum::<f32>()
            * size;
        let w = estimate.min(rect.w);
        let x = match text.align {
            TextAlign::Center => rect.x + (rect.w - w) / 2.0,
            _ => rect.x,
        };
        let y = top + i as f32 * line_h + (line_h - size * 0.55) / 2.0;
        let bar = RectDip {
            x,
            y,
            w,
            h: size * 0.55,
        };
        if let Some(path) = rounded_rect_path(bar, size * 0.2) {
            fill(pixmap, &path, [r, g, b, a * 0.75], mask);
        }
    }
}

/// Compares `pixmap` with the golden PNG at `golden`. A pixel differs when any channel is
/// more than 3 levels apart; up to 0.1 % of pixels may (antialiasing on another CPU).
/// With `UPDATE_SNAPSHOTS=1`, or when the golden is missing, the golden is (re)written and
/// the test fails so the new image gets reviewed. On a mismatch the actual image is written
/// beside the golden as `<name>.actual.png`.
pub fn assert_snapshot(pixmap: &Pixmap, golden: &std::path::Path) {
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some_and(|v| v == "1");
    let png = pixmap.encode_png().expect("encode snapshot");
    if update || !golden.exists() {
        if let Some(dir) = golden.parent() {
            std::fs::create_dir_all(dir).expect("snapshot directory");
        }
        std::fs::write(golden, &png).expect("write golden");
        assert!(
            update,
            "wrote new golden {}: review it and rerun",
            golden.display()
        );
        return;
    }
    let expected =
        Pixmap::decode_png(&std::fs::read(golden).expect("read golden")).expect("decode golden");
    let actual_path = golden.with_extension("actual.png");
    let same_size = expected.width() == pixmap.width() && expected.height() == pixmap.height();
    let differing = if same_size {
        expected
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(pixmap.data().as_chunks::<4>().0)
            .filter(|(e, a)| e.iter().zip(a.iter()).any(|(x, y)| x.abs_diff(*y) > 3))
            .count()
    } else {
        usize::MAX
    };
    let allowed = (pixmap.width() * pixmap.height()) as usize / 1000;
    if differing > allowed {
        std::fs::write(&actual_path, &png).expect("write actual");
        panic!(
            "{} differs from its golden ({} pixels; {} allowed); see {}. Rerun with \
             UPDATE_SNAPSHOTS=1 to accept.",
            golden.display(),
            if same_size {
                differing.to_string()
            } else {
                "size".into()
            },
            allowed,
            actual_path.display()
        );
    }
    let _ = std::fs::remove_file(actual_path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecofence_plugin_api::{Canvas, FontRole, PointDip, StrokeStyle};

    #[test]
    fn draws_fills_strokes_clips_and_text_bars() {
        let mut canvas = crate::HeadlessCanvas::default();
        canvas
            .fill_rect(
                RectDip {
                    x: 0.0,
                    y: 0.0,
                    w: 40.0,
                    h: 40.0,
                },
                [1.0, 0.0, 0.0, 1.0],
            )
            .unwrap();
        canvas
            .push_clip(RectDip {
                x: 50.0,
                y: 0.0,
                w: 10.0,
                h: 40.0,
            })
            .unwrap();
        canvas
            .fill_rounded_rect(
                RectDip {
                    x: 40.0,
                    y: 0.0,
                    w: 40.0,
                    h: 40.0,
                },
                8.0,
                [0.0, 0.0, 1.0, 1.0],
            )
            .unwrap();
        canvas.pop_clip().unwrap();
        canvas
            .stroke_path(
                &Path {
                    points: vec![PointDip { x: 0.0, y: 60.0 }, PointDip { x: 80.0, y: 60.0 }],
                    closed: false,
                },
                StrokeStyle {
                    rgba: [0.0, 1.0, 0.0, 1.0],
                    width: 2.0,
                },
            )
            .unwrap();
        let text = TextSpec {
            text: "ab\n中文".into(),
            size_dip: 10.0,
            weight: 400,
            font: FontRole::Text,
            align: TextAlign::TopLeading,
        };
        canvas
            .draw_text(
                RectDip {
                    x: 0.0,
                    y: 70.0,
                    w: 80.0,
                    h: 30.0,
                },
                &text,
                [0.0, 0.0, 0.0, 1.0],
            )
            .unwrap();
        let pixmap = rasterize(canvas.commands(), 80, 100, [1.0, 1.0, 1.0, 1.0]);
        let at = |x: u32, y: u32| pixmap.pixel(x, y).unwrap();
        assert_eq!((at(20, 20).red(), at(20, 20).blue()), (255, 0));
        assert_eq!(
            (at(55, 20).red(), at(55, 20).blue()),
            (0, 255),
            "inside the clip"
        );
        assert_eq!(at(70, 20).red(), 255, "outside the clip: white background");
        assert!(at(40, 60).green() > 200 && at(40, 60).red() < 60);
        assert!(at(3, 76).red() < 200, "first line bar");
        assert!(
            at(60, 76).red() > 250,
            "bar ends with the estimated line width"
        );
    }
}

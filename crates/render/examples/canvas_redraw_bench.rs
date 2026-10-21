//! CPU cost of redrawing a diagram-sized scene through `Direct2dCanvas`: ~30 labels,
//! ~20 polylines and their boxes, at 560 × 400. Compares a warm, persistent
//! `TextFormatCache` (layouts shaped once) with a fresh cache every frame (every label
//! re-shaped, as `DrawText` did before the layout cache), and reports median and p95 ms.
//!
//! cargo run -p pecofence-render --example canvas_redraw_bench --release
//! No windows or desktop capture; draws to an offscreen GPU target.

use pecofence_plugin_api::{
    Canvas, FontRole, Path, PointDip, RectDip, StrokeStyle, TextAlign, TextSpec,
};
use pecofence_render::{Direct2dCanvas, TextFormatCache};
use std::time::Instant;
use windows_canvas::GpuDevice;

const FRAMES: usize = 300;

fn scene(canvas: &mut dyn Canvas) -> pecofence_plugin_api::Result<()> {
    let ink = [0.17, 0.15, 0.13, 1.0];
    let card = [1.0, 0.99, 0.96, 1.0];
    let line = StrokeStyle {
        rgba: [0.4, 0.37, 0.33, 1.0],
        width: 1.2,
    };
    canvas.fill_rounded_rect(
        RectDip {
            x: 16.0,
            y: 66.0,
            w: 528.0,
            h: 318.0,
        },
        8.0,
        [0.93, 0.9, 0.85, 1.0],
    )?;
    for i in 0..30 {
        let (col, row) = ((i % 6) as f32, (i / 6) as f32);
        let rect = RectDip {
            x: 30.0 + col * 86.0,
            y: 80.0 + row * 58.0,
            w: 76.0,
            h: 34.0,
        };
        canvas.fill_rounded_rect(rect, 6.0, card)?;
        canvas.draw_rect(rect, line)?;
        let text = TextSpec {
            text: if i % 3 == 0 {
                format!("节点 {i}\n第二行")
            } else {
                format!("Node {i} 标签")
            },
            size_dip: 13.0,
            weight: 400,
            font: FontRole::PaperBody,
            align: TextAlign::Center,
        };
        canvas.draw_text(rect, &text, ink)?;
        if i < 20 {
            canvas.stroke_path(
                &Path {
                    points: vec![
                        PointDip {
                            x: rect.x + rect.w,
                            y: rect.y + 17.0,
                        },
                        PointDip {
                            x: rect.x + rect.w + 5.0,
                            y: rect.y + 17.0,
                        },
                        PointDip {
                            x: rect.x + rect.w + 5.0,
                            y: rect.y + 45.0,
                        },
                        PointDip {
                            x: rect.x + rect.w + 10.0,
                            y: rect.y + 45.0,
                        },
                    ],
                    closed: false,
                },
                line,
            )?;
        }
    }
    Ok(())
}

fn stats(mut ms: Vec<f64>) -> (f64, f64) {
    ms.sort_by(f64::total_cmp);
    (ms[ms.len() / 2], ms[ms.len() * 95 / 100])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ole = pecofence_platform::com::OleGuard::init()?;
    let gpu = GpuDevice::new_or_warp()?;
    let target = gpu.create_render_target(560, 400)?;
    let mut warm = TextFormatCache::default();
    let (mut cached, mut cold) = (Vec::new(), Vec::new());
    for frame in 0..FRAMES * 2 {
        let fresh = frame % 2 == 1;
        let mut scratch = TextFormatCache::default();
        let cache = if fresh { &mut scratch } else { &mut warm };
        let started = Instant::now();
        target.draw(|session| {
            session.clear(windows_canvas::ColorF::new(0.96, 0.94, 0.9, 1.0));
            let mut canvas = Direct2dCanvas::with_text_cache(session, cache);
            scene(&mut canvas)
                .map_err(|e| windows_core::Error::new(windows_core::HRESULT(-1), e.to_string()))
        })?;
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        // The first frames warm up the device and fonts.
        if frame >= 20 {
            if fresh { &mut cold } else { &mut cached }.push(ms);
        }
    }
    let _ = target.read_pixels()?; // wait for the GPU before exiting
    let (cached_median, cached_p95) = stats(cached);
    let (cold_median, cold_p95) = stats(cold);
    println!("layouts cached:   median {cached_median:.3} ms, p95 {cached_p95:.3} ms");
    println!("shaped per frame: median {cold_median:.3} ms, p95 {cold_p95:.3} ms");
    Ok(())
}

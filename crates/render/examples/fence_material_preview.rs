//! Corner and material study for every fence style through the production renderers:
//! Fluent over its acrylic crop, Liquid Glass through `GpuGlass` (the pixel shader, or the
//! built-in effect chain with `--builtin`) and Paper (its sheet shader), each in light and
//! dark.
//!
//! cargo run -p pecofence-render --example fence_material_preview -- <out-dir> [scale] [--builtin]
//!
//! Writes `materials.bmp` (four plates over a structured wallpaper) and `corners.bmp`
//! (each plate's top-left and bottom-right corner, enlarged 4× with nearest-neighbour).
//! Composition and DWM clip the real windows; this export clips in software with the same
//! antialiased rounded outline. No windows, desktop capture or configuration access.

use pecofence_render::{
    BitmapCache, ColorF, Image, Matrix3x2, MonitorBackdrop, Rect, RoundedRect, Theme, Vector2,
    WallpaperPosition,
    fence_chrome::{Backdrop, BackdropCrop, FenceChrome, FenceStyle, TitleDeco, TitleState},
    gpu_glass::GpuGlass,
    liquid_glass,
};
use std::{cell::RefCell, io::Write, path::Path, rc::Rc};
use windows_canvas::{GpuDevice, GradientStop, TextFormat};

const PLATE: (f32, f32) = (300.0, 250.0);
const CANVAS: (f32, f32) = (1060.0, 700.0);
const ZOOM: u32 = 4;
const CORNER_DIP: f32 = 36.0;

fn save_bmp(path: &Path, width: u32, height: u32, pixels: &[u8]) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    f.write_all(b"BM")?;
    f.write_all(&(54 + pixels.len() as u32).to_le_bytes())?;
    f.write_all(&[0; 4])?;
    f.write_all(&54u32.to_le_bytes())?;
    f.write_all(&40u32.to_le_bytes())?;
    f.write_all(&(width as i32).to_le_bytes())?;
    f.write_all(&(-(height as i32)).to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&32u16.to_le_bytes())?;
    f.write_all(&[0; 24])?;
    f.write_all(pixels)
}

/// A wallpaper with edges in every direction, so refraction and rims are visible: a light
/// band over a dark one, a grid, diagonal colour stripes, discs and lines of "text".
fn wallpaper(gpu: &GpuDevice, scale: f32) -> Result<Image, Box<dyn std::error::Error>> {
    let (w, h) = ((CANVAS.0 * scale) as u32, (CANVAS.1 * scale) as u32);
    let target = gpu.create_render_target(w, h)?;
    let font = TextFormat::new("Segoe UI", 15.0)?;
    target.draw(|s| {
        s.set_transform(&Matrix3x2 {
            m11: scale,
            m12: 0.0,
            m21: 0.0,
            m22: scale,
            m31: 0.0,
            m32: 0.0,
        });
        for (top, from, to) in [
            (
                0.0,
                ColorF::new(0.80, 0.86, 0.95, 1.0),
                ColorF::new(0.97, 0.93, 0.86, 1.0),
            ),
            (
                CANVAS.1 / 2.0,
                ColorF::new(0.06, 0.08, 0.16, 1.0),
                ColorF::new(0.20, 0.09, 0.22, 1.0),
            ),
        ] {
            let band = s.create_linear_gradient(
                Vector2::new(0.0, top),
                Vector2::new(CANVAS.0, top + CANVAS.1 / 2.0),
                &[GradientStop::new(0.0, from), GradientStop::new(1.0, to)],
            )?;
            s.fill_rect(&Rect::from_xywh(0.0, top, CANVAS.0, CANVAS.1 / 2.0), &band);
        }
        let colors = [
            ColorF::new(0.95, 0.35, 0.30, 0.85),
            ColorF::new(0.15, 0.62, 0.92, 0.85),
            ColorF::new(0.98, 0.78, 0.20, 0.85),
            ColorF::new(0.30, 0.78, 0.45, 0.85),
        ];
        for i in 0..28 {
            let brush = s.create_solid_brush(colors[i % colors.len()])?;
            let x = -400.0 + i as f32 * 52.0;
            s.draw_line(
                Vector2::new(x, 0.0),
                Vector2::new(x + CANVAS.1, CANVAS.1),
                &brush,
                9.0,
            );
        }
        let grid = s.create_solid_brush(ColorF::new(1.0, 1.0, 1.0, 0.35))?;
        for x in (0..CANVAS.0 as i32).step_by(24) {
            s.fill_rect(&Rect::from_xywh(x as f32, 0.0, 1.0, CANVAS.1), &grid);
        }
        for y in (0..CANVAS.1 as i32).step_by(24) {
            s.fill_rect(&Rect::from_xywh(0.0, y as f32, CANVAS.0, 1.0), &grid);
        }
        for (cx, cy, r, c) in [
            (120.0, 90.0, 46.0, 1),
            (520.0, 300.0, 38.0, 0),
            (880.0, 120.0, 52.0, 3),
            (200.0, 560.0, 50.0, 2),
            (760.0, 610.0, 44.0, 1),
        ] {
            let brush = s.create_solid_brush(colors[c])?;
            s.fill_rounded_rect(
                &RoundedRect::uniform(Rect::from_xywh(cx - r, cy - r, r * 2.0, r * 2.0), r),
                &brush,
            );
        }
        for (row, ink) in [
            (0, ColorF::new(0.08, 0.09, 0.12, 1.0)),
            (1, ColorF::new(0.95, 0.96, 0.98, 1.0)),
        ] {
            let brush = s.create_solid_brush(ink)?;
            for line in 0..9 {
                let y = row as f32 * CANVAS.1 / 2.0 + 30.0 + line as f32 * 36.0;
                s.draw_text(
                    "PecoFence Liquid Glass · refraction study 0123456789 ▲●■",
                    &font,
                    &Rect::from_xywh(30.0 + (line % 3) as f32 * 120.0, y, 900.0, 24.0),
                    &brush,
                );
            }
        }
        Ok(())
    })?;
    Ok(Image {
        width: w,
        height: h,
        bgra: target.read_pixels()?,
    })
}

struct Plate {
    /// Device-pixel rectangle on the canvas.
    rect: [i32; 4],
    radius_px: f32,
    pixels: Vec<u8>,
}

fn rounded_distance(px: f32, py: f32, w: f32, h: f32, r: f32) -> f32 {
    let qx = (px - w / 2.0).abs() - (w / 2.0 - r);
    let qy = (py - h / 2.0).abs() - (h / 2.0 - r);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::PathBuf::from(args.first().cloned().unwrap_or(".cache/materials".into()));
    let scale: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1.5);
    let builtin = args.iter().any(|a| a == "--builtin");
    std::fs::create_dir_all(&out)?;
    let _ole = pecofence_platform::com::OleGuard::init()?;
    let gpu = GpuDevice::new_or_warp()?;
    let paper = wallpaper(&gpu, scale)?;
    let (cw, ch) = (paper.width, paper.height);
    let mut canvas = paper.bgra.clone();
    let chrome = FenceChrome::new()?;
    let font = TextFormat::new("Segoe UI", 13.0)?;
    let mut plates = Vec::new();

    for (row, dark) in [(0, false), (1, true)] {
        for col in 0..3 {
            let (glass, is_paper) = (col == 1, col == 2);
            let base = if dark { Theme::dark() } else { Theme::light() };
            let theme = match col {
                1 => base.with_liquid_glass(),
                2 => base.with_paper(),
                _ => base,
            };
            let left = (30.0 + col as f32 * (PLATE.0 + 50.0)) * scale;
            let top = (45.0 + row as f32 * CANVAS.1 / 2.0) * scale;
            let (w, h) = ((PLATE.0 * scale) as i32, (PLATE.1 * scale) as i32);
            let rect = [left as i32, top as i32, w, h];
            let backdrop = MonitorBackdrop::build(
                &paper,
                WallpaperPosition::Fill,
                0,
                0,
                cw as i32,
                ch as i32,
                [0, 0, 0],
                {
                    let mut tint = theme.acrylic_tint();
                    tint.blur_sigma_dip *= scale;
                    tint
                },
                1,
            );
            let wallpaper = Rc::new(vec![backdrop]);
            let crop = wallpaper[0].crop_screen_rect(rect[0], rect[1], w, h);
            let theme = if glass {
                theme.over_glass(&liquid_glass::foreground_sample(&wallpaper, rect))
            } else {
                theme
            };
            let material = RefCell::new(GpuGlass::default());
            if builtin {
                material.borrow_mut().use_builtin_effects();
            }
            let mut bitmaps = BitmapCache::new();
            let target = gpu.create_render_target(w as u32, h as u32)?;
            target.draw(|s| {
                s.set_transform(&Matrix3x2 {
                    m11: scale,
                    m12: 0.0,
                    m21: 0.0,
                    m22: scale,
                    m31: 0.0,
                    m32: 0.0,
                });
                let backdrop = if glass || is_paper {
                    Backdrop::GpuGlass {
                        material: &material,
                        wallpaper: &wallpaper,
                        rect,
                        scale,
                        seed: (row * 2 + col) as f32 * 97.0,
                    }
                } else {
                    Backdrop::Glass(
                        BackdropCrop {
                            image: &crop,
                            key: "fluent",
                            height: PLATE.1,
                        },
                        scale,
                    )
                };
                chrome.draw(
                    s,
                    &mut bitmaps,
                    &theme,
                    scale,
                    PLATE.0,
                    PLATE.1,
                    backdrop,
                    ["Fluent", "Liquid Glass", "Paper"][col],
                    None,
                    false,
                    0.0,
                    1.0,
                    &[],
                    None,
                    TitleDeco::default(),
                    TitleState {
                        title: 1.0,
                        ..Default::default()
                    },
                    FenceStyle::default(),
                )?;
                let ink = s.create_solid_brush(theme.text_primary)?;
                for (i, name) in ["Documents", "Pictures", "Projects"]
                    .into_iter()
                    .enumerate()
                {
                    let x = 28.0 + i as f32 * 96.0;
                    let icon = s.create_solid_brush(
                        [
                            ColorF::new(0.95, 0.72, 0.25, 1.0),
                            ColorF::new(0.27, 0.56, 0.90, 1.0),
                            ColorF::new(0.58, 0.44, 0.80, 1.0),
                            ColorF::new(0.35, 0.72, 0.42, 1.0),
                        ][i],
                    )?;
                    s.fill_rounded_rect(
                        &RoundedRect::uniform(Rect::from_xywh(x + 10.0, 70.0, 44.0, 38.0), 6.0),
                        &icon,
                    );
                    s.draw_text(
                        name,
                        &font,
                        &Rect::from_xywh(x - 4.0, 116.0, 90.0, 22.0),
                        &ink,
                    );
                }
                Ok(())
            })?;
            if glass || is_paper {
                println!(
                    "{} {}: glass shader {:?}, failed {}",
                    ["Fluent", "Liquid Glass", "Paper"][col],
                    if dark { "dark" } else { "light" },
                    material.borrow().uses_shader(),
                    material.borrow().failed()
                );
            }
            plates.push(Plate {
                rect,
                radius_px: theme.corner_radius * scale,
                pixels: target.read_pixels()?,
            });
        }
    }

    // Software equivalent of the window clip (antialiased rounded outline) and a soft
    // outside shadow, as the composition root and DWM produce for the real windows.
    for plate in &plates {
        let [left, top, w, h] = plate.rect;
        let r = plate.radius_px;
        let pad = (24.0 * scale) as i32;
        for y in (top - pad).max(0)..(top + h + pad).min(ch as i32) {
            for x in (left - pad).max(0)..(left + w + pad).min(cw as i32) {
                let (px, py) = ((x - left) as f32 + 0.5, (y - top) as f32 + 0.5);
                let d = rounded_distance(px, py, w as f32, h as f32, r);
                let dst = ((y as u32 * cw + x as u32) * 4) as usize;
                if d > 0.0 {
                    let sd = rounded_distance(px, py - 6.0 * scale, w as f32, h as f32, r).max(0.0);
                    let sigma = 12.0 * scale;
                    let shadow = (-sd * sd / (2.0 * sigma * sigma)).exp() * 0.16;
                    for c in 0..3 {
                        canvas[dst + c] = (canvas[dst + c] as f32 * (1.0 - shadow)) as u8;
                    }
                }
                if x >= left && x < left + w && y >= top && y < top + h {
                    let a = (0.5 - d).clamp(0.0, 1.0);
                    let src = (((y - top) * w + x - left) * 4) as usize;
                    let sa = plate.pixels[src + 3] as f32 / 255.0 * a;
                    for c in 0..3 {
                        canvas[dst + c] = (plate.pixels[src + c] as f32 * a
                            + canvas[dst + c] as f32 * (1.0 - sa))
                            .round() as u8;
                    }
                }
            }
        }
    }
    save_bmp(&out.join("materials.bmp"), cw, ch, &canvas)?;

    // Corner sheet: per plate, its top-left and bottom-right corner side by side.
    let side = (CORNER_DIP * scale) as u32;
    let cell = side * ZOOM;
    let (sw, sh) = (cell * 2 * 3 + 24, cell * 2 + 4);
    let mut sheet = vec![40u8; (sw * sh * 4) as usize];
    for (index, plate) in plates.iter().enumerate() {
        let [left, top, w, h] = plate.rect;
        let (row, col) = ((index / 3) as u32, (index % 3) as u32);
        for (k, (ox, oy)) in [
            (left - (side as i32) / 4, top - (side as i32) / 4),
            (
                left + w - side as i32 * 3 / 4,
                top + h - side as i32 * 3 / 4,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let x0 = col * (cell * 2 + 12) + k as u32 * cell;
            let y0 = row * (cell + 4);
            for y in 0..cell {
                for x in 0..cell {
                    let (sx, sy) = (ox + (x / ZOOM) as i32, oy + (y / ZOOM) as i32);
                    if sx < 0 || sy < 0 || sx >= cw as i32 || sy >= ch as i32 {
                        continue;
                    }
                    let src = ((sy as u32 * cw + sx as u32) * 4) as usize;
                    let dst = (((y0 + y) * sw + x0 + x) * 4) as usize;
                    sheet[dst..dst + 4].copy_from_slice(&canvas[src..src + 4]);
                }
            }
        }
    }
    save_bmp(&out.join("corners.bmp"), sw, sh, &sheet)?;
    println!("Saved {} at {scale}x", out.display());
    Ok(())
}

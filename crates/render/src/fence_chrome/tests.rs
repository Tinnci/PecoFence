use super::helpers::{StateBrushes, draw_item_state, empty_text_top};
use super::types::{Px, TitleDeco};
use super::*;
use crate::{BitmapCache, Theme};
use std::cell::RefCell;
use std::rc::Rc;
use windows_canvas::{ColorF, Rect, TextLayout};

#[test]
fn contrast_chrome_bypasses_sampled_material_even_with_user_tint_and_low_opacity() {
    let _ole = pecofence_platform::com::OleGuard::init().unwrap();
    let stack = crate::RenderStack::new().unwrap();
    let mut surface = crate::Panel::new(&stack).unwrap();
    surface.resize(320, 180).unwrap();
    let chrome = FenceChrome::new().unwrap();
    let material = RefCell::new(crate::gpu_glass::GpuGlass::default());
    let wallpaper = Rc::new(Vec::new());
    // Any accidental material or foreground-sampling call would panic on
    // this borrow. Contrast rendering must use the opaque system pair.
    let _no_material_access = material.borrow_mut();
    let mut bitmaps = BitmapCache::new();
    for (bg, text) in [([0; 3], [255; 3]), ([255; 3], [0; 3])] {
        let theme = Theme::dark()
            .with_liquid_glass()
            .with_contrast_colors(bg, text);
        for dpi in [96, 144] {
            assert!(
                surface
                    .draw_candidate(dpi, |session, width, height| {
                        chrome.draw(
                            session,
                            &mut bitmaps,
                            &theme,
                            dpi as f32 / 96.0,
                            width,
                            height,
                            Backdrop::GpuGlass {
                                material: &material,
                                wallpaper: &wallpaper,
                                rect: [0, 0, 320, 180],
                                scale: dpi as f32 / 96.0,
                                seed: 0.0,
                            },
                            "文件 Archives",
                            Some("中文 details"),
                            false,
                            0.0,
                            0.4,
                            &[],
                            None,
                            TitleDeco {
                                up_button: true,
                                folder_icon: true,
                                captions: false,
                            },
                            TitleState {
                                hover: 1.0,
                                up_hover: 1.0,
                                up_pressed: true,
                                chevron_hover: 1.0,
                                chevron_pressed: true,
                                ..TitleState::default()
                            },
                            FenceStyle {
                                tint: Some(ColorF::new(1.0, 0.0, 0.0, 1.0)),
                                title_color: Some(ColorF::new(1.0, 0.0, 1.0, 1.0)),
                                title_size: 1,
                            },
                            &TitleChrome::default(),
                        )?;
                        let brushes = StateBrushes::new(session, &theme)?;
                        for (hover, selection, pressed, focused) in [
                            (1.0, 0.0, false, false),
                            (1.0, 0.0, true, false),
                            (1.0, 1.0, false, false),
                            (1.0, 1.0, true, true),
                        ] {
                            draw_item_state(
                                session,
                                &theme,
                                &brushes,
                                Px::new(dpi as f32 / 96.0),
                                Rect::from_xywh(12.0, 48.0, 80.0, 32.0),
                                hover,
                                selection,
                                0.0,
                                pressed,
                                focused,
                                0.0,
                            );
                        }
                        Ok(())
                    })
                    .unwrap()
            );
        }
    }
}

#[test]
fn title_tab_and_count_fonts_share_text_scale_and_height_returns_to_baseline() {
    let chrome = FenceChrome::new().unwrap();
    for factor in [1.0, 2.25, 1.5, 1.0] {
        chrome.set_text_scale(factor).unwrap();
        let typography = chrome.typography.borrow();
        for size in 0..3 {
            let single = TextLayout::new("文件 Archives", &typography.titles[size], 1000.0, 100.0)
                .unwrap()
                .metrics();
            let tab = TextLayout::new("文件 Archives", &typography.tabs[size], 1000.0, 100.0)
                .unwrap()
                .metrics();
            assert_eq!(single.height, tab.height);
            assert!((single.width - tab.width).abs() < 0.001);
            assert!(typography.title_height >= single.height.ceil() + 12.0);
        }
        let count = TextLayout::new("12 items", &typography.count, 1000.0, 100.0)
            .unwrap()
            .metrics();
        assert!(typography.title_height > count.height);
        if factor == 1.0 {
            assert_eq!(typography.title_height, 36.0);
        } else {
            assert!(typography.title_height > 36.0);
        }
    }
}

#[test]
fn typography_scales_all_roles_and_reports_their_actual_line_heights() {
    let chrome = FenceChrome::new().unwrap();
    chrome.set_text_scale(1.0).unwrap();
    let base = chrome.row_line_h();
    for factor in [1.0, 1.5, 2.0, 2.25] {
        chrome.set_text_scale(factor).unwrap();
        let typography = chrome.typography.borrow();
        for (format, line_h) in [
            (&typography.row, typography.row_line_h),
            (&typography.row_right, typography.row_line_h),
            (&typography.group, typography.group_line_h),
            (&typography.empty, typography.empty_line_h),
        ] {
            let measured = TextLayout::new("Ag", format, 10_000.0, 10_000.0)
                .unwrap()
                .metrics()
                .height
                .ceil();
            assert_eq!(line_h, measured);
            assert!(line_h >= base);
        }
        let column = crate::text::fit_width("2026-10-17 15:30", &typography.row_right, 76.0);
        assert!(crate::text::measure_width(&column, &typography.row_right) <= 76.0);
        assert_eq!(typography.scale, factor);
        drop(typography);
        assert!(!chrome.set_text_scale(factor).unwrap());
    }
    chrome.set_text_scale(f32::NAN).unwrap();
    assert_eq!(chrome.typography.borrow().scale, 1.0);
    chrome.set_text_scale(10.0).unwrap();
    assert_eq!(chrome.typography.borrow().scale, 2.25);
}

#[test]
fn stacked_and_standalone_titles_have_identical_metrics() {
    let chrome = FenceChrome::new().unwrap();
    let mut widths = Vec::new();
    for size in 0..=2 {
        for caption in ["文件与文档", "Folder pictures", "资料 Archives 2026"] {
            let single = TextLayout::new(caption, &chrome.title_format(size), 1000.0, 100.0)
                .unwrap()
                .metrics();
            let tab = TextLayout::new(caption, &chrome.tab_format(size), 1000.0, 100.0)
                .unwrap()
                .metrics();
            assert!((single.width - tab.width).abs() < 0.001);
            assert!((single.height - tab.height).abs() < 0.001);
            for width in [0.0, 2.0, 8.0, 20.0, 80.0] {
                let text = crate::text::fit_width(caption, &chrome.tab_format(size), width);
                assert!(
                    crate::text::measure_width(&text, &chrome.tab_format(size)) <= width + 0.001
                );
            }
        }
        widths.push(crate::text::measure_width(
            "文件与文档",
            &chrome.tab_format(size),
        ));
    }
    assert!(widths[0] < widths[1] && widths[1] < widths[2]);
}

#[test]
fn glass_header_controls_follow_the_plate_and_leave_room_for_captions() {
    let theme = Theme::light().with_liquid_glass();
    for width in [40.0, 80.0, 160.0] {
        let rect = Rect::from_xywh(8.0, 4.0, width, 28.0);
        assert_eq!(header_control_radius(&theme, rect), 14.0);
    }
    assert_eq!(
        header_control_radius(&theme, TitleDeco::chevron_box(320.0, 36.0)),
        16.0
    );
    assert_eq!(header_count_width(136.0, 1, 16.0), 0.0);
    assert_eq!(header_count_width(136.0, 2, 16.0), 0.0);
    assert_eq!(header_count_width(400.0, 2, 16.0), ROLLED_COUNT_W);
}

#[test]
fn chevron_turns_half_a_circle_with_the_roll() {
    assert_eq!(chevron_angle(0.0), 0.0);
    assert_eq!(chevron_angle(0.5), 90.0);
    assert_eq!(chevron_angle(1.0), 180.0);
    // Overshooting curves stay inside the half turn.
    assert_eq!(chevron_angle(1.2), 180.0);
    assert_eq!(chevron_angle(-0.1), 0.0);
}

/// The thumb keeps its centre while it widens, and that centre is the 12 DIP track's
/// (6 DIP from the right edge, where the track fill and arrow buttons are centred too).
#[test]
fn scrollbar_thumb_widens_about_its_centre() {
    let w = 300.0;
    let rest = ScrollbarDraw::thumb_x(w, ScrollbarDraw::REST_WIDTH);
    let wide = ScrollbarDraw::thumb_x(w, ScrollbarDraw::EXPANDED_WIDTH);
    assert_eq!(rest, w - 7.0);
    assert_eq!(wide, w - 9.0);
    assert_eq!(
        rest + ScrollbarDraw::REST_WIDTH / 2.0,
        w - ScrollbarDraw::BAR_W / 2.0
    );
    let centre = |x: f32, width: f32| x + width / 2.0;
    assert_eq!(
        centre(rest, ScrollbarDraw::REST_WIDTH),
        centre(wide, ScrollbarDraw::EXPANDED_WIDTH)
    );
    // Intermediate widths stay between the two end positions.
    let mid = ScrollbarDraw::thumb_x(w, 4.0);
    assert!(mid < rest && mid > wide);
}

#[test]
fn hairlines_are_whole_device_pixels_at_every_dpi() {
    for scale in [1.0f32, 1.25, 1.5, 1.75, 2.0, 2.5] {
        let px = Px::new(scale);
        let hair_px = px.hair() * scale;
        assert!(
            (hair_px - hair_px.round()).abs() < 1e-4,
            "{scale}: {hair_px}"
        );
        assert!(hair_px >= 1.0);
        // A snapped coordinate lands on a pixel boundary.
        let v = px.snap(7.3) * scale;
        assert!((v - v.round()).abs() < 1e-4, "{scale}");
        // The stroke rect is centred on whole pixels: edge ± hair/2 is on the grid.
        let r = px.stroke_rect(Rect::from_xywh(0.0, 0.0, 100.0, 50.0));
        let edge = (r.left - px.hair() / 2.0) * scale;
        assert!((edge - edge.round()).abs() < 1e-4, "{scale}");
        let w = (r.right - r.left + px.hair()) * scale;
        assert!((w - w.round()).abs() < 1e-3, "{scale}: {w}");
    }
    // 100 %: identity, the classic 0.5 centring.
    let px = Px::new(1.0);
    assert_eq!(px.hair(), 1.0);
    let r = px.stroke_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0));
    assert_eq!((r.left, r.top, r.right, r.bottom), (0.5, 0.5, 9.5, 9.5));
    assert_eq!(Px::new(0.0).hair(), 1.0);
    assert_eq!(Px::new(f32::NAN).grid.scale(), 1.0);
}

#[test]
fn chevron_and_up_boxes() {
    let c = TitleDeco::chevron_box(300.0, 36.0);
    assert_eq!(
        (c.left, c.top, c.right, c.bottom),
        (264.0, 2.0, 296.0, 34.0)
    );
    let u = TitleDeco::up_box(36.0);
    assert_eq!((u.left, u.top, u.right, u.bottom), (6.0, 4.0, 34.0, 32.0));
}

#[test]
fn empty_text_sits_in_the_upper_part_or_centres_when_short() {
    assert_eq!(empty_text_top(200.0, 20.0), 48.0);
    assert_eq!(empty_text_top(60.0, 20.0), 20.0);
    assert_eq!(empty_text_top(10.0, 20.0), 0.0);
    assert_eq!(empty_text_top(60.0, 40.0), 10.0);
    assert_eq!(empty_text_top(30.0, 40.0), 0.0);
}

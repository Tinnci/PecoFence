//! A sprite visual backed by a Direct2D drawing surface, sized in physical pixels and drawn
//! in device-independent pixels.

use crate::stack::RenderStack;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use windows_canvas::{
    DrawingSession, ID2D1DeviceContext, Matrix3x2, Rect, check_device_lost, is_device_lost,
};
use windows_composition::{
    CompositionDrawingSurface, CompositionGraphicsDevice, Compositor, SpriteVisual,
};
use windows_core::Result;

/// Clip control for the surface being drawn (see [`Panel::draw_ex`]). windows-canvas has no
/// clip API, so this reaches the device context behind the session.
pub struct Clip<'a> {
    context: &'a ID2D1DeviceContext,
}

impl Clip<'_> {
    /// Runs `f` with drawing clipped to `rect` (DIPs, the same space as the session's drawing
    /// calls); the clip is popped afterwards even when `f` fails.
    pub fn scoped(&self, rect: &Rect, f: impl FnOnce() -> Result<()>) -> Result<()> {
        let _clip = pecofence_platform::d2d::push_axis_aligned_clip(
            self.context,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
        )?;
        f()
    }
}

/// Ends the surface draw only on the panic path (mirrors the windows-canvas composition bridge).
struct EndDrawGuard<'a>(&'a CompositionDrawingSurface);

impl Drop for EndDrawGuard<'_> {
    fn drop(&mut self) {
        let _ = self.0.end_draw();
    }
}

// A cache identity for backing storage, independent of DPI or window size.
// Returning to the same size must not revive geometry for pixels discarded by resize.
static NEXT_SURFACE_EPOCH: AtomicU64 = AtomicU64::new(1);
fn next_surface_epoch() -> u64 {
    let mut epoch = NEXT_SURFACE_EPOCH.load(Ordering::Relaxed);
    loop {
        let next = epoch.checked_add(1).expect("surface epoch exhausted");
        match NEXT_SURFACE_EPOCH.compare_exchange_weak(
            epoch,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return epoch,
            Err(current) => epoch = current,
        }
    }
}

pub struct Panel {
    pub visual: SpriteVisual,
    surface: RefCell<Rc<CompositionDrawingSurface>>,
    staging: RefCell<Option<Rc<CompositionDrawingSurface>>>,
    graphics: CompositionGraphicsDevice,
    compositor: Compositor,
    width_px: i32,
    height_px: i32,
    surface_epoch: u64,
}

impl Panel {
    pub fn new(stack: &RenderStack) -> Result<Self> {
        let surface = stack.graphics().create_drawing_surface(1.0, 1.0)?;
        let visual = stack.compositor.create_sprite_visual();
        visual.set_brush(&stack.compositor.create_surface_brush(&surface));
        visual.set_size(1.0, 1.0);
        Ok(Self {
            visual,
            surface: RefCell::new(Rc::new(surface)),
            staging: RefCell::new(None),
            graphics: stack.graphics(),
            compositor: stack.compositor.clone(),
            width_px: 1,
            height_px: 1,
            surface_epoch: next_surface_epoch(),
        })
    }

    /// Changes when a panel is created or its backing pixels are discarded.
    pub fn surface_epoch(&self) -> u64 {
        self.surface_epoch
    }

    pub fn size_px(&self) -> (i32, i32) {
        (self.width_px, self.height_px)
    }

    /// Replaces the backing surface with one from the (rebuilt) graphics device, keeping the
    /// visual in place. Contents must be redrawn afterwards.
    pub fn recreate(&mut self, stack: &RenderStack) -> Result<()> {
        let surface = stack
            .graphics()
            .create_drawing_surface(self.width_px as f32, self.height_px as f32)?;
        self.visual
            .set_brush(&stack.compositor.create_surface_brush(&surface));
        *self.surface.get_mut() = Rc::new(surface);
        self.surface_epoch = next_surface_epoch();
        *self.staging.get_mut() = None;
        self.graphics = stack.graphics();
        self.compositor = stack.compositor.clone();
        Ok(())
    }

    /// Resizes the backing surface (physical pixels). Contents must be redrawn afterwards.
    pub fn resize(&mut self, width_px: i32, height_px: i32) -> Result<bool> {
        let width_px = width_px.max(1);
        let height_px = height_px.max(1);
        if (width_px, height_px) == (self.width_px, self.height_px) {
            return Ok(false);
        }
        self.surface.get_mut().resize(width_px, height_px)?;
        *self.staging.get_mut() = None;
        self.visual.set_size(width_px as f32, height_px as f32);
        self.width_px = width_px;
        self.height_px = height_px;
        self.surface_epoch = next_surface_epoch();
        Ok(true)
    }

    /// Redraws the whole surface. `draw` receives the session with a DIP transform applied
    /// for `dpi`, plus the surface size in DIPs. Returns `Ok(false)` on device loss.
    pub fn draw(
        &self,
        dpi: u32,
        draw: impl FnOnce(&DrawingSession<'_>, f32, f32) -> Result<()>,
    ) -> Result<bool> {
        self.draw_ex(dpi, |session, _clip, w, h| draw(session, w, h))
    }

    /// [`Panel::draw`] plus a [`Clip`] handle for the callback. Same BeginDraw/EndDraw bracket
    /// and device-loss classification as `CanvasCompositionExt::draw`, done here because that
    /// bridge keeps the device context private.
    pub fn draw_ex(
        &self,
        dpi: u32,
        draw: impl FnOnce(&DrawingSession<'_>, &Clip<'_>, f32, f32) -> Result<()>,
    ) -> Result<bool> {
        self.draw_surface(&self.surface.borrow(), dpi, draw)
    }

    /// Paint into an unpublished surface. Only a successful callback and EndDraw
    /// publish the candidate; rejected frames leave the previous pixels intact.
    /// A second surface is allocated lazily and reused until resize/device reset.
    pub fn draw_candidate(
        &self,
        dpi: u32,
        draw: impl FnOnce(&DrawingSession<'_>, f32, f32) -> Result<()>,
    ) -> Result<bool> {
        self.draw_candidate_ex(dpi, |session, _, w, h| draw(session, w, h))
    }

    /// Candidate submission with the same clip access as `draw_ex`.
    pub fn draw_candidate_ex(
        &self,
        dpi: u32,
        draw: impl FnOnce(&DrawingSession<'_>, &Clip<'_>, f32, f32) -> Result<()>,
    ) -> Result<bool> {
        let mut staging = self.staging.borrow_mut();
        if staging.is_none() {
            *staging = Some(Rc::new(
                self.graphics
                    .create_drawing_surface(self.width_px as f32, self.height_px as f32)?,
            ));
        }
        let candidate = staging.as_ref().expect("staging surface allocated");
        if !self.draw_surface(candidate, dpi, draw)? {
            return Ok(false);
        }
        self.visual
            .set_brush(&self.compositor.create_surface_brush(candidate));
        std::mem::swap(&mut *self.surface.borrow_mut(), staging.as_mut().unwrap());
        Ok(true)
    }

    fn draw_surface(
        &self,
        surface: &CompositionDrawingSurface,
        dpi: u32,
        draw: impl FnOnce(&DrawingSession<'_>, &Clip<'_>, f32, f32) -> Result<()>,
    ) -> Result<bool> {
        let scale = dpi as f32 / 96.0;
        let width_dip = self.width_px as f32 / scale;
        let height_dip = self.height_px as f32 / scale;
        let (context, (offset_x, offset_y)) = match surface.begin_draw::<ID2D1DeviceContext>() {
            Ok(v) => v,
            Err(e) if is_device_lost(e.code()) => return Ok(false),
            Err(e) => return Err(e),
        };
        let guard = EndDrawGuard(surface);
        let draw_result = {
            let session = DrawingSession::from_borrowed_context_with_dpi(
                &context,
                Matrix3x2::translation(offset_x as f32, offset_y as f32),
                96.0,
            );
            session.set_transform(&Matrix3x2 {
                m11: scale,
                m12: 0.0,
                m21: 0.0,
                m22: scale,
                m31: 0.0,
                m32: 0.0,
            });
            let clip = Clip { context: &context };
            draw(&session, &clip, width_dip, height_dip)
        };
        std::mem::forget(guard);
        let end_result = surface.end_draw();
        if check_device_lost(&draw_result) || check_device_lost(&end_result) {
            return Ok(false);
        }
        end_result?;
        draw_result.map(|()| true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_canvas::ColorF;

    #[test]
    fn scaled_fence_fonts_draw_chrome_and_rows_at_independent_monitor_dpis() {
        use crate::fence_chrome::{
            FenceChrome, GroupHeaderDraw, HeaderColumn, RowCell, RowColumns, RowsDraw,
        };
        use pecofence_plugin_api::file_geometry::text_band_height;
        let _ole = pecofence_platform::com::OleGuard::init().unwrap();
        let stack = RenderStack::new().unwrap();
        let chrome = FenceChrome::new().unwrap();
        let mut panel = Panel::new(&stack).unwrap();
        let mut bitmaps = crate::BitmapCache::new();
        for text_scale in [1.0, 1.5, 2.25] {
            chrome.set_text_scale(text_scale).unwrap();
            let row_h = text_band_height(28.0, chrome.row_line_h());
            let group_h = text_band_height(24.0, chrome.group_line_h());
            let rows = [RowCell {
                y: row_h + group_h - 4.0,
                h: row_h,
                icon_key: "typography-test",
                icon: None,
                name: "很长的文件名 Long filename.txt",
                date: "2026-10-17 15:30",
                type_name: "Text document",
                size: "123456 KB",
                hover: 1.0,
                selection: 1.0,
                pressed: false,
                focused: true,
                dimmed: false,
                failed: true,
                drop_target: 0.0,
                alpha: 1.0,
                icon_alpha: 1.0,
            }];
            let groups = [GroupHeaderDraw {
                y: row_h - 4.0,
                h: group_h,
                text: "Today 今天",
            }];
            let mut draw = RowsDraw {
                rows: &rows,
                group_headers: &groups,
                columns: RowColumns {
                    name_x: 8.0,
                    name_w: 132.0,
                    date: Some((148.0, 64.0)),
                    type_: None,
                    size: Some((228.0, 60.0)),
                },
                icon_size: 20.0,
                header_h: row_h,
                sort_column: Some(HeaderColumn::Date),
                sort_descending: false,
                header_hover: &[],
                header_pressed: None,
                scrollbar: None,
                empty_text: Some("Empty 空文件夹"),
                drop_highlight: 0.0,
                marquee: None,
                insert_caret: None,
                insert_caret_alpha: 0.0,
                selection_inactive: 0.0,
            };
            for dpi in [96, 120, 144, 192] {
                let scale = dpi as f32 / 96.0;
                panel
                    .resize((320.0 * scale) as i32, (180.0 * scale) as i32)
                    .unwrap();
                assert!(
                    panel
                        .draw_candidate_ex(dpi, |session, clip, w, h| {
                            chrome.draw_rows(
                                session,
                                Some(clip),
                                &crate::Theme::dark(),
                                scale,
                                &mut bitmaps,
                                w,
                                h,
                                &draw,
                                None,
                            )
                        })
                        .unwrap()
                );
            }
            for dpi in [96, 120, 144, 192] {
                use crate::fence_chrome::{Backdrop, FenceStyle, TitleDeco, TitleState};
                let scale = dpi as f32 / 96.0;
                let mut theme = crate::Theme::dark();
                theme.title_height = chrome.title_height();
                panel
                    .resize(
                        (320.0 * scale) as i32,
                        (theme.title_height * scale).round() as i32,
                    )
                    .unwrap();
                for title_size in 0..=2 {
                    assert!(
                        panel
                            .draw_candidate(dpi, |session, w, h| {
                                chrome.draw(
                                    session,
                                    &mut bitmaps,
                                    &theme,
                                    scale,
                                    w,
                                    h,
                                    Backdrop::SystemMaterial,
                                    "文件 Archives 很长的标题",
                                    Some("120000 items"),
                                    true,
                                    1.0,
                                    1.0,
                                    &[],
                                    None,
                                    TitleDeco {
                                        folder_icon: true,
                                        up_button: true,
                                    },
                                    TitleState {
                                        title: 1.0,
                                        hover: 1.0,
                                        count: 1.0,
                                        ..Default::default()
                                    },
                                    FenceStyle {
                                        title_size,
                                        ..Default::default()
                                    },
                                )
                            })
                            .unwrap()
                    );
                }
            }
            panel.resize(640, 360).unwrap();
            draw.rows = &[];
            draw.group_headers = &[];
            assert!(
                panel
                    .draw_candidate_ex(192, |session, clip, w, h| {
                        chrome.draw_rows(
                            session,
                            Some(clip),
                            &crate::Theme::dark(),
                            2.0,
                            &mut bitmaps,
                            w,
                            h,
                            &draw,
                            None,
                        )
                    })
                    .unwrap()
            );
        }
    }

    #[test]
    fn candidate_surface_is_published_only_after_success_and_reused() {
        let _ole = pecofence_platform::com::OleGuard::init().unwrap();
        let stack = RenderStack::new().unwrap();
        let mut panel = Panel::new(&stack).unwrap();
        panel.resize(120, 80).unwrap();
        let first = panel.surface.borrow().clone();
        assert!(
            panel
                .draw_candidate(144, |session, w, h| {
                    assert_eq!((w, h), (80.0, 80.0 / 1.5));
                    session.clear(ColorF::new(0.0, 0.0, 0.0, 0.0));
                    Ok(())
                })
                .unwrap()
        );
        let committed = panel.surface.borrow().clone();
        assert!(!Rc::ptr_eq(&first, &committed));
        assert!(
            panel
                .draw_candidate(144, |session, _, _| {
                    session.clear(ColorF::new(1.0, 0.0, 0.0, 1.0));
                    Err(windows_core::Error::from_hresult(windows_core::HRESULT(
                        0x80004005u32 as i32,
                    )))
                })
                .is_err()
        );
        assert!(Rc::ptr_eq(&committed, &panel.surface.borrow()));
        assert!(
            panel
                .draw_candidate(144, |session, _, _| {
                    session.clear(ColorF::new(0.0, 1.0, 0.0, 1.0));
                    Ok(())
                })
                .unwrap()
        );
        assert!(Rc::ptr_eq(&first, &panel.surface.borrow()));
        let epoch = panel.surface_epoch();
        assert!(!panel.resize(120, 80).unwrap());
        assert_eq!(panel.surface_epoch(), epoch);
        panel.resize(240, 160).unwrap();
        panel.resize(120, 80).unwrap();
        assert_ne!(panel.surface_epoch(), epoch);
        assert!(panel.staging.borrow().is_none());
        let epoch = panel.surface_epoch();
        panel.recreate(&stack).unwrap();
        assert_ne!(panel.surface_epoch(), epoch);
        assert_ne!(
            panel.surface_epoch(),
            Panel::new(&stack).unwrap().surface_epoch()
        );
        assert!(panel.staging.borrow().is_none());
    }
}

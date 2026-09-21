//! Generic bridge between object-safe panel instances and the host composition surface.

use super::*;
use pecofence_plugin_api::{
    Canvas, InstanceKey, LayoutInput, LayoutSnapshot, MountContext, MountKey, PanelEvent,
    PanelInstance, Presentation, RectDip, StopReason, TextSpec,
};
use pecofence_render::TextFormat;

#[derive(Clone)]
pub(crate) struct PanelHandle {
    key: InstanceKey,
    panel: Rc<RefCell<Box<dyn PanelInstance>>>,
}

impl PanelHandle {
    pub(crate) fn new(key: InstanceKey, panel: Box<dyn PanelInstance>) -> Self {
        Self {
            key,
            panel: Rc::new(RefCell::new(panel)),
        }
    }

    pub(crate) fn key(&self) -> InstanceKey {
        self.key
    }

    pub(crate) fn stop(&mut self, reason: StopReason) {
        self.panel.borrow_mut().begin_stop(reason);
    }

    pub(crate) fn event(
        &self,
        event: PanelEvent,
    ) -> pecofence_plugin_api::Result<pecofence_plugin_api::PanelUpdate> {
        self.panel.borrow_mut().event(event)
    }

    pub(crate) fn mount(&self, context: MountContext) -> pecofence_plugin_api::Result<()> {
        self.panel.borrow_mut().mount(context)
    }

    pub(crate) fn unmount(&self, key: MountKey) {
        self.panel.borrow_mut().unmount(key);
    }
}

pub(super) struct PluginPanelContent {
    handle: PanelHandle,
    layout: LayoutSnapshot,
    pressed: Option<(u64, u64)>,
}

impl PluginPanelContent {
    pub fn new(handle: PanelHandle) -> Self {
        Self {
            handle,
            layout: LayoutSnapshot::default(),
            pressed: None,
        }
    }

    pub fn key(&self) -> InstanceKey {
        self.handle.key()
    }

    pub fn draw(&mut self, surface: &Panel, dpi: u32, _theme: &Theme) -> Result<bool> {
        let (width_px, height_px) = surface.size_px();
        let scale = dpi.max(96) as f32 / 96.0;
        let viewport = RectDip {
            x: 0.0,
            y: 0.0,
            w: width_px as f32 / scale,
            h: height_px as f32 / scale,
        };
        let layout = match self.handle.panel.borrow_mut().layout(LayoutInput {
            viewport,
            mode: Presentation::Workspace,
            text_scale: 1.0,
        }) {
            Ok(layout) => layout,
            Err(error) => {
                tracing::warn!(%error, "panel layout rejected");
                return Ok(true);
            }
        };
        let panel = self.handle.panel.clone();
        let mut paint_error = None;
        let drawn = surface.draw(dpi, |session, _width, _height| {
            session.clear(ColorF {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            });
            let mut canvas = Direct2dCanvas { session };
            if let Err(error) = panel.borrow().paint(&mut canvas, &layout) {
                paint_error = Some(error);
            }
            Ok(())
        })?;
        if let Some(error) = paint_error {
            tracing::warn!(%error, "panel paint rejected");
        } else if drawn {
            self.layout = layout;
        }
        Ok(drawn)
    }

    fn action_at(&self, x: f32, y: f32) -> Option<u64> {
        self.layout
            .hits
            .iter()
            .rev()
            .find(|node| node.rect.contains(x, y))
            .map(|node| node.action)
    }
}

struct Direct2dCanvas<'a> {
    session: &'a pecofence_render::DrawingSession<'a>,
}

impl Canvas for Direct2dCanvas<'_> {
    fn fill(&mut self, rect: RectDip, rgba: [f32; 4]) -> pecofence_plugin_api::Result<()> {
        let brush = self
            .session
            .create_solid_brush(ColorF {
                r: rgba[0],
                g: rgba[1],
                b: rgba[2],
                a: rgba[3],
            })
            .map_err(|error| pecofence_plugin_api::Error::Backend(error.to_string()))?;
        self.session
            .fill_rect(&Rect::from_xywh(rect.x, rect.y, rect.w, rect.h), &brush);
        Ok(())
    }

    fn text(
        &mut self,
        rect: RectDip,
        text: &TextSpec,
        rgba: [f32; 4],
    ) -> pecofence_plugin_api::Result<()> {
        let brush = self
            .session
            .create_solid_brush(ColorF {
                r: rgba[0],
                g: rgba[1],
                b: rgba[2],
                a: rgba[3],
            })
            .map_err(|error| pecofence_plugin_api::Error::Backend(error.to_string()))?;
        let format = TextFormat::new("Segoe UI Variable", text.size_dip)
            .map_err(|error| pecofence_plugin_api::Error::Backend(error.to_string()))?;
        self.session.draw_text(
            &text.text,
            &format,
            &Rect::from_xywh(
                rect.x + 6.0,
                rect.y + 4.0,
                (rect.w - 12.0).max(0.0),
                (rect.h - 8.0).max(0.0),
            ),
            &brush,
        );
        Ok(())
    }
}

pub(super) fn handle_message(
    handler: &super::handler::HandlerCtx,
    message: u32,
    _wparam: usize,
    lparam: isize,
) -> Option<isize> {
    let mut guard = handler.view.try_borrow_mut().ok()?;
    let view = guard.as_mut()?;
    view.plugin_panel.as_ref()?;
    if message == msg::WM_GETMINMAXINFO {
        let scale = view.scale();
        // SAFETY: this branch is only reached for WM_GETMINMAXINFO.
        unsafe {
            window::minmaxinfo_set_min_track(
                lparam,
                (480.0 * scale).round() as i32,
                view.title_h_px() + (520.0 * scale).round() as i32,
            );
        }
        return Some(0);
    }
    let scale = view.scale();
    let x = msg::lo_i16(lparam) as f32 / scale;
    let y = (msg::hi_i16(lparam) as i32 - view.title_h_px()) as f32 / scale;
    let panel = view.plugin_panel.as_mut()?;
    match message {
        msg::WM_LBUTTONDOWN if y >= 0.0 => {
            panel.pressed = panel
                .action_at(x, y)
                .map(|action| (action, panel.layout.revision));
            let capture = panel.pressed.is_some();
            let hwnd = view.hwnd;
            drop(guard);
            if capture {
                window::set_capture(hwnd);
            }
            Some(0)
        }
        msg::WM_LBUTTONUP if y >= 0.0 || panel.pressed.is_some() => {
            let pressed = panel.pressed.take();
            let invoke = pressed.and_then(|(action, revision)| {
                (panel.layout.revision == revision && panel.action_at(x, y) == Some(action))
                    .then_some((action, revision))
            });
            if let Some((action, layout_revision)) = invoke {
                if let Err(error) = panel.handle.panel.borrow_mut().event(PanelEvent::Invoke {
                    action,
                    layout_revision,
                }) {
                    tracing::warn!(%error, "panel event rejected");
                }
                let _ = view.redraw_content();
            }
            drop(guard);
            window::release_capture();
            Some(0)
        }
        msg::WM_CAPTURECHANGED => {
            panel.pressed = None;
            Some(0)
        }
        _ => None,
    }
}

//! Generic bridge between object-safe panel instances and the host composition surface.

use super::*;
use pecofence_plugin_api::{
    Exposure, FrameInput, Grouping, InstanceKey, MountContext, MountKey, PanelEvent, PanelInstance,
    PreparedFrame, Presentation, RectDip, StopReason,
};

#[derive(Clone)]
pub(crate) struct PanelHandle {
    key: InstanceKey,
    panel: Rc<RefCell<Box<dyn PanelInstance>>>,
    presentation: Rc<Cell<Presentation>>,
    grouping: Rc<Cell<Grouping>>,
    exposure: Rc<Cell<Exposure>>,
    active: Rc<Cell<bool>>,
}

impl PanelHandle {
    pub(crate) fn new(key: InstanceKey, panel: Box<dyn PanelInstance>) -> Self {
        Self {
            key,
            panel: Rc::new(RefCell::new(panel)),
            presentation: Rc::new(Cell::new(Presentation::Workspace)),
            grouping: Rc::new(Cell::new(Grouping::Single)),
            exposure: Rc::new(Cell::new(Exposure::Desktop)),
            active: Rc::new(Cell::new(true)),
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
        if let PanelEvent::VisibilityChanged { visible } = &event {
            self.exposure.set(if *visible {
                Exposure::Desktop
            } else {
                Exposure::Hidden
            });
            self.active.set(*visible);
        }
        let update = self.panel.borrow_mut().event(event)?;
        for command in &update.commands {
            if let pecofence_plugin_api::HostCommand::RequestMode(mode) = command {
                self.set_presentation(*mode);
            }
        }
        Ok(update)
    }

    pub(crate) fn mount(&self, context: MountContext) -> pecofence_plugin_api::Result<()> {
        self.panel.borrow_mut().mount(context)
    }

    pub(crate) fn unmount(&self, key: MountKey) {
        self.panel.borrow_mut().unmount(key);
    }

    pub(crate) fn set_presentation(&self, presentation: Presentation) {
        self.presentation.set(presentation);
    }

    pub(crate) fn presentation(&self) -> Presentation {
        self.presentation.get()
    }

    pub(crate) fn exposure(&self) -> Exposure {
        self.exposure.get()
    }

    pub(crate) fn set_container_state(&self, grouping: Grouping, exposure: Exposure, active: bool) {
        self.grouping.set(grouping);
        self.exposure.set(exposure);
        self.active.set(active);
    }
}

pub(super) struct PluginPanelContent {
    handle: PanelHandle,
    committed: Option<Rc<dyn PreparedFrame>>,
    pressed: Option<(u64, Rc<dyn PreparedFrame>)>,
}

impl PluginPanelContent {
    pub fn new(handle: PanelHandle) -> Self {
        Self {
            handle,
            committed: None,
            pressed: None,
        }
    }

    pub fn key(&self) -> InstanceKey {
        self.handle.key()
    }

    #[allow(dead_code)]
    pub fn set_presentation(&self, presentation: Presentation) {
        self.handle.set_presentation(presentation);
    }

    pub fn presentation(&self) -> Presentation {
        self.handle.presentation()
    }

    pub fn exposure(&self) -> Exposure {
        self.handle.exposure()
    }

    pub fn set_container_state(&self, grouping: Grouping, exposure: Exposure, active: bool) {
        self.handle.set_container_state(grouping, exposure, active);
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
        if self.handle.exposure.get() == Exposure::Hidden || !self.handle.active.get() {
            return Ok(false);
        }
        let candidate = match self.handle.panel.borrow_mut().prepare_frame(FrameInput {
            viewport,
            presentation: self.handle.presentation.get(),
            grouping: self.handle.grouping.get(),
            exposure: self.handle.exposure.get(),
            active: self.handle.active.get(),
            text_scale: 1.0,
            theme_epoch: 0,
            device_epoch: u64::from(dpi),
        }) {
            Ok(frame) => frame,
            Err(error) => {
                tracing::warn!(%error, "panel frame preparation rejected");
                return Ok(true);
            }
        };
        let mut paint_error = None;
        let paint_frame = candidate.clone();
        let drawn = surface.draw(dpi, |session, _width, _height| {
            session.clear(ColorF {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            });
            let mut canvas = pecofence_render::Direct2dCanvas::new(session);
            if let Err(error) = paint_frame.paint(&mut canvas) {
                paint_error = Some(error);
            }
            Ok(())
        })?;
        if let Some(error) = paint_error {
            tracing::warn!(%error, "panel paint rejected");
        } else if drawn {
            self.committed = Some(candidate);
        }
        Ok(drawn)
    }

    fn action_at(&self, x: f32, y: f32) -> Option<u64> {
        self.committed
            .as_ref()?
            .hit_tree()
            .iter()
            .rev()
            .find(|node| node.rect.contains(x, y))
            .map(|node| node.action)
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
        let presentation = view
            .plugin_panel
            .as_ref()
            .map(PluginPanelContent::presentation)
            .unwrap_or_default();
        let (minimum_width, minimum_height) = presentation.minimum_size(1.0);
        // SAFETY: this branch is only reached for WM_GETMINMAXINFO.
        unsafe {
            window::minmaxinfo_set_min_track(
                lparam,
                (minimum_width * scale).round() as i32,
                view.title_h_px() + (minimum_height * scale).round() as i32,
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
            panel.pressed = panel.action_at(x, y).and_then(|action| {
                panel
                    .committed
                    .as_ref()
                    .map(|frame| (action, frame.clone()))
            });
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
            let invoke = pressed.and_then(|(action, down_frame)| {
                let down_identity = down_frame.identity();
                let current_identity = panel.committed.as_ref()?.identity();
                (down_identity.mount_key == current_identity.mount_key
                    && down_identity.layout_revision == current_identity.layout_revision
                    && down_frame
                        .hit_tree()
                        .iter()
                        .rev()
                        .find(|node| node.rect.contains(x, y))
                        .map(|node| node.action)
                        == Some(action))
                .then_some((action, down_identity.layout_revision))
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

//! Generic bridge between object-safe panel instances and the host composition surface.

#[cfg(test)]
use super::interaction::finish_capture_release;
use super::interaction::flush_capture_release;
use super::*;
use pecofence_plugin_api::ThemeSnapshot;
use pecofence_plugin_api::design::FrameInvalidation;
use pecofence_plugin_api::design::{ControlInteraction, PixelGrid};
use pecofence_plugin_api::{
    Exposure, FrameInput, Grouping, HostCommand, InstanceKey, MountContext, MountKey, PanelEvent,
    PanelInstance, PanelUpdate, PreparedFrame, Presentation, RectDip, StopReason,
};
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct PanelHandle {
    key: InstanceKey,
    panel: Rc<RefCell<Box<dyn PanelInstance>>>,
    presentation: Rc<Cell<Presentation>>,
    grouping: Rc<Cell<Grouping>>,
    exposure: Rc<Cell<Exposure>>,
    active: Rc<Cell<bool>>,
    theme_revision: Rc<Cell<u64>>,
}

impl PanelHandle {
    pub(crate) fn new(key: InstanceKey, panel: Box<dyn PanelInstance>) -> Self {
        let presentation = panel.initial_presentation();
        Self {
            key,
            panel: Rc::new(RefCell::new(panel)),
            presentation: Rc::new(Cell::new(presentation)),
            grouping: Rc::new(Cell::new(Grouping::Single)),
            exposure: Rc::new(Cell::new(Exposure::Desktop)),
            active: Rc::new(Cell::new(true)),
            theme_revision: Rc::new(Cell::new(0)),
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

    pub(crate) fn next_wake_after(&self) -> Option<Duration> {
        self.panel.borrow().next_wake_after()
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
    storage: Option<(u64, u32, u64)>,
    pressed: Option<(u64, u64, Rc<dyn PreparedFrame>)>,
    capture_release_pending: Rc<Cell<bool>>,
    pub(super) invalidation: FrameInvalidation,
    interaction: ControlInteraction,
    viewport: RectDip,
    feedback_theme: ThemeSnapshot,
    fallback_feedback_radius: f32,
    text_formats: RefCell<pecofence_render::TextFormatCache>,
    /// Opacity of the frameless widget's grab handle (0 hidden … 1 shown).
    pub(super) grab: f32,
}

impl PluginPanelContent {
    pub fn new(handle: PanelHandle, capture_release_pending: Rc<Cell<bool>>) -> Self {
        Self {
            handle,
            committed: None,
            storage: None,
            pressed: None,
            capture_release_pending,
            invalidation: FrameInvalidation::Content,
            interaction: ControlInteraction::default(),
            viewport: RectDip::default(),
            feedback_theme: ThemeSnapshot::dark(),
            fallback_feedback_radius: pecofence_plugin_api::design::CONTROL_RADIUS,
            text_formats: RefCell::default(),
            grab: 0.0,
        }
    }

    /// Cancel a pointer gesture without discarding the successfully published frame.
    /// Return capture ownership before clearing it; native ReleaseCapture must run
    /// outside the view borrow because WM_CAPTURECHANGED is synchronous.
    fn cancel_pointer(&mut self) -> bool {
        let captured = self.pressed.take().is_some();
        self.capture_release_pending
            .set(self.capture_release_pending.get() || captured);
        self.interaction.pressed = None;
        self.interaction.hovered = None;
        captured
    }

    pub(super) fn cancel_input(&mut self) -> bool {
        let captured = self.cancel_pointer();
        self.interaction.focused = None;
        captured
    }

    pub(super) fn wake_capture_release(&self, hwnd: HWND) {
        if self.capture_release_pending.get() {
            // Host commands can redraw outside this fence's window procedure.
            // Wake its pump without borrowing it synchronously or synthesizing
            // WM_CANCELMODE (a delayed cancel must not cancel a newer gesture).
            window::post_message(hwnd, WM_APP_PLUGIN_CAPTURE_RELEASE, 0, 0);
        }
    }

    fn capture_lost(&mut self) {
        self.cancel_pointer();
        // The OS already moved ownership away. Do not release a later owner.
        self.capture_release_pending.set(false);
    }

    fn release_at(&mut self, x: f32, y: f32) -> Option<(u64, u64, u64)> {
        self.interaction.pressed = None;
        let (control, action, down_frame) = self.pressed.take()?;
        let down_identity = down_frame.identity();
        let current = self.committed.as_ref()?;
        pecofence_plugin_api::design::released_action(
            down_identity,
            current.identity(),
            down_frame.hit_tree(),
            current.hit_tree(),
            control,
            self.hit_at(x, y),
        )
        .filter(|released| *released == action)
        .map(|action| (control, action, down_identity.layout_revision))
    }

    fn bind_storage(&mut self, surface_epoch: u64, dpi: u32, device_epoch: u64) {
        let storage = Some((surface_epoch, dpi, device_epoch));
        if self.storage != storage {
            self.committed = None;
            self.cancel_input();
            self.storage = storage;
            self.invalidation.request(FrameInvalidation::Layout);
        }
    }

    pub fn accessibility_frame(
        &self,
        surface: u64,
        dpi: u32,
        device: u64,
    ) -> Option<&dyn PreparedFrame> {
        if self.storage != Some((surface, dpi, device))
            || !self.handle.active.get()
            || self.exposure() == Exposure::Hidden
        {
            return None;
        }
        self.committed.as_deref()
    }

    pub(super) fn invoke_committed(
        &self,
        control: u64,
        action: u64,
        layout_revision: u64,
    ) -> pecofence_plugin_api::Result<PanelUpdate> {
        if !self.handle.active.get()
            || self.exposure() == Exposure::Hidden
            || !self.committed.as_ref().is_some_and(|frame| {
                frame.identity().layout_revision == layout_revision
                    && frame
                        .hit_tree()
                        .iter()
                        .any(|hit| hit.id == control && hit.action == action)
            })
        {
            return Err(pecofence_plugin_api::Error::Invalid(
                "stale or unavailable panel action".into(),
            ));
        }
        self.handle.event(PanelEvent::Invoke {
            action,
            layout_revision,
        })
    }

    pub fn semantic_count(&self) -> usize {
        self.committed
            .as_ref()
            .map_or(0, |frame| frame.semantics().len())
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

    pub fn draw(
        &mut self,
        surface: &Panel,
        dpi: u32,
        text_scale: f32,
        device_epoch: u64,
        theme: &Theme,
    ) -> Result<bool> {
        self.invalidation.request(FrameInvalidation::Content);
        self.bind_storage(surface.surface_epoch(), dpi, device_epoch);
        let (width_px, height_px) = surface.size_px();
        let scale = dpi.max(96) as f32 / 96.0;
        let viewport = RectDip {
            x: 0.0,
            y: 0.0,
            w: width_px as f32 / scale,
            h: height_px as f32 / scale,
        };
        if self.handle.exposure.get() == Exposure::Hidden || !self.handle.active.get() {
            self.committed = None;
            self.cancel_input();
            self.invalidation.request(FrameInvalidation::Content);
            // A skipped hidden frame is not a graphics-device failure.
            return Ok(true);
        }
        // Sync the fence window's live theme into the shared theme host, then
        // let the panel know when the derived tokens changed. The epoch only
        // advances on actual token changes, so this is free in steady state.
        let theme_host = crate::app::panel_manager::theme_host();
        theme_host.sync(theme);
        let theme_revision = theme_host.snapshot().revision;
        if self.handle.theme_revision.get() != theme_revision {
            self.handle.theme_revision.set(theme_revision);
            let _ = self
                .handle
                .event(PanelEvent::ThemeChanged {
                    revision: theme_revision,
                })
                .map(|_| ());
        }
        let candidate = match self.handle.panel.borrow_mut().prepare_frame(FrameInput {
            viewport,
            locale: pecofence_core::i18n::language().tag(),
            presentation: self.handle.presentation.get(),
            grouping: self.handle.grouping.get(),
            exposure: self.handle.exposure.get(),
            active: self.handle.active.get(),
            text_scale,
            theme_epoch: theme_host.epoch(),
            device_epoch,
        }) {
            Ok(frame) => frame,
            Err(error) => {
                tracing::warn!(%error, "panel frame preparation rejected");
                return Ok(true);
            }
        };
        let feedback_theme = theme_host.snapshot();
        let mut interaction = self.interaction.clone();
        interaction.reconcile_viewport(candidate.hit_tree(), viewport);
        let grab = self.grab;
        let mut paint_error = None;
        let paint_frame = candidate.clone();
        let mut formats = self.text_formats.borrow_mut();
        let drawn = surface.draw_candidate(dpi, |session, _width, _height| {
            session.clear(ColorF {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            });
            let mut canvas =
                pecofence_render::Direct2dCanvas::with_text_cache(session, &mut formats);
            let paint = paint_frame.paint(&mut canvas).and_then(|_| {
                canvas_feedback(
                    &mut canvas,
                    viewport,
                    &interaction,
                    paint_frame.as_ref(),
                    &feedback_theme,
                    scale,
                    pecofence_plugin_api::design::ContentScale::new(viewport, text_scale)
                        .value(pecofence_plugin_api::design::CONTROL_RADIUS),
                    grab,
                )
            });
            if let Err(error) = paint {
                paint_error = Some(error);
                return Err(windows_core::Error::from_hresult(PANEL_PAINT_REJECTED));
            }
            Ok(())
        });
        if let Some(error) = paint_error {
            tracing::warn!(%error, "panel paint rejected");
            return rejected_paint_result(drawn);
        }
        let drawn = drawn?;
        if drawn {
            self.viewport = viewport;
            self.feedback_theme = (*feedback_theme).clone();
            self.fallback_feedback_radius =
                pecofence_plugin_api::design::ContentScale::new(viewport, text_scale)
                    .value(pecofence_plugin_api::design::CONTROL_RADIUS);
            self.interaction = interaction;
            self.committed = Some(candidate);
            self.invalidation = FrameInvalidation::Clean;
        }
        Ok(drawn)
    }

    /// Whether a window-client point (pixels) lands on an interactive widget control.
    pub(super) fn control_at_client(
        &self,
        x_px: i32,
        y_px: i32,
        title_px: i32,
        scale: f32,
    ) -> bool {
        self.hit_at(
            x_px as f32 / scale.max(0.01),
            (y_px - title_px) as f32 / scale.max(0.01),
        )
        .is_some()
    }

    fn hit_at(&self, x: f32, y: f32) -> Option<&pecofence_plugin_api::HitNode> {
        if !self.viewport.contains(x, y) {
            return None;
        }
        self.committed
            .as_ref()?
            .hit_tree()
            .iter()
            .rev()
            .find(|node| node.rect.contains(x, y))
    }

    fn focusable_hits(&self) -> Vec<pecofence_plugin_api::HitNode> {
        self.committed
            .as_ref()
            .map(|frame| {
                frame
                    .hit_tree()
                    .iter()
                    .filter(|hit| {
                        pecofence_plugin_api::design::control_visible(hit.rect, self.viewport)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn repaint_feedback(&mut self, surface: &Panel, dpi: u32) -> Result<bool> {
        self.invalidation.request(FrameInvalidation::Feedback);
        if !self.storage.is_some_and(|(epoch, frame_dpi, _)| {
            epoch == surface.surface_epoch() && frame_dpi == dpi
        }) {
            return Ok(true);
        }
        let Some(frame) = &self.committed else {
            return Ok(true);
        };
        let mut error = None;
        let mut formats = self.text_formats.borrow_mut();
        let drawn = surface.draw_candidate(dpi, |session, _, _| {
            session.clear(ColorF::new(0.0, 0.0, 0.0, 0.0));
            let mut canvas =
                pecofence_render::Direct2dCanvas::with_text_cache(session, &mut formats);
            if let Err(issue) = frame.paint(&mut canvas).and_then(|_| {
                canvas_feedback(
                    &mut canvas,
                    self.viewport,
                    &self.interaction,
                    frame.as_ref(),
                    &self.feedback_theme,
                    dpi as f32 / 96.0,
                    self.fallback_feedback_radius,
                    self.grab,
                )
            }) {
                error = Some(issue);
                return Err(windows_core::Error::from_hresult(PANEL_PAINT_REJECTED));
            }
            Ok(())
        });
        if let Some(error) = error {
            tracing::warn!(%error, "panel feedback paint rejected");
            return rejected_paint_result(drawn);
        }
        if matches!(drawn, Ok(true)) {
            self.invalidation.feedback_committed();
        }
        drawn
    }
}

impl Drop for PluginPanelContent {
    fn drop(&mut self) {
        // The window, not a mounted component, owns native capture effects.
        // Unmounting must retain release work even after this frame is gone.
        self.cancel_input();
    }
}

// Customer-defined FACILITY_ITF error, distinct from native D2D/Composition failures.
const PANEL_PAINT_REJECTED: windows_core::HRESULT = windows_core::HRESULT(0xA0040001u32 as i32);

/// Preserve native surface errors and device-loss signals for host recovery.
fn rejected_paint_result(result: Result<bool>) -> Result<bool> {
    match result {
        Err(error) if error.code() == PANEL_PAINT_REJECTED => Ok(true),
        other => other,
    }
}

fn canvas_feedback(
    canvas: &mut dyn pecofence_plugin_api::Canvas,
    viewport: RectDip,
    state: &ControlInteraction,
    frame: &dyn PreparedFrame,
    theme: &ThemeSnapshot,
    scale: f32,
    radius: f32,
    grab: f32,
) -> pecofence_plugin_api::Result<()> {
    canvas.push_clip(viewport)?;
    let painted = state
        .paint_with_radii(
            canvas,
            frame.hit_tree(),
            theme,
            PixelGrid::new(scale),
            |id| frame.control_corner_radius(id).unwrap_or(radius),
        )
        .and_then(|_| paint_grab_handle(canvas, viewport, grab, PixelGrid::new(scale)));
    painted.and(canvas.pop_clip())
}

/// Frameless widgets show a small grab handle at the top centre while the pointer is
/// over them, so it is clear the window can be dragged (bottom-sheet style). A warm
/// mid grey reads on both the paper widgets and the clock's glass.
fn paint_grab_handle(
    canvas: &mut dyn pecofence_plugin_api::Canvas,
    viewport: RectDip,
    grab: f32,
    grid: PixelGrid,
) -> pecofence_plugin_api::Result<()> {
    if grab <= 0.0 || viewport.w < 48.0 {
        return Ok(());
    }
    const WIDTH: f32 = 32.0;
    const HEIGHT: f32 = 4.0;
    let rect = RectDip {
        x: grid.snap(viewport.x + (viewport.w - WIDTH) / 2.0),
        y: grid.snap(viewport.y + 5.0),
        w: WIDTH,
        h: HEIGHT,
    };
    canvas.fill_rounded_rect(rect, HEIGHT / 2.0, [0.47, 0.43, 0.38, 0.6 * grab.min(1.0)])
}

impl FenceViewState {
    pub(super) fn redraw_plugin_feedback(&mut self) {
        if let Some(panel) = &mut self.plugin_panel {
            match panel.repaint_feedback(&self.content_panel, self.dpi) {
                Ok(ok) => self.note_draw_result(ok),
                Err(error) => tracing::warn!(%error, "panel feedback redraw failed"),
            }
        }
    }

    pub(super) fn apply_plugin_update(&mut self, update: PanelUpdate) -> Result<()> {
        let mut redraw_content = update.relayout;
        let mut redraw_chrome = false;
        let mut relayout = update.relayout;
        for command in update.commands {
            match command {
                HostCommand::Invalidate => redraw_content = true,
                HostCommand::SetTitle(title) => {
                    if self.title != title {
                        self.title = title.clone();
                        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == self.active) {
                            tab.title = title;
                        }
                        redraw_chrome = true;
                    }
                }
                HostCommand::RequestMode(mode) => {
                    if let Some(panel) = &self.plugin_panel {
                        panel.set_presentation(mode);
                    }
                    relayout = true;
                    redraw_content = true;
                }
                HostCommand::PersistConfig(bytes) => {
                    if let Some(panel) = &self.plugin_panel {
                        self.queue.push(Command::PanelConfigChanged {
                            key: panel.key(),
                            bytes,
                        });
                    }
                }
                HostCommand::RequestTextInput {
                    action,
                    initial,
                    multiline,
                } => {
                    if let Some(panel) = &self.plugin_panel {
                        self.queue.push(Command::PanelTextRequested {
                            key: panel.key(),
                            action,
                            initial,
                            multiline,
                        });
                    }
                }
            }
        }
        if redraw_content && let Some(panel) = &self.plugin_panel {
            self.queue
                .push(Command::PanelWakeChanged { key: panel.key() });
        }
        if redraw_content && let Some(panel) = &mut self.plugin_panel {
            panel.invalidation.request(if relayout {
                FrameInvalidation::Layout
            } else {
                FrameInvalidation::Content
            });
        }
        if !desktop::is_visible(self.hwnd) {
            return Ok(());
        }
        if redraw_chrome {
            self.redraw_chrome_only()?;
        }
        if redraw_content && (!self.rolled_up || self.roll_anim.is_some()) {
            self.redraw_content()?;
        }
        Ok(())
    }
}

pub(super) fn handle_message(
    handler: &super::handler::HandlerCtx,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> Option<isize> {
    {
        let mut guard = handler.view.try_borrow_mut().ok()?;
        let view = guard.as_mut()?;
        let device_epoch = (u64::from(view.stack.device_generation()) << 32) | u64::from(view.dpi);
        if let Some(panel) = view.plugin_panel.as_mut() {
            panel.bind_storage(view.content_panel.surface_epoch(), view.dpi, device_epoch);
        }
    }
    // A cancelled old body press must release capture before the current
    // message can start a new native title/tab gesture on this same HWND.
    let hwnd = handler.view.borrow().as_ref()?.hwnd;
    flush_capture_release(&handler.view, hwnd);
    if message == WM_APP_PLUGIN_CAPTURE_RELEASE {
        return Some(0);
    }
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
        let text_scale = view.behavior.text_scale.get();
        // Widget spans include the title bar; other panels add it to their content minimum.
        let (minimum_width, minimum_height) = match view.widget_grid {
            Some(grid) => {
                let (w, h) = grid.rules.minimum_size(grid.cell, text_scale);
                ((w * scale).round() as i32, (h * scale).round() as i32)
            }
            None => {
                let (w, h) = presentation.minimum_size(text_scale);
                (
                    (w * scale).round() as i32,
                    view.title_h_px() + (h * scale).round() as i32,
                )
            }
        };
        // SAFETY: this branch is only reached for WM_GETMINMAXINFO.
        unsafe {
            window::minmaxinfo_set_min_track(lparam, minimum_width, minimum_height);
        }
        return Some(0);
    }
    // Frozen content surfaces remain available during roll/fade animations, but
    // their controls must not remain interactive while concealed or retiring.
    let input_available = view.input_lifecycle().body()
        && view
            .plugin_panel
            .as_ref()
            .is_some_and(|panel| panel.handle.active.get() && panel.exposure() != Exposure::Hidden);
    if !input_available {
        let panel = view.plugin_panel.as_mut()?;
        let feedback_changed = panel.interaction != ControlInteraction::default();
        let captured = panel.cancel_input();
        if feedback_changed {
            // The frozen body can be revealed again without a new source frame.
            // Clear its old focus/press ink as well as the corresponding state.
            view.redraw_plugin_feedback();
        }
        let hwnd = view.hwnd;
        drop(guard);
        if captured && window::get_capture() == hwnd {
            window::release_capture();
        }
        return None;
    }
    if message == msg::WM_SETCURSOR && msg::lo_i16(lparam) as isize == msg::HTCLIENT {
        let point = window::screen_to_client(view.hwnd, window::cursor_pos());
        let scale = view.scale();
        if !view.rolled_up
            && view
                .plugin_panel
                .as_ref()?
                .hit_at(
                    point.x as f32 / scale,
                    (point.y - view.title_h_px()) as f32 / scale,
                )
                .is_some()
        {
            drop(guard);
            window::set_standard_cursor(StandardCursor::Hand);
            return Some(1);
        }
    }
    let scale = view.scale();
    let x = msg::lo_i16(lparam) as f32 / scale;
    let y = (msg::hi_i16(lparam) as i32 - view.title_h_px()) as f32 / scale;
    let header_dragging =
        view.tab_drag.is_some() || view.remote_drag.is_some() || view.detach_pending;
    let panel = view.plugin_panel.as_mut()?;
    match message {
        msg::WM_MOUSEMOVE => {
            let hit = panel.hit_at(x, y).map(|hit| hit.id);
            if panel.interaction.hover(hit) {
                view.redraw_plugin_feedback();
            }
            // Continue the ordinary handler for title/peek/mouse-leave tracking.
            None
        }
        msg::WM_MOUSELEAVE => {
            if panel.interaction.hover(None) {
                view.redraw_plugin_feedback();
            }
            None
        }
        msg::WM_KILLFOCUS => {
            let captured = panel.cancel_input();
            view.redraw_plugin_feedback();
            let hwnd = view.hwnd;
            drop(guard);
            if captured && window::get_capture() == hwnd {
                window::release_capture();
            }
            None
        }
        msg::WM_KEYDOWN
            if wparam as u32 == msg::VK_ESCAPE
                && !header_dragging
                && (panel.pressed.is_some() || panel.interaction.focused.is_some()) =>
        {
            let captured = panel.cancel_input();
            view.redraw_plugin_feedback();
            let hwnd = view.hwnd;
            drop(guard);
            if captured && window::get_capture() == hwnd {
                window::release_capture();
            }
            Some(0)
        }
        // WM_CANCELMODE is sent before modal menus and other system input
        // ownership changes, even when keyboard focus has not changed yet.
        msg::WM_CANCELMODE => {
            let captured = panel.cancel_pointer();
            view.redraw_plugin_feedback();
            let hwnd = view.hwnd;
            drop(guard);
            if captured && window::get_capture() == hwnd {
                window::release_capture();
            }
            None
        }
        msg::WM_KEYDOWN if !window::key_down(msg::VK_CONTROL) && !window::key_down(0x12) => {
            match wparam as u32 {
                0x09 => {
                    let hits = panel.focusable_hits();
                    panel
                        .interaction
                        .focus_next(&hits, window::key_down(msg::VK_SHIFT));
                    view.redraw_plugin_feedback();
                    Some(0)
                }
                0x0D | 0x20 if panel.interaction.focused.is_some() => {
                    // Do not repeatedly invoke an edit/action while a key is held.
                    if lparam & (1 << 30) == 0 {
                        let focused = panel.interaction.focused;
                        let target = panel.committed.as_ref().and_then(|frame| {
                            frame
                                .hit_tree()
                                .iter()
                                .find(|hit| Some(hit.id) == focused)
                                .map(|hit| (hit.id, hit.action, frame.identity().layout_revision))
                        });
                        if let Some((control, action, layout_revision)) = target
                            && let Err(error) =
                                view.invoke_plugin_control(control, action, layout_revision)
                        {
                            tracing::warn!(%error, "keyboard panel action failed");
                        }
                    }
                    Some(0)
                }
                _ => None,
            }
        }
        msg::WM_CHAR
            if matches!(wparam, 0x09 | 0x0D | 0x20) && panel.interaction.focused.is_some() =>
        {
            Some(0)
        }
        msg::WM_LBUTTONDOWN if y >= 0.0 => {
            panel.pressed = panel.hit_at(x, y).and_then(|hit| {
                panel
                    .committed
                    .as_ref()
                    .map(|frame| (hit.id, hit.action, frame.clone()))
            });
            panel.interaction.focused = None;
            panel.interaction.pressed = panel.pressed.as_ref().map(|(id, _, _)| *id);
            panel.interaction.hovered = panel.interaction.pressed;
            let capture = panel.pressed.is_some();
            panel.capture_release_pending.set(false);
            view.redraw_plugin_feedback();
            let hwnd = view.hwnd;
            drop(guard);
            if capture {
                window::set_capture(hwnd);
            }
            Some(0)
        }
        msg::WM_LBUTTONUP if y >= 0.0 || panel.pressed.is_some() => {
            let invoke = panel.release_at(x, y);
            if let Some((control, action, layout_revision)) = invoke
                && let Err(error) = view.invoke_plugin_control(control, action, layout_revision)
            {
                tracing::warn!(%error, "pointer panel action failed");
            }
            view.redraw_plugin_feedback();
            drop(guard);
            window::release_capture();
            Some(0)
        }
        msg::WM_CAPTURECHANGED => {
            panel.capture_lost();
            view.redraw_plugin_feedback();
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod candidate_error_tests {
    use super::*;

    struct TestPanel {
        prepares: Rc<Cell<u64>>,
        fail: Rc<Cell<bool>>,
    }
    struct TestFrame {
        scene: pecofence_plugin_api::scene::Scene,
        fail: Rc<Cell<bool>>,
    }
    impl PreparedFrame for TestFrame {
        fn identity(&self) -> pecofence_plugin_api::FrameIdentity {
            self.scene.identity()
        }
        fn control_corner_radius(&self, id: u64) -> Option<f32> {
            self.scene.control_corner_radius(id)
        }
        fn hit_tree(&self) -> &[pecofence_plugin_api::HitNode] {
            self.scene.hit_tree()
        }
        fn semantics(&self) -> &[pecofence_plugin_api::SemanticNode] {
            self.scene.semantics()
        }
        fn paint(
            &self,
            canvas: &mut dyn pecofence_plugin_api::Canvas,
        ) -> pecofence_plugin_api::Result<()> {
            self.scene.paint(canvas)?;
            if self.fail.get() {
                Err(pecofence_plugin_api::Error::Backend(
                    "injected partial paint failure".into(),
                ))
            } else {
                Ok(())
            }
        }
    }
    impl PanelInstance for TestPanel {
        fn mount(&mut self, _: MountContext) -> pecofence_plugin_api::Result<()> {
            Ok(())
        }
        fn event(&mut self, _: PanelEvent) -> pecofence_plugin_api::Result<PanelUpdate> {
            Ok(PanelUpdate::default())
        }
        fn prepare_frame(
            &mut self,
            input: FrameInput,
        ) -> pecofence_plugin_api::Result<Rc<dyn PreparedFrame>> {
            use pecofence_plugin_api::{
                FontRole, FrameIdentity,
                scene::{NodeStyle, Scene, SceneNode},
            };
            let revision = self.prepares.get() + 1;
            self.prepares.set(revision);
            let scene = Scene::new(
                FrameIdentity {
                    mount_key: MountKey {
                        instance: InstanceKey {
                            id: 1,
                            activation: 1,
                        },
                        generation: 1,
                    },
                    frame_revision: revision,
                    layout_revision: revision,
                    theme_epoch: input.theme_epoch,
                    device_epoch: input.device_epoch,
                },
                input.viewport,
                vec![SceneNode {
                    id: 1,
                    parent: None,
                    rect: RectDip {
                        x: 4.0,
                        y: 4.0,
                        w: 72.0,
                        h: 32.0,
                    },
                    role: "button".into(),
                    name: "Action".into(),
                    text: "Action".into(),
                    action: Some(7),
                    style: NodeStyle {
                        stroke: None,
                        size: 14.0,
                        weight: 400,
                        font: FontRole::Text,
                        foreground: [1.0; 4],
                        fill: Some([0.2, 0.3, 0.4, 1.0]),
                        radius: 4.0,
                        text_inset: [0.0; 2],
                        align: Default::default(),
                        path: None,
                    },
                }],
            )?;
            Ok(Rc::new(TestFrame {
                scene,
                fail: self.fail.clone(),
            }))
        }
        fn unmount(&mut self, _: MountKey) {}
        fn begin_stop(&mut self, _: StopReason) {}
    }

    #[test]
    fn host_feedback_uses_committed_shape_instead_of_global_fallback() {
        let mut provider = TestPanel {
            prepares: Rc::new(Cell::new(0)),
            fail: Rc::new(Cell::new(false)),
        };
        let input = FrameInput {
            viewport: RectDip {
                x: 0.0,
                y: 0.0,
                w: 120.0,
                h: 80.0,
            },
            locale: "en",
            presentation: Presentation::Compact,
            grouping: Grouping::Single,
            exposure: Exposure::Desktop,
            active: true,
            text_scale: 1.0,
            theme_epoch: 1,
            device_epoch: 1,
        };
        let frame = provider.prepare_frame(input).unwrap();
        let mut canvas = pecofence_render::HeadlessCanvas::default();
        canvas_feedback(
            &mut canvas,
            input.viewport,
            &ControlInteraction {
                hovered: Some(1),
                ..Default::default()
            },
            frame.as_ref(),
            &ThemeSnapshot::dark(),
            2.0,
            99.0,
            0.0,
        )
        .unwrap();
        assert!(canvas.is_balanced());
        assert!(
            matches!(canvas.commands(), [pecofence_render::DrawCommand::PushClip(_),
            pecofence_render::DrawCommand::FillRoundedRect(rect, radius, _),
            pecofence_render::DrawCommand::PopClip] if *rect == frame.hit_tree()[0].rect && *radius == 4.0)
        );
    }

    #[test]
    fn host_preserves_committed_input_on_paint_failure_and_invalidates_discarded_storage() {
        let _ole = pecofence_platform::com::OleGuard::init().unwrap();
        let stack = RenderStack::new().unwrap();
        let mut surface = Panel::new(&stack).unwrap();
        surface.resize(120, 80).unwrap();
        let prepares = Rc::new(Cell::new(0));
        let fail = Rc::new(Cell::new(false));
        let capture_release = Rc::new(Cell::new(false));
        let mut content = PluginPanelContent::new(
            PanelHandle::new(
                InstanceKey {
                    id: 1,
                    activation: 1,
                },
                Box::new(TestPanel {
                    prepares: prepares.clone(),
                    fail: fail.clone(),
                }),
            ),
            capture_release.clone(),
        );
        let theme = Theme::dark();
        assert!(content.draw(&surface, 96, 1.0, 1, &theme).unwrap());
        let committed = content.committed.as_ref().unwrap().clone();
        let press = |content: &mut PluginPanelContent| {
            content.pressed = Some((1, 7, committed.clone()));
            content.interaction.pressed = Some(1);
            content.interaction.hovered = Some(1);
            content.interaction.focused = Some(1);
        };
        press(&mut content);
        assert_eq!(content.release_at(10.0, 10.0), Some((1, 7, 1)));
        assert_eq!(content.release_at(10.0, 10.0), None, "one release only");
        press(&mut content);
        assert!(content.cancel_pointer());
        assert_eq!(
            content.interaction.focused,
            Some(1),
            "capture loss keeps keyboard focus"
        );
        assert!(content.interaction.hovered.is_none());
        assert!(content.repaint_feedback(&surface, 96).unwrap());
        assert_eq!(
            content.release_at(10.0, 10.0),
            None,
            "cancelled pointer cannot invoke unchanged layout"
        );
        assert_eq!(
            prepares.get(),
            1,
            "cancellation replays the published frame"
        );
        assert!(Rc::ptr_eq(content.committed.as_ref().unwrap(), &committed));
        press(&mut content);
        assert!(content.cancel_input());
        assert_eq!(content.interaction, ControlInteraction::default());
        assert!(!content.cancel_input(), "cancellation is idempotent");
        assert!(
            capture_release.replace(false),
            "cancel retains the native release effect"
        );
        assert!(
            !capture_release.replace(false),
            "one native release per cancellation"
        );
        assert_eq!(
            content.release_at(10.0, 10.0),
            None,
            "Escape/focus suspension cannot invoke"
        );
        content.invalidation.request(FrameInvalidation::Layout);
        fail.set(true);
        assert!(content.draw(&surface, 96, 1.0, 1, &theme).unwrap());
        assert!(Rc::ptr_eq(content.committed.as_ref().unwrap(), &committed));
        assert_eq!(content.hit_at(10.0, 10.0).unwrap().action, 7);
        assert!(content.invalidation.needs_layout());
        fail.set(false);
        assert!(content.repaint_feedback(&surface, 96).unwrap());
        assert_eq!(
            prepares.get(),
            2,
            "feedback must not prepare a source frame"
        );
        assert!(content.invalidation.needs_layout());
        press(&mut content);
        surface.resize(240, 80).unwrap();
        content.bind_storage(surface.surface_epoch(), 96, 1);
        content.bind_storage(surface.surface_epoch(), 96, 1);
        assert!(
            capture_release.replace(false),
            "storage cancellation survives repeated binding"
        );
        assert!(!capture_release.replace(false));
        assert!(content.committed.is_none());
        assert!(content.hit_at(10.0, 10.0).is_none());
        content.set_container_state(Grouping::Single, Exposure::Hidden, false);
        assert!(
            content.draw(&surface, 96, 1.0, 1, &theme).unwrap(),
            "hidden content is not device loss"
        );
        assert_eq!(prepares.get(), 2);
        content.set_container_state(Grouping::Single, Exposure::Desktop, true);
        assert!(content.draw(&surface, 96, 1.0, 1, &theme).unwrap());
        assert_eq!(prepares.get(), 3);
        assert_eq!(content.invalidation, FrameInvalidation::Clean);
        let frame = content.committed.as_ref().unwrap().clone();
        content.pressed = Some((1, 7, frame));
        content.interaction.pressed = Some(1);
        drop(content);
        assert!(
            capture_release.replace(false),
            "unmount cannot drop native capture release"
        );
        assert!(!capture_release.get());
    }

    #[test]
    fn rejected_plugin_paint_does_not_mask_device_loss_or_native_surface_failure() {
        assert!(
            rejected_paint_result(Err(windows_core::Error::from_hresult(PANEL_PAINT_REJECTED)))
                .unwrap()
        );
        assert!(!rejected_paint_result(Ok(false)).unwrap());
        let native = windows_core::HRESULT(0x80004005u32 as i32);
        assert_eq!(
            rejected_paint_result(Err(windows_core::Error::from_hresult(native)))
                .unwrap_err()
                .code(),
            native
        );
    }

    #[test]
    fn deferred_release_only_affects_its_native_capture_owner() {
        let class = WindowClass::register(
            &format!("PecoFence.CaptureReleaseTest.{}", std::process::id()),
            window::ClassOptions::default(),
        )
        .unwrap();
        let losses = Rc::new(Cell::new(0));
        let observed = losses.clone();
        let owner = WindowBuilder::new(&class)
            .bounds(-10000, -10000, 40, 40)
            .create(Box::new(move |_, message, _, _| {
                if message == msg::WM_CAPTURECHANGED {
                    observed.set(observed.get() + 1);
                }
                None
            }))
            .unwrap();
        let other = WindowBuilder::new(&class)
            .bounds(-10000, -10000, 40, 40)
            .create(Box::new(|_, _, _, _| None))
            .unwrap();
        window::set_capture(owner.hwnd());
        assert_eq!(window::get_capture(), owner.hwnd());
        finish_capture_release(owner.hwnd(), false);
        assert_eq!(window::get_capture(), owner.hwnd());
        finish_capture_release(other.hwnd(), true);
        assert_eq!(
            window::get_capture(),
            owner.hwnd(),
            "unrelated HWND keeps capture"
        );
        finish_capture_release(owner.hwnd(), true);
        assert!(window::get_capture().0.is_null());
        assert_eq!(
            losses.get(),
            1,
            "native capture notification is synchronous"
        );
        finish_capture_release(owner.hwnd(), true);
        assert_eq!(losses.get(), 1);
        window::set_capture(other.hwnd());
        finish_capture_release(owner.hwnd(), true);
        assert_eq!(
            window::get_capture(),
            other.hwnd(),
            "late release cannot steal capture"
        );
        window::release_capture();
    }
}

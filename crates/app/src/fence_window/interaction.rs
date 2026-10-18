//! Window and body input availability, cancellation and deferred native effects.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct InputLifecycle {
    pub visible: bool,
    pub retired: bool,
    pub hiding: bool,
    pub rolled: bool,
    pub rolling: bool,
}

impl InputLifecycle {
    pub fn window(self) -> bool {
        self.visible && !self.retired && !self.hiding
    }
    pub fn body(self) -> bool {
        self.window() && !self.rolled && !self.rolling
    }
}

impl FenceViewState {
    pub(super) fn input_lifecycle(&self) -> InputLifecycle {
        InputLifecycle {
            visible: desktop::is_visible(self.hwnd),
            retired: self.retired,
            hiding: matches!(self.vis_fade, Some(VisFade::Hiding { .. })),
            rolled: self.rolled_up,
            rolling: self.roll_anim.is_some(),
        }
    }

    /// Body gestures cannot survive a roll transition. Title controls remain
    /// available so the transition can still be reversed from the title bar.
    pub(super) fn suspend_body_input(&mut self) -> bool {
        let body_press = matches!(
            self.pressed,
            Some(PressTarget::Item(_) | PressTarget::Header(_))
        );
        let captured = self.drag.take().is_some()
            | self.end_marquee()
            | self.end_scrollbar_press()
            | self.col_drag.take().is_some()
            | (body_press && self.take_pressed().is_some());
        self.input_capture_release
            .set(self.input_capture_release.get() || captured);
        if let Some(panel) = self.plugin_panel.as_mut() {
            panel.cancel_input();
            self.redraw_plugin_feedback();
        }
        self.focus_visible = false;
        self.type_ahead.clear();
        self.cancel_pending_rename();
        self.hide_tip();
        self.drag_scroll = None;
        self.set_drop_feedback(None);
        self.set_hover(None, false);
        self.header_hover = None;
        captured
    }

    /// Cancel state while it is borrowed, defer all synchronous native effects.
    /// Selection and content survive a plain hide; no release may complete an old gesture.
    pub(super) fn suspend_window_input(&mut self) -> Option<DragCancel> {
        let cancelled_drag = self.cancel_tab_or_remote_drag();
        let captured =
            self.suspend_body_input() | self.take_pressed().is_some() | cancelled_drag.is_some();
        self.input_capture_release
            .set(self.input_capture_release.get() || captured);
        self.up_hovered = false;
        self.chevron_hovered = false;
        self.tab_hover = None;
        window::kill_timer(self.hwnd, TIMER_PEEK_OPEN);
        window::kill_timer(self.hwnd, TIMER_PEEK_CLOSE);
        cancelled_drag
    }
}

/// Lifecycle messages and cancellation notifications must still be dispatched.
/// Only messages that can start or complete a user action are consumed.
pub(super) fn inactive_input_result(message: u32) -> Option<isize> {
    match message {
        msg::WM_NCHITTEST => Some(msg::HTNOWHERE),
        msg::WM_MOUSEACTIVATE => Some(msg::MA_NOACTIVATE),
        msg::WM_SETCURSOR => Some(1),
        msg::WM_MOUSEMOVE
        | msg::WM_NCMOUSEMOVE
        | msg::WM_LBUTTONDOWN
        | msg::WM_LBUTTONUP
        | msg::WM_LBUTTONDBLCLK
        | msg::WM_RBUTTONDOWN
        | msg::WM_RBUTTONUP
        | msg::WM_MBUTTONDOWN
        | msg::WM_NCLBUTTONDOWN
        | WM_NCLBUTTONUP
        | msg::WM_NCLBUTTONDBLCLK
        | msg::WM_NCRBUTTONUP
        | msg::WM_MOUSEWHEEL
        | msg::WM_KEYDOWN
        | msg::WM_KEYUP
        | msg::WM_CHAR
        | msg::WM_SYSKEYDOWN
        | msg::WM_CONTEXTMENU
        | msg::WM_SYSCOMMAND => Some(0),
        _ => None,
    }
}

/// Finish host effects only after releasing view state: ReleaseCapture sends
/// WM_CAPTURECHANGED synchronously through the same handler.
pub(super) fn finish_capture_release(hwnd: HWND, requested: bool) {
    if requested && window::get_capture() == hwnd {
        window::release_capture();
    }
}

pub(super) fn flush_capture_release(view: &ViewCell, hwnd: HWND) {
    let requested = {
        let Ok(mut guard) = view.try_borrow_mut() else {
            // An outer message/host operation still owns the view. Keep the
            // request until its message boundary, rather than dropping it.
            return;
        };
        guard
            .as_mut()
            .is_some_and(|view| view.input_capture_release.replace(false))
    };
    finish_capture_release(hwnd, requested);
}

/// Covers early returns and resource invalidation performed by ordinary native
/// handlers (resize, DPI, theme, redraw), after their local view borrows end.
pub(super) struct CaptureReleaseBoundary<'a> {
    pub view: &'a ViewCell,
    pub hwnd: HWND,
}

impl Drop for CaptureReleaseBoundary<'_> {
    fn drop(&mut self) {
        flush_capture_release(self.view, self.hwnd);
        let publisher = if let Ok(guard) = self.view.try_borrow() {
            guard.as_ref().map(|view| {
                view.publish_accessibility();
                view.accessibility.clone()
            })
        } else {
            None
        };
        // UIA can call native providers while dispatching events. No view borrow
        // or snapshot lock may be held across this boundary.
        if let Some(publisher) = publisher {
            publisher.flush_events();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_lifecycle_contract_separates_body_from_title_and_window_lifetime() {
        for visible in [false, true] {
            for retired in [false, true] {
                for hiding in [false, true] {
                    for rolled in [false, true] {
                        for rolling in [false, true] {
                            let state = InputLifecycle {
                                visible,
                                retired,
                                hiding,
                                rolled,
                                rolling,
                            };
                            let window = visible && !retired && !hiding;
                            assert_eq!(state.window(), window);
                            assert_eq!(state.body(), window && !rolled && !rolling);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn hidden_input_gate_keeps_cancellation_and_lifecycle_messages_live() {
        for message in [
            msg::WM_LBUTTONUP,
            msg::WM_KEYDOWN,
            msg::WM_CONTEXTMENU,
            msg::WM_SYSCOMMAND,
        ] {
            assert_eq!(inactive_input_result(message), Some(0));
        }
        assert_eq!(
            inactive_input_result(msg::WM_NCHITTEST),
            Some(msg::HTNOWHERE)
        );
        for message in [
            msg::WM_CAPTURECHANGED,
            msg::WM_KILLFOCUS,
            msg::WM_TIMER,
            msg::WM_SIZE,
            msg::WM_SHOWWINDOW,
            WM_APP_FADE_DONE,
            WM_APP_PLUGIN_CAPTURE_RELEASE,
        ] {
            assert_eq!(inactive_input_result(message), None);
        }
    }
}

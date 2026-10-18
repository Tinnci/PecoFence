//! Themed native EDIT controls with single-line and multiline key handling.

use crate::bindings::*;
use crate::wide::{from_wide, to_wide};
use windows_core::{Error, PCWSTR, PWSTR, Result};

pub const EN_KILLFOCUS: u32 = crate::bindings::EN_KILLFOCUS as u32;
pub const EN_CHANGE_MSG: u32 = crate::bindings::EN_CHANGE as u32;
pub const EM_SETSEL_MSG: u32 = EM_SETSEL as u32;
pub const WM_CTLCOLOREDIT_MSG: u32 = WM_CTLCOLOREDIT as u32;

type KeyHandler = fn(u32) -> bool;

#[derive(Clone, Copy)]
pub struct EditAppearance {
    pub foreground: u32,
    pub background: u32,
    pub font_size: f32,
}

pub struct EditControl {
    hwnd: HWND,
    font: HFONT,
    brush: HBRUSH,
    text_color: u32,
    back_color: u32,
    multiline: bool,
    auto_scrollbar: bool,
}

impl EditControl {
    /// Creates the control filling `parent`'s client area.
    pub fn create(
        parent: HWND,
        width: i32,
        height: i32,
        dpi: u32,
        text: &str,
        dark: bool,
        on_key: KeyHandler,
    ) -> Result<Self> {
        Self::create_with_mode(parent, width, height, dpi, text, dark, on_key, false, None)
    }

    pub fn create_multiline(
        parent: HWND,
        width: i32,
        height: i32,
        dpi: u32,
        text: &str,
        dark: bool,
        on_key: KeyHandler,
    ) -> Result<Self> {
        Self::create_with_mode(parent, width, height, dpi, text, dark, on_key, true, None)
    }

    /// A provider-owned surface keeps its own ink, paper and text scale. Native
    /// wrapping/autoscroll still work without a permanently visible scrollbar.
    pub fn create_styled(
        parent: HWND,
        width: i32,
        height: i32,
        dpi: u32,
        text: &str,
        on_key: KeyHandler,
        multiline: bool,
        appearance: EditAppearance,
    ) -> Result<Self> {
        Self::create_with_mode(
            parent,
            width,
            height,
            dpi,
            text,
            false,
            on_key,
            multiline,
            Some(appearance),
        )
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    fn create_with_mode(
        parent: HWND,
        width: i32,
        height: i32,
        dpi: u32,
        text: &str,
        dark: bool,
        on_key: KeyHandler,
        multiline: bool,
        appearance: Option<EditAppearance>,
    ) -> Result<Self> {
        let class = to_wide("EDIT");
        let text_w = to_wide(text);
        let (text_color, back_color) = if dark {
            (0x00FF_FFFF, 0x0032_3232)
        } else {
            (0x001B_1B1B, 0x00FB_FBFB)
        };
        let (text_color, back_color) =
            appearance.map_or((text_color, back_color), |a| (a.foreground, a.background));
        let font_size = appearance.map_or(14.0, |a| a.font_size.clamp(8.0, 72.0));
        // SAFETY: standard control creation; all buffers outlive the calls.
        unsafe {
            let hwnd = CreateWindowExW(
                0,
                PCWSTR(class.as_ptr()),
                PCWSTR(text_w.as_ptr()),
                (WS_CHILD
                    | WS_VISIBLE
                    | if multiline {
                        ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN | WS_VSCROLL
                    } else {
                        ES_AUTOHSCROLL
                    }) as u32,
                0,
                0,
                width,
                height,
                Some(parent),
                None,
                None,
                None,
            );
            if hwnd.0.is_null() {
                return Err(Error::from_thread());
            }
            let face = to_wide("Segoe UI Variable Text");
            let font = CreateFontW(
                -(font_size * dpi as f32 / 96.0).round() as i32,
                0,
                0,
                0,
                FW_NORMAL,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                (DEFAULT_PITCH | FF_DONTCARE) as u32,
                PCWSTR(face.as_ptr()),
            );
            SendMessageW(hwnd, WM_SETFONT as u32, WPARAM(font.0 as usize), LPARAM(1));
            let brush = CreateSolidBrush(COLORREF(back_color));
            if multiline {
                SendMessageW(hwnd, EM_LIMITTEXT as u32, WPARAM(8192), LPARAM(0));
            }
            let proc = if multiline {
                subclass_proc_multiline
            } else {
                subclass_proc
            };
            let _ = SetWindowSubclass(hwnd, Some(proc), 1, on_key as usize);
            Ok(Self {
                hwnd,
                font,
                brush,
                text_color,
                back_color,
                multiline,
                auto_scrollbar: multiline && appearance.is_some(),
            })
        }
    }

    pub fn focus_and_select_all(&self) {
        self.focus_and_select(0, -1);
    }

    /// Focuses the control and selects `start..end` (UTF-16 units; `end` = -1 selects to the
    /// end). Explorer's rename box selects only the stem this way.
    pub fn focus_and_select(&self, start: i32, end: i32) {
        // SAFETY: plain FFI calls on our control.
        unsafe {
            let _ = SetFocus(Some(self.hwnd));
            SendMessageW(
                self.hwnd,
                EM_SETSEL as u32,
                WPARAM(start as usize),
                LPARAM(end as isize),
            );
        }
    }

    pub fn text(&self) -> String {
        // SAFETY: buffer sized from the control's own length.
        unsafe {
            let len = GetWindowTextLengthW(self.hwnd);
            let mut buf = vec![0u16; (len.max(0) as usize) + 1];
            let n = GetWindowTextW(self.hwnd, PWSTR(buf.as_mut_ptr()), buf.len() as i32);
            from_wide(&buf[..n.max(0) as usize])
        }
    }

    /// Match scrollbar visibility to the native control's wrapped line count.
    /// Keep native wheel/keyboard scrolling when text actually overflows.
    pub fn refresh_scrollbar(&self) {
        if !self.auto_scrollbar {
            return;
        }
        // SAFETY: the control, font and temporary DC belong to this live editor.
        unsafe {
            let hdc = GetDC(Some(self.hwnd));
            if hdc.0.is_null() {
                return;
            }
            let old = SelectObject(hdc, HGDIOBJ(self.font.0));
            let mut metrics = TEXTMETRICW::default();
            let measured = GetTextMetricsW(hdc, &mut metrics).as_bool();
            let _ = SelectObject(hdc, old);
            ReleaseDC(Some(self.hwnd), hdc);
            if !measured {
                return;
            }
            let mut r = RECT::default();
            let _ = GetClientRect(self.hwnd, &mut r);
            let lines =
                SendMessageW(self.hwnd, EM_GETLINECOUNT as u32, WPARAM(0), LPARAM(0)).0 as i32;
            let overflow = lines.saturating_mul(metrics.tmHeight) > (r.bottom - r.top).max(1);
            let _ = ShowScrollBar(self.hwnd, SB_VERT, overflow);
        }
    }

    /// Handles `WM_CTLCOLOREDIT` for the parent: sets colours and returns the brush.
    pub fn ctl_color(&self, hdc_wparam: usize) -> isize {
        // SAFETY: wParam is the control's HDC for this message.
        unsafe {
            let hdc = HDC(hdc_wparam as *mut core::ffi::c_void);
            let _ = SetTextColor(hdc, COLORREF(self.text_color));
            let _ = SetBkColor(hdc, COLORREF(self.back_color));
        }
        self.brush.0 as isize
    }
}

impl Drop for EditControl {
    fn drop(&mut self) {
        // SAFETY: we own these GDI objects; the control dies with its parent window.
        unsafe {
            let proc = if self.multiline {
                subclass_proc_multiline
            } else {
                subclass_proc
            };
            let _ = RemoveWindowSubclass(self.hwnd, Some(proc), 1);
            let _ = DestroyWindow(self.hwnd);
            let _ = DeleteObject(HGDIOBJ(self.font.0));
            let _ = DeleteObject(HGDIOBJ(self.brush.0));
        }
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    refdata: usize,
) -> LRESULT {
    // SAFETY: forwarded unchanged from the registered subclass callback.
    unsafe { subclass_message(hwnd, msg, wparam, lparam, refdata, false) }
}

unsafe extern "system" fn subclass_proc_multiline(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    refdata: usize,
) -> LRESULT {
    // SAFETY: forwarded unchanged from the registered subclass callback.
    unsafe { subclass_message(hwnd, msg, wparam, lparam, refdata, true) }
}

unsafe fn subclass_message(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    refdata: usize,
    multiline: bool,
) -> LRESULT {
    // SAFETY: refdata is the `KeyHandler` fn pointer registered in `create`.
    unsafe {
        if msg == WM_KEYDOWN as u32 && refdata != 0 {
            let handler: KeyHandler = core::mem::transmute::<usize, KeyHandler>(refdata);
            if handler(wparam.0 as u32) {
                return LRESULT(0);
            }
        }
        if msg == WM_CHAR as u32 && ((wparam.0 == 13 && !multiline) || wparam.0 == 27) {
            return LRESULT(0); // swallow the beep for Enter/Esc
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::{ClassOptions, WindowBuilder, WindowClass, style};

    fn ignore_key(_: u32) -> bool {
        false
    }

    #[test]
    fn paper_editor_shows_scrollbar_only_for_overflow_and_keeps_wheel_scrolling() {
        let class =
            WindowClass::register("PecoFence.EditControl.Paper.Test", ClassOptions::default())
                .unwrap();
        let parent = WindowBuilder::new(&class)
            .style(style::POPUP)
            .bounds(0, 0, 400, 120)
            .create(Box::new(|_, _, _, _| None))
            .unwrap();
        let ctl = EditControl::create_styled(
            parent.hwnd(),
            400,
            120,
            96,
            "short note",
            ignore_key,
            true,
            EditAppearance {
                foreground: 0x0022_272B,
                background: 0x00D6_E4EC,
                font_size: 14.0,
            },
        )
        .unwrap();
        ctl.refresh_scrollbar();
        crate::window::show_normal(parent.hwnd());
        // SAFETY: all messages target this test's live EDIT control; buffers outlive SendMessageW.
        unsafe {
            assert_eq!(
                GetWindowLongPtrW(ctl.hwnd, GWL_STYLE) as u32 & WS_VSCROLL as u32,
                0
            );
            let text = (0..100)
                .map(|n| format!("Native line {n}\r\n"))
                .collect::<String>();
            let wide = to_wide(&text);
            SendMessageW(
                ctl.hwnd,
                WM_SETTEXT as u32,
                WPARAM(0),
                LPARAM(wide.as_ptr() as isize),
            );
            ctl.refresh_scrollbar();
            assert_ne!(
                GetWindowLongPtrW(ctl.hwnd, GWL_STYLE) as u32 & WS_VSCROLL as u32,
                0
            );
            let end = text.encode_utf16().count();
            ctl.focus_and_select(end as i32, end as i32);
            SendMessageW(
                ctl.hwnd,
                EM_SETSEL as u32,
                WPARAM(end),
                LPARAM(end as isize),
            );
            SendMessageW(ctl.hwnd, EM_SCROLLCARET as u32, WPARAM(0), LPARAM(0));
            let before = SendMessageW(
                ctl.hwnd,
                EM_GETFIRSTVISIBLELINE as u32,
                WPARAM(0),
                LPARAM(0),
            )
            .0;
            SendMessageW(ctl.hwnd, WM_MOUSEWHEEL as u32, WPARAM(120 << 16), LPARAM(0));
            let after = SendMessageW(
                ctl.hwnd,
                EM_GETFIRSTVISIBLELINE as u32,
                WPARAM(0),
                LPARAM(0),
            )
            .0;
            assert!(
                before > 0 && after < before,
                "native wheel must scroll overflow: {before} -> {after}"
            );
            let short = to_wide("short again");
            SendMessageW(
                ctl.hwnd,
                WM_SETTEXT as u32,
                WPARAM(0),
                LPARAM(short.as_ptr() as isize),
            );
            ctl.refresh_scrollbar();
            assert_eq!(
                GetWindowLongPtrW(ctl.hwnd, GWL_STYLE) as u32 & WS_VSCROLL as u32,
                0
            );
        }
    }

    #[test]
    fn multiline_enter_inserts_newline_but_single_line_enter_does_not() {
        let class =
            WindowClass::register("PecoFence.EditControl.Test", ClassOptions::default()).unwrap();
        let parent = WindowBuilder::new(&class)
            .style(style::POPUP)
            .bounds(0, 0, 400, 240)
            .create(Box::new(|_, _, _, _| None))
            .unwrap();
        let multiline = EditControl::create_multiline(
            parent.hwnd(),
            400,
            200,
            96,
            "first\r\nsecond",
            false,
            ignore_key,
        )
        .unwrap();
        assert_eq!(multiline.text(), "first\r\nsecond");
        let end = multiline.text().encode_utf16().count();
        // SAFETY: the messages target our live, hidden test EDIT control.
        unsafe {
            SendMessageW(
                multiline.hwnd,
                EM_SETSEL as u32,
                WPARAM(end),
                LPARAM(end as isize),
            );
            SendMessageW(multiline.hwnd, WM_CHAR as u32, WPARAM(13), LPARAM(0));
        }
        assert_eq!(multiline.text(), "first\r\nsecond\r\n");
        drop(multiline);
        let single =
            EditControl::create(parent.hwnd(), 400, 28, 96, "title", false, ignore_key).unwrap();
        // SAFETY: the control is still owned by this test.
        unsafe {
            SendMessageW(single.hwnd, WM_CHAR as u32, WPARAM(13), LPARAM(0));
        }
        assert_eq!(single.text(), "title");
    }
}

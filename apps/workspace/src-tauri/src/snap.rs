//! Windows 11 Snap Layouts for the undecorated window. A native child window over the web
//! maximise button answers WM_NCHITTEST with HTMAXBUTTON, so Windows shows its own layout
//! flyout on hover. The button is fixed — second from the right, 46 × 48 CSS px — so its
//! rectangle follows from the client width and the window DPI; nothing is measured in the
//! page. Hover and press reach the page as `caption-max` events (enter, leave, down, up) and
//! a click maximises or restores natively. Windows 10 gets the button without the flyout.
const BUTTON: f64 = 46.0;
const BAND: f64 = 48.0;
/// Close is rightmost; maximise is the one before it.
const FROM_RIGHT: f64 = 2.0;

/// The maximise button's rectangle in physical client pixels.
pub fn button_rect(client_width: i32, dpi: u32) -> (i32, i32, i32, i32) {
    let scale = f64::from(dpi.max(96)) / 96.0;
    let right = f64::from(client_width) - (FROM_RIGHT - 1.0) * BUTTON * scale;
    let left = right - BUTTON * scale;
    (
        left.round() as i32,
        0,
        (right.round() - left.round()) as i32,
        (BAND * scale).round() as i32,
    )
}

#[cfg(windows)]
pub use native::install;

#[cfg(windows)]
mod native {
    use super::button_rect;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent,
    };
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, GetClientRect, HTMAXBUTTON, HWND_TOP, IsZoomed,
        PostMessageW, RegisterClassExW, SC_MAXIMIZE, SC_RESTORE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
        SetWindowPos, WM_DPICHANGED, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDBLCLK,
        WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_SIZE, WM_SYSCOMMAND,
        WNDCLASSEXW, WS_CHILD, WS_CLIPSIBLINGS, WS_VISIBLE,
    };
    use windows_sys::core::w;

    const SUBCLASS_ID: usize = 0x5157_4D58;

    static EMIT: OnceLock<Box<dyn Fn(&str) + Send + Sync>> = OnceLock::new();
    static OVERLAY: AtomicIsize = AtomicIsize::new(0);
    static INSIDE: AtomicBool = AtomicBool::new(false);
    static PRESSED: AtomicBool = AtomicBool::new(false);

    fn emit(state: &str) {
        if let Some(emit) = EMIT.get() {
            emit(state);
        }
    }

    /// Installs the overlay on the main window; failures leave the plain web button working.
    pub fn install<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
        let window: WebviewWindow<R> = app.get_webview_window("main").ok_or("no main window")?;
        let handle = app.clone();
        let _ = EMIT.set(Box::new(move |state: &str| {
            let _ = handle.emit_to("main", "caption-max", state);
        }));
        let parent = window.hwnd().map_err(|e| e.to_string())?.0 as HWND;
        // SAFETY: `parent` is the live top-level HWND Tauri created; the class and child window
        // are created on the thread that owns it (setup runs on the main thread).
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(overlay_proc),
                hInstance: instance,
                lpszClassName: w!("TinnciWorkspaceSnap"),
                ..std::mem::zeroed()
            };
            RegisterClassExW(&class);
            let overlay = CreateWindowExW(
                0,
                w!("TinnciWorkspaceSnap"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
                0,
                0,
                0,
                0,
                parent,
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if overlay.is_null() {
                return Err(format!(
                    "CreateWindowExW: {}",
                    std::io::Error::last_os_error()
                ));
            }
            OVERLAY.store(overlay as isize, Ordering::SeqCst);
            if SetWindowSubclass(parent, Some(parent_proc), SUBCLASS_ID, 0) == 0 {
                return Err("SetWindowSubclass failed".into());
            }
            place(parent);
        }
        Ok(())
    }

    /// Keeps the overlay on top of the webview, over the maximise button.
    unsafe fn place(parent: HWND) {
        let overlay = OVERLAY.load(Ordering::SeqCst) as HWND;
        if overlay.is_null() {
            return;
        }
        let mut client: RECT = unsafe { std::mem::zeroed() };
        // SAFETY: both HWNDs are live while the parent receives messages.
        unsafe {
            GetClientRect(parent, &mut client);
            let (x, y, w, h) = button_rect(client.right - client.left, GetDpiForWindow(parent));
            SetWindowPos(
                overlay,
                HWND_TOP,
                x,
                y,
                w,
                h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    unsafe extern "system" fn parent_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        // SAFETY: Win32 supplied these arguments to the installed subclass callback.
        unsafe {
            let result = DefSubclassProc(hwnd, msg, wparam, lparam);
            match msg {
                WM_SIZE | WM_DPICHANGED => place(hwnd),
                WM_NCDESTROY => {
                    OVERLAY.store(0, Ordering::SeqCst);
                    RemoveWindowSubclass(hwnd, Some(parent_proc), SUBCLASS_ID);
                }
                _ => {}
            }
            result
        }
    }

    unsafe extern "system" fn overlay_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_NCHITTEST => HTMAXBUTTON as LRESULT,
            WM_NCMOUSEMOVE => {
                if !INSIDE.swap(true, Ordering::SeqCst) {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE | TME_NONCLIENT,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    // SAFETY: `track` is initialised for this live overlay.
                    unsafe { TrackMouseEvent(&mut track) };
                    emit("enter");
                }
                0
            }
            WM_NCMOUSELEAVE => {
                INSIDE.store(false, Ordering::SeqCst);
                PRESSED.store(false, Ordering::SeqCst);
                emit("leave");
                0
            }
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK => {
                PRESSED.store(true, Ordering::SeqCst);
                emit("down");
                0
            }
            WM_NCLBUTTONUP => {
                if PRESSED.swap(false, Ordering::SeqCst) {
                    emit("up");
                    // SAFETY: the parent of a live child overlay is live.
                    unsafe {
                        let parent = windows_sys::Win32::UI::WindowsAndMessaging::GetParent(hwnd);
                        let command = if IsZoomed(parent) != 0 {
                            SC_RESTORE
                        } else {
                            SC_MAXIMIZE
                        };
                        PostMessageW(parent, WM_SYSCOMMAND, command as WPARAM, 0);
                    }
                }
                0
            }
            // SAFETY: unchanged arguments forwarded to the default procedure.
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::button_rect;

    #[test]
    fn maximise_button_is_second_from_the_right_at_every_scale() {
        assert_eq!(button_rect(1180, 96), (1088, 0, 46, 48));
        assert_eq!(button_rect(1770, 144), (1632, 0, 69, 72));
        assert_eq!(button_rect(1475, 120), (1360, 0, 58, 60));
    }
}

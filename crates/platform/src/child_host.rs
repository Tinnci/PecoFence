//! Spike: host an existing top-level PecoFence window as a child of another window
//! (the SPM workspace's WinUI window) and report how it behaves there.
//!
//! This is evaluation code for one design question. It changes window styles in
//! place and does not try to restore them.

use crate::bindings::*;
use crate::desktop::class_name;

const GW_CHILD: u32 = 5;

/// Makes `child` a `WS_CHILD` of `parent`, without a frame or tool-window style.
pub fn adopt(child: HWND, parent: HWND) -> windows_core::Result<()> {
    windows_core::link!("user32.dll" "system" fn SetParent(hwndchild : HWND, hwndnewparent : HWND) -> HWND);
    // SAFETY: plain Win32 calls on windows this thread owns.
    unsafe {
        let style = GetWindowLongPtrW(child, GWL_STYLE) as u32;
        let style = (style & !(WS_POPUP | WS_THICKFRAME as u32 | WS_CAPTION as u32))
            | WS_CHILD as u32
            | WS_CLIPSIBLINGS as u32;
        SetWindowLongPtrW(child, GWL_STYLE, style as isize);
        let ex = GetWindowLongPtrW(child, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(
            child,
            GWL_EXSTYLE,
            (ex & !(WS_EX_TOOLWINDOW as u32)) as isize,
        );
        if SetParent(child, parent).0.is_null() {
            return Err(windows_core::Error::from_thread());
        }
    }
    Ok(())
}

/// Moves `child` to `rect` in its parent's client pixels and puts it on top of its siblings.
pub fn place(child: HWND, rect: RECT) -> bool {
    // SAFETY: plain Win32 call.
    unsafe {
        SetWindowPos(
            child,
            Some(HWND_TOP),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            (SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED) as u32,
        )
        .as_bool()
    }
}

/// Client size in physical pixels and the window's DPI.
pub fn client(hwnd: HWND) -> (i32, i32, u32) {
    let mut r = RECT::default();
    // SAFETY: `r` outlives the call.
    unsafe {
        let _ = GetClientRect(hwnd, &mut r);
        (r.right - r.left, r.bottom - r.top, GetDpiForWindow(hwnd))
    }
}

/// One observation of the hosted child, for the spike report.
#[derive(Debug)]
pub struct Probe {
    pub parent_is_host: bool,
    /// The child's position among the parent's children, 0 = topmost.
    pub z_index: Option<usize>,
    pub siblings: Vec<String>,
    pub child_rect_in_parent: RECT,
    /// Which window is under the child's centre: "child", or another class name.
    pub hit_at_center: String,
    pub focus: String,
    pub foreground_is_host: bool,
    pub child_dpi: u32,
    pub host_dpi: u32,
}

pub fn probe(child: HWND, host: HWND) -> Probe {
    // SAFETY: plain Win32 queries.
    unsafe {
        let parent_is_host = GetAncestor(child, 1 /* GA_PARENT */) == host;
        let mut siblings = Vec::new();
        let mut z_index = None;
        let mut next = GetWindow(host, GW_CHILD);
        while !next.0.is_null() {
            if next == child {
                z_index = Some(siblings.len());
            }
            siblings.push(class_name(next));
            next = GetWindow(next, GW_HWNDNEXT as u32);
        }
        let mut wr = RECT::default();
        let _ = GetWindowRect(child, &mut wr);
        let mut origin = POINT { x: 0, y: 0 };
        windows_core::link!("user32.dll" "system" fn ClientToScreen(hwnd : HWND, lppoint : *mut POINT) -> windows_core::BOOL);
        let _ = ClientToScreen(host, &mut origin);
        let center = POINT {
            x: (wr.left + wr.right) / 2,
            y: (wr.top + wr.bottom) / 2,
        };
        let hit = WindowFromPoint(center);
        let describe = |h: HWND| {
            if h.0.is_null() {
                "none".to_owned()
            } else if h == child {
                "child".to_owned()
            } else if h == host {
                "host".to_owned()
            } else {
                class_name(h)
            }
        };
        Probe {
            parent_is_host,
            z_index,
            siblings,
            child_rect_in_parent: RECT {
                left: wr.left - origin.x,
                top: wr.top - origin.y,
                right: wr.right - origin.x,
                bottom: wr.bottom - origin.y,
            },
            hit_at_center: describe(hit),
            focus: describe(GetFocus()),
            foreground_is_host: GetForegroundWindow() == host,
            child_dpi: GetDpiForWindow(child),
            host_dpi: GetDpiForWindow(host),
        }
    }
}

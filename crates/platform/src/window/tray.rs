//! Notification-area icon and popup menus.

use crate::bindings::*;
use crate::wide::to_wide;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use windows_core::{Error, PCWSTR, Result};

/// Creates a 32-bit icon from premultiplied BGRA pixels.
pub fn icon_from_bgra(width: i32, height: i32, bgra: &[u8]) -> Result<HICON> {
    // SAFETY: the pixel buffer matches width*height*4; GDI copies it.
    unsafe {
        let color = CreateBitmap(width, height, 1, 32, Some(bgra.as_ptr().cast()));
        let mask = CreateBitmap(width, height, 1, 1, None);
        let info = ICONINFO {
            fIcon: windows_core::BOOL(1),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info);
        let _ = DeleteObject(HGDIOBJ(color.0));
        let _ = DeleteObject(HGDIOBJ(mask.0));
        if icon.0.is_null() {
            return Err(Error::from_thread());
        }
        Ok(icon)
    }
}

/// An HICON destroyed on drop (window caption icons and the like).
pub struct OwnedIcon(HICON);

impl OwnedIcon {
    pub fn from_bgra(size: i32, bgra: &[u8]) -> Result<Self> {
        icon_from_bgra(size, size, bgra).map(Self)
    }

    /// Raw handle value for `WM_SETICON`.
    pub fn raw(&self) -> isize {
        self.0.0 as isize
    }
}

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        destroy_icon(self.0);
    }
}

pub fn destroy_icon(icon: HICON) {
    // SAFETY: plain FFI call.
    unsafe {
        let _ = DestroyIcon(icon);
    }
}

/// A notification-area icon; removed on drop.
pub struct TrayIcon {
    hwnd: HWND,
    id: u32,
}

impl TrayIcon {
    /// Adds the icon; notifications arrive as `callback_message` on `hwnd` (NOTIFYICON_VERSION_4:
    /// `LOWORD(lParam)` = event such as `WM_CONTEXTMENU`/`NIN_SELECT`, x/y in wParam).
    pub fn add(hwnd: HWND, id: u32, callback_message: u32, icon: HICON, tip: &str) -> Result<Self> {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: id,
            uFlags: (NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP) as u32,
            uCallbackMessage: callback_message,
            hIcon: icon,
            ..Default::default()
        };
        copy_tip(&mut data.szTip, tip);
        // SAFETY: fully initialized structure.
        unsafe {
            if !Shell_NotifyIconW(NIM_ADD as u32, &data).as_bool() {
                return Err(Error::from_thread());
            }
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4 as u32;
            let _ = Shell_NotifyIconW(NIM_SETVERSION as u32, &data);
        }
        Ok(Self { hwnd, id })
    }

    pub fn set_tip(&self, tip: &str) {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: self.id,
            uFlags: (NIF_TIP | NIF_SHOWTIP) as u32,
            ..Default::default()
        };
        copy_tip(&mut data.szTip, tip);
        // SAFETY: fully initialized structure.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY as u32, &data);
        }
    }

    /// Shows a toast/balloon notification.
    pub fn show_info(&self, title: &str, text: &str, warning: bool) {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: self.id,
            uFlags: NIF_INFO as u32,
            dwInfoFlags: if warning {
                NIIF_WARNING as u32
            } else {
                NIIF_INFO as u32
            },
            ..Default::default()
        };
        copy_wide(&mut data.szInfoTitle, title);
        copy_wide(&mut data.szInfo, text);
        // SAFETY: fully initialized structure.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY as u32, &data);
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        let data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: self.id,
            ..Default::default()
        };
        // SAFETY: fully initialized structure.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE as u32, &data);
        }
    }
}

fn copy_tip(dst: &mut [u16; 128], s: &str) {
    copy_wide(dst, s);
}

fn copy_wide<const N: usize>(dst: &mut [u16; N], s: &str) {
    let w = to_wide(s);
    let n = w.len().min(N - 1);
    dst[..n].copy_from_slice(&w[..n]);
    dst[n] = 0;
}

/// Tray callback decoding for NOTIFYICON_VERSION_4.
pub struct TrayEvent {
    pub event: u32,
    pub x: i32,
    pub y: i32,
}

pub fn decode_tray_message(wparam: usize, lparam: isize) -> TrayEvent {
    TrayEvent {
        event: (lparam & 0xffff) as u32,
        x: (wparam & 0xffff) as u16 as i16 as i32,
        y: ((wparam >> 16) & 0xffff) as u16 as i16 as i32,
    }
}

pub const TRAY_EVENT_CONTEXTMENU: u32 = WM_CONTEXTMENU as u32;
pub const TRAY_EVENT_SELECT: u32 = NIN_SELECT as u32;
pub const TRAY_EVENT_KEYSELECT: u32 = NIN_KEYSELECT as u32;
/// The user clicked the most recent balloon notice (`show_info`).
pub const TRAY_EVENT_BALLOON_CLICK: u32 = NIN_BALLOONUSERCLICK as u32;
pub const TRAY_EVENT_LBUTTONDBLCLK: u32 = WM_LBUTTONDBLCLK as u32;

/// Application-owned menu data, independent of HMENU and the visual backend.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntry {
    Item {
        id: u32,
        label: String,
        checked: Option<bool>,
        disabled: bool,
    },
    Separator,
    Submenu {
        label: String,
        entries: Vec<MenuEntry>,
    },
}

type MenuRenderer = dyn Fn(&[MenuEntry], i32, i32, bool) -> Result<u32>;
thread_local! {
    static MENU_RENDERER: RefCell<Option<Rc<MenuRenderer>>> = const { RefCell::new(None) };
    static MENU_DARK: Cell<bool> = const { Cell::new(false) };
}

/// UI-thread registration; restoring the prior renderer also releases its UI context.
pub struct MenuRendererGuard {
    previous: Option<Rc<MenuRenderer>>,
}
impl Drop for MenuRendererGuard {
    fn drop(&mut self) {
        MENU_RENDERER.with(|renderer| *renderer.borrow_mut() = self.previous.take());
    }
}
pub fn install_menu_renderer(
    renderer: impl Fn(&[MenuEntry], i32, i32, bool) -> Result<u32> + 'static,
) -> MenuRendererGuard {
    let previous = MENU_RENDERER.with(|slot| slot.borrow_mut().replace(Rc::new(renderer)));
    MenuRendererGuard { previous }
}
pub fn set_menu_dark(dark: bool) {
    MENU_DARK.with(|mode| mode.set(dark));
}
pub fn menu_dark() -> bool {
    MENU_DARK.with(Cell::get)
}

/// A popup menu built from items; destroyed on drop.
pub struct PopupMenu {
    hmenu: HMENU,
    entries: RefCell<Vec<MenuEntry>>,
}

impl Default for PopupMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl PopupMenu {
    pub fn new() -> Self {
        // SAFETY: plain FFI call.
        Self {
            hmenu: unsafe { CreatePopupMenu() },
            entries: RefCell::new(Vec::new()),
        }
    }

    pub fn handle(&self) -> HMENU {
        self.hmenu
    }

    /// Number of items currently in the menu (= the insertion index for appending).
    pub fn len(&self) -> u32 {
        // SAFETY: plain FFI call on a menu we own.
        unsafe { GetMenuItemCount(Some(self.hmenu)).max(0) as u32 }
    }

    pub fn item(&self, id: u32, text: &str, checked: bool, disabled: bool) -> &Self {
        self.entries.borrow_mut().push(MenuEntry::Item {
            id,
            label: text.to_owned(),
            checked: checked.then_some(true),
            disabled,
        });
        let w = to_wide(text);
        let mut flags = MF_STRING as u32;
        if checked {
            flags |= MF_CHECKED as u32;
        }
        if disabled {
            flags |= (MF_GRAYED | MF_DISABLED) as u32;
        }
        // SAFETY: string outlives the call.
        unsafe {
            let _ = AppendMenuW(self.hmenu, flags, id as usize, PCWSTR(w.as_ptr()));
        }
        self
    }

    /// A checkable command; unchecked items retain the same native toggle semantics.
    pub fn check_item(&self, id: u32, text: &str, checked: bool, disabled: bool) -> &Self {
        self.item(id, text, checked, disabled);
        if let Some(MenuEntry::Item { checked: state, .. }) = self.entries.borrow_mut().last_mut() {
            *state = Some(checked);
        }
        self
    }

    /// Marks `id` as the default item (drawn bold; Enter / double-click pick it), as the shell's
    /// right-drag menu does for the effect the modifiers would have produced.
    pub fn default_item(&self, id: u32) -> &Self {
        windows_core::link!("user32.dll" "system" fn SetMenuDefaultItem(hmenu: HMENU, uitem: u32, fbypos: u32) -> windows_core::BOOL);
        // SAFETY: plain FFI call on a menu we own; `fbypos` = FALSE means `uitem` is an id.
        unsafe {
            let _ = SetMenuDefaultItem(self.hmenu, id, 0);
        }
        self
    }

    pub fn separator(&self) -> &Self {
        self.entries.borrow_mut().push(MenuEntry::Separator);
        // SAFETY: plain FFI call.
        unsafe {
            let _ = AppendMenuW(self.hmenu, MF_SEPARATOR as u32, 0, PCWSTR::null());
        }
        self
    }

    /// Adds `sub` as a submenu (ownership transfers to this menu).
    pub fn submenu(&self, text: &str, sub: PopupMenu) -> &Self {
        let w = to_wide(text);
        let h = sub.hmenu;
        self.entries.borrow_mut().push(MenuEntry::Submenu {
            label: text.to_owned(),
            entries: sub.entries.take(),
        });
        std::mem::forget(sub);
        // SAFETY: string outlives the call; submenu handle is now owned by the parent.
        unsafe {
            let _ = AppendMenuW(
                self.hmenu,
                (MF_STRING | MF_POPUP) as u32,
                h.0 as usize,
                PCWSTR(w.as_ptr()),
            );
        }
        self
    }

    /// Shows the menu at screen coordinates and returns the chosen command id (0 = dismissed).
    /// `owner` is brought to the foreground first (required for tray menus to dismiss properly).
    pub fn show(&self, owner: HWND, x: i32, y: i32) -> u32 {
        self.show_aligned(owner, x, y, true)
    }

    /// Context-menu placement: opens downwards from the click point (Explorer-style).
    pub fn show_context(&self, owner: HWND, x: i32, y: i32) -> u32 {
        self.show_aligned(owner, x, y, false)
    }

    fn show_aligned(&self, owner: HWND, x: i32, y: i32, bottom_align: bool) -> u32 {
        let renderer = MENU_RENDERER.with(|slot| slot.borrow().clone());
        if let Some(renderer) = renderer {
            return renderer(&self.entries.borrow(), x, y, bottom_align).unwrap_or(0);
        }
        self.show_classic_aligned(owner, x, y, bottom_align)
    }

    /// Explicit Windows Shell/OLE protocol handoff; not an application-style fallback.
    pub fn show_classic_context(&self, owner: HWND, x: i32, y: i32) -> u32 {
        self.show_classic_aligned(owner, x, y, false)
    }

    fn show_classic_aligned(&self, owner: HWND, x: i32, y: i32, bottom_align: bool) -> u32 {
        let mut flags = (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN) as u32;
        if bottom_align {
            flags |= TPM_BOTTOMALIGN as u32;
        }
        // SAFETY: plain FFI calls; WM_NULL nudge is the documented tray-menu workaround.
        unsafe {
            let _ = SetForegroundWindow(owner);
            let cmd = TrackPopupMenuEx(self.hmenu, flags, x, y, owner, None);
            let _ = PostMessageW(Some(owner), WM_NULL as u32, WPARAM(0), LPARAM(0));
            cmd.0 as u32
        }
    }
}

impl Drop for PopupMenu {
    fn drop(&mut self) {
        // SAFETY: we own the menu.
        unsafe {
            let _ = DestroyMenu(self.hmenu);
        }
    }
}

//! The OLE clipboard as Explorer uses it: a `CF_HDROP` file list plus the shell's
//! `Preferred DropEffect` word that distinguishes 剪切 (move) from 复制 (copy).

use crate::bindings::*;
use crate::dragdrop;
use std::path::PathBuf;
use std::sync::OnceLock;

static PREFERRED_EFFECT_FORMAT: OnceLock<u16> = OnceLock::new();

/// Files currently on the clipboard and whether they were cut (`true`) or copied (`false`).
/// A missing `Preferred DropEffect` counts as copy, as in Explorer.
pub fn file_list() -> Option<(Vec<PathBuf>, bool)> {
    // SAFETY: plain OLE call; the returned object is released when dropped.
    let data = unsafe { OleGetClipboard() }.ok()?;
    if !dragdrop::has_hdrop(&data) {
        return None;
    }
    let paths = dragdrop::hdrop_paths(&data);
    if paths.is_empty() {
        return None;
    }
    let fmt = dragdrop::hglobal_format(dragdrop::registered_format(
        "Preferred DropEffect",
        &PREFERRED_EFFECT_FORMAT,
    ));
    let effect = dragdrop::hglobal_bytes(&data, &fmt)
        .filter(|b| b.len() >= 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .unwrap_or(DROPEFFECT_COPY as u32);
    let cut = effect & DROPEFFECT_MOVE as u32 != 0 && effect & DROPEFFECT_COPY as u32 == 0;
    Some((paths, cut))
}

/// Cheap check for menu enabling: is there a file list on the clipboard?
pub fn has_file_list() -> bool {
    // SAFETY: plain OLE call.
    unsafe { OleGetClipboard() }
        .map(|d| dragdrop::has_hdrop(&d))
        .unwrap_or(false)
}

/// Empties the clipboard (Explorer does this after pasting cut files).
pub fn clear() {
    // SAFETY: plain OLE call; a null object empties the clipboard.
    unsafe {
        let _ = OleSetClipboard(None::<&IDataObject>);
    }
}

/// `GetClipboardSequenceNumber`: changes whenever any program changes the clipboard.
pub fn sequence() -> u32 {
    // SAFETY: plain FFI call.
    unsafe { GetClipboardSequenceNumber() }
}

/// Writes Unicode text and transfers the allocation to the system clipboard on success.
pub fn set_text(text: &str) -> windows_core::Result<()> {
    windows_core::link!("user32.dll" "system" fn OpenClipboard(hwnd: isize) -> i32);
    windows_core::link!("user32.dll" "system" fn CloseClipboard() -> i32);
    windows_core::link!("user32.dll" "system" fn EmptyClipboard() -> i32);
    windows_core::link!("user32.dll" "system" fn SetClipboardData(format: u32, memory: *mut core::ffi::c_void) -> *mut core::ffi::c_void);
    windows_core::link!("kernel32.dll" "system" fn GlobalAlloc(flags: u32, bytes: usize) -> *mut core::ffi::c_void);
    windows_core::link!("kernel32.dll" "system" fn GlobalLock(memory: *mut core::ffi::c_void) -> *mut core::ffi::c_void);
    windows_core::link!("kernel32.dll" "system" fn GlobalUnlock(memory: *mut core::ffi::c_void) -> i32);
    windows_core::link!("kernel32.dll" "system" fn GlobalFree(memory: *mut core::ffi::c_void) -> *mut core::ffi::c_void);
    const CF_UNICODETEXT: u32 = 13;
    const GMEM_MOVEABLE: u32 = 2;
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * std::mem::size_of::<u16>();
    // SAFETY: the movable allocation is retained until SetClipboardData succeeds, at which point
    // ownership transfers to the system. Every successful OpenClipboard is paired with close.
    unsafe {
        if OpenClipboard(0) == 0 {
            return Err(windows_core::Error::from_thread());
        }
        if EmptyClipboard() == 0 {
            CloseClipboard();
            return Err(windows_core::Error::from_thread());
        }
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if memory.is_null() {
            CloseClipboard();
            return Err(windows_core::Error::from_thread());
        }
        let destination = GlobalLock(memory);
        if destination.is_null() {
            GlobalFree(memory);
            CloseClipboard();
            return Err(windows_core::Error::from_thread());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, destination as *mut u8, bytes);
        GlobalUnlock(memory);
        if SetClipboardData(CF_UNICODETEXT, memory).is_null() {
            GlobalFree(memory);
            CloseClipboard();
            return Err(windows_core::Error::from_thread());
        }
        CloseClipboard();
    }
    Ok(())
}

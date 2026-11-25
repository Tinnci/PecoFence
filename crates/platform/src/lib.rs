//! Thin, safe-ish wrappers over the Win32 / DWM / OLE / shell APIs PecoFence needs.
//!
//! All `unsafe` in the project lives in this crate and in `pecofence-render`. Every
//! unsafe block carries a `// SAFETY:` note. The generated `bindings` module is private;
//! other crates use the wrappers exported here.

#![cfg(windows)]
#![allow(clippy::missing_safety_doc)]

#[allow(
    non_snake_case,
    non_upper_case_globals,
    non_camel_case_types,
    clippy::upper_case_acronyms,
    clippy::missing_transmute_annotations,
    clippy::useless_transmute,
    clippy::too_many_arguments,
    dead_code,
    unused_imports
)]
mod bindings;

#[allow(
    non_snake_case,
    non_camel_case_types,
    non_upper_case_globals,
    clippy::all,
    dead_code
)]
mod uia_bindings;

pub mod com;
pub mod gfx;
pub mod os;
pub mod shell;
pub mod window;

pub use self::com::{accessibility, clipboard, dispatcher, dragdrop};
pub use self::gfx::{d2d, fonts, frameclock, monitors, theme, wallpaper};
pub use self::os::{
    autostart, crashlog, http, locale, memstats, named_pipe, ports, process, sysparams, wide,
};
pub use self::shell::{
    desktop, filedialog, fileinfo, portal_reader, quicklook, shell_icons, shell_menu, shell_notify,
    watcher,
};
#[cfg(debug_assertions)]
pub use self::window::child_host;
pub use self::window::{
    dialogs, dwm, edit, hotkey, layered, msg, rawinput, tooltip, tray, winevent,
};

pub use bindings::{HWND, POINT, RECT};
pub use windows_core::{Error, Result};

/// Raw pointer form of an `HWND`, for interop with the windows-* crates.
pub fn hwnd_ptr(hwnd: HWND) -> *mut core::ffi::c_void {
    hwnd.0
}

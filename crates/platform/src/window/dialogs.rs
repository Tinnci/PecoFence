//! Native close-failure choice. Dialog ownership stays on the UI thread.
use crate::bindings::MessageBoxW;
use crate::{HWND, wide};
use windows_core::PCWSTR;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFailureChoice {
    KeepRunning,
    Retry,
    Discard,
}

pub fn save_failure(owner: HWND, caption: &str, text: &str) -> SaveFailureChoice {
    // Win32 MB_CANCELTRYCONTINUE | MB_ICONERROR. The default button is Cancel.
    const FLAGS: u32 = 0x0000_0006 | 0x0000_0010;
    let caption = wide::to_wide(caption);
    let text = wide::to_wide(text);
    // SAFETY: strings are terminated and live through the modal call; owner is a live UI HWND.
    let choice = unsafe {
        MessageBoxW(
            Some(owner),
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            FLAGS,
        )
    };
    match choice {
        10 => SaveFailureChoice::Retry,      // IDTRYAGAIN
        11 => SaveFailureChoice::Discard,    // IDCONTINUE
        _ => SaveFailureChoice::KeepRunning, // IDCANCEL or dialog failure: never discard.
    }
}

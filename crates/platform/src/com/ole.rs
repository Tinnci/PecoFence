//! COM / OLE apartment initialization.

use crate::bindings::*;
use windows_core::Result;

/// Keeps OLE initialized (single-threaded apartment) on the current thread.
///
/// Must be created on the UI thread before native composition and drag/drop
/// registration. An existing WinUI STA is compatible (`S_FALSE` is success).
pub struct OleGuard(());

impl OleGuard {
    pub fn init() -> Result<Self> {
        // SAFETY: plain FFI call; S_FALSE (already initialized) is a success code.
        unsafe { OleInitialize(core::ptr::null()).ok()? };
        Ok(Self(()))
    }
}

impl Drop for OleGuard {
    fn drop(&mut self) {
        // SAFETY: balances the successful OleInitialize in `init`.
        unsafe { OleUninitialize() };
    }
}

/// Dedicated Shell metadata worker apartment. It must pump its own thread messages
/// between tasks; it is never the interactive UI apartment and cannot move between threads.
pub struct StaGuard(std::marker::PhantomData<std::rc::Rc<()>>);

impl StaGuard {
    pub fn init() -> Result<Self> {
        // SAFETY: initializes COM on this calling thread; S_FALSE is also success.
        unsafe {
            CoInitializeEx(
                None,
                COINIT_APARTMENTTHREADED as u32 | COINIT_DISABLE_OLE1DDE as u32,
            )
            .ok()?;
        }
        Ok(Self(std::marker::PhantomData))
    }
}

impl Drop for StaGuard {
    fn drop(&mut self) {
        // SAFETY: same-thread balance of the successful CoInitializeEx in init.
        unsafe { CoUninitialize() };
    }
}

/// Initializes a multithreaded apartment on a worker thread (icon extraction etc.).
pub struct MtaGuard(());

impl MtaGuard {
    pub fn init() -> Result<Self> {
        // SAFETY: plain FFI call.
        unsafe {
            CoInitializeEx(
                None,
                COINIT_MULTITHREADED as u32 | COINIT_DISABLE_OLE1DDE as u32,
            )
            .ok()?
        };
        Ok(Self(()))
    }
}

impl Drop for MtaGuard {
    fn drop(&mut self) {
        // SAFETY: balances CoInitializeEx.
        unsafe { CoUninitialize() };
    }
}

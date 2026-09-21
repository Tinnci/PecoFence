//! `SetWinEventHook` wrapper (out-of-context, callbacks arrive on this thread's message loop).

use crate::bindings::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use windows_core::{Error, Result};

pub type EventCallback = Box<dyn FnMut(u32, HWND)>;

thread_local! {
    static CALLBACKS: RefCell<HashMap<isize, CallbackEntry>> = RefCell::new(HashMap::new());
    static NEXT_GENERATION: Cell<u64> = const { Cell::new(1) };
}

pub const SYSTEM_FOREGROUND: u32 = EVENT_SYSTEM_FOREGROUND as u32;
pub const SYSTEM_MINIMIZESTART: u32 = EVENT_SYSTEM_MINIMIZESTART as u32;
pub const SYSTEM_MINIMIZEEND: u32 = EVENT_SYSTEM_MINIMIZEEND as u32;

/// An installed hook; unhooked on drop.
pub struct WinEventHook {
    state: Rc<HookState>,
}

struct HookState {
    raw: HWINEVENTHOOK,
    generation: u64,
    alive: Cell<bool>,
    callbacks_in_flight: Cell<u32>,
}

struct CallbackEntry {
    state: Rc<HookState>,
    callback: Option<EventCallback>,
}

impl WinEventHook {
    /// Installs an out-of-context hook for `[event_min, event_max]` that skips our own
    /// process. The thread must pump messages for callbacks to be delivered.
    pub fn install(event_min: u32, event_max: u32, callback: EventCallback) -> Result<Self> {
        // SAFETY: the callback is a `extern "system"` fn with the documented signature.
        let hook = unsafe {
            SetWinEventHook(
                event_min,
                event_max,
                None,
                Some(event_proc),
                0,
                0,
                (WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS) as u32,
            )
        };
        if hook.0.is_null() {
            return Err(Error::from_thread());
        }
        let generation = NEXT_GENERATION.with(|next| {
            let generation = next.get();
            next.set(generation.checked_add(1).unwrap_or(1));
            generation
        });
        let state = Rc::new(HookState {
            raw: hook,
            generation,
            alive: Cell::new(true),
            callbacks_in_flight: Cell::new(0),
        });
        CALLBACKS.with(|callbacks| {
            callbacks.borrow_mut().insert(
                hook.0 as isize,
                CallbackEntry {
                    state: state.clone(),
                    callback: Some(callback),
                },
            )
        });
        Ok(Self { state })
    }
}

impl Drop for WinEventHook {
    fn drop(&mut self) {
        self.state.alive.set(false);
        CALLBACKS.with(|callbacks| {
            let key = self.state.raw.0 as isize;
            let remove = callbacks
                .borrow()
                .get(&key)
                .is_some_and(|entry| entry.state.generation == self.state.generation);
            if remove {
                callbacks.borrow_mut().remove(&key);
            }
        });
        // SAFETY: balances SetWinEventHook on the installing message-loop thread.
        unsafe {
            let _ = UnhookWinEvent(self.state.raw);
        }
    }
}

unsafe extern "system" fn event_proc(
    hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // OBJID_WINDOW == 0: only whole-window events.
    if id_object != 0 {
        return;
    }
    // Take the callback out while running it so a re-entrant event cannot alias it.
    let key = hook.0 as isize;
    let taken = CALLBACKS.with(|callbacks| callbacks.borrow_mut().remove(&key));
    if let Some(mut entry) = taken {
        if !entry.state.alive.get() {
            return;
        }
        entry
            .state
            .callbacks_in_flight
            .set(entry.state.callbacks_in_flight.get().saturating_add(1));
        if let Some(callback) = entry.callback.as_mut() {
            callback(event, hwnd);
        }
        entry
            .state
            .callbacks_in_flight
            .set(entry.state.callbacks_in_flight.get().saturating_sub(1));
        if entry.state.alive.get() {
            CALLBACKS.with(|callbacks| {
                let mut callbacks = callbacks.borrow_mut();
                let same_slot = callbacks
                    .get(&key)
                    .is_none_or(|current| current.state.generation == entry.state.generation);
                if same_slot && entry.state.alive.get() {
                    callbacks.entry(key).or_insert(entry);
                }
            });
        }
    }
}

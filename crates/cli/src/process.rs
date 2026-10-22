//! The few Win32 calls `status` and `quit` need, without a Windows crate dependency. On other
//! platforms nothing can be checked and every function says so.

/// Whether `pid` is alive and is the process that started at `started_at` (Unix seconds), not a
/// later process that reused the id. `None` when it cannot be checked.
pub fn alive(pid: u32, started_at: i64) -> Option<bool> {
    imp::alive(pid, started_at)
}

/// Posts the graceful quit request to the control window titled `title`. Returns the pid that
/// owns the window, or `None` when no such window exists.
pub fn request_quit(title: &str) -> Option<u32> {
    imp::request_quit(title)
}

/// Waits up to `timeout_ms` for `pid` to exit. `Some(true)`: it exited; `Some(false)`: still
/// running; `None`: it could not be opened (already gone, or no access).
pub fn wait_exit(pid: u32, timeout_ms: u32) -> Option<bool> {
    imp::wait_exit(pid, timeout_ms)
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    type Handle = *mut c_void;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_TIMEOUT: u32 = 258;

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn GetExitCodeProcess(process: Handle, code: *mut u32) -> i32;
        fn GetProcessTimes(
            process: Handle,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn WaitForSingleObject(handle: Handle, millis: u32) -> u32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(class: *const u16, title: *const u16) -> Handle;
        fn GetWindowThreadProcessId(window: Handle, pid: *mut u32) -> u32;
        fn PostMessageW(window: Handle, message: u32, wparam: usize, lparam: isize) -> i32;
    }

    struct Process(Handle);

    impl Process {
        fn open(pid: u32, access: u32) -> Option<Self> {
            // SAFETY: plain FFI call; a null handle means failure.
            let handle = unsafe { OpenProcess(access, 0, pid) };
            (!handle.is_null()).then_some(Self(handle))
        }
    }

    impl Drop for Process {
        fn drop(&mut self) {
            // SAFETY: the handle came from OpenProcess and is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }

    fn unix_seconds(time: FileTime) -> i64 {
        let ticks = (u64::from(time.high) << 32) | u64::from(time.low);
        (ticks / 10_000_000) as i64 - 11_644_473_600
    }

    pub fn alive(pid: u32, started_at: i64) -> Option<bool> {
        let Some(process) = Process::open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
            // ERROR_INVALID_PARAMETER for a pid that does not exist; access is granted to a
            // same-user process, so treat any failure as gone.
            return Some(false);
        };
        let mut code = 0;
        // SAFETY: valid handle and out pointer.
        if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 {
            return None;
        }
        if code != STILL_ACTIVE {
            return Some(false);
        }
        let (mut creation, mut exit, mut kernel, mut user) = Default::default();
        // SAFETY: valid handle and out pointers.
        if unsafe { GetProcessTimes(process.0, &mut creation, &mut exit, &mut kernel, &mut user) }
            == 0
        {
            return None;
        }
        // The app records its start a moment after the process is created. A process created
        // well before that, or after it, reused the id.
        let created = unix_seconds(creation);
        Some((started_at - 600..=started_at + 5).contains(&created))
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    pub fn request_quit(title: &str) -> Option<u32> {
        let title = wide(title);
        // SAFETY: null class matches any class; the title is NUL-terminated.
        let window = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
        if window.is_null() {
            return None;
        }
        let mut pid = 0;
        // SAFETY: valid window handle and out pointer.
        unsafe { GetWindowThreadProcessId(window, &mut pid) };
        // SAFETY: posting a message id the control window defines; no pointers are passed.
        let posted =
            unsafe { PostMessageW(window, pecofence_shell_contract::WM_APP_QUIT_REQUEST, 0, 0) };
        (posted != 0).then_some(pid)
    }

    pub fn wait_exit(pid: u32, timeout_ms: u32) -> Option<bool> {
        let process = Process::open(pid, SYNCHRONIZE)?;
        // SAFETY: valid handle.
        match unsafe { WaitForSingleObject(process.0, timeout_ms) } {
            WAIT_OBJECT_0 => Some(true),
            WAIT_TIMEOUT => Some(false),
            _ => None,
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn alive(_pid: u32, _started_at: i64) -> Option<bool> {
        None
    }
    pub fn request_quit(_title: &str) -> Option<u32> {
        None
    }
    pub fn wait_exit(_pid: u32, _timeout_ms: u32) -> Option<bool> {
        None
    }
}

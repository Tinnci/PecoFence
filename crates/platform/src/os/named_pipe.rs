//! Local named-pipe identity and endpoint construction.

use crate::bindings::{CloseHandle, GetCurrentProcess, GetCurrentProcessId, HANDLE};
use core::ffi::c_void;
use std::{io, ptr};

const TOKEN_QUERY: u32 = 0x0008;
const TOKEN_USER_INFO_CLASS: u32 = 1;

#[repr(C)]
struct SidAndAttributes {
    sid: *mut c_void,
    attributes: u32,
}

#[repr(C)]
struct TokenUser {
    user: SidAndAttributes,
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(
        process_handle: HANDLE,
        desired_access: u32,
        token_handle: *mut HANDLE,
    ) -> i32;
    fn GetTokenInformation(
        token_handle: HANDLE,
        token_information_class: u32,
        token_information: *mut c_void,
        token_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
    fn ConvertSidToStringSidW(sid: *mut c_void, string_sid: *mut *mut u16) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LocalFree(hmem: *mut c_void) -> *mut c_void;
    fn ProcessIdToSessionId(process_id: u32, session_id: *mut u32) -> i32;
}

/// Returns the current process token's user SID. Mutable environment variables are not used.
pub fn current_user_sid() -> io::Result<String> {
    // SAFETY: the token and LocalAlloc string are closed/freed on all successful acquisition paths;
    // the aligned usize buffer remains alive while TokenUser and SID are read.
    unsafe {
        let mut token = HANDLE(ptr::null_mut());
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut bytes = 0;
        GetTokenInformation(token, TOKEN_USER_INFO_CLASS, ptr::null_mut(), 0, &mut bytes);
        let mut buffer = vec![0usize; (bytes as usize).div_ceil(std::mem::size_of::<usize>())];
        let ok = GetTokenInformation(
            token,
            TOKEN_USER_INFO_CLASS,
            buffer.as_mut_ptr().cast(),
            bytes,
            &mut bytes,
        );
        let error = io::Error::last_os_error();
        let _ = CloseHandle(token);
        if ok == 0 {
            return Err(error);
        }
        let user = &*(buffer.as_ptr().cast::<TokenUser>());
        let mut text = ptr::null_mut();
        if ConvertSidToStringSidW(user.user.sid, &mut text) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0;
        while *text.add(len) != 0 {
            len += 1;
        }
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        Ok(sid)
    }
}

pub fn current_windows_session_id() -> io::Result<u32> {
    let mut session_id = 0;
    // SAFETY: the output pointer is valid for the duration of the call.
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session_id) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(session_id)
}

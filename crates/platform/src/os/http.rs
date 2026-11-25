//! Small blocking HTTPS reader for public data. Call only on a worker thread.
use crate::bindings::*;
use crate::wide::to_wide;
use std::io;
use std::time::{Duration, Instant};
use windows_core::PCWSTR;

struct Handle(HINTERNET);
impl Handle {
    fn checked(raw: HINTERNET) -> io::Result<Self> {
        if raw.0.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(raw))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the WinHTTP handle, and request handles drop first.
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

pub fn get_https(host: &str, path: &str, max_bytes: usize) -> io::Result<Vec<u8>> {
    if host.contains(['\0', '/', ':']) || !path.starts_with('/') || path.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid HTTPS target",
        ));
    }
    let agent = to_wide("PecoFence-Calendar/1.0");
    let host = to_wide(host);
    let path = to_wide(path);
    let get = to_wide("GET");
    let began = Instant::now();
    // SAFETY: all strings and buffers outlive synchronous calls. HTTPS keeps
    // Windows certificate verification; no credentials or cookies are supplied.
    unsafe {
        let session = Handle::checked(WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY as u32,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        if !WinHttpSetTimeouts(session.0, 5_000, 5_000, 5_000, 5_000).as_bool() {
            return Err(io::Error::last_os_error());
        }
        let connect = Handle::checked(WinHttpConnect(
            session.0,
            PCWSTR(host.as_ptr()),
            INTERNET_PORT(443),
            0,
        ))?;
        let request = Handle::checked(WinHttpOpenRequest(
            connect.0,
            PCWSTR(get.as_ptr()),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            None,
            WINHTTP_FLAG_SECURE as u32,
        ))?;
        if !WinHttpSendRequest(request.0, None, None, 0, 0, 0).as_bool()
            || !WinHttpReceiveResponse(request.0, std::ptr::null()).as_bool()
        {
            return Err(io::Error::last_os_error());
        }
        let mut status = 0u32;
        let mut size = size_of::<u32>() as u32;
        if !WinHttpQueryHeaders(
            request.0,
            (WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER) as u32,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut size,
            None,
        )
        .as_bool()
        {
            return Err(io::Error::last_os_error());
        }
        if status != 200 {
            return Err(io::Error::other(format!("HTTP {status}")));
        }
        let mut body = Vec::new();
        loop {
            if began.elapsed() > Duration::from_secs(20) {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "HTTPS read deadline",
                ));
            }
            let mut buffer = [0u8; 8192];
            let mut read = 0u32;
            if !WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )
            .as_bool()
            {
                return Err(io::Error::last_os_error());
            }
            if read == 0 {
                return Ok(body);
            }
            append_bounded(&mut body, &buffer[..read as usize], max_bytes)?;
        }
    }
}

fn append_bounded(body: &mut Vec<u8>, chunk: &[u8], max_bytes: usize) -> io::Result<()> {
    if body.len().saturating_add(chunk.len()) > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "HTTPS response is too large",
        ));
    }
    body.extend_from_slice(chunk);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_chunks_never_append_beyond_the_limit() {
        let mut body = Vec::new();
        append_bounded(&mut body, b"1234", 8).unwrap();
        append_bounded(&mut body, b"5678", 8).unwrap();
        assert_eq!(
            append_bounded(&mut body, b"9", 8).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(body, b"12345678");
    }
    #[test]
    #[ignore = "requires public internet from Windows"]
    fn public_holiday_feed_is_readable_and_response_limits_are_enforced() {
        let path = "/NateScarlet/holiday-cn/master/2026.json";
        let bytes = get_https("raw.githubusercontent.com", path, 256 * 1024).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("\"isOffDay\"") && text.contains("\"year\": 2026"));
        let error = append_bounded(&mut Vec::new(), text.as_bytes(), 16).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}

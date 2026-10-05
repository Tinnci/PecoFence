//! Bounded Windows folder-read adapter. No HWND, UI transport or COM interface crosses
//! this boundary. Shutdown disconnects delivery; it never joins a blocked OS read.
//! Expected IO/COM errors are results. Panic recovery works only with unwinding builds;
//! the workspace's release panic=abort terminates the process on a Rust panic.

use crate::{bindings::*, com::StaGuard, shell};
use pecofence_core::portal::*;
use std::os::windows::fs::MetadataExt;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};

pub struct PortalReader {
    requests: Option<SyncSender<PortalRead>>,
    results: Option<Receiver<PortalResult>>,
    start_failure: Option<ReadFailure>,
    in_flight: Option<PortalRead>,
}

impl Default for PortalReader {
    fn default() -> Self {
        Self::start(
            || StaGuard::init().map_err(|e| ReadFailure::Apartment(e.to_string())),
            read_folder,
        )
    }
}

impl PortalReader {
    fn start<G: 'static>(
        init: impl FnOnce() -> Result<G, ReadFailure> + Send + 'static,
        mut read: impl FnMut(&Path) -> PortalOutcome + Send + 'static,
    ) -> Self {
        let (requests, input) = mpsc::sync_channel::<PortalRead>(1);
        let (output, results) = mpsc::sync_channel(1);
        let spawn = std::thread::Builder::new()
            .name("portal-shell-sta".into())
            .spawn(move || {
                let apartment = std::panic::catch_unwind(std::panic::AssertUnwindSafe(init))
                    .unwrap_or_else(|_| {
                        Err(ReadFailure::Worker("reader initialization panicked".into()))
                    });
                loop {
                    pump_sta();
                    let request = match input.recv_timeout(std::time::Duration::from_millis(25)) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let outcome = match &apartment {
                        Ok(_) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            read(&request.path)
                        }))
                        .unwrap_or_else(|_| {
                            PortalOutcome::Unavailable(ReadFailure::Worker(
                                "folder reader panicked".into(),
                            ))
                        }),
                        Err(failure) => PortalOutcome::Unavailable(failure.clone()),
                    };
                    pump_sta();
                    if output.send(PortalResult { request, outcome }).is_err() {
                        break;
                    }
                }
            });
        Self::from_spawn(requests, results, spawn)
    }

    fn from_spawn(
        requests: SyncSender<PortalRead>,
        results: Receiver<PortalResult>,
        spawn: std::io::Result<std::thread::JoinHandle<()>>,
    ) -> Self {
        match spawn {
            // Detach intentionally. An OS read (network share, Shell extension) may never
            // return. Cancellation invalidates commit, not the blocking call itself.
            Ok(handle) => {
                drop(handle);
                Self {
                    requests: Some(requests),
                    results: Some(results),
                    start_failure: None,
                    in_flight: None,
                }
            }
            Err(error) => Self {
                requests: None,
                results: None,
                start_failure: Some(ReadFailure::Worker(error.to_string())),
                in_flight: None,
            },
        }
    }

    /// An enqueue/start failure is a real correlated completion, not an orphaned job.
    pub fn submit(&mut self, request: PortalRead) -> Option<PortalResult> {
        if self.in_flight.is_some() {
            return Some(PortalResult {
                request,
                outcome: PortalOutcome::Unavailable(ReadFailure::Worker(
                    "a portal read is already outstanding".into(),
                )),
            });
        }
        let failure = if let Some(sender) = &self.requests {
            match sender.try_send(request.clone()) {
                Ok(()) => {
                    self.in_flight = Some(request);
                    return None;
                }
                Err(error) => {
                    let text = error.to_string();
                    let (mpsc::TrySendError::Full(request)
                    | mpsc::TrySendError::Disconnected(request)) = error;
                    return Some(PortalResult {
                        request,
                        outcome: PortalOutcome::Unavailable(ReadFailure::Worker(text)),
                    });
                }
            }
        } else {
            self.start_failure
                .clone()
                .unwrap_or_else(|| ReadFailure::Worker("reader closed".into()))
        };
        Some(PortalResult {
            request,
            outcome: PortalOutcome::Unavailable(failure),
        })
    }

    pub fn try_result(&mut self) -> Option<PortalResult> {
        match self.results.as_ref()?.try_recv() {
            Ok(result) => {
                self.in_flight = None;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                let request = self.in_flight.take()?;
                Some(PortalResult {
                    request,
                    outcome: PortalOutcome::Unavailable(ReadFailure::Worker(
                        "portal reader disconnected before completion".into(),
                    )),
                })
            }
        }
    }

    /// Must be called before native control-window destruction. Delivery uses polling,
    /// never PostMessage, so an in-flight completion cannot target a destroyed HWND.
    pub fn close(&mut self) {
        self.results.take();
        self.requests.take();
        self.in_flight = None;
    }
}

impl Drop for PortalReader {
    fn drop(&mut self) {
        self.close();
    }
}

fn unix_time(ft: u64) -> i64 {
    (ft as i64 / 10_000_000) - 11_644_473_600
}

/// Pump only this worker's messages, never native App messages. A finite batch also
/// prevents a noisy Shell extension from monopolizing the task boundary.
fn pump_sta() {
    // SAFETY: local MSG storage and dispatch on the same thread that owns this queue.
    unsafe {
        let mut message = MSG::default();
        for _ in 0..64 {
            if !PeekMessageW(&mut message, None, 0, 0, PM_REMOVE as u32).as_bool() {
                break;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn read_folder(path: &Path) -> PortalOutcome {
    // Bound both traversal work and result memory. A truncated directory is never complete.
    const MAX_ENTRIES: usize = 20_000;
    let directory = match std::fs::read_dir(path) {
        Ok(directory) => directory,
        Err(error) => return PortalOutcome::Unavailable(ReadFailure::Open(error.to_string())),
    };
    let entries = directory.map(|entry| {
        let entry = entry.map_err(|error| ReadFailure::Traversal(error.to_string()))?;
        let metadata = entry
            .metadata()
            .map_err(|error| ReadFailure::Metadata(error.to_string()))?;
        let attributes = metadata.file_attributes();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if attributes & (FILE_ATTRIBUTE_HIDDEN as u32 | FILE_ATTRIBUTE_SYSTEM as u32) != 0
            || file_name.eq_ignore_ascii_case("desktop.ini")
        {
            return Ok(None);
        }
        let path = entry.path();
        let display_name = shell::display_name(&path)
            .ok_or_else(|| ReadFailure::DisplayName(path.display().to_string()))?;
        // Pumping cannot interrupt a blocked filesystem call or Shell extension.
        // Generation invalidation cancels commit, not the blocking call itself.
        pump_sta();
        Ok(Some(PortalEntry {
            path,
            display_name,
            is_folder: metadata.is_dir(),
            attributes,
            mtime: unix_time(metadata.last_write_time()),
            size: metadata.file_size(),
        }))
    });
    collect_read(entries, MAX_ENTRIES)
}

/// Completeness and the traversal budget apply to all visited entries, including filtered
/// ones. Retain the first issue; partial entries never become an authoritative snapshot.
fn collect_read(
    entries: impl IntoIterator<Item = Result<Option<PortalEntry>, ReadFailure>>,
    max_entries: usize,
) -> PortalOutcome {
    let mut complete = Vec::new();
    let mut failure = None;
    for (index, entry) in entries.into_iter().enumerate() {
        if index == max_entries {
            failure.get_or_insert(ReadFailure::Limit { max_entries });
            break;
        }
        match entry {
            Ok(Some(entry)) => complete.push(entry),
            Ok(None) => {}
            Err(error) => {
                failure.get_or_insert(error);
            }
        }
    }
    match failure {
        Some(failure) => PortalOutcome::Partial {
            entries: complete,
            failure,
        },
        None => PortalOutcome::Complete(complete),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn request() -> PortalRead {
        PortalRead {
            fence: pecofence_core::FenceId::new_v4(),
            generation: 1,
            request_id: 1,
            path: "fake".into(),
        }
    }
    fn receive(reader: &mut PortalReader) -> PortalResult {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(result) = reader.try_result() {
                return result;
            }
            assert!(std::time::Instant::now() < deadline, "reader timed out");
            std::thread::yield_now();
        }
    }
    #[test]
    fn apartment_failure_completes_every_job() {
        let mut reader = PortalReader::start(
            || Err::<(), _>(ReadFailure::Apartment("injected".into())),
            |_| panic!("must not run without COM"),
        );
        for _ in 0..2 {
            let request = request();
            assert!(reader.submit(request.clone()).is_none());
            assert_eq!(
                receive(&mut reader),
                PortalResult {
                    request,
                    outcome: PortalOutcome::Unavailable(ReadFailure::Apartment("injected".into()))
                }
            );
        }
    }
    #[test]
    fn spawn_failure_is_correlated() {
        let (tx, _) = mpsc::sync_channel(1);
        let (_, rx) = mpsc::sync_channel(1);
        let mut reader = PortalReader::from_spawn(tx, rx, Err(std::io::Error::other("injected")));
        let request = request();
        let result = reader.submit(request.clone()).unwrap();
        assert_eq!(result.request, request);
        assert!(matches!(
            result.outcome,
            PortalOutcome::Unavailable(ReadFailure::Worker(_))
        ));
    }
    #[test]
    fn late_read_after_close_cannot_deliver_and_does_not_join() {
        let (release, blocked) = mpsc::channel();
        let (started, ready) = mpsc::channel();
        let mut reader = PortalReader::start(
            || Ok(()),
            move |_| {
                started.send(()).unwrap();
                blocked.recv().unwrap();
                PortalOutcome::Complete(vec![])
            },
        );
        reader.submit(request());
        ready.recv_timeout(Duration::from_secs(10)).unwrap();
        reader.close(); // would deadlock if shutdown joined
        assert!(reader.try_result().is_none());
        release.send(()).unwrap();
    }

    #[test]
    fn rejected_second_job_does_not_replace_running_correlation() {
        let (release, blocked) = mpsc::channel();
        let (started, ready) = mpsc::channel();
        let mut reader = PortalReader::start(
            || Ok(()),
            move |_| {
                started.send(()).unwrap();
                blocked.recv().unwrap();
                PortalOutcome::Complete(vec![])
            },
        );
        let first = request();
        assert!(reader.submit(first.clone()).is_none());
        ready.recv_timeout(Duration::from_secs(10)).unwrap();
        let second = request();
        assert_eq!(reader.submit(second.clone()).unwrap().request, second);
        release.send(()).unwrap();
        assert_eq!(receive(&mut reader).request, first);
        assert!(reader.in_flight.is_none());
    }

    #[test]
    fn disconnect_terminalizes_accepted_read_once() {
        let (requests, _input) = mpsc::sync_channel(1);
        let (output, results) = mpsc::sync_channel(1);
        let mut reader = PortalReader {
            requests: Some(requests),
            results: Some(results),
            start_failure: None,
            in_flight: None,
        };
        let request = request();
        assert!(reader.submit(request.clone()).is_none());
        drop(output);
        let result = reader.try_result().unwrap();
        assert_eq!(result.request, request);
        assert!(matches!(
            result.outcome,
            PortalOutcome::Unavailable(ReadFailure::Worker(_))
        ));
        assert!(reader.try_result().is_none());
    }

    #[test]
    fn collection_distinguishes_filtered_empty_partial_and_budget() {
        assert_eq!(
            collect_read([Ok(None), Ok(None)], 2),
            PortalOutcome::Complete(vec![])
        );
        for failure in [
            ReadFailure::Traversal("interrupted".into()),
            ReadFailure::Metadata("denied".into()),
            ReadFailure::DisplayName("unavailable".into()),
        ] {
            let result = collect_read([Ok(None), Err(failure.clone()), Ok(None)], 3);
            assert_eq!(
                result,
                PortalOutcome::Partial {
                    entries: vec![],
                    failure,
                }
            );
        }
        assert_eq!(
            collect_read([Ok(None), Ok(None), Ok(None)], 2),
            PortalOutcome::Partial {
                entries: vec![],
                failure: ReadFailure::Limit { max_entries: 2 },
            }
        );
    }

    #[test]
    fn windows_temp_directory_smoke() {
        let dir = std::env::temp_dir().join(format!(
            "pecofence-portal-{}",
            pecofence_core::FenceId::new_v4()
        ));
        std::fs::create_dir(&dir).unwrap();
        let mut reader = PortalReader::default();
        let mut request = request();
        request.path = dir.clone();
        reader.submit(request.clone());
        assert_eq!(
            receive(&mut reader).outcome,
            PortalOutcome::Complete(vec![])
        );
        std::fs::write(dir.join("visible.txt"), b"hello").unwrap();
        reader.submit(request.clone());
        let PortalOutcome::Complete(entries) = receive(&mut reader).outcome else {
            panic!("expected complete Windows/Shell read");
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size, 5);
        assert!(!entries[0].display_name.is_empty());
        request.path = dir.join("missing");
        reader.submit(request);
        assert!(matches!(
            receive(&mut reader).outcome,
            PortalOutcome::Unavailable(ReadFailure::Open(_))
        ));
        // Only this test's newly-created fixture is removed.
        std::fs::remove_file(dir.join("visible.txt")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}

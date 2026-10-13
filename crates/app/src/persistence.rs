//! One bounded writer for the application, independent of windows and page lifetimes.
//! The coordinator owns accepted writes until their terminal result, including after import.

use pecofence_core::settings_protocol::DocumentStamp;
use pecofence_core::{Config, ConfigStore, SaveReceipt};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};

pub struct SavePlan {
    pub stamp: DocumentStamp,
    pub replace: bool,
    pub config: Config,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub id: u64,
    pub stamp: DocumentStamp,
    pub replace: bool,
}

pub struct WriteRequest {
    pub ticket: Ticket,
    pub config: Config,
}

pub struct WriteResult {
    pub ticket: Ticket,
    pub outcome: Result<SaveReceipt, String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Closing {
    #[default]
    Open,
    Waiting,
    Failed,
    Discarded,
    Finished,
}

#[derive(Default)]
pub struct SaveCoordinator {
    next: u64,
    active: Option<Ticket>,
    requested: bool,
    closing: Closing,
}

impl SaveCoordinator {
    pub fn request(&mut self) {
        if !matches!(self.closing, Closing::Finished | Closing::Discarded) {
            self.requested = true;
        }
    }
    pub fn ready(&self) -> bool {
        self.active.is_none() && self.requested
    }
    pub fn busy(&self) -> bool {
        self.active.is_some()
    }
    pub fn closing(&self) -> Closing {
        self.closing
    }
    pub fn begin_close(&mut self) {
        if matches!(self.closing, Closing::Finished | Closing::Discarded) {
            return;
        }
        self.closing = Closing::Waiting;
        self.request();
    }
    pub fn keep_running(&mut self) {
        if matches!(self.closing, Closing::Waiting | Closing::Failed) {
            self.closing = Closing::Open;
        }
    }
    pub fn finish_close(&mut self) -> bool {
        if self.busy() || !matches!(self.closing, Closing::Waiting | Closing::Discarded) {
            return false;
        }
        if self.closing != Closing::Discarded {
            self.closing = Closing::Finished;
        }
        self.requested = false;
        true
    }
    pub fn discard_failed(&mut self) -> bool {
        if self.closing != Closing::Failed || self.busy() {
            return false;
        }
        self.closing = Closing::Discarded;
        self.requested = false;
        true
    }
    pub fn no_work(&mut self) {
        if self.active.is_none() {
            self.requested = false;
        }
    }
    pub fn start(&mut self, plan: SavePlan) -> Option<WriteRequest> {
        if !self.ready() {
            return None;
        }
        self.next = self.next.checked_add(1).expect("commit ticket exhausted");
        let ticket = Ticket {
            id: self.next,
            stamp: plan.stamp,
            replace: plan.replace,
        };
        self.active = Some(ticket);
        self.requested = false;
        Some(WriteRequest {
            ticket,
            config: plan.config,
        })
    }
    /// Duplicate/wrong results cannot release another write's slot.
    pub fn accept(&mut self, result: &WriteResult) -> bool {
        if self.active != Some(result.ticket) {
            return false;
        }
        self.active = None;
        if result.outcome.is_ok() {
            self.requested = true; // Coalesce intervening changes into one latest snapshot.
        } else if self.closing == Closing::Waiting && !self.requested {
            self.closing = Closing::Failed;
        }
        true
    }
    pub fn can_close(&self, writable_dirty: bool) -> bool {
        self.closing == Closing::Waiting && !self.busy() && !writable_dirty
    }
}

pub struct Writer {
    requests: Option<SyncSender<WriteRequest>>,
    results: Receiver<WriteResult>,
    active: Option<Ticket>,
    unavailable: Option<String>,
}

impl Writer {
    pub fn new(directory: PathBuf) -> Self {
        Self::spawn(move |request| {
            let store = ConfigStore::new(&directory);
            let outcome = if request.ticket.replace {
                store.replace(&request.config)
            } else {
                store.save(&request.config)
            };
            outcome.map_err(|error| error.to_string())
        })
    }
    fn spawn<F>(mut commit: F) -> Self
    where
        F: FnMut(&WriteRequest) -> Result<SaveReceipt, String> + Send + 'static,
    {
        let (send, receive) = mpsc::sync_channel::<WriteRequest>(1);
        let (results, result_receive) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("pecofence-writer".into())
            .spawn(move || {
                while let Ok(request) = receive.recv() {
                    let outcome = commit(&request);
                    if results
                        .send(WriteResult {
                            ticket: request.ticket,
                            outcome,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
        let unavailable = spawned.err().map(|error| error.to_string());
        Self {
            requests: unavailable.is_none().then_some(send),
            results: result_receive,
            active: None,
            unavailable,
        }
    }
    /// Rejection is synchronous; the caller terminalizes the ticket with this error.
    pub fn submit(&mut self, request: WriteRequest) -> Result<(), String> {
        if self.active.is_some() {
            return Err("writer already owns an unfinished commit".into());
        }
        let Some(sender) = &self.requests else {
            return Err(self
                .unavailable
                .clone()
                .unwrap_or_else(|| "writer is closed".into()));
        };
        let ticket = request.ticket;
        match sender.try_send(request) {
            Ok(()) => {
                self.active = Some(ticket);
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err("writer request channel is full".into()),
            Err(TrySendError::Disconnected(_)) => Err("writer request channel disconnected".into()),
        }
    }
    pub fn poll(&mut self) -> Option<WriteResult> {
        match self.results.try_recv() {
            Ok(result) => {
                self.active = None;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => self.active.take().map(|ticket| WriteResult {
                ticket,
                outcome: Err(
                    "writer stopped without a commit receipt; disk state may be uncertain".into(),
                ),
            }),
        }
    }
    pub fn close(&mut self) {
        self.requests = None;
        // Do not join a possibly blocked filesystem call. Normal close drains first.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecofence_core::BackupStatus;
    use pecofence_core::settings_protocol::DocumentClock;
    use std::time::{Duration, Instant};

    fn plan(stamp: DocumentStamp, replace: bool) -> SavePlan {
        SavePlan {
            stamp,
            replace,
            config: Config::default(),
        }
    }
    fn success(ticket: Ticket) -> WriteResult {
        WriteResult {
            ticket,
            outcome: Ok(SaveReceipt {
                backup: BackupStatus::AlreadyExists,
            }),
        }
    }
    fn receive(writer: &mut Writer) -> WriteResult {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(result) = writer.poll() {
                return result;
            }
            assert!(Instant::now() < deadline, "writer result timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn saves_are_single_flight_and_only_the_latest_document_is_snapshotted_next() {
        let mut scheduler = SaveCoordinator::default();
        let mut clock = DocumentClock::new(true);
        clock.change();
        scheduler.request();
        let first = scheduler.start(plan(clock.stamp(), false)).unwrap();
        for _ in 0..100 {
            clock.change();
            scheduler.request();
        }
        assert!(!scheduler.ready());
        assert!(scheduler.start(plan(clock.stamp(), false)).is_none());
        assert!(scheduler.accept(&success(first.ticket)));
        let next = scheduler.start(plan(clock.stamp(), false)).unwrap();
        assert_eq!(next.ticket.stamp.revision, 101);
        assert_ne!(first.ticket.id, next.ticket.id);
        assert!(!scheduler.accept(&success(first.ticket)));
        assert!(scheduler.busy());
    }

    #[test]
    fn failure_is_not_an_automatic_retry_loop_and_retry_has_a_new_ticket() {
        let mut scheduler = SaveCoordinator::default();
        let stamp = DocumentClock::new(true).stamp();
        scheduler.request();
        let first = scheduler.start(plan(stamp, true)).unwrap();
        assert!(scheduler.accept(&WriteResult {
            ticket: first.ticket,
            outcome: Err("disk full".into())
        }));
        assert!(!scheduler.ready());
        scheduler.request();
        let retry = scheduler.start(plan(stamp, true)).unwrap();
        assert_ne!(first.ticket, retry.ticket);
        assert!(!scheduler.accept(&success(first.ticket)));
    }

    #[test]
    fn close_waits_for_primary_receipt_and_offers_retry_or_keep_running_on_failure() {
        let mut scheduler = SaveCoordinator::default();
        let mut clock = DocumentClock::new(true);
        clock.change();
        scheduler.begin_close();
        let write = scheduler.start(plan(clock.stamp(), false)).unwrap();
        assert!(!scheduler.can_close(true));
        scheduler.accept(&WriteResult {
            ticket: write.ticket,
            outcome: Err("denied".into()),
        });
        assert_eq!(scheduler.closing(), Closing::Failed);
        scheduler.keep_running();
        assert_eq!(scheduler.closing(), Closing::Open);
        scheduler.begin_close();
        let retry = scheduler.start(plan(clock.stamp(), false)).unwrap();
        scheduler.accept(&success(retry.ticket));
        assert!(!scheduler.can_close(true));
        clock.commit(retry.ticket.stamp);
        assert!(scheduler.can_close(clock.dirty()));
    }

    #[test]
    fn blocked_io_neither_blocks_submit_nor_creates_a_second_writer() {
        let (entered, entry) = mpsc::sync_channel(1);
        let (release, wait) = mpsc::sync_channel(1);
        let ui_thread = std::thread::current().id();
        let mut writer = Writer::spawn(move |_| {
            entered.send(std::thread::current().id()).unwrap();
            wait.recv().unwrap();
            Ok(SaveReceipt {
                backup: BackupStatus::AlreadyExists,
            })
        });
        let mut scheduler = SaveCoordinator::default();
        scheduler.request();
        let request = scheduler
            .start(plan(DocumentClock::new(true).stamp(), false))
            .unwrap();
        let ticket = request.ticket;
        writer.submit(request).unwrap();
        assert_ne!(
            entry.recv_timeout(Duration::from_secs(5)).unwrap(),
            ui_thread
        );
        assert!(writer.poll().is_none());
        assert!(
            writer
                .submit(WriteRequest {
                    ticket,
                    config: Config::default()
                })
                .is_err()
        );
        release.send(()).unwrap();
        assert_eq!(receive(&mut writer).ticket, ticket);
        writer.close();
    }

    #[test]
    fn disconnect_without_receipt_terminalizes_once_and_drop_never_joins() {
        let (release, wait) = mpsc::sync_channel(1);
        let mut writer = Writer::spawn(move |_| {
            wait.recv().unwrap();
            panic!("test worker crash");
        });
        let ticket = Ticket {
            id: 1,
            stamp: DocumentClock::new(true).stamp(),
            replace: false,
        };
        writer
            .submit(WriteRequest {
                ticket,
                config: Config::default(),
            })
            .unwrap();
        writer.close(); // Does not wait for the blocked commit.
        release.send(()).unwrap();
        assert!(receive(&mut writer).outcome.is_err());
        assert!(writer.poll().is_none());
    }

    #[test]
    fn discard_is_explicit_and_never_abandons_an_active_write() {
        let mut scheduler = SaveCoordinator::default();
        scheduler.begin_close();
        let write = scheduler
            .start(plan(DocumentClock::new(true).stamp(), false))
            .unwrap();
        assert!(!scheduler.discard_failed());
        scheduler.accept(&WriteResult {
            ticket: write.ticket,
            outcome: Err("denied".into()),
        });
        assert!(scheduler.discard_failed());
        assert_eq!(scheduler.closing(), Closing::Discarded);
        assert!(!scheduler.ready());
    }

    #[test]
    fn terminal_close_cannot_be_reopened_by_a_late_cancel_or_save_intent() {
        let mut scheduler = SaveCoordinator::default();
        scheduler.begin_close();
        assert!(scheduler.finish_close());
        scheduler.keep_running();
        scheduler.begin_close();
        scheduler.request();
        assert_eq!(scheduler.closing(), Closing::Finished);
        assert!(!scheduler.ready());
    }

    #[test]
    fn real_writer_serializes_replace_and_preserves_the_previous_primary_bytes() {
        let dir =
            std::env::temp_dir().join(format!("pecofence-async-writer-{}", uuid::Uuid::new_v4()));
        let mut writer = Writer::new(dir.clone());
        let mut first = Config::default();
        first.settings.autostart = false;
        first.settings.icon_size = 48;
        let ticket = Ticket {
            id: 1,
            stamp: DocumentClock::new(true).stamp(),
            replace: false,
        };
        writer
            .submit(WriteRequest {
                ticket,
                config: first,
            })
            .unwrap();
        assert!(receive(&mut writer).outcome.is_ok());
        let previous = std::fs::read(dir.join("workspace.v2.json")).unwrap();
        let mut second = Config::default();
        second.settings.autostart = false;
        second.settings.icon_size = 64;
        writer
            .submit(WriteRequest {
                ticket: Ticket {
                    id: 2,
                    stamp: DocumentClock::new(false).stamp(),
                    replace: true,
                },
                config: second,
            })
            .unwrap();
        assert!(receive(&mut writer).outcome.is_ok());
        let pecofence_core::LoadOutcome::Primary(loaded) = ConfigStore::new(&dir).load() else {
            panic!("primary missing")
        };
        assert_eq!(loaded.settings.icon_size, 64);
        let archives: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|p| p.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("workspace.v2.replaced-")
            })
            .collect();
        assert_eq!(archives.len(), 1);
        assert_eq!(std::fs::read(&archives[0]).unwrap(), previous);
        writer.close();
        std::fs::remove_dir_all(dir).unwrap();
    }
}

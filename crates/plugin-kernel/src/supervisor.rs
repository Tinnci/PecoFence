use pecofence_plugin_api::{Error, Result, ScopeId, Token};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    notify: Arc<tokio::sync::Notify>,
}

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::AcqRel) {
            self.notify.notify_waiters();
        }
    }
    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        let notified = self.notify.notified();
        if self.is_cancelled() {
            return;
        }
        notified.await;
    }
}

enum JoinKind {
    Thread(JoinHandle<()>),
    Tokio(tokio::task::JoinHandle<()>),
}

struct TaskRecord {
    owner: ScopeId,
    generation: u64,
    cancellation: CancellationToken,
    join: Option<JoinKind>,
}

struct NativeRecord {
    owner: ScopeId,
    generation: u64,
    cancel: Box<dyn Fn() + Send>,
    completed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrainReport {
    pub joined: usize,
    pub pending: usize,
    pub native_pending: usize,
    pub quarantined: bool,
}

pub struct TaskSupervisor {
    next_token: u64,
    tasks: HashMap<Token, TaskRecord>,
    completions_tx: mpsc::Sender<(Token, u64)>,
    completions_rx: mpsc::Receiver<(Token, u64)>,
    completed: HashSet<(Token, u64)>,
    native: HashMap<Token, NativeRecord>,
}

impl Default for TaskSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskSupervisor {
    pub fn new() -> Self {
        let (completions_tx, completions_rx) = mpsc::channel();
        Self {
            next_token: 1,
            tasks: HashMap::new(),
            completions_tx,
            completions_rx,
            completed: HashSet::new(),
            native: HashMap::new(),
        }
    }

    pub fn spawn(
        &mut self,
        owner: ScopeId,
        generation: u64,
        task: impl FnOnce(CancellationToken) + Send + 'static,
    ) -> Result<Token> {
        let token = Token(self.next_token);
        self.next_token = self.next_token.checked_add(1).ok_or(Error::Exhausted)?;
        let cancellation = CancellationToken {
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(tokio::sync::Notify::new()),
        };
        let child_token = cancellation.clone();
        let tx = self.completions_tx.clone();
        let join = thread::spawn(move || {
            task(child_token);
            let _ = tx.send((token, generation));
        });
        self.tasks.insert(
            token,
            TaskRecord {
                owner,
                generation,
                cancellation,
                join: Some(JoinKind::Thread(join)),
            },
        );
        Ok(token)
    }

    pub fn spawn_tokio<F, Fut>(
        &mut self,
        runtime: &tokio::runtime::Handle,
        owner: ScopeId,
        generation: u64,
        task: F,
    ) -> Result<Token>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let token = Token(self.next_token);
        self.next_token = self.next_token.checked_add(1).ok_or(Error::Exhausted)?;
        let cancellation = CancellationToken {
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(tokio::sync::Notify::new()),
        };
        let child_token = cancellation.clone();
        let tx = self.completions_tx.clone();
        let join = runtime.spawn(async move {
            task(child_token).await;
            let _ = tx.send((token, generation));
        });
        self.tasks.insert(
            token,
            TaskRecord {
                owner,
                generation,
                cancellation,
                join: Some(JoinKind::Tokio(join)),
            },
        );
        Ok(token)
    }

    pub fn register_native(
        &mut self,
        owner: ScopeId,
        generation: u64,
        cancel: impl Fn() + Send + 'static,
    ) -> Result<Token> {
        let token = Token(self.next_token);
        self.next_token = self.next_token.checked_add(1).ok_or(Error::Exhausted)?;
        self.native.insert(
            token,
            NativeRecord {
                owner,
                generation,
                cancel: Box::new(cancel),
                completed: false,
            },
        );
        Ok(token)
    }

    /// Records an overlapped/native completion once. A stale generation cannot complete a new op.
    pub fn complete_native(&mut self, token: Token, generation: u64) -> bool {
        let Some(record) = self.native.get_mut(&token) else {
            return false;
        };
        if record.generation != generation || record.completed {
            return false;
        }
        record.completed = true;
        true
    }

    pub fn cancel_scope(&self, owner: ScopeId) -> usize {
        let mut count = 0;
        for task in self.tasks.values().filter(|task| task.owner == owner) {
            task.cancellation.cancel();
            count += 1;
        }
        for native in self.native.values().filter(|native| native.owner == owner) {
            (native.cancel)();
            count += 1;
        }
        count
    }

    pub fn cancel(&self, token: Token) -> bool {
        if let Some(task) = self.tasks.get(&token) {
            task.cancellation.cancel();
            return true;
        }
        if let Some(native) = self.native.get(&token) {
            (native.cancel)();
            return true;
        }
        false
    }

    fn collect_completions(&mut self) {
        while let Ok(completion) = self.completions_rx.try_recv() {
            self.completed.insert(completion);
        }
    }

    pub fn drain_scope(&mut self, owner: ScopeId, deadline: Instant) -> DrainReport {
        self.cancel_scope(owner);
        let mut joined_total = 0;
        loop {
            let mut report = self.poll_scope(owner);
            joined_total += report.joined;
            if report.pending == 0 && report.native_pending == 0 {
                report.joined = joined_total;
                return report;
            }
            if Instant::now() >= deadline {
                report.joined = joined_total;
                report.quarantined = true;
                return report;
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// Non-blocking drain observation for UI-turn supervisors.
    pub fn poll_scope(&mut self, owner: ScopeId) -> DrainReport {
        self.collect_completions();
        let ready: Vec<Token> = self
            .tasks
            .iter()
            .filter(|(token, task)| {
                task.owner == owner
                    && self.completed.contains(&(**token, task.generation))
                    && task.join.as_ref().is_some_and(join_finished)
            })
            .map(|(token, _)| *token)
            .collect();
        let mut joined = 0;
        for token in ready {
            if let Some(mut record) = self.tasks.remove(&token) {
                if let Some(join) = record.join.take() {
                    observe_join(join);
                }
                self.completed.remove(&(token, record.generation));
                joined += 1;
            }
        }
        let completed_native: Vec<Token> = self
            .native
            .iter()
            .filter(|(_, native)| native.owner == owner && native.completed)
            .map(|(token, _)| *token)
            .collect();
        for token in completed_native {
            self.native.remove(&token);
        }
        DrainReport {
            joined,
            pending: self
                .tasks
                .values()
                .filter(|task| task.owner == owner)
                .count(),
            native_pending: self
                .native
                .values()
                .filter(|native| native.owner == owner)
                .count(),
            quarantined: false,
        }
    }

    pub fn accepts_completion(&self, token: Token, generation: u64) -> bool {
        self.tasks
            .get(&token)
            .is_some_and(|task| task.generation == generation)
    }
}

fn join_finished(join: &JoinKind) -> bool {
    match join {
        JoinKind::Thread(join) => join.is_finished(),
        JoinKind::Tokio(join) => join.is_finished(),
    }
}

struct NoopWake;
impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

fn observe_join(join: JoinKind) {
    match join {
        JoinKind::Thread(join) => {
            let _ = join.join();
        }
        JoinKind::Tokio(mut join) => {
            let waker = Waker::from(Arc::new(NoopWake));
            let mut context = Context::from_waker(&waker);
            match Pin::new(&mut join).poll(&mut context) {
                Poll::Ready(_) => {}
                Poll::Pending => unreachable!("is_finished Tokio handle returned Pending"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn cancellation_is_requested_and_completion_is_joined() {
        let mut supervisor = TaskSupervisor::new();
        let owner = ScopeId(4);
        let (seen_tx, seen_rx) = mpsc::channel();
        let token = supervisor
            .spawn(owner, 9, move |cancel| {
                while !cancel.is_cancelled() {
                    thread::yield_now();
                }
                seen_tx.send(()).unwrap();
            })
            .unwrap();
        assert!(supervisor.accepts_completion(token, 9));
        assert!(!supervisor.accepts_completion(token, 8));
        let report = supervisor.drain_scope(owner, Instant::now() + Duration::from_secs(1));
        seen_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(
            report,
            DrainReport {
                joined: 1,
                pending: 0,
                native_pending: 0,
                quarantined: false
            }
        );
    }

    #[test]
    fn deadline_quarantines_but_retains_join_handle() {
        let mut supervisor = TaskSupervisor::new();
        let (release_tx, release_rx) = mpsc::channel();
        supervisor
            .spawn(ScopeId(2), 1, move |_| {
                let _ = release_rx.recv();
            })
            .unwrap();
        let report = supervisor.drain_scope(ScopeId(2), Instant::now());
        assert!(report.quarantined);
        assert_eq!(report.pending, 1);
        release_tx.send(()).unwrap();
        let report = supervisor.drain_scope(ScopeId(2), Instant::now() + Duration::from_secs(1));
        assert_eq!(report.pending, 0);
    }

    #[test]
    fn tokio_join_and_native_io_are_observed_before_drain() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut supervisor = TaskSupervisor::new();
        let owner = ScopeId(8);
        supervisor
            .spawn_tokio(runtime.handle(), owner, 3, |cancel| async move {
                cancel.cancelled().await;
            })
            .unwrap();
        let native = supervisor.register_native(owner, 3, || {}).unwrap();
        assert!(!supervisor.complete_native(native, 2));
        assert!(supervisor.complete_native(native, 3));
        assert!(!supervisor.complete_native(native, 3));
        let report = supervisor.drain_scope(owner, Instant::now() + Duration::from_secs(1));
        assert_eq!(report.pending, 0);
        assert_eq!(report.native_pending, 0);
        assert!(!report.quarantined);
    }

    #[test]
    fn incomplete_native_io_crosses_quarantine_deadline() {
        let mut supervisor = TaskSupervisor::new();
        let owner = ScopeId(9);
        let native = supervisor.register_native(owner, 4, || {}).unwrap();
        let report = supervisor.drain_scope(owner, Instant::now());
        assert!(report.quarantined);
        assert_eq!(report.native_pending, 1);
        assert!(supervisor.complete_native(native, 4));
        let report = supervisor.drain_scope(owner, Instant::now() + Duration::from_secs(1));
        assert!(!report.quarantined);
    }
}

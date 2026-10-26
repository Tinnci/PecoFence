//! Scope-checked host adapter for the SPM transport.
use pecofence_plugin_api::*;
use std::collections::{HashMap, VecDeque};
#[cfg(test)]
use std::rc::Rc;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
pub struct PipeEvent {
    pub scope: ScopeId,
    pub generation: u64,
    pub token: Token,
    pub event: PanelEvent,
}

struct PipeCommand {
    owner: ScopeId,
    pub generation: u64,
    subscription: bool,
    settled: bool,
}

pub struct PipeState {
    next: AtomicU64,
    wake: Box<dyn Fn() + Send + Sync>,
    events: Mutex<VecDeque<PipeEvent>>,
    commands: Mutex<HashMap<Token, PipeCommand>>,
}

impl PipeState {
    pub fn drain(&self) -> Vec<PipeEvent> {
        self.events
            .lock()
            .map(|mut events| events.drain(..).collect())
            .unwrap_or_default()
    }
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            next: AtomicU64::new(1),
            wake: Box::new(wake),
            events: Mutex::new(VecDeque::new()),
            commands: Mutex::new(HashMap::new()),
        }
    }

    fn token(&self) -> Result<Token> {
        let value = self
            .next
            .try_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .map_err(|_| Error::Exhausted)?;
        Ok(Token(value))
    }

    pub fn publish(&self, scope: ScopeId, generation: u64, subscription: Token, bytes: Arc<[u8]>) {
        let live = self.commands.lock().is_ok_and(|commands| {
            commands.get(&subscription).is_some_and(|command| {
                command.subscription && command.owner == scope && command.generation == generation
            })
        });
        if !live {
            return;
        }
        self.enqueue(PipeEvent {
            scope,
            generation,
            token: subscription,
            event: PanelEvent::Snapshot {
                subscription,
                bytes,
            },
        });
    }

    pub fn complete(
        &self,
        scope: ScopeId,
        generation: u64,
        operation: Token,
        result: Result<Arc<[u8]>>,
    ) {
        let Ok(mut commands) = self.commands.lock() else {
            return;
        };
        let Some(command) = commands.get_mut(&operation) else {
            return;
        };
        if command.subscription
            || command.settled
            || command.owner != scope
            || command.generation != generation
        {
            return;
        }
        command.settled = true;
        drop(commands);
        self.enqueue(PipeEvent {
            scope,
            generation,
            token: operation,
            event: PanelEvent::Completion { operation, result },
        });
    }

    fn enqueue(&self, event: PipeEvent) {
        if let Ok(mut events) = self.events.lock() {
            if matches!(&event.event, PanelEvent::Snapshot { .. })
                && let Some(index) = events.iter().position(|queued| {
                    queued.token == event.token
                        && matches!(&queued.event, PanelEvent::Snapshot { .. })
                })
            {
                events.remove(index);
            }
            if events.len() >= 128 + crate::MAX_OPERATIONS
                && let Some(index) = events
                    .iter()
                    .position(|queued| matches!(&queued.event, PanelEvent::Snapshot { .. }))
            {
                events.remove(index);
            }
            // Completed operations retain their bounded slot until claimed or cancelled.
            events.push_back(event);
        }
        (self.wake)();
    }

    pub fn claim(&self, event: &PipeEvent) -> bool {
        let Ok(mut commands) = self.commands.lock() else {
            return false;
        };
        let Some(command) = commands.get(&event.token) else {
            return false;
        };
        if command.owner != event.scope || command.generation != event.generation {
            return false;
        }
        if matches!(&event.event, PanelEvent::Completion { .. }) {
            if command.subscription || !command.settled {
                return false;
            }
            commands.remove(&event.token);
        } else if !command.subscription {
            return false;
        }
        true
    }

    pub fn remove_scope(&self, scope: ScopeId) -> Vec<(Token, bool)> {
        let Ok(mut commands) = self.commands.lock() else {
            return vec![];
        };
        let tokens: Vec<_> = commands
            .iter()
            .filter(|(_, command)| command.owner == scope)
            .map(|(token, command)| (*token, command.subscription))
            .collect();
        for (token, _) in &tokens {
            commands.remove(token);
        }
        drop(commands);
        if let Ok(mut events) = self.events.lock() {
            events.retain(|event| event.scope != scope);
        }
        tokens
    }
}

#[cfg(test)]
impl PipeState {
    pub fn test_operation(&self, token: Token, owner: ScopeId, generation: u64) {
        self.commands.lock().unwrap().insert(
            token,
            PipeCommand {
                owner,
                generation,
                subscription: false,
                settled: false,
            },
        );
    }

    pub fn test_events(&self) -> Vec<PanelEvent> {
        self.events
            .lock()
            .unwrap()
            .drain(..)
            .map(|event| event.event)
            .collect()
    }
}

pub struct PipeIpc {
    pub state: Arc<PipeState>,
    pub transport: crate::TransportHandle,
}

impl IpcService for PipeIpc {
    fn subscribe(&self, scope: &ScopeHandle, query: Query) -> Result<Token> {
        let owner = scope.check()?;
        let generation = scope.generation()?;
        if query.endpoint != "spm.v2/read-model" {
            return Err(Error::Invalid("unsupported local endpoint".into()));
        }
        let subscription = self.state.token()?;
        let project_query: spm_contracts::ProjectQuery = serde_json::from_slice(&query.payload)
            .map_err(|error| Error::Invalid(error.to_string()))?;
        self.state
            .commands
            .lock()
            .map_err(|_| Error::Backend("IPC command mutex poisoned".into()))?
            .insert(
                subscription,
                PipeCommand {
                    owner,
                    generation,
                    subscription: true,
                    settled: false,
                },
            );
        if self
            .transport
            .subscribe(subscription, owner, generation, project_query)
            .is_err()
        {
            self.state
                .commands
                .lock()
                .map_err(|_| Error::Closed)?
                .remove(&subscription);
            return Err(Error::Backend("SPM transport stopped".into()));
        }
        Ok(subscription)
    }
    fn send(&self, scope: &ScopeHandle, payload: Arc<[u8]>) -> Result<Token> {
        let owner = scope.check()?;
        let generation = scope.generation()?;
        let request: spm_contracts::Request =
            serde_json::from_slice(&payload).map_err(|error| Error::Invalid(error.to_string()))?;
        request
            .validate()
            .map_err(|error| Error::Invalid(error.to_string()))?;
        if !matches!(
            &request,
            spm_contracts::Request::Refresh(_)
                | spm_contracts::Request::BuildBriefing(_)
                | spm_contracts::Request::ResolveNavigation(_)
        ) {
            return Err(Error::Invalid("unsupported panel operation".into()));
        }
        let operation = self.state.token()?;
        let mut commands = self
            .state
            .commands
            .lock()
            .map_err(|_| Error::Backend("IPC command mutex poisoned".into()))?;
        if commands
            .values()
            .filter(|command| !command.subscription)
            .count()
            >= crate::MAX_OPERATIONS
        {
            return Err(Error::Exhausted);
        }
        commands.insert(
            operation,
            PipeCommand {
                owner,
                generation,
                subscription: false,
                settled: false,
            },
        );
        drop(commands);
        if self
            .transport
            .request(operation, owner, generation, request)
            .is_err()
        {
            self.state
                .commands
                .lock()
                .map_err(|_| Error::Closed)?
                .remove(&operation);
            return Err(Error::Backend("SPM transport stopped".into()));
        }
        Ok(operation)
    }
    fn cancel(&self, token: Token) -> Result<()> {
        let command = self
            .state
            .commands
            .lock()
            .map_err(|_| Error::Backend("IPC command mutex poisoned".into()))?
            .remove(&token)
            .ok_or(Error::Revoked)?;
        if command.subscription {
            self.transport.unsubscribe(token)
        } else {
            self.transport.cancel_operation(token)
        }
        .map_err(|_| Error::Backend("SPM transport stopped".into()))?;
        Ok(())
    }
}

#[cfg(test)]
mod pipe_tests {
    use super::*;
    use spm_contracts::{
        DeliveryScopeId, DetailLevel, IdempotencyKey, ProjectId, ProjectQuery, RefreshRequest,
        Request, ViewKind,
    };

    struct Lease;
    impl ScopeLease for Lease {
        fn id(&self) -> ScopeId {
            ScopeId(7)
        }
        fn generation(&self) -> u64 {
            3
        }
        fn is_open(&self) -> bool {
            true
        }
    }

    fn setup() -> (PipeIpc, Rc<dyn ScopeLease>, ScopeHandle, crate::Receivers) {
        let (transport, receivers) = crate::channel();
        let lease: Rc<dyn ScopeLease> = Rc::new(Lease);
        let scope = ScopeHandle::from_lease(&lease);
        (
            PipeIpc {
                state: Arc::new(PipeState::new(|| {})),
                transport,
            },
            lease,
            scope,
            receivers,
        )
    }

    fn query() -> Query {
        Query {
            endpoint: "spm.v2/read-model".into(),
            payload: serde_json::to_vec(&ProjectQuery {
                project_id: ProjectId::new("fictional-project").unwrap(),
                delivery_scope_id: DeliveryScopeId::new("fictional-scope").unwrap(),
                view: ViewKind::Summary,
                filter: None,
                detail_level: DetailLevel::Standard,
                sort: vec![],
            })
            .unwrap()
            .into(),
        }
    }

    fn request() -> Arc<[u8]> {
        serde_json::to_vec(&Request::Refresh(RefreshRequest {
            project_id: ProjectId::new("fictional-project").unwrap(),
            delivery_scope_id: DeliveryScopeId::new("fictional-scope").unwrap(),
            idempotency_key: IdempotencyKey::new(),
        }))
        .unwrap()
        .into()
    }

    #[test]
    fn one_call_with_two_subscriptions_is_sent_once_and_cancelled_as_an_operation() {
        let (ipc, _lease, scope, mut receivers) = setup();
        let a = ipc.subscribe(&scope, query()).unwrap();
        let b = ipc.subscribe(&scope, query()).unwrap();
        let operation = ipc.send(&scope, request()).unwrap();
        assert_ne!(operation, a);
        assert_ne!(operation, b);
        assert!(
            matches!(receivers.data.try_recv().unwrap(), crate::Command::Subscribe { local, .. } if local == a)
        );
        assert!(
            matches!(receivers.data.try_recv().unwrap(), crate::Command::Subscribe { local, .. } if local == b)
        );
        assert!(
            matches!(receivers.data.try_recv().unwrap(), crate::Command::Request { local, .. } if local == operation)
        );
        assert!(receivers.data.try_recv().is_err());
        ipc.cancel(operation).unwrap();
        assert!(
            matches!(receivers.control.try_recv().unwrap(), crate::Command::CancelOperation { local } if local == operation)
        );
        assert!(ipc.state.commands.lock().unwrap().contains_key(&a));
        ipc.cancel(a).unwrap();
        assert!(
            matches!(receivers.control.try_recv().unwrap(), crate::Command::Unsubscribe { local } if local == a)
        );
    }

    #[test]
    fn completions_are_generation_bound_claimed_once_and_do_not_need_a_subscription() {
        let (ipc, _lease, scope, _receivers) = setup();
        let operation = ipc.send(&scope, request()).unwrap();
        ipc.state
            .complete(ScopeId(7), 2, operation, Ok(Arc::from(&b"wrong"[..])));
        assert!(ipc.state.events.lock().unwrap().is_empty());
        ipc.state
            .complete(ScopeId(7), 3, operation, Ok(Arc::from(&b"correct"[..])));
        ipc.state
            .complete(ScopeId(7), 3, operation, Err(Error::Closed));
        let event = ipc.state.events.lock().unwrap().pop_front().unwrap();
        assert!(
            matches!(&event.event, PanelEvent::Completion { operation: id, result: Ok(bytes) } if *id == operation && bytes.as_ref() == b"correct")
        );
        assert!(ipc.state.claim(&event));
        assert!(!ipc.state.claim(&event));
        assert!(ipc.state.events.lock().unwrap().is_empty());
    }

    #[test]
    fn settled_calls_keep_their_budget_until_claimed_and_scope_close_discards_delivery() {
        let (ipc, _lease, scope, mut receivers) = setup();
        for _ in 0..crate::MAX_OPERATIONS {
            let operation = ipc.send(&scope, request()).unwrap();
            receivers.data.try_recv().unwrap();
            ipc.state
                .complete(ScopeId(7), 3, operation, Err(Error::Closed));
        }
        assert_eq!(ipc.send(&scope, request()).unwrap_err(), Error::Exhausted);
        assert_eq!(
            ipc.state.events.lock().unwrap().len(),
            crate::MAX_OPERATIONS
        );
        assert_eq!(
            ipc.state.remove_scope(ScopeId(7)).len(),
            crate::MAX_OPERATIONS
        );
        assert!(ipc.state.events.lock().unwrap().is_empty());
        assert!(ipc.send(&scope, request()).is_ok());
    }

    #[test]
    fn rejected_queue_submission_leaves_no_operation_or_subscription_record() {
        let (ipc, _lease, scope, receivers) = setup();
        drop(receivers);
        assert!(ipc.send(&scope, request()).is_err());
        assert!(ipc.subscribe(&scope, query()).is_err());
        assert!(ipc.state.commands.lock().unwrap().is_empty());
    }
}

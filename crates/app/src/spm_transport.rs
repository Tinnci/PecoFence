use crate::app::panel_manager::PipeState;
use pecofence_plugin_api::{Error, ScopeId, Token};
use pecofence_plugin_kernel::CancellationToken;
use spm_contracts::{
    Body, DaemonSessionId, Direction, Envelope, Event, Feature, HelloRequest, PROTOCOL_MAJOR,
    PROTOCOL_MINOR, ProjectQuery, Request, RequestId, Response, RpcMethod, SubscribeRequest,
    SubscriptionId, UnsubscribeRequest,
};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub(crate) const MAX_OPERATIONS: usize = 64;
const OPERATION_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionState {
    Connecting = 0,
    Connected = 1,
    Reconnecting = 2,
    Disconnected = 3,
}

#[derive(Clone)]
pub struct TransportHandle {
    data_sender: mpsc::Sender<Command>,
    control_sender: mpsc::UnboundedSender<Command>,
    state: Arc<AtomicU8>,
}

impl TransportHandle {
    pub fn subscribe(
        &self,
        local: Token,
        owner: ScopeId,
        generation: u64,
        query: ProjectQuery,
    ) -> Result<(), ()> {
        self.data_sender
            .try_send(Command::Subscribe {
                local,
                route: Route { owner, generation },
                query,
            })
            .map_err(send_error)
    }

    pub fn unsubscribe(&self, local: Token) -> Result<(), ()> {
        self.control_sender
            .send(Command::Unsubscribe { local })
            .map_err(|_| ())
    }

    pub fn request(
        &self,
        local: Token,
        owner: ScopeId,
        generation: u64,
        request: Request,
    ) -> Result<(), ()> {
        self.data_sender
            .try_send(Command::Request {
                local,
                route: Route { owner, generation },
                request,
            })
            .map_err(send_error)
    }

    pub fn cancel_operation(&self, local: Token) -> Result<(), ()> {
        self.control_sender
            .send(Command::CancelOperation { local })
            .map_err(|_| ())
    }

    #[allow(dead_code)]
    pub fn connection_state(&self) -> ConnectionState {
        match self.state.load(Ordering::Acquire) {
            0 => ConnectionState::Connecting,
            1 => ConnectionState::Connected,
            2 => ConnectionState::Reconnecting,
            _ => ConnectionState::Disconnected,
        }
    }
}

fn send_error(error: mpsc::error::TrySendError<Command>) {
    if matches!(error, mpsc::error::TrySendError::Full(_)) {
        tracing::warn!("transport.data_commands_full");
    }
}

pub struct Receivers {
    pub(crate) data: mpsc::Receiver<Command>,
    pub(crate) control: mpsc::UnboundedReceiver<Command>,
}

// PipeState::token uses a checked, increasing u64 counter; tokens never repeat.
// An unsubscribe may overtake its subscribe across these independent channels.
pub fn channel() -> (TransportHandle, Receivers) {
    let (data_sender, data) = mpsc::channel(64);
    let (control_sender, control) = mpsc::unbounded_channel();
    let state = Arc::new(AtomicU8::new(ConnectionState::Disconnected as u8));
    (
        TransportHandle {
            data_sender,
            control_sender,
            state,
        },
        Receivers { data, control },
    )
}

#[derive(Clone, Copy)]
pub(crate) struct Route {
    owner: ScopeId,
    generation: u64,
}

// ProjectQuery/Envelope payloads dominate the variant size; the transport
// clones these commands across the actor boundary and boxing would churn
// every construction site for no measurable benefit.
#[allow(clippy::large_enum_variant)]
pub enum Command {
    Subscribe {
        local: Token,
        route: Route,
        query: ProjectQuery,
    },
    Unsubscribe {
        local: Token,
    },
    Request {
        local: Token,
        route: Route,
        request: Request,
    },
    CancelOperation {
        local: Token,
    },
}

struct PendingOperation {
    local: Token,
    route: Route,
    method: RpcMethod,
    deadline: Instant,
}

struct ActiveQuery {
    routes: HashMap<Token, Route>,
    remote: Option<SubscriptionId>,
    latest: Option<Arc<[u8]>>,
}

struct Actor {
    endpoint: String,
    receivers: Receivers,
    connection_state: Arc<AtomicU8>,
    sink: Arc<PipeState>,
    active: HashMap<ProjectQuery, ActiveQuery>,
    local_queries: HashMap<Token, ProjectQuery>,
    cancelled_tokens: HashSet<Token>,
    remote_queries: HashMap<SubscriptionId, ProjectQuery>,
    pending_subscribes: HashMap<RequestId, ProjectQuery>,
    pending_operations: HashMap<RequestId, PendingOperation>,
    daemon_session: Option<DaemonSessionId>,
}

pub async fn run(
    endpoint: String,
    receivers: Receivers,
    handle: TransportHandle,
    sink: Arc<PipeState>,
    cancel: CancellationToken,
) {
    let endpoint_for_log = endpoint.clone();
    let mut actor = Actor {
        endpoint,
        receivers,
        connection_state: handle.state,
        sink,
        active: HashMap::new(),
        local_queries: HashMap::new(),
        cancelled_tokens: HashSet::new(),
        remote_queries: HashMap::new(),
        pending_subscribes: HashMap::new(),
        pending_operations: HashMap::new(),
        daemon_session: None,
    };
    tracing::info!(endpoint = %endpoint_for_log, "transport.run");
    let mut delay = Duration::from_millis(500);
    actor.set_state(ConnectionState::Connecting);
    while !cancel.is_cancelled() {
        let pipe = tokio::net::windows::named_pipe::ClientOptions::new().open(&actor.endpoint);
        match pipe {
            Ok(pipe) => {
                match actor.connected(pipe, &cancel).await {
                    Ok(()) if cancel.is_cancelled() => break,
                    Ok(()) | Err(()) => {
                        actor.set_state(ConnectionState::Reconnecting);
                        actor.reset_remote();
                    }
                }
                delay = Duration::from_millis(500);
            }
            Err(error) => {
                tracing::warn!(%error, endpoint = %endpoint_for_log, "transport.connect_failed");
                actor.set_state(if actor.daemon_session.is_some() {
                    ConnectionState::Reconnecting
                } else {
                    ConnectionState::Connecting
                });
            }
        }
        let wait = jitter(delay);
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(wait) => {},
            command = actor.receivers.control.recv() => {
                let Some(command) = command else { break };
                actor.apply_offline(command);
            }
            command = actor.receivers.data.recv() => {
                let Some(command) = command else { break };
                actor.apply_offline(command);
            }
        }
        delay = (delay * 2).min(Duration::from_secs(15));
    }
    actor.fail_operations("transport stopped; remote outcome unknown");
    while let Ok(command) = actor.receivers.data.try_recv() {
        actor.apply_offline(command);
    }
    actor.set_state(ConnectionState::Disconnected);
}

impl Actor {
    fn discard_cancelled(&mut self, local: Token) -> bool {
        if self.cancelled_tokens.remove(&local) {
            tracing::debug!(token = local.0, "transport.subscribe_cancelled");
            true
        } else {
            false
        }
    }

    fn unsubscribe_local(&mut self, local: Token) -> Option<SubscriptionId> {
        if self.local_queries.contains_key(&local) {
            self.remove_local(local)
        } else {
            self.cancelled_tokens.insert(local);
            if self.cancelled_tokens.len() > 4096 {
                self.cancelled_tokens.clear();
                tracing::warn!("transport.cancelled_tokens_overflow");
            }
            None
        }
    }

    fn set_state(&self, state: ConnectionState) {
        self.connection_state.store(state as u8, Ordering::Release);
    }

    fn reset_remote(&mut self) {
        self.fail_operations("connection lost; remote outcome unknown");
        self.remote_queries.clear();
        self.pending_subscribes.clear();
        for active in self.active.values_mut() {
            active.remote = None;
        }
    }

    fn apply_offline(&mut self, command: Command) {
        match command {
            Command::Subscribe {
                local,
                route,
                query,
            } => {
                if self.discard_cancelled(local) {
                    return;
                }
                self.local_queries.insert(local, query.clone());
                let active = self.active.entry(query).or_insert_with(|| ActiveQuery {
                    routes: HashMap::new(),
                    remote: None,
                    latest: None,
                });
                active.routes.insert(local, route);
                if let Some(bytes) = &active.latest {
                    self.sink
                        .publish(route.owner, route.generation, local, bytes.clone());
                }
            }
            Command::Unsubscribe { local } => {
                self.unsubscribe_local(local);
            }
            Command::Request { local, route, .. } => {
                if !self.discard_cancelled(local) {
                    self.sink.complete(
                        route.owner,
                        route.generation,
                        local,
                        Err(Error::Backend(
                            "SPM unavailable; request was not dispatched".into(),
                        )),
                    );
                }
            }
            Command::CancelOperation { local } => self.cancel_operation(local),
        }
    }

    fn remove_local(&mut self, local: Token) -> Option<SubscriptionId> {
        let query = self.local_queries.remove(&local)?;
        let active = self.active.get_mut(&query)?;
        active.routes.remove(&local);
        if active.routes.is_empty() {
            let remote = active.remote;
            self.active.remove(&query);
            remote
        } else {
            None
        }
    }

    async fn connected(
        &mut self,
        pipe: tokio::net::windows::named_pipe::NamedPipeClient,
        cancel: &CancellationToken,
    ) -> Result<(), ()> {
        let (reader, mut writer) = tokio::io::split(pipe);
        let hello_id = RequestId::new();
        write_envelope(
            &mut writer,
            &Envelope {
                protocol_major: PROTOCOL_MAJOR,
                protocol_minor: PROTOCOL_MINOR,
                daemon_session: None,
                request_id: Some(hello_id),
                subscription_id: None,
                revision: None,
                body: Body::Request(Request::Hello(HelloRequest {
                    client_build: env!("CARGO_PKG_VERSION").into(),
                    protocol_major: PROTOCOL_MAJOR,
                    protocol_minor: PROTOCOL_MINOR,
                    features: BTreeSet::from([
                        Feature::Multiplexing,
                        Feature::Pagination,
                        Feature::Navigation,
                        Feature::Briefing,
                    ]),
                })),
            },
        )
        .await
        .map_err(|_| ())?;
        // Dedicated reader task: owns the read half and the framing state so a
        // command waking the select loop can no longer discard a half-read
        // frame. Complete envelopes arrive through the channel instead.
        let (envelope_tx, mut envelope_rx) = mpsc::channel::<Envelope>(32);
        let reader_task = spawn_reader(reader, envelope_tx);
        let result = async {
            let hello =
                match tokio::time::timeout(std::time::Duration::from_secs(10), envelope_rx.recv())
                    .await
                {
                    Ok(Some(envelope)) => envelope,
                    Ok(None) => return Ok(()),
                    Err(_) => return Err(()),
                };
            hello.validate(Direction::ServerToClient).map_err(|_| ())?;
            let Body::Response(Response::Hello(response)) = hello.body else {
                return Err(());
            };
            if hello.request_id != Some(hello_id) {
                return Err(());
            }
            if response.protocol_major != PROTOCOL_MAJOR || response.protocol_minor > PROTOCOL_MINOR
            {
                return Err(());
            }
            let session_changed = self.daemon_session != Some(response.daemon_session);
            self.daemon_session = Some(response.daemon_session);
            self.reset_remote();
            if session_changed {
                for active in self.active.values_mut() {
                    active.latest = None;
                }
            }
            self.set_state(ConnectionState::Connected);
            let queries: Vec<_> = self.active.keys().cloned().collect();
            for query in queries {
                self.send_subscribe(&mut writer, query).await?;
            }
            let mut deadlines = tokio::time::interval(Duration::from_secs(1));
            deadlines.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = deadlines.tick() => self.expire_operations(Instant::now()),
                    _ = cancel.cancelled() => return Ok(()),
                    command = self.receivers.control.recv() => {
                        let Some(command) = command else { return Ok(()) };
                        self.apply_connected(command, &mut writer).await?;
                    }
                    command = self.receivers.data.recv() => {
                        let Some(command) = command else { return Ok(()) };
                        self.apply_connected(command, &mut writer).await?;
                    }
                    incoming = envelope_rx.recv() => {
                        let Some(envelope) = incoming else {
                            // Reader task ended: connection is closed.
                            return Ok(());
                        };
                        envelope.validate(Direction::ServerToClient).map_err(|_| ())?;
                        self.route_incoming(envelope);
                    }
                }
            }
        }
        .await;
        reap_reader(reader_task, Duration::from_secs(2)).await;
        result
    }

    async fn send_subscribe(
        &mut self,
        writer: &mut (impl AsyncWrite + Unpin),
        query: ProjectQuery,
    ) -> Result<(), ()> {
        let request_id = RequestId::new();
        let envelope = self.request_envelope(
            request_id,
            Request::Subscribe(SubscribeRequest {
                query: query.clone(),
            }),
        )?;
        write_envelope(writer, &envelope).await.map_err(|_| ())?;
        self.pending_subscribes.insert(request_id, query);
        Ok(())
    }

    async fn apply_connected(
        &mut self,
        command: Command,
        writer: &mut (impl AsyncWrite + Unpin),
    ) -> Result<(), ()> {
        match command {
            Command::Subscribe {
                local,
                route,
                query,
            } => {
                if self.discard_cancelled(local) {
                    return Ok(());
                }
                self.local_queries.insert(local, query.clone());
                if let Some(active) = self.active.get_mut(&query) {
                    active.routes.insert(local, route);
                    if let Some(bytes) = &active.latest {
                        self.sink
                            .publish(route.owner, route.generation, local, bytes.clone());
                    }
                } else {
                    self.active.insert(
                        query.clone(),
                        ActiveQuery {
                            routes: HashMap::from([(local, route)]),
                            remote: None,
                            latest: None,
                        },
                    );
                    self.send_subscribe(writer, query).await?;
                }
            }
            Command::Unsubscribe { local } => {
                if let Some(remote) = self.unsubscribe_local(local) {
                    self.remote_queries.remove(&remote);
                    let request_id = RequestId::new();
                    let envelope = self.request_envelope(
                        request_id,
                        Request::Unsubscribe(UnsubscribeRequest {
                            subscription_id: remote,
                        }),
                    )?;
                    write_envelope(writer, &envelope).await.map_err(|_| ())?;
                }
            }
            Command::Request {
                local,
                route,
                request,
            } => {
                if self.discard_cancelled(local) {
                    return Ok(());
                }
                if self.pending_operations.len() >= MAX_OPERATIONS {
                    self.sink
                        .complete(route.owner, route.generation, local, Err(Error::Exhausted));
                    return Ok(());
                }
                let request_id = RequestId::new();
                let method = request.method();
                let envelope = self.request_envelope(request_id, request)?;
                if let Err(error) = envelope.validate(Direction::ClientToServer) {
                    self.sink.complete(
                        route.owner,
                        route.generation,
                        local,
                        Err(Error::Invalid(error.to_string())),
                    );
                    return Ok(());
                }
                // Register before writing: partial writes have an indeterminate outcome.
                self.pending_operations.insert(
                    request_id,
                    PendingOperation {
                        local,
                        route,
                        method,
                        deadline: Instant::now() + OPERATION_TIMEOUT,
                    },
                );
                write_envelope(writer, &envelope).await.map_err(|_| ())?;
            }
            Command::CancelOperation { local } => self.cancel_operation(local),
        }
        Ok(())
    }

    fn request_envelope(&self, request_id: RequestId, request: Request) -> Result<Envelope, ()> {
        Ok(Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            daemon_session: self.daemon_session,
            request_id: Some(request_id),
            subscription_id: None,
            revision: None,
            body: Body::Request(request),
        })
    }

    fn route_incoming(&mut self, envelope: Envelope) {
        if envelope.daemon_session != self.daemon_session
            || envelope.protocol_minor > PROTOCOL_MINOR
        {
            return;
        }
        if let Some(request_id) = envelope.request_id
            && let Some(operation) = self.pending_operations.remove(&request_id)
        {
            let result = match &envelope.body {
                Body::Response(response) if response_method(response) == operation.method => {
                    serde_json::to_vec(&envelope)
                        .map(Arc::<[u8]>::from)
                        .map_err(|error| Error::Invalid(error.to_string()))
                }
                Body::Error(error) => Err(Error::Backend(format!(
                    "{:?}: {}",
                    error.code, error.message
                ))),
                _ => Err(Error::Invalid("unexpected action response method".into())),
            };
            self.sink.complete(
                operation.route.owner,
                operation.route.generation,
                operation.local,
                result,
            );
            return;
        }
        if let Body::Response(Response::Subscribe(response)) = &envelope.body {
            if let Some(request_id) = envelope.request_id {
                tracing::info!(request_id = ?request_id, subscription_id = ?response.subscription_id, "subscribe.acknowledged");
                if let Some(query) = self.pending_subscribes.remove(&request_id) {
                    if response.accepted_query != query || !self.active.contains_key(&query) {
                        return;
                    }
                    self.remote_queries
                        .insert(response.subscription_id, query.clone());
                    if let Some(active) = self.active.get_mut(&query) {
                        active.remote = Some(response.subscription_id);
                    }
                }
            }
            return;
        }
        if matches!(&envelope.body, Body::Event(Event::OperationCompleted(_))) {
            // RefreshEvents is not negotiated. An operation notification is not
            // a subscription snapshot, nor an uncorrelated global completion.
            return;
        }
        let Body::Event(Event::Snapshot(_)) = &envelope.body else {
            return;
        };
        let Some(remote) = envelope.subscription_id else {
            return;
        };
        let Some(query) = self.remote_queries.get(&remote).cloned() else {
            return;
        };
        let Ok(bytes) = serde_json::to_vec(&envelope).map(Arc::<[u8]>::from) else {
            return;
        };
        tracing::info!(
            revision = envelope.revision.map(|r| r.0).unwrap_or(0),
            query = ?query,
            "snapshot.received"
        );
        if let Some(active) = self.active.get_mut(&query) {
            active.latest = Some(bytes.clone());
            for (local, route) in &active.routes {
                self.sink
                    .publish(route.owner, route.generation, *local, bytes.clone());
            }
        }
    }

    fn cancel_operation(&mut self, local: Token) {
        let id = self
            .pending_operations
            .iter()
            .find(|(_, operation)| operation.local == local)
            .map(|(id, _)| *id);
        if let Some(id) = id {
            self.pending_operations.remove(&id);
        } else {
            self.cancelled_tokens.insert(local);
            if self.cancelled_tokens.len() > 4096 {
                self.cancelled_tokens.clear();
            }
        }
    }

    fn fail_operations(&mut self, message: &str) {
        for (_, operation) in self.pending_operations.drain() {
            self.sink.complete(
                operation.route.owner,
                operation.route.generation,
                operation.local,
                Err(Error::Backend(message.into())),
            );
        }
    }

    fn expire_operations(&mut self, now: Instant) {
        let expired: Vec<_> = self
            .pending_operations
            .iter()
            .filter(|(_, operation)| operation.deadline <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            if let Some(operation) = self.pending_operations.remove(&id) {
                self.sink.complete(
                    operation.route.owner,
                    operation.route.generation,
                    operation.local,
                    Err(Error::Backend(
                        "operation timed out; remote outcome unknown".into(),
                    )),
                );
            }
        }
    }
}

fn response_method(response: &Response) -> RpcMethod {
    match response {
        Response::Hello(_) => RpcMethod::Hello,
        Response::GetCapabilities(_) => RpcMethod::GetCapabilities,
        Response::Subscribe(_) => RpcMethod::Subscribe,
        Response::Unsubscribe(_) => RpcMethod::Unsubscribe,
        Response::QueryPage(_) => RpcMethod::QueryPage,
        Response::Refresh(_) => RpcMethod::Refresh,
        Response::ResolveNavigation(_) => RpcMethod::ResolveNavigation,
        Response::BuildBriefing(_) => RpcMethod::BuildBriefing,
        Response::Ping(_) => RpcMethod::Ping,
    }
}

fn jitter(base: Duration) -> Duration {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let percent = 80 + (nanos % 41) as u64;
    base.mul_f64(percent as f64 / 100.0)
}

fn spawn_reader<R: AsyncRead + Unpin + Send + 'static>(
    mut reader: R,
    tx: mpsc::Sender<Envelope>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Ok(envelope) = read_envelope(&mut reader).await {
            if tx.send(envelope).await.is_err() {
                break;
            }
        }
    })
}

async fn reap_reader(handle: JoinHandle<()>, timeout: Duration) -> bool {
    handle.abort();
    match tokio::time::timeout(timeout, handle).await {
        Ok(_) => true,
        Err(_) => {
            tracing::error!("transport.reader_reap_timeout");
            false
        }
    }
}

async fn read_envelope(reader: &mut (impl AsyncRead + Unpin)) -> std::io::Result<Envelope> {
    let mut prefix = [0u8; 4];
    reader.read_exact(&mut prefix).await?;
    let size = spm_contracts::validate_declared_length(prefix)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let mut frame = Vec::with_capacity(size + 4);
    frame.extend_from_slice(&prefix);
    frame.resize(size + 4, 0);
    reader.read_exact(&mut frame[4..]).await?;
    spm_contracts::decode_frame(&frame)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

async fn write_envelope(
    writer: &mut (impl AsyncWrite + Unpin),
    envelope: &Envelope,
) -> std::io::Result<()> {
    let frame = spm_contracts::encode_frame(envelope)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    tokio::time::timeout(Duration::from_secs(10), async {
        writer.write_all(&frame).await?;
        writer.flush().await
    })
    .await
    .map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "SPM write timed out; remote outcome unknown",
        )
    })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use spm_contracts::{DeliveryScopeId, DetailLevel, ProjectId, ViewKind};

    fn query() -> ProjectQuery {
        ProjectQuery {
            project_id: ProjectId::new("project").unwrap(),
            delivery_scope_id: DeliveryScopeId::new("scope").unwrap(),
            view: ViewKind::Summary,
            filter: None,
            detail_level: DetailLevel::Standard,
            sort: Vec::new(),
        }
    }

    fn route() -> Route {
        Route {
            owner: ScopeId(1),
            generation: 1,
        }
    }

    fn subscribe(local: Token) -> Command {
        Command::Subscribe {
            local,
            route: route(),
            query: query(),
        }
    }

    fn actor() -> Actor {
        let (_, receivers) = channel();
        Actor {
            endpoint: String::new(),
            receivers,
            connection_state: Arc::new(AtomicU8::new(0)),
            sink: Arc::new(PipeState::new(pecofence_platform::HWND(
                std::ptr::null_mut(),
            ))),
            active: HashMap::new(),
            local_queries: HashMap::new(),
            cancelled_tokens: HashSet::new(),
            remote_queries: HashMap::new(),
            pending_subscribes: HashMap::new(),
            pending_operations: HashMap::new(),
            daemon_session: None,
        }
    }

    fn action(actor: &mut Actor, token: Token, method: RpcMethod) -> RequestId {
        actor
            .sink
            .test_operation(token, route().owner, route().generation);
        let id = RequestId::new();
        actor.pending_operations.insert(
            id,
            PendingOperation {
                local: token,
                route: route(),
                method,
                deadline: Instant::now() + OPERATION_TIMEOUT,
            },
        );
        id
    }

    fn action_reply(session: DaemonSessionId, id: RequestId) -> Envelope {
        Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            daemon_session: Some(session),
            request_id: Some(id),
            subscription_id: None,
            revision: None,
            body: Body::Response(Response::Refresh(spm_contracts::RefreshResponse {
                operation_id: spm_contracts::OperationId::new(),
                disposition: spm_contracts::RefreshDisposition::Accepted,
            })),
        }
    }

    #[test]
    fn action_response_is_correlated_once_and_ignores_foreign_sessions() {
        let mut actor = actor();
        let session = DaemonSessionId::new();
        actor.daemon_session = Some(session);
        let id = action(&mut actor, Token(10), RpcMethod::Refresh);
        let reply = action_reply(session, id);
        actor.route_incoming(action_reply(DaemonSessionId::new(), id));
        actor.route_incoming(action_reply(session, RequestId::new()));
        assert_eq!(actor.pending_operations.len(), 1);
        assert!(actor.sink.test_events().is_empty());
        actor.route_incoming(reply.clone());
        actor.route_incoming(reply);
        let events = actor.sink.test_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            pecofence_plugin_api::PanelEvent::Completion {
                operation: Token(10),
                result: Ok(_)
            }
        ));
        assert!(actor.pending_operations.is_empty());
    }

    #[test]
    fn wrong_method_deadline_and_disconnect_terminalize_only_the_owned_call() {
        let mut actor = actor();
        let session = DaemonSessionId::new();
        actor.daemon_session = Some(session);
        let wrong = action(&mut actor, Token(10), RpcMethod::BuildBriefing);
        actor.route_incoming(action_reply(session, wrong));
        let expired = action(&mut actor, Token(11), RpcMethod::Refresh);
        actor.pending_operations.get_mut(&expired).unwrap().deadline = Instant::now();
        action(&mut actor, Token(12), RpcMethod::Refresh);
        actor.expire_operations(Instant::now());
        assert_eq!(actor.pending_operations.len(), 1);
        actor.fail_operations("connection lost; remote outcome unknown");
        let events = actor.sink.test_events();
        assert_eq!(events.len(), 3);
        for (event, expected) in events.iter().zip([Token(10), Token(11), Token(12)]) {
            assert!(
                matches!(event, pecofence_plugin_api::PanelEvent::Completion { operation, result: Err(_) } if *operation == expected)
            );
        }
        assert!(actor.pending_operations.is_empty());
    }

    #[test]
    fn cancelled_operations_do_not_receive_late_responses() {
        let mut actor = actor();
        let session = DaemonSessionId::new();
        actor.daemon_session = Some(session);
        let id = action(&mut actor, Token(10), RpcMethod::Refresh);
        actor.cancel_operation(Token(10));
        actor.route_incoming(action_reply(session, id));
        assert!(actor.sink.test_events().is_empty());
    }

    fn test_envelope() -> Envelope {
        Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            daemon_session: None,
            request_id: None,
            subscription_id: None,
            revision: None,
            body: Body::Request(Request::Hello(HelloRequest {
                client_build: String::new(),
                protocol_major: PROTOCOL_MAJOR,
                protocol_minor: PROTOCOL_MINOR,
                features: BTreeSet::new(),
            })),
        }
    }

    #[tokio::test]
    async fn silent_stream_reaped_within_2s() {
        let (reader, _silent_peer) = tokio::io::duplex(1024);
        let (tx, _rx) = mpsc::channel(32);
        let handle = spawn_reader(reader, tx);
        assert!(
            tokio::time::timeout(
                Duration::from_secs(2),
                reap_reader(handle, Duration::from_secs(2))
            )
            .await
            .unwrap()
        );
    }

    #[tokio::test]
    async fn eof_ends_reader() {
        let (reader, mut peer) = tokio::io::duplex(1024);
        let (tx, mut rx) = mpsc::channel(32);
        let handle = spawn_reader(reader, tx);
        write_envelope(&mut peer, &test_envelope()).await.unwrap();
        drop(peer);
        assert!(rx.recv().await.is_some());
        assert!(
            tokio::time::timeout(Duration::from_secs(2), handle)
                .await
                .unwrap()
                .is_ok()
        );
    }

    #[tokio::test]
    async fn receiver_dropped_ends_reader() {
        let (reader, mut peer) = tokio::io::duplex(1024);
        let (tx, rx) = mpsc::channel(32);
        let handle = spawn_reader(reader, tx);
        drop(rx);
        write_envelope(&mut peer, &test_envelope()).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), handle)
                .await
                .unwrap()
                .is_ok()
        );
    }

    #[tokio::test]
    async fn reap_finished_reader_is_fast() {
        let (reader, peer) = tokio::io::duplex(1024);
        let (tx, _rx) = mpsc::channel(32);
        let handle = spawn_reader(reader, tx);
        drop(peer);
        while !handle.is_finished() {
            tokio::task::yield_now().await;
        }
        assert!(
            tokio::time::timeout(
                Duration::from_millis(100),
                reap_reader(handle, Duration::from_secs(2))
            )
            .await
            .unwrap()
        );
    }

    #[test]
    fn data_capacity_and_control_bypass() {
        let (handle, mut receivers) = channel();
        for id in 1..=64 {
            assert!(handle.subscribe(Token(id), ScopeId(1), 1, query()).is_ok());
        }
        assert!(handle.subscribe(Token(65), ScopeId(1), 1, query()).is_err());
        assert!(handle.unsubscribe(Token(1)).is_ok());
        assert!(matches!(
            receivers.control.try_recv(),
            Ok(Command::Unsubscribe { local: Token(1) })
        ));
    }

    #[test]
    fn closed_receivers_reject_all_commands() {
        let (handle, receivers) = channel();
        drop(receivers);
        assert!(handle.subscribe(Token(1), ScopeId(1), 1, query()).is_err());
        assert!(handle.unsubscribe(Token(1)).is_err());
        let envelope = Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            daemon_session: None,
            request_id: None,
            subscription_id: None,
            revision: None,
            body: Body::Request(Request::Hello(HelloRequest {
                client_build: String::new(),
                protocol_major: PROTOCOL_MAJOR,
                protocol_minor: PROTOCOL_MINOR,
                features: BTreeSet::new(),
            })),
        };
        let Body::Request(request) = envelope.body else {
            unreachable!()
        };
        assert!(handle.request(Token(1), ScopeId(1), 1, request).is_err());
    }

    #[test]
    fn tombstones_resolve_cross_channel_order() {
        let mut actor = actor();
        let local = Token(7);
        actor.apply_offline(Command::Unsubscribe { local });
        assert!(actor.cancelled_tokens.contains(&local));
        actor.apply_offline(subscribe(local));
        assert!(!actor.local_queries.contains_key(&local));
        assert!(actor.active.is_empty());
        assert!(actor.cancelled_tokens.is_empty());

        actor.apply_offline(subscribe(local));
        assert!(actor.local_queries.contains_key(&local));
        assert_eq!(actor.active.len(), 1);
        actor.apply_offline(Command::Unsubscribe { local });
        assert!(actor.local_queries.is_empty());
        assert!(actor.active.is_empty());
        assert!(actor.cancelled_tokens.is_empty());
    }

    #[test]
    fn hundred_subscribe_unsubscribe_cycles_leave_no_residue() {
        let (handle, receivers) = channel();
        let mut actor = actor();
        actor.receivers = receivers;
        for id in 1..=100 {
            let local = Token(id);
            if id % 10 == 0 {
                handle.unsubscribe(local).unwrap();
                let command = actor.receivers.control.try_recv().unwrap();
                actor.apply_offline(command);
                handle.subscribe(local, ScopeId(1), 1, query()).unwrap();
                let command = actor.receivers.data.try_recv().unwrap();
                actor.apply_offline(command);
                assert!(!actor.cancelled_tokens.contains(&local));
            }
            handle.subscribe(local, ScopeId(1), 1, query()).unwrap();
            let command = actor.receivers.data.try_recv().unwrap();
            actor.apply_offline(command);
            handle.unsubscribe(local).unwrap();
            let command = actor.receivers.control.try_recv().unwrap();
            actor.apply_offline(command);
        }
        assert!(actor.local_queries.is_empty());
        assert!(actor.active.is_empty());
        assert!(actor.remote_queries.is_empty());
        assert!(actor.pending_subscribes.is_empty());
        assert!(actor.cancelled_tokens.is_empty());
    }

    #[test]
    fn tombstone_overflow_clears_and_continues() {
        let mut actor = actor();
        for id in 1..=4096 {
            actor.apply_offline(Command::Unsubscribe { local: Token(id) });
        }
        assert_eq!(actor.cancelled_tokens.len(), 4096);
        actor.apply_offline(Command::Unsubscribe { local: Token(4097) });
        assert!(actor.cancelled_tokens.is_empty());
        actor.apply_offline(Command::Unsubscribe { local: Token(4098) });
        actor.apply_offline(subscribe(Token(4098)));
        assert!(actor.local_queries.is_empty());
        actor.apply_offline(subscribe(Token(4099)));
        assert!(actor.local_queries.contains_key(&Token(4099)));
    }
}

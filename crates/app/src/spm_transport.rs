use crate::app::panel_manager::{PipeState, SnapshotStamp};
use pecofence_plugin_api::{ScopeId, Token};
use pecofence_plugin_kernel::CancellationToken;
use spm_contracts::{
    Body, DaemonSessionId, Direction, Envelope, ErrorCode, ErrorDto, Event, Feature, HelloRequest,
    PROTOCOL_MAJOR, PROTOCOL_MINOR, ProjectQuery, Request, RequestId, Response, RpcMethod,
    SubscribeRequest, SubscriptionId, UnsubscribeRequest,
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

    pub fn request(&self, local: Token, envelope: Envelope) -> Result<(), ()> {
        if self.connection_state() != ConnectionState::Connected {
            return Err(());
        }
        self.data_sender
            .try_send(Command::Request { local, envelope })
            .map_err(send_error)
    }

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
    data: mpsc::Receiver<Command>,
    control: mpsc::UnboundedReceiver<Command>,
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
        envelope: Envelope,
    },
}

struct ActiveQuery {
    routes: HashMap<Token, Route>,
    remote: Option<SubscriptionId>,
    pending: Option<RequestId>,
    latest: Option<(SnapshotStamp, Arc<[u8]>)>,
}

#[derive(Clone, Copy)]
struct PendingRequest {
    local: Token,
    route: Route,
    method: RpcMethod,
    sent_at: Instant,
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
    pending_requests: HashMap<RequestId, PendingRequest>,
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
        pending_requests: HashMap::new(),
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
                        actor.reset_remote().await;
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
                actor.apply_offline(command).await;
            }
            command = actor.receivers.data.recv() => {
                let Some(command) = command else { break };
                actor.apply_offline(command).await;
            }
        }
        delay = (delay * 2).min(Duration::from_secs(15));
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
        self.pending_requests
            .retain(|_, pending| pending.local != local);
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

    async fn reset_remote(&mut self) {
        self.remote_queries.clear();
        self.pending_subscribes.clear();
        let pending = std::mem::take(&mut self.pending_requests);
        for (request_id, request) in pending {
            self.fail_request(
                request_id,
                request,
                ErrorCode::Unavailable,
                true,
                "SPM connection ended before the request completed",
            )
            .await;
        }
        for active in self.active.values_mut() {
            active.remote = None;
            active.pending = None;
        }
    }

    async fn apply_offline(&mut self, command: Command) {
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
                    pending: None,
                    latest: None,
                });
                active.routes.insert(local, route);
                if let Some((stamp, bytes)) = &active.latest {
                    self.sink.publish_snapshot(
                        route.owner,
                        route.generation,
                        local,
                        *stamp,
                        bytes.clone(),
                    );
                }
            }
            Command::Unsubscribe { local } => {
                self.unsubscribe_local(local);
            }
            Command::Request { local, envelope } => {
                if let (Some(query), Some(request_id), Body::Request(request)) = (
                    self.local_queries.get(&local),
                    envelope.request_id,
                    &envelope.body,
                ) && let Some(route) = self
                    .active
                    .get(query)
                    .and_then(|active| active.routes.get(&local))
                    .copied()
                {
                    self.fail_request(
                        request_id,
                        PendingRequest {
                            local,
                            route,
                            method: request.method(),
                            sent_at: Instant::now(),
                        },
                        ErrorCode::Unavailable,
                        true,
                        "SPM is not connected",
                    )
                    .await;
                }
            }
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
            let session_changed = self.daemon_session != Some(response.daemon_session);
            self.daemon_session = Some(response.daemon_session);
            self.reset_remote().await;
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
            let mut retry = tokio::time::interval(Duration::from_secs(5));
            retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => return Ok(()),
                    _ = retry.tick() => {
                        self.expire_pending_requests().await;
                        self.retry_unbound(&mut writer).await?;
                    },
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
                        let orphan = self.route_incoming(envelope).await;
                        if let Some(orphan) = orphan {
                            let request = self.request_envelope(
                                RequestId::new(),
                                Request::Unsubscribe(UnsubscribeRequest {
                                    subscription_id: orphan,
                                }),
                            )?;
                            write_envelope(&mut writer, &request).await.map_err(|_| ())?;
                        }
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
        self.pending_subscribes.insert(request_id, query.clone());
        if let Some(active) = self.active.get_mut(&query) {
            active.pending = Some(request_id);
        }
        Ok(())
    }

    async fn retry_unbound(&mut self, writer: &mut (impl AsyncWrite + Unpin)) -> Result<(), ()> {
        let queries: Vec<_> = self
            .active
            .iter()
            .filter(|(_, active)| active.remote.is_none() && active.pending.is_none())
            .map(|(query, _)| query.clone())
            .collect();
        for query in queries {
            self.send_subscribe(writer, query).await?;
        }
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
                    if let Some((stamp, bytes)) = &active.latest {
                        self.sink.publish_snapshot(
                            route.owner,
                            route.generation,
                            local,
                            *stamp,
                            bytes.clone(),
                        );
                    }
                } else {
                    self.active.insert(
                        query.clone(),
                        ActiveQuery {
                            routes: HashMap::from([(local, route)]),
                            remote: None,
                            pending: None,
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
            Command::Request { local, envelope } => {
                let Some(query) = self.local_queries.get(&local) else {
                    return Ok(());
                };
                let Some(route) = self
                    .active
                    .get(query)
                    .and_then(|active| active.routes.get(&local))
                    .copied()
                else {
                    return Ok(());
                };
                let (Some(request_id), Body::Request(request)) =
                    (envelope.request_id, &envelope.body)
                else {
                    return Err(());
                };
                let method = request.method();
                let pending = PendingRequest {
                    local,
                    route,
                    method,
                    sent_at: Instant::now(),
                };
                if envelope.daemon_session != self.daemon_session {
                    self.fail_request(
                        request_id,
                        pending,
                        ErrorCode::Unavailable,
                        true,
                        "SPM session changed before the request was sent",
                    )
                    .await;
                    return Ok(());
                }
                envelope
                    .validate(Direction::ClientToServer)
                    .map_err(|_| ())?;
                self.expire_pending_requests().await;
                if self.pending_requests.contains_key(&request_id) {
                    return Err(());
                }
                if self.pending_requests.len() >= 256 {
                    self.fail_request(
                        request_id,
                        pending,
                        ErrorCode::Backpressure,
                        true,
                        "too many pending SPM requests",
                    )
                    .await;
                    return Ok(());
                }
                if write_envelope(writer, &envelope).await.is_err() {
                    self.fail_request(
                        request_id,
                        pending,
                        ErrorCode::Unavailable,
                        true,
                        "SPM connection failed while sending the request",
                    )
                    .await;
                    return Err(());
                }
                self.pending_requests.insert(request_id, pending);
            }
        }
        Ok(())
    }

    async fn expire_pending_requests(&mut self) {
        let expired: Vec<_> = self
            .pending_requests
            .iter()
            .filter(|(_, pending)| pending.sent_at.elapsed() >= Duration::from_secs(30))
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            if let Some(pending) = self.pending_requests.remove(&id) {
                self.fail_request(
                    id,
                    pending,
                    ErrorCode::DeadlineExceeded,
                    true,
                    "SPM request timed out",
                )
                .await;
            }
        }
    }

    async fn fail_request(
        &self,
        request_id: RequestId,
        pending: PendingRequest,
        code: ErrorCode,
        retryable: bool,
        message: &str,
    ) {
        let envelope = Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            daemon_session: self.daemon_session,
            request_id: Some(request_id),
            subscription_id: None,
            revision: None,
            body: Body::Error(ErrorDto {
                code,
                request_id: Some(request_id),
                retryable,
                message: message.into(),
                details: None,
            }),
        };
        if let Ok(bytes) = serde_json::to_vec(&envelope).map(Arc::<[u8]>::from)
            && let Err(error) = self
                .sink
                .publish_reliable(
                    pending.route.owner,
                    pending.route.generation,
                    pending.local,
                    bytes,
                )
                .await
        {
            tracing::error!(%error, "transport.local_error_delivery_failed");
        }
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

    /// Return a remote subscription that was acknowledged after its local owner left.
    async fn route_incoming(&mut self, envelope: Envelope) -> Option<SubscriptionId> {
        if let Body::Response(Response::Subscribe(response)) = &envelope.body {
            if let Some(request_id) = envelope.request_id {
                tracing::info!(request_id = ?request_id, subscription_id = ?response.subscription_id, "subscribe.acknowledged");
                if let Some(query) = self.pending_subscribes.remove(&request_id) {
                    if let Some(active) = self.active.get_mut(&query) {
                        if active.pending == Some(request_id) {
                            active.pending = None;
                            active.remote = Some(response.subscription_id);
                            self.remote_queries.insert(response.subscription_id, query);
                            return None;
                        }
                    }
                    return Some(response.subscription_id);
                }
            }
            return None;
        }
        if let Body::Error(error) = &envelope.body {
            if let Some(request_id) = envelope.request_id {
                if let Some(query) = self.pending_subscribes.remove(&request_id) {
                    tracing::warn!(code = ?error.code, query = ?query, "subscribe.rejected");
                    if let Some(active) = self.active.get_mut(&query) {
                        if active.pending == Some(request_id)
                            && (error.code == ErrorCode::UnknownScope || error.retryable)
                        {
                            active.pending = None;
                        }
                    }
                    return None;
                }
            }
        }
        if matches!(&envelope.body, Body::Response(_) | Body::Error(_)) {
            let Some(request_id) = envelope.request_id else {
                return None;
            };
            let Some(pending) = self.pending_requests.get(&request_id).copied() else {
                return None;
            };
            if envelope.daemon_session != self.daemon_session
                || !self.local_queries.contains_key(&pending.local)
                || !self.active.values().any(|active| {
                    active.routes.get(&pending.local).is_some_and(|route| {
                        route.owner == pending.route.owner
                            && route.generation == pending.route.generation
                    })
                })
            {
                return None;
            }
            let matches_method = match &envelope.body {
                Body::Response(Response::Refresh(_)) => pending.method == RpcMethod::Refresh,
                Body::Response(Response::ResolveNavigation(_)) => {
                    pending.method == RpcMethod::ResolveNavigation
                }
                Body::Response(Response::BuildBriefing(_)) => {
                    pending.method == RpcMethod::BuildBriefing
                }
                Body::Error(_) => true,
                _ => false,
            };
            if matches_method {
                if let Ok(bytes) = serde_json::to_vec(&envelope).map(Arc::<[u8]>::from) {
                    self.pending_requests.remove(&request_id);
                    if let Err(error) = self
                        .sink
                        .publish_reliable(
                            pending.route.owner,
                            pending.route.generation,
                            pending.local,
                            bytes,
                        )
                        .await
                    {
                        tracing::error!(%error, "transport.response_delivery_failed");
                    }
                }
            }
            return None;
        }
        if matches!(&envelope.body, Body::Event(Event::OperationCompleted(_))) {
            if let Ok(bytes) = serde_json::to_vec(&envelope).map(Arc::<[u8]>::from) {
                let routes: Vec<_> = self
                    .active
                    .values()
                    .flat_map(|active| active.routes.iter().map(|(local, route)| (*local, *route)))
                    .collect();
                for (local, route) in routes {
                    if let Err(error) = self
                        .sink
                        .publish_reliable(route.owner, route.generation, local, bytes.clone())
                        .await
                    {
                        tracing::error!(%error, "transport.operation_delivery_failed");
                    }
                }
            }
            return None;
        }
        let Body::Event(Event::Snapshot(_)) = &envelope.body else {
            return None;
        };
        let (Some(session), Some(revision)) = (envelope.daemon_session, envelope.revision) else {
            return None;
        };
        let stamp = SnapshotStamp { session, revision };
        let Some(remote) = envelope.subscription_id else {
            return None;
        };
        let Some(query) = self.remote_queries.get(&remote).cloned() else {
            return None;
        };
        let Ok(bytes) = serde_json::to_vec(&envelope).map(Arc::<[u8]>::from) else {
            return None;
        };
        tracing::info!(
            revision = envelope.revision.map(|r| r.0).unwrap_or(0),
            query = ?query,
            "snapshot.received"
        );
        if let Some(active) = self.active.get_mut(&query) {
            if active.latest.as_ref().is_some_and(|(old, _)| {
                old.session == stamp.session && old.revision >= stamp.revision
            }) {
                return None;
            }
            active.latest = Some((stamp, bytes.clone()));
            for (local, route) in &active.routes {
                self.sink.publish_snapshot(
                    route.owner,
                    route.generation,
                    *local,
                    stamp,
                    bytes.clone(),
                );
            }
        }
        None
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
    writer.write_all(&frame).await?;
    writer.flush().await
}

/// A candidate binary can verify its own pinned SPM contract before replacing the desktop UI.
pub async fn probe_current() -> std::io::Result<()> {
    let endpoint = spm_contracts::v2_pipe_name(&spm_contracts::EndpointIdentity {
        user_sid: pecofence_platform::named_pipe::current_user_sid()?,
        windows_session_id: pecofence_platform::named_pipe::current_windows_session_id()?,
        instance: pecofence_core::brand::var("PECOFENCE_INSTANCE").ok(),
    })
    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut pipe = tokio::net::windows::named_pipe::ClientOptions::new().open(&endpoint)?;
        let request_id = RequestId::new();
        let required = BTreeSet::from([
            Feature::Multiplexing,
            Feature::Pagination,
            Feature::Navigation,
            Feature::Briefing,
        ]);
        write_envelope(
            &mut pipe,
            &Envelope {
                protocol_major: PROTOCOL_MAJOR,
                protocol_minor: PROTOCOL_MINOR,
                daemon_session: None,
                request_id: Some(request_id),
                subscription_id: None,
                revision: None,
                body: Body::Request(Request::Hello(HelloRequest {
                    client_build: env!("CARGO_PKG_VERSION").into(),
                    protocol_major: PROTOCOL_MAJOR,
                    protocol_minor: PROTOCOL_MINOR,
                    features: required.clone(),
                })),
            },
        )
        .await?;
        let reply = read_envelope(&mut pipe).await?;
        reply
            .validate(Direction::ServerToClient)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        if reply.request_id != Some(request_id) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "SPM probe request id mismatch",
            ));
        }
        match reply.body {
            Body::Response(Response::Hello(hello)) if required.is_subset(&hello.features) => Ok(()),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "SPM daemon rejected the PecoFence contract or required features",
            )),
        }
    })
    .await
    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "SPM probe timed out"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use spm_contracts::{DeliveryScopeId, DetailLevel, ProjectId, SubscribeResponse, ViewKind};

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
            pending_requests: HashMap::new(),
            daemon_session: None,
        }
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
    async fn late_ack_does_not_attach_to_a_recreated_subscription() {
        let mut actor = actor();
        actor.daemon_session = Some(DaemonSessionId::new());
        let (mut writer, mut reader) = tokio::io::duplex(4096);
        actor
            .apply_connected(subscribe(Token(1)), &mut writer)
            .await
            .unwrap();
        let first = read_envelope(&mut reader).await.unwrap();
        actor
            .apply_connected(Command::Unsubscribe { local: Token(1) }, &mut writer)
            .await
            .unwrap();
        actor
            .apply_connected(subscribe(Token(2)), &mut writer)
            .await
            .unwrap();
        let second = read_envelope(&mut reader).await.unwrap();
        let session = actor.daemon_session;
        let ack = |request_id, subscription_id| Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            daemon_session: session,
            request_id,
            subscription_id: None,
            revision: None,
            body: Body::Response(Response::Subscribe(SubscribeResponse {
                subscription_id,
                accepted_query: query(),
                initial_revision: spm_contracts::Revision(1),
            })),
        };
        let old_remote = SubscriptionId::new();
        let new_remote = SubscriptionId::new();
        assert_eq!(
            actor
                .route_incoming(ack(first.request_id, old_remote))
                .await,
            Some(old_remote)
        );
        assert!(!actor.remote_queries.contains_key(&old_remote));
        assert_eq!(
            actor
                .route_incoming(ack(second.request_id, new_remote))
                .await,
            None
        );
        assert_eq!(actor.active.get(&query()).unwrap().remote, Some(new_remote));
        assert_eq!(actor.remote_queries.len(), 1);
    }

    #[tokio::test]
    async fn unknown_scope_at_startup_retries_after_first_snapshot_is_committed() {
        let mut actor = actor();
        let session = DaemonSessionId::new();
        actor.daemon_session = Some(session);
        let (mut writer, mut reader) = tokio::io::duplex(4096);
        actor
            .apply_connected(subscribe(Token(1)), &mut writer)
            .await
            .unwrap();
        let first = read_envelope(&mut reader).await.unwrap();
        actor
            .route_incoming(Envelope {
                protocol_major: PROTOCOL_MAJOR,
                protocol_minor: PROTOCOL_MINOR,
                daemon_session: Some(session),
                request_id: first.request_id,
                subscription_id: None,
                revision: None,
                body: Body::Error(spm_contracts::ErrorDto {
                    code: ErrorCode::UnknownScope,
                    request_id: first.request_id,
                    retryable: false,
                    message: "snapshot not committed yet".into(),
                    details: None,
                }),
            })
            .await;
        assert!(actor.active.get(&query()).unwrap().pending.is_none());
        actor.retry_unbound(&mut writer).await.unwrap();
        let second = read_envelope(&mut reader).await.unwrap();
        assert_ne!(second.request_id, first.request_id);
        actor
            .route_incoming(Envelope {
                protocol_major: PROTOCOL_MAJOR,
                protocol_minor: PROTOCOL_MINOR,
                daemon_session: Some(session),
                request_id: second.request_id,
                subscription_id: None,
                revision: None,
                body: Body::Response(Response::Subscribe(SubscribeResponse {
                    subscription_id: SubscriptionId::new(),
                    accepted_query: query(),
                    initial_revision: spm_contracts::Revision(1),
                })),
            })
            .await;
        assert!(actor.active.get(&query()).unwrap().remote.is_some());
        actor.retry_unbound(&mut writer).await.unwrap();
        assert!(actor.pending_subscribes.is_empty());
    }

    #[tokio::test]
    async fn action_response_returns_only_to_its_live_request_owner() {
        use spm_contracts::{
            IdempotencyKey, OperationId, RefreshDisposition, RefreshRequest, RefreshResponse,
        };
        let mut actor = actor();
        let session = DaemonSessionId::new();
        actor.daemon_session = Some(session);
        let (mut writer, mut reader) = tokio::io::duplex(4096);
        actor
            .apply_connected(subscribe(Token(1)), &mut writer)
            .await
            .unwrap();
        read_envelope(&mut reader).await.unwrap();
        let request_id = RequestId::new();
        let request = Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            daemon_session: Some(session),
            request_id: Some(request_id),
            subscription_id: None,
            revision: None,
            body: Body::Request(Request::Refresh(RefreshRequest {
                project_id: query().project_id,
                delivery_scope_id: query().delivery_scope_id,
                idempotency_key: IdempotencyKey::new(),
            })),
        };
        actor
            .apply_connected(
                Command::Request {
                    local: Token(1),
                    envelope: request,
                },
                &mut writer,
            )
            .await
            .unwrap();
        assert_eq!(
            read_envelope(&mut reader).await.unwrap().request_id,
            Some(request_id)
        );
        let response = Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            daemon_session: Some(session),
            request_id: Some(request_id),
            subscription_id: None,
            revision: None,
            body: Body::Response(Response::Refresh(RefreshResponse {
                operation_id: OperationId::new(),
                disposition: RefreshDisposition::Accepted,
            })),
        };
        actor.route_incoming(response.clone()).await;
        let events = actor.sink.drain_for_test();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, Token(1));
        let delivered: Envelope = serde_json::from_slice(&events[0].1).unwrap();
        assert_eq!(delivered.request_id, Some(request_id));
        actor.route_incoming(response.clone()).await;
        assert!(actor.sink.drain_for_test().is_empty());

        let mismatched = RequestId::new();
        actor.pending_requests.insert(
            mismatched,
            PendingRequest {
                local: Token(1),
                route: route(),
                method: RpcMethod::ResolveNavigation,
                sent_at: Instant::now(),
            },
        );
        let mut wrong_method = response;
        wrong_method.request_id = Some(mismatched);
        actor.route_incoming(wrong_method).await;
        assert!(actor.pending_requests.contains_key(&mismatched));
        assert!(actor.sink.drain_for_test().is_empty());

        let request_id = RequestId::new();
        actor.pending_requests.insert(
            request_id,
            PendingRequest {
                local: Token(1),
                route: route(),
                method: RpcMethod::Refresh,
                sent_at: Instant::now(),
            },
        );
        actor.unsubscribe_local(Token(1));
        assert!(!actor.pending_requests.contains_key(&request_id));
    }

    #[tokio::test]
    async fn pending_limit_rejects_new_request_without_evicting_old_owners() {
        use spm_contracts::{IdempotencyKey, RefreshRequest};
        let mut actor = actor();
        let session = DaemonSessionId::new();
        actor.daemon_session = Some(session);
        let (mut writer, mut reader) = tokio::io::duplex(4096);
        actor
            .apply_connected(subscribe(Token(1)), &mut writer)
            .await
            .unwrap();
        read_envelope(&mut reader).await.unwrap();
        let oldest = RequestId::new();
        actor.pending_requests.insert(
            oldest,
            PendingRequest {
                local: Token(1),
                route: route(),
                method: RpcMethod::Refresh,
                sent_at: Instant::now(),
            },
        );
        for _ in 1..256 {
            actor.pending_requests.insert(
                RequestId::new(),
                PendingRequest {
                    local: Token(1),
                    route: route(),
                    method: RpcMethod::Refresh,
                    sent_at: Instant::now(),
                },
            );
        }
        let rejected = RequestId::new();
        let request = Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            daemon_session: Some(session),
            request_id: Some(rejected),
            subscription_id: None,
            revision: None,
            body: Body::Request(Request::Refresh(RefreshRequest {
                project_id: query().project_id,
                delivery_scope_id: query().delivery_scope_id,
                idempotency_key: IdempotencyKey::new(),
            })),
        };
        actor
            .apply_connected(
                Command::Request {
                    local: Token(1),
                    envelope: request,
                },
                &mut writer,
            )
            .await
            .unwrap();
        assert_eq!(actor.pending_requests.len(), 256);
        assert!(actor.pending_requests.contains_key(&oldest));
        assert!(!actor.pending_requests.contains_key(&rejected));
        let events = actor.sink.drain_for_test();
        assert_eq!(events.len(), 1);
        let response: Envelope = serde_json::from_slice(&events[0].1).unwrap();
        assert_eq!(response.request_id, Some(rejected));
        assert!(matches!(
            response.body,
            Body::Error(ErrorDto {
                code: ErrorCode::Backpressure,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn expired_and_disconnected_requests_report_failures() {
        let mut actor = actor();
        actor.daemon_session = Some(DaemonSessionId::new());
        let expired = RequestId::new();
        actor.pending_requests.insert(
            expired,
            PendingRequest {
                local: Token(1),
                route: route(),
                method: RpcMethod::Refresh,
                sent_at: Instant::now() - Duration::from_secs(31),
            },
        );
        actor.expire_pending_requests().await;
        let events = actor.sink.drain_for_test();
        assert_eq!(events.len(), 1);
        let timeout: Envelope = serde_json::from_slice(&events[0].1).unwrap();
        assert_eq!(timeout.request_id, Some(expired));
        assert!(matches!(
            timeout.body,
            Body::Error(ErrorDto {
                code: ErrorCode::DeadlineExceeded,
                ..
            })
        ));

        let disconnected = RequestId::new();
        actor.pending_requests.insert(
            disconnected,
            PendingRequest {
                local: Token(1),
                route: route(),
                method: RpcMethod::Refresh,
                sent_at: Instant::now(),
            },
        );
        actor.reset_remote().await;
        let events = actor.sink.drain_for_test();
        assert_eq!(events.len(), 1);
        let unavailable: Envelope = serde_json::from_slice(&events[0].1).unwrap();
        assert_eq!(unavailable.request_id, Some(disconnected));
        assert!(matches!(
            unavailable.body,
            Body::Error(ErrorDto {
                code: ErrorCode::Unavailable,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn queued_request_reaching_offline_actor_gets_unavailable() {
        use spm_contracts::{IdempotencyKey, RefreshRequest};
        let mut actor = actor();
        actor.daemon_session = Some(DaemonSessionId::new());
        actor.apply_offline(subscribe(Token(1))).await;
        let request_id = RequestId::new();
        let session = actor.daemon_session;
        actor
            .apply_offline(Command::Request {
                local: Token(1),
                envelope: Envelope {
                    protocol_major: PROTOCOL_MAJOR,
                    protocol_minor: PROTOCOL_MINOR,
                    daemon_session: session,
                    request_id: Some(request_id),
                    subscription_id: None,
                    revision: None,
                    body: Body::Request(Request::Refresh(RefreshRequest {
                        project_id: query().project_id,
                        delivery_scope_id: query().delivery_scope_id,
                        idempotency_key: IdempotencyKey::new(),
                    })),
                },
            })
            .await;
        let events = actor.sink.drain_for_test();
        assert_eq!(events.len(), 1);
        let response: Envelope = serde_json::from_slice(&events[0].1).unwrap();
        assert_eq!(response.request_id, Some(request_id));
        assert!(matches!(
            response.body,
            Body::Error(ErrorDto {
                code: ErrorCode::Unavailable,
                ..
            })
        ));
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
        assert!(handle.request(Token(1), envelope).is_err());
    }

    #[tokio::test]
    async fn tombstones_resolve_cross_channel_order() {
        let mut actor = actor();
        let local = Token(7);
        actor.apply_offline(Command::Unsubscribe { local }).await;
        assert!(actor.cancelled_tokens.contains(&local));
        actor.apply_offline(subscribe(local)).await;
        assert!(!actor.local_queries.contains_key(&local));
        assert!(actor.active.is_empty());
        assert!(actor.cancelled_tokens.is_empty());

        actor.apply_offline(subscribe(local)).await;
        assert!(actor.local_queries.contains_key(&local));
        assert_eq!(actor.active.len(), 1);
        actor.apply_offline(Command::Unsubscribe { local }).await;
        assert!(actor.local_queries.is_empty());
        assert!(actor.active.is_empty());
        assert!(actor.cancelled_tokens.is_empty());
    }

    #[tokio::test]
    async fn hundred_subscribe_unsubscribe_cycles_leave_no_residue() {
        let (handle, receivers) = channel();
        let mut actor = actor();
        actor.receivers = receivers;
        for id in 1..=100 {
            let local = Token(id);
            if id % 10 == 0 {
                handle.unsubscribe(local).unwrap();
                let command = actor.receivers.control.try_recv().unwrap();
                actor.apply_offline(command).await;
                handle.subscribe(local, ScopeId(1), 1, query()).unwrap();
                let command = actor.receivers.data.try_recv().unwrap();
                actor.apply_offline(command).await;
                assert!(!actor.cancelled_tokens.contains(&local));
            }
            handle.subscribe(local, ScopeId(1), 1, query()).unwrap();
            let command = actor.receivers.data.try_recv().unwrap();
            actor.apply_offline(command).await;
            handle.unsubscribe(local).unwrap();
            let command = actor.receivers.control.try_recv().unwrap();
            actor.apply_offline(command).await;
        }
        assert!(actor.local_queries.is_empty());
        assert!(actor.active.is_empty());
        assert!(actor.remote_queries.is_empty());
        assert!(actor.pending_subscribes.is_empty());
        assert!(actor.cancelled_tokens.is_empty());
    }

    #[tokio::test]
    async fn tombstone_overflow_clears_and_continues() {
        let mut actor = actor();
        for id in 1..=4096 {
            actor
                .apply_offline(Command::Unsubscribe { local: Token(id) })
                .await;
        }
        assert_eq!(actor.cancelled_tokens.len(), 4096);
        actor
            .apply_offline(Command::Unsubscribe { local: Token(4097) })
            .await;
        assert!(actor.cancelled_tokens.is_empty());
        actor
            .apply_offline(Command::Unsubscribe { local: Token(4098) })
            .await;
        actor.apply_offline(subscribe(Token(4098))).await;
        assert!(actor.local_queries.is_empty());
        actor.apply_offline(subscribe(Token(4099))).await;
        assert!(actor.local_queries.contains_key(&Token(4099)));
    }
}

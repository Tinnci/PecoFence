use crate::app::panel_manager::PipeState;
use pecofence_plugin_api::{ScopeId, Token};
use pecofence_plugin_kernel::CancellationToken;
use spm_contracts::{
    Body, DaemonSessionId, Direction, Envelope, Event, Feature, HelloRequest, PROTOCOL_MAJOR,
    PROTOCOL_MINOR, ProjectQuery, Request, RequestId, Response, SubscribeRequest, SubscriptionId,
    UnsubscribeRequest,
};
use std::{
    collections::{BTreeSet, HashMap},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

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
    sender: mpsc::UnboundedSender<Command>,
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
        self.sender
            .send(Command::Subscribe {
                local,
                route: Route { owner, generation },
                query,
            })
            .map_err(|_| ())
    }

    pub fn unsubscribe(&self, local: Token) -> Result<(), ()> {
        self.sender
            .send(Command::Unsubscribe { local })
            .map_err(|_| ())
    }

    pub fn request(&self, local: Token, envelope: Envelope) -> Result<(), ()> {
        self.sender
            .send(Command::Request { local, envelope })
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

pub fn channel() -> (TransportHandle, mpsc::UnboundedReceiver<Command>) {
    let (sender, receiver) = mpsc::unbounded_channel();
    let state = Arc::new(AtomicU8::new(ConnectionState::Disconnected as u8));
    (TransportHandle { sender, state }, receiver)
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
    latest: Option<Arc<[u8]>>,
}

struct Actor {
    endpoint: String,
    receiver: mpsc::UnboundedReceiver<Command>,
    connection_state: Arc<AtomicU8>,
    sink: Arc<PipeState>,
    active: HashMap<ProjectQuery, ActiveQuery>,
    local_queries: HashMap<Token, ProjectQuery>,
    remote_queries: HashMap<SubscriptionId, ProjectQuery>,
    pending_subscribes: HashMap<RequestId, ProjectQuery>,
    daemon_session: Option<DaemonSessionId>,
}

pub async fn run(
    endpoint: String,
    receiver: mpsc::UnboundedReceiver<Command>,
    handle: TransportHandle,
    sink: Arc<PipeState>,
    cancel: CancellationToken,
) {
    let endpoint_for_log = endpoint.clone();
    let mut actor = Actor {
        endpoint,
        receiver,
        connection_state: handle.state,
        sink,
        active: HashMap::new(),
        local_queries: HashMap::new(),
        remote_queries: HashMap::new(),
        pending_subscribes: HashMap::new(),
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
            command = actor.receiver.recv() => {
                let Some(command) = command else { break };
                actor.apply_offline(command);
            }
        }
        delay = (delay * 2).min(Duration::from_secs(15));
    }
    actor.set_state(ConnectionState::Disconnected);
}

impl Actor {
    fn set_state(&self, state: ConnectionState) {
        self.connection_state.store(state as u8, Ordering::Release);
    }

    fn reset_remote(&mut self) {
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
                self.remove_local(local);
            }
            Command::Request { .. } => {}
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
        let (mut reader, mut writer) = tokio::io::split(pipe);
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
                        Feature::RefreshEvents,
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
        let _reader_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    incoming = read_envelope(&mut reader) => {
                        match incoming {
                            Ok(envelope) => {
                                if envelope_tx.send(envelope).await.is_err() {
                                    break;
                                }
                            }
                            // Reader error or EOF: drop the sender so the actor
                            // observes the closed connection.
                            Err(_) => break,
                        }
                    }
                }
            }
        });
        let hello = match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            envelope_rx.recv(),
        )
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
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                command = self.receiver.recv() => {
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
                if let Some(remote) = self.remove_local(local) {
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
                mut envelope,
            } => {
                if self.local_queries.contains_key(&local) {
                    envelope.daemon_session = self.daemon_session;
                    envelope.request_id = Some(RequestId::new());
                    envelope
                        .validate(Direction::ClientToServer)
                        .map_err(|_| ())?;
                    write_envelope(writer, &envelope).await.map_err(|_| ())?;
                }
            }
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
        if let Body::Response(Response::Subscribe(response)) = &envelope.body {
            if let Some(request_id) = envelope.request_id {
                tracing::info!(request_id = ?request_id, subscription_id = ?response.subscription_id, "subscribe.acknowledged");
                if let Some(query) = self.pending_subscribes.remove(&request_id) {
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
            if let Ok(bytes) = serde_json::to_vec(&envelope).map(Arc::<[u8]>::from) {
                for active in self.active.values() {
                    for (local, route) in &active.routes {
                        self.sink
                            .publish(route.owner, route.generation, *local, bytes.clone());
                    }
                }
            }
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
}

fn jitter(base: Duration) -> Duration {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let percent = 80 + (nanos % 41) as u64;
    base.mul_f64(percent as f64 / 100.0)
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

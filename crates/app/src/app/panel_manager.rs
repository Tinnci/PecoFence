use crate::fence_window::PanelHandle;
use pecofence_core::PanelSpec;
use pecofence_plugin_api::*;
use pecofence_plugin_kernel::{ScopeKind, ScopeTree, ServiceKey, ServiceRegistry, TaskSupervisor};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

const RENDER: ServiceKey<dyn RenderService> = ServiceKey::new("host", "render", 1);
const THEME: ServiceKey<dyn ThemeService> = ServiceKey::new("host", "theme", 1);
const DESKTOP: ServiceKey<dyn DesktopService> = ServiceKey::new("host", "desktop", 1);
const IPC: ServiceKey<dyn IpcService> = ServiceKey::new("host", "ipc", 1);
const STORAGE: ServiceKey<dyn StorageService> = ServiceKey::new("host", "storage", 1);
const NAVIGATION: ServiceKey<dyn NavigationService> = ServiceKey::new("host", "navigation", 1);
const CLIPBOARD: ServiceKey<dyn ClipboardService> = ServiceKey::new("host", "clipboard", 1);

struct InstanceRecord {
    provider: String,
    config: serde_json::Value,
    activation: u64,
    scope: ScopeId,
    handle: PanelHandle,
    drain_started: Option<Instant>,
}

/// Host composition root for provider descriptors, runtime instances, and mount generations.
pub(crate) struct PanelManager {
    providers: HashMap<String, Rc<dyn PanelProvider>>,
    instances: HashMap<u128, InstanceRecord>,
    scopes: ScopeTree,
    services: ServiceRegistry,
    next_activation: u64,
    _runtime: Arc<tokio::runtime::Runtime>,
    supervisor: Rc<RefCell<TaskSupervisor>>,
    ipc_state: Arc<PipeState>,
}

impl PanelManager {
    pub fn new(notify_hwnd: pecofence_platform::HWND) -> Self {
        let mut scopes = ScopeTree::new();
        let root = scopes.root();
        let service_scope = scopes
            .create(root, ScopeKind::Service)
            .expect("root is open");
        let runtime = Arc::new(tokio::runtime::Runtime::new().expect("Tokio runtime"));
        let supervisor = Rc::new(RefCell::new(TaskSupervisor::new()));
        let ipc_state = Arc::new(PipeState::new(notify_hwnd));
        let mut services = ServiceRegistry::new();
        services
            .publish(RENDER, service_scope, Box::new(HostRender))
            .expect("unique service");
        services
            .publish(THEME, service_scope, Box::new(HostTheme))
            .expect("unique service");
        services
            .publish(DESKTOP, service_scope, Box::new(HostDesktop))
            .expect("unique service");
        services
            .publish(
                IPC,
                service_scope,
                Box::new(PipeIpc {
                    runtime: runtime.clone(),
                    supervisor: supervisor.clone(),
                    state: ipc_state.clone(),
                }),
            )
            .expect("unique service");
        services
            .publish(STORAGE, service_scope, Box::new(MemoryStorage::default()))
            .expect("unique service");
        services
            .publish(NAVIGATION, service_scope, Box::new(HostNavigation))
            .expect("unique service");
        services
            .publish(CLIPBOARD, service_scope, Box::new(HostClipboard))
            .expect("unique service");
        Self {
            providers: HashMap::new(),
            instances: HashMap::new(),
            scopes,
            services,
            next_activation: 1,
            _runtime: runtime,
            supervisor,
            ipc_state,
        }
    }

    pub fn register_provider(&mut self, provider: Rc<dyn PanelProvider>) -> Result<()> {
        let id = provider.descriptor().id.to_string();
        if self.providers.insert(id, provider).is_some() {
            return Err(Error::Duplicate);
        }
        Ok(())
    }

    pub fn resolve(&mut self, spec: &PanelSpec) -> Result<PanelHandle> {
        let id = spec.instance_id.as_u128();
        if let Some(record) = self.instances.get(&id)
            && record.provider == spec.provider
            && record.config == spec.config
            && record.drain_started.is_none()
        {
            debug_assert_eq!(record.activation, record.handle.key().activation);
            return Ok(record.handle.clone());
        }
        if let Some(old) = self.instances.get_mut(&id) {
            if let Some(started) = old.drain_started {
                let report = self.supervisor.borrow_mut().poll_scope(old.scope);
                if report.pending == 0 && report.native_pending == 0 {
                    self.scopes.finish_dispose(old.scope)?;
                    self.instances.remove(&id);
                } else if started.elapsed() >= Duration::from_secs(5) {
                    self.scopes.local(old.scope)?.quarantine();
                    return Err(Error::Backend(
                        "previous panel activation exceeded the 5 second drain deadline".into(),
                    ));
                } else {
                    return Err(Error::Backend(
                        "previous panel activation is draining".into(),
                    ));
                }
            } else {
                old.handle.stop(StopReason::Reconfigured);
                self.scopes.begin_stop(old.scope)?;
                self.supervisor.borrow().cancel_scope(old.scope);
                old.drain_started = Some(Instant::now());
                return Err(Error::Backend(
                    "previous panel activation entered drain".into(),
                ));
            }
        }
        let provider =
            self.providers.get(&spec.provider).cloned().ok_or_else(|| {
                Error::Invalid(format!("unknown panel provider {}", spec.provider))
            })?;
        let plugin_scope = self.scopes.create(self.scopes.root(), ScopeKind::Plugin)?;
        let instance_scope = self.scopes.create(plugin_scope, ScopeKind::Instance)?;
        let activation = self.next_activation;
        self.next_activation = self
            .next_activation
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        let scope = self.scopes.handle(instance_scope)?;
        let context = PluginContext {
            scope: scope.clone(),
            render: self
                .services
                .resolve(RENDER, instance_scope, scope.clone())?,
            theme: self
                .services
                .resolve(THEME, instance_scope, scope.clone())?,
            desktop: self
                .services
                .resolve(DESKTOP, instance_scope, scope.clone())?,
            ipc: self.services.resolve(IPC, instance_scope, scope.clone())?,
            storage: self
                .services
                .resolve(STORAGE, instance_scope, scope.clone())?,
            navigation: Some(
                self.services
                    .resolve(NAVIGATION, instance_scope, scope.clone())?,
            ),
            clipboard: Some(
                self.services
                    .resolve(CLIPBOARD, instance_scope, scope.clone())?,
            ),
        };
        let bytes = Arc::from(
            serde_json::to_vec(&spec.config).map_err(|error| Error::Invalid(error.to_string()))?,
        );
        let key = InstanceKey { id, activation };
        let mut panel = provider.create(
            context,
            CreatePanel {
                key,
                config: PanelConfig {
                    version: spec.config_version,
                    bytes,
                },
            },
        )?;
        panel.mount(MountContext {
            key: MountKey {
                instance: key,
                generation: 1,
            },
            scope,
            viewport: RectDip::default(),
            dpi: 96,
        })?;
        let handle = PanelHandle::new(key, panel);
        self.instances.insert(
            id,
            InstanceRecord {
                provider: spec.provider.clone(),
                config: spec.config.clone(),
                activation,
                scope: instance_scope,
                handle: handle.clone(),
                drain_started: None,
            },
        );
        Ok(handle)
    }

    pub fn shutdown(&mut self) {
        let ids: Vec<u128> = self.instances.keys().copied().collect();
        for id in ids {
            if let Some(mut record) = self.instances.remove(&id) {
                record.handle.stop(StopReason::Shutdown);
                let _ = self.scopes.stop_and_drain(
                    record.scope,
                    &mut self.supervisor.borrow_mut(),
                    Instant::now() + Duration::from_secs(5),
                );
            }
        }
    }

    pub fn poll_events(&mut self) -> usize {
        let events: Vec<_> = self
            .ipc_state
            .events
            .lock()
            .map(|mut events| events.drain(..).collect())
            .unwrap_or_default();
        let mut delivered = 0;
        for event in events {
            let Some(record) = self
                .instances
                .values()
                .find(|record| record.scope == event.scope)
            else {
                continue;
            };
            let current_generation = self
                .scopes
                .handle(record.scope)
                .and_then(|scope| scope.generation());
            if current_generation != Ok(event.generation) {
                continue;
            }
            match record.handle.event(PanelEvent::Snapshot {
                subscription: event.subscription,
                bytes: event.bytes,
            }) {
                Ok(_) => delivered += 1,
                Err(error) => tracing::warn!(%error, "plugin IPC event rejected"),
            }
        }
        delivered
    }
}

struct HostRender;
impl RenderService for HostRender {
    fn measure(&self, text: &TextSpec, width: f32) -> Result<TextMetrics> {
        Ok(TextMetrics {
            width: (text.text.chars().count() as f32 * text.size_dip * 0.55).min(width),
            height: text.size_dip * 1.45,
        })
    }
    fn invalidate(&self, _mount: MountKey, _rect: Option<RectDip>) -> Result<()> {
        Ok(())
    }
}

struct HostTheme;
impl ThemeService for HostTheme {
    fn snapshot(&self) -> Result<Arc<ThemeSnapshot>> {
        Ok(Arc::new(ThemeSnapshot {
            revision: 1,
            high_contrast: false,
            text_primary: [0.95, 0.96, 0.98, 1.0],
            surface_panel: [0.125, 0.15, 0.19, 1.0],
            accent: [0.55, 0.77, 1.0, 1.0],
        }))
    }
}

struct HostDesktop;
impl DesktopService for HostDesktop {
    fn request_mode(&self, _instance: InstanceKey, _mode: Presentation) -> Result<()> {
        Ok(())
    }
}

struct PipeEvent {
    scope: ScopeId,
    generation: u64,
    subscription: Token,
    bytes: Arc<[u8]>,
}

struct PipeCommand {
    owner: ScopeId,
    generation: u64,
    task: Token,
    sender: mpsc::Sender<Arc<[u8]>>,
}

struct PipeState {
    next: AtomicU64,
    notify_hwnd: isize,
    events: Mutex<VecDeque<PipeEvent>>,
    commands: Mutex<HashMap<Token, PipeCommand>>,
}

impl PipeState {
    fn new(notify_hwnd: pecofence_platform::HWND) -> Self {
        Self {
            next: AtomicU64::new(1),
            notify_hwnd: notify_hwnd.0 as isize,
            events: Mutex::new(VecDeque::new()),
            commands: Mutex::new(HashMap::new()),
        }
    }

    fn token(&self) -> Result<Token> {
        let value = self
            .next
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .map_err(|_| Error::Exhausted)?;
        Ok(Token(value))
    }

    fn publish(&self, event: PipeEvent) {
        if let Ok(mut events) = self.events.lock() {
            if events.len() == 128 {
                events.pop_front();
            }
            events.push_back(event);
        }
        pecofence_platform::window::post_message(
            pecofence_platform::HWND(self.notify_hwnd as *mut core::ffi::c_void),
            crate::commands::WM_APP_PLUGIN_EVENT,
            0,
            0,
        );
    }
}

struct PipeIpc {
    runtime: Arc<tokio::runtime::Runtime>,
    supervisor: Rc<RefCell<TaskSupervisor>>,
    state: Arc<PipeState>,
}

impl IpcService for PipeIpc {
    fn subscribe(&self, scope: &ScopeHandle, query: Query) -> Result<Token> {
        let owner = scope.check()?;
        let generation = scope.generation()?;
        if query.endpoint != "spm.v2/read-model" {
            return Err(Error::Invalid("unsupported local endpoint".into()));
        }
        let endpoint = pecofence_platform::named_pipe::spm_v2_endpoint()
            .map_err(|error| Error::Backend(error.to_string()))?;
        let subscription = self.state.token()?;
        let (sender, receiver) = mpsc::channel(16);
        let state = self.state.clone();
        let payload = query.payload;
        let task = self.supervisor.borrow_mut().spawn_tokio(
            self.runtime.handle(),
            owner,
            generation,
            move |cancel| async move {
                run_pipe_subscription(
                    endpoint,
                    owner,
                    generation,
                    subscription,
                    payload,
                    receiver,
                    cancel,
                    state,
                )
                .await;
            },
        )?;
        self.state
            .commands
            .lock()
            .map_err(|_| Error::Backend("IPC command mutex poisoned".into()))?
            .insert(
                subscription,
                PipeCommand {
                    owner,
                    generation,
                    task,
                    sender,
                },
            );
        Ok(subscription)
    }
    fn send(&self, scope: &ScopeHandle, payload: Arc<[u8]>) -> Result<Token> {
        let owner = scope.check()?;
        let generation = scope.generation()?;
        let commands = self
            .state
            .commands
            .lock()
            .map_err(|_| Error::Backend("IPC command mutex poisoned".into()))?;
        let mut accepted = false;
        for command in commands
            .values()
            .filter(|command| command.owner == owner && command.generation == generation)
        {
            command
                .sender
                .try_send(payload.clone())
                .map_err(|error| Error::Backend(error.to_string()))?;
            accepted = true;
        }
        if !accepted {
            return Err(Error::Revoked);
        }
        self.state.token()
    }
    fn cancel(&self, token: Token) -> Result<()> {
        let command = self
            .state
            .commands
            .lock()
            .map_err(|_| Error::Backend("IPC command mutex poisoned".into()))?
            .remove(&token)
            .ok_or(Error::Revoked)?;
        self.supervisor.borrow().cancel(command.task);
        Ok(())
    }
}

async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    message: &serde_json::Value,
) -> std::io::Result<()> {
    let frame = pecofence_plugin_api::encode_local_frame(message)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    writer.write_all(&frame).await?;
    writer.flush().await
}

async fn read_frame(reader: &mut (impl AsyncRead + Unpin)) -> std::io::Result<serde_json::Value> {
    let size = reader.read_u32_le().await? as usize;
    if size == 0 || size > pecofence_plugin_api::MAX_LOCAL_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid v2 frame length",
        ));
    }
    let mut frame = Vec::with_capacity(size + 4);
    frame.extend_from_slice(&(size as u32).to_le_bytes());
    frame.resize(size + 4, 0);
    reader.read_exact(&mut frame[4..]).await?;
    pecofence_plugin_api::decode_local_frame(&frame)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[allow(clippy::too_many_arguments)]
async fn run_pipe_subscription(
    endpoint: String,
    owner: ScopeId,
    generation: u64,
    subscription: Token,
    subscribe_payload: Arc<[u8]>,
    mut commands: mpsc::Receiver<Arc<[u8]>>,
    cancel: pecofence_plugin_kernel::CancellationToken,
    state: Arc<PipeState>,
) {
    let payload: serde_json::Value = match serde_json::from_slice(&subscribe_payload) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::warn!(%error, "invalid SPM v2 subscription payload");
            return;
        }
    };
    let session = uuid::Uuid::new_v4().to_string();
    let mut retry = Duration::from_millis(100);
    while !cancel.is_cancelled() {
        let connected = tokio::net::windows::named_pipe::ClientOptions::new().open(&endpoint);
        let pipe = match connected {
            Ok(pipe) => pipe,
            Err(error) => {
                tracing::debug!(%error, %endpoint, "SPM v2 pipe connect failed");
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(retry) => {}
                }
                retry = (retry * 2).min(Duration::from_secs(5));
                continue;
            }
        };
        retry = Duration::from_millis(100);
        let (mut reader, mut writer) = tokio::io::split(pipe);
        let hello = serde_json::json!({
            "version": LOCAL_IPC_MAJOR,
            "session": session,
            "requestId": subscription.0,
            "method": "spm.hello"
        });
        if write_frame(&mut writer, &hello).await.is_err() {
            continue;
        }
        let hello_response = tokio::select! {
            _ = cancel.cancelled() => return,
            response = read_frame(&mut reader) => response,
        };
        let Ok(hello_response) = hello_response else {
            continue;
        };
        if hello_response
            .get("version")
            .and_then(|value| value.as_u64())
            != Some(LOCAL_IPC_MAJOR as u64)
            || hello_response
                .get("method")
                .and_then(|value| value.as_str())
                != Some("spm.hello")
        {
            tracing::warn!("SPM daemon rejected protocol v2 handshake");
            return;
        }
        let subscribe = serde_json::json!({
            "version": LOCAL_IPC_MAJOR,
            "session": session,
            "requestId": subscription.0,
            "method": "spm.panel.subscribe",
            "payload": payload,
        });
        if write_frame(&mut writer, &subscribe).await.is_err() {
            continue;
        }
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                command = commands.recv() => {
                    let Some(command) = command else { return };
                    let payload = serde_json::from_slice::<serde_json::Value>(&command)
                        .unwrap_or(serde_json::Value::Null);
                    let request = serde_json::json!({
                        "version": LOCAL_IPC_MAJOR,
                        "session": session,
                        "requestId": subscription.0,
                        "method": "spm.refresh",
                        "payload": payload,
                    });
                    if write_frame(&mut writer, &request).await.is_err() { break; }
                }
                response = read_frame(&mut reader) => {
                    let Ok(response) = response else { break };
                    if response.get("version").and_then(|value| value.as_u64()) != Some(LOCAL_IPC_MAJOR as u64)
                        || response.get("session").and_then(|value| value.as_str()) != Some(session.as_str())
                        || response.get("method").and_then(|value| value.as_str()) != Some("spm.panel.snapshot")
                    {
                        continue;
                    }
                    let Some(snapshot) = response.get("payload").or_else(|| response.get("snapshot")) else { continue };
                    let Ok(bytes) = serde_json::to_vec(snapshot) else { continue };
                    state.publish(PipeEvent {
                        scope: owner,
                        generation,
                        subscription,
                        bytes: Arc::from(bytes),
                    });
                }
            }
        }
    }
}

#[derive(Default)]
struct MemoryStorage {
    values: RefCell<HashMap<String, VersionedValue>>,
    next: Cell<u64>,
}
impl MemoryStorage {
    fn token(&self) -> Result<Token> {
        let value = self.next.get().checked_add(1).ok_or(Error::Exhausted)?;
        self.next.set(value);
        Ok(Token(value))
    }
}
impl StorageService for MemoryStorage {
    fn read(&self, scope: &ScopeHandle, _key: &str) -> Result<Token> {
        scope.check()?;
        self.token()
    }
    fn compare_and_set(
        &self,
        scope: &ScopeHandle,
        key: &str,
        expected: Option<u64>,
        bytes: Arc<[u8]>,
    ) -> Result<Token> {
        scope.check()?;
        let mut values = self.values.borrow_mut();
        let current = values.get(key).map(|value| value.revision);
        if current != expected {
            return Err(Error::Revoked);
        }
        let revision = current
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        values.insert(key.into(), VersionedValue { revision, bytes });
        self.token()
    }
}

struct HostNavigation;
impl NavigationService for HostNavigation {
    fn open(&self, scope: &ScopeHandle, target: ExternalTarget) -> Result<Token> {
        scope.check()?;
        if !target.https_uri.starts_with("https://") {
            return Err(Error::Invalid("only HTTPS navigation is allowed".into()));
        }
        pecofence_platform::shell::shell_execute(
            std::path::Path::new(&target.https_uri),
            None,
            None,
        )
        .map_err(|error| Error::Backend(error.to_string()))?;
        Ok(Token(1))
    }
}

struct HostClipboard;
impl ClipboardService for HostClipboard {
    fn write_text(&self, scope: &ScopeHandle, text: String) -> Result<Token> {
        scope.check()?;
        pecofence_platform::clipboard::set_text(&text)
            .map_err(|error| Error::Backend(error.to_string()))?;
        Ok(Token(1))
    }
}

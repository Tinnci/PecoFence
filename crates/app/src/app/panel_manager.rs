use crate::fence_window::PanelHandle;
use pecofence_core::PanelSpec;
use pecofence_plugin_api::*;
use pecofence_plugin_kernel::{ScopeKind, ScopeTree, ServiceKey, ServiceRegistry, TaskSupervisor};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const RENDER: ServiceKey<dyn RenderService> = ServiceKey::new("host", "render", 1);
const THEME: ServiceKey<dyn ThemeService> = ServiceKey::new("host", "theme", 1);
const DESKTOP: ServiceKey<dyn DesktopService> = ServiceKey::new("host", "desktop", 1);
const IPC: ServiceKey<dyn IpcService> = ServiceKey::new("host", "ipc", 1);
const STORAGE: ServiceKey<dyn StorageService> = ServiceKey::new("host", "storage", 1);
const NAVIGATION: ServiceKey<dyn NavigationService> = ServiceKey::new("host", "navigation", 1);
const CLIPBOARD: ServiceKey<dyn ClipboardService> = ServiceKey::new("host", "clipboard", 1);

fn spm_endpoint(
    user_sid: String,
    windows_session_id: u32,
    instance: Option<&str>,
) -> std::result::Result<String, spm_contracts::ContractError> {
    spm_contracts::v2_pipe_name(&spm_contracts::EndpointIdentity {
        user_sid,
        windows_session_id,
        instance: instance
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
    })
}

#[cfg(test)]
mod endpoint_tests {
    use super::{PipeState, RELIABLE_EVENT_CAPACITY, ScopeId, SnapshotStamp, Token, spm_endpoint};
    use std::sync::Arc;

    #[test]
    fn instance_scopes_spm_endpoint() {
        let endpoint = |instance| spm_endpoint("S-1-5-21-123".into(), 7, instance).unwrap();
        let default = endpoint(None);
        assert_eq!(default, r"\\.\pipe\pecofence.spmd.v2.S-1-5-21-123.7");
        assert_eq!(endpoint(Some("  ")), default);

        let lab_a = endpoint(Some("lab-a"));
        let lab_b = endpoint(Some("lab-b"));
        assert_ne!(lab_a, lab_b);
        assert!(lab_a.starts_with(&(default.clone() + ".i-")));
        assert!(lab_b.starts_with(&(default + ".i-")));
    }

    #[tokio::test]
    async fn snapshots_coalesce_without_discarding_reliable_results() {
        let inbox = PipeState::new(pecofence_platform::HWND(std::ptr::null_mut()));
        let session = spm_contracts::DaemonSessionId::new();
        let old = SnapshotStamp {
            session,
            revision: spm_contracts::Revision(1),
        };
        let new = SnapshotStamp {
            session,
            revision: spm_contracts::Revision(2),
        };
        inbox.publish_snapshot(ScopeId(1), 1, Token(7), old, Arc::from(&b"old"[..]));
        inbox
            .publish_reliable(ScopeId(1), 1, Token(7), Arc::from(&b"reply"[..]))
            .await
            .unwrap();
        inbox.publish_snapshot(ScopeId(1), 1, Token(7), new, Arc::from(&b"new"[..]));
        inbox.publish_snapshot(ScopeId(1), 1, Token(7), old, Arc::from(&b"late-old"[..]));
        let events = inbox.drain_for_test();
        assert_eq!(events.len(), 2);
        assert_eq!(&*events[0].1, b"reply");
        assert_eq!(&*events[1].1, b"new");
    }

    #[tokio::test]
    async fn reliable_results_wait_for_ui_capacity() {
        let inbox = PipeState::new(pecofence_platform::HWND(std::ptr::null_mut()));
        for _ in 0..RELIABLE_EVENT_CAPACITY {
            inbox
                .publish_reliable(ScopeId(1), 1, Token(7), Arc::from(&b"reply"[..]))
                .await
                .unwrap();
        }
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(10),
                inbox.publish_reliable(ScopeId(1), 1, Token(7), Arc::from(&b"next"[..])),
            )
            .await
            .is_err()
        );
        assert_eq!(inbox.drain_for_test().len(), RELIABLE_EVENT_CAPACITY);
        inbox
            .publish_reliable(ScopeId(1), 1, Token(7), Arc::from(&b"next"[..]))
            .await
            .unwrap();
        assert_eq!(inbox.drain_for_test().len(), 1);
    }
}

struct InstanceRecord {
    provider: String,
    config: serde_json::Value,
    activation: u64,
    scope: ScopeId,
    handle: PanelHandle,
    mount_scope: Option<ScopeId>,
    mount_key: Option<MountKey>,
    drain_started: Option<Instant>,
    wake_at: Option<Instant>,
}

/// Host composition root for provider descriptors, runtime instances, and mount generations.
pub(crate) struct PanelManager {
    providers: HashMap<String, Rc<dyn PanelProvider>>,
    instances: HashMap<u128, InstanceRecord>,
    scopes: ScopeTree,
    services: ServiceRegistry,
    next_activation: u64,
    next_mount_generation: u64,
    _runtime: Arc<tokio::runtime::Runtime>,
    supervisor: Rc<RefCell<TaskSupervisor>>,
    ipc_state: Arc<PipeState>,
    render_invalidations: Arc<Mutex<HashSet<MountKey>>>,
    notify_hwnd: pecofence_platform::HWND,
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
        let render_invalidations = Arc::new(Mutex::new(HashSet::new()));
        let instance = pecofence_core::brand::var("PECOFENCE_INSTANCE").ok();
        let endpoint = spm_endpoint(
            pecofence_platform::named_pipe::current_user_sid().expect("current process user SID"),
            pecofence_platform::named_pipe::current_windows_session_id()
                .expect("current Windows session ID"),
            instance.as_deref(),
        )
        .expect("valid SPM v2 endpoint identity");
        let (transport, receivers) = crate::spm_transport::channel();
        let service_generation = scopes
            .handle(service_scope)
            .and_then(|scope| scope.generation())
            .expect("service scope is open");
        let actor_handle = transport.clone();
        let actor_state = ipc_state.clone();
        supervisor
            .borrow_mut()
            .spawn_tokio(
                runtime.handle(),
                service_scope,
                service_generation,
                move |cancel| async move {
                    crate::spm_transport::run(
                        endpoint,
                        receivers,
                        actor_handle,
                        actor_state,
                        cancel,
                    )
                    .await;
                },
            )
            .expect("SPM transport supervisor capacity");
        let mut services = ServiceRegistry::new();
        services
            .publish(
                RENDER,
                service_scope,
                Box::new(HostRender {
                    pending: render_invalidations.clone(),
                    notify_hwnd: notify_hwnd.0 as isize,
                }),
            )
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
                    state: ipc_state.clone(),
                    transport,
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
            next_mount_generation: 1,
            _runtime: runtime,
            supervisor,
            ipc_state,
            render_invalidations,
            notify_hwnd,
        }
    }

    pub fn register_provider(&mut self, provider: Rc<dyn PanelProvider>) -> Result<()> {
        let id = provider.descriptor().id.to_string();
        if self.providers.contains_key(&id) {
            return Err(Error::Duplicate);
        }
        self.providers.insert(id, provider);
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
        tracing::info!(provider = %spec.provider, "panel.activation_requested");
        let provider =
            self.providers.get(&spec.provider).cloned().ok_or_else(|| {
                Error::Invalid(format!("unknown panel provider {}", spec.provider))
            })?;
        let plugin_scope = self
            .scopes
            .create(self.scopes.root(), ScopeKind::Provider)?;
        let instance_scope = self.scopes.create(plugin_scope, ScopeKind::Instance)?;
        let mount_scope = self.scopes.create(instance_scope, ScopeKind::Mount)?;
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
        let generation = self.next_mount_generation;
        self.next_mount_generation = self
            .next_mount_generation
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        let mount_key = MountKey {
            instance: key,
            generation,
        };
        let mut transaction = pecofence_plugin_kernel::ActivationTransaction::begin();
        if let Err(failure) = transaction.create(|| {
            provider.create(
                context,
                CreatePanel {
                    key,
                    config: PanelConfig {
                        version: spec.config_version,
                        bytes,
                    },
                },
            )
        }) {
            let _ = self.scopes.begin_stop(plugin_scope);
            let _ = self.scopes.finish_dispose(plugin_scope);
            let _ = self.scopes.remove_disposed_subtree(plugin_scope);
            return Err(failure.into());
        }
        if let Err(failure) = transaction.attach_initial(Some(MountContext {
            key: mount_key,
            scope: self.scopes.handle(mount_scope)?,
            viewport: RectDip::default(),
            dpi: 96,
        })) {
            transaction.rollback(failure.clone());
            let _ = self.scopes.begin_stop(plugin_scope);
            let _ = self.scopes.finish_dispose(plugin_scope);
            let _ = self.scopes.remove_disposed_subtree(plugin_scope);
            return Err(failure.into());
        }
        let (panel, _) = transaction.commit().map_err(Error::from)?;
        let handle = PanelHandle::new(key, panel);
        let wake_at = handle
            .next_wake_after()
            .and_then(|delay| Instant::now().checked_add(delay.max(Duration::from_millis(10))));
        self.instances.insert(
            id,
            InstanceRecord {
                provider: spec.provider.clone(),
                config: spec.config.clone(),
                activation,
                scope: instance_scope,
                handle: handle.clone(),
                mount_scope: Some(mount_scope),
                mount_key: Some(mount_key),
                drain_started: None,
                wake_at,
            },
        );
        self.arm_next_wake();
        Ok(handle)
    }

    pub fn open_panel(&mut self, spec: &PanelSpec) -> Result<PanelHandle> {
        self.resolve(spec)
    }

    pub fn persist_config(
        &mut self,
        key: InstanceKey,
        bytes: Arc<[u8]>,
    ) -> Result<serde_json::Value> {
        if bytes.len() > 64 * 1024 {
            return Err(Error::Invalid("panel configuration exceeds 64 KiB".into()));
        }
        let config: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| Error::Invalid(e.to_string()))?;
        let record = self.instances.get_mut(&key.id).ok_or(Error::Closed)?;
        if record.activation != key.activation || record.drain_started.is_some() {
            return Err(Error::Revoked);
        }
        let provider = self.providers.get(&record.provider).ok_or(Error::Closed)?;
        provider.validate(&PanelConfig {
            version: provider.descriptor().config_major,
            bytes,
        })?;
        record.config = config.clone();
        Ok(config)
    }

    pub fn submit_text(&self, key: InstanceKey, action: u64, text: String) -> Result<PanelUpdate> {
        let record = self.instances.get(&key.id).ok_or(Error::Closed)?;
        if record.activation != key.activation || record.drain_started.is_some() {
            return Err(Error::Revoked);
        }
        record
            .handle
            .event(PanelEvent::TextSubmitted { action, text })
    }

    #[allow(dead_code)]
    pub fn set_presentation(&mut self, id: u128, presentation: Presentation) -> Result<()> {
        let record = self
            .instances
            .get(&id)
            .ok_or_else(|| Error::Invalid("unknown panel instance".into()))?;
        record.handle.set_presentation(presentation);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn set_exposure(&mut self, id: u128, exposure: Exposure) -> Result<()> {
        let record = self
            .instances
            .get(&id)
            .ok_or_else(|| Error::Invalid("unknown panel instance".into()))?;
        record
            .handle
            .set_container_state(Grouping::Single, exposure, exposure != Exposure::Hidden);
        Ok(())
    }

    pub fn close_panel(&mut self, id: u128, reason: StopReason) -> Result<bool> {
        let Some(record) = self.instances.get_mut(&id) else {
            return Ok(false);
        };
        if record.drain_started.is_some() {
            return Ok(false);
        }
        if let Some(key) = record.mount_key.take() {
            record.handle.unmount(key);
        }
        if let Some(scope) = record.mount_scope.take() {
            let _ = self.scopes.begin_stop(scope);
            self.supervisor.borrow().cancel_scope(scope);
        }
        record.handle.stop(reason);
        self.scopes.begin_stop(record.scope)?;
        self.supervisor.borrow().cancel_scope(record.scope);
        record.drain_started = Some(Instant::now());
        record.wake_at = None;
        self.arm_next_wake();
        Ok(true)
    }

    #[allow(dead_code)]
    pub fn attach_mount(
        &mut self,
        id: u128,
        viewport: RectDip,
        dpi: u32,
        _presentation: Presentation,
    ) -> Result<MountKey> {
        let record = self.instances.get_mut(&id).ok_or(Error::Closed)?;
        if record.mount_key.is_some() || record.drain_started.is_some() {
            return Err(Error::Duplicate);
        }
        let scope = self.scopes.create(record.scope, ScopeKind::Mount)?;
        let generation = self.next_mount_generation;
        self.next_mount_generation = self
            .next_mount_generation
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        let key = MountKey {
            instance: record.handle.key(),
            generation,
        };
        if let Err(error) = record.handle.mount(MountContext {
            key,
            scope: self.scopes.handle(scope)?,
            viewport,
            dpi,
        }) {
            let _ = self.scopes.begin_stop(scope);
            let _ = self.scopes.finish_dispose(scope);
            let _ = self.scopes.remove_disposed_subtree(scope);
            return Err(error);
        }
        record.mount_scope = Some(scope);
        record.mount_key = Some(key);
        Ok(key)
    }

    #[allow(dead_code)]
    pub fn detach_mount(&mut self, id: u128) -> Result<bool> {
        let record = self.instances.get_mut(&id).ok_or(Error::Closed)?;
        let Some(key) = record.mount_key.take() else {
            return Ok(false);
        };
        record.handle.unmount(key);
        if let Some(scope) = record.mount_scope.take() {
            self.scopes.begin_stop(scope)?;
            self.supervisor.borrow().cancel_scope(scope);
            let report = self.supervisor.borrow_mut().poll_scope(scope);
            if report.pending == 0 && report.native_pending == 0 {
                self.scopes.finish_dispose(scope)?;
                self.scopes.remove_disposed_subtree(scope)?;
            }
        }
        Ok(true)
    }

    #[allow(dead_code)]
    pub fn reconcile_fences(&mut self, desired: &[PanelSpec]) -> Vec<(u128, Result<PanelHandle>)> {
        let wanted: HashSet<_> = desired
            .iter()
            .map(|spec| spec.instance_id.as_u128())
            .collect();
        let stale: Vec<_> = self
            .instances
            .keys()
            .copied()
            .filter(|id| !wanted.contains(id))
            .collect();
        for id in stale {
            let _ = self.close_panel(id, StopReason::Deleted);
        }
        desired
            .iter()
            .map(|spec| (spec.instance_id.as_u128(), self.open_panel(spec)))
            .collect()
    }

    /// Dispose instances no longer present in the saved layout without activating
    /// inactive tabs. Their timers and subscriptions stop with the instance.
    pub fn retain_specs(&mut self, desired: &[PanelSpec]) {
        let wanted: HashSet<_> = desired
            .iter()
            .map(|spec| spec.instance_id.as_u128())
            .collect();
        let stale: Vec<_> = self
            .instances
            .keys()
            .copied()
            .filter(|id| !wanted.contains(id))
            .collect();
        for id in stale {
            let _ = self.close_panel(id, StopReason::Deleted);
        }
    }

    pub fn poll(&mut self) -> HashMap<InstanceKey, PanelUpdate> {
        let mut updates = self.poll_events();
        for (key, update) in self.poll_wakes() {
            updates.entry(key).or_default().merge(update);
        }
        let draining: Vec<_> = self
            .instances
            .iter()
            .filter_map(|(id, record)| {
                record
                    .drain_started
                    .map(|started| (*id, record.scope, started))
            })
            .collect();
        for (id, scope, started) in draining {
            let report = self.supervisor.borrow_mut().poll_scope(scope);
            if report.pending == 0 && report.native_pending == 0 {
                let provider_scope = self.scopes.parent(scope);
                let _ = self.scopes.finish_dispose(scope);
                if let Some(provider_scope) = provider_scope {
                    let _ = self.scopes.finish_dispose(provider_scope);
                    let _ = self.scopes.remove_disposed_subtree(provider_scope);
                }
                self.instances.remove(&id);
            } else if started.elapsed() >= Duration::from_secs(5) {
                let _ = self.scopes.local(scope).map(|scope| scope.quarantine());
            }
        }
        updates
    }

    /// The UI thread owns one Win32 timer for all provider wake-ups. No worker
    /// thread or IPC round trip is needed for local time-based panels.
    fn arm_next_wake(&self) {
        let Some(next) = self
            .instances
            .values()
            .filter_map(|record| record.wake_at)
            .min()
        else {
            pecofence_platform::window::kill_timer(self.notify_hwnd, super::TIMER_PANEL_WAKE);
            return;
        };
        let millis = next
            .saturating_duration_since(Instant::now())
            .as_millis()
            .clamp(10, u32::MAX as u128) as u32;
        pecofence_platform::window::set_timer(self.notify_hwnd, super::TIMER_PANEL_WAKE, millis);
    }

    fn poll_wakes(&mut self) -> HashMap<InstanceKey, PanelUpdate> {
        let now = Instant::now();
        let mut updates: HashMap<InstanceKey, PanelUpdate> = HashMap::new();
        let mut fired = false;
        for record in self.instances.values_mut() {
            if record.drain_started.is_some() || !record.wake_at.is_some_and(|at| at <= now) {
                continue;
            }
            fired = true;
            let result = record.handle.event(PanelEvent::Wake);
            record.wake_at = record
                .handle
                .next_wake_after()
                .and_then(|delay| Instant::now().checked_add(delay.max(Duration::from_millis(10))));
            match result {
                Ok(update) => updates
                    .entry(record.handle.key())
                    .or_default()
                    .merge(update),
                Err(error) => tracing::warn!(%error, "panel wake rejected"),
            }
        }
        if fired {
            self.arm_next_wake();
        }
        updates
    }

    pub fn time_changed(&mut self) {
        let now = Instant::now();
        for record in self.instances.values_mut() {
            if record.wake_at.is_some() && record.drain_started.is_none() {
                record.wake_at = Some(now);
            }
        }
        pecofence_platform::window::post_message(
            self.notify_hwnd,
            crate::commands::WM_APP_PLUGIN_EVENT,
            0,
            0,
        );
    }

    pub fn shutdown(&mut self) {
        let ids: Vec<u128> = self.instances.keys().copied().collect();
        for id in ids {
            let _ = self.close_panel(id, StopReason::Shutdown);
        }
    }

    pub fn poll_events(&mut self) -> HashMap<InstanceKey, PanelUpdate> {
        let events = self.ipc_state.drain();
        let mut updates: HashMap<InstanceKey, PanelUpdate> = HashMap::new();
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
                Ok(update) if update.relayout || !update.commands.is_empty() => {
                    updates
                        .entry(record.handle.key())
                        .or_default()
                        .merge(update);
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "plugin IPC event rejected"),
            }
        }
        let invalidations: Vec<_> = self
            .render_invalidations
            .lock()
            .map(|mut pending| pending.drain().collect())
            .unwrap_or_default();
        for mount in invalidations {
            if let Some(record) = self.instances.get(&mount.instance.id)
                && record.mount_key == Some(mount)
                && record.drain_started.is_none()
            {
                updates
                    .entry(mount.instance)
                    .or_default()
                    .merge(PanelUpdate {
                        commands: vec![HostCommand::Invalidate],
                        relayout: false,
                    });
            }
        }
        updates
    }
}

struct HostRender {
    pending: Arc<Mutex<HashSet<MountKey>>>,
    notify_hwnd: isize,
}
impl RenderService for HostRender {
    fn measure(&self, text: &TextSpec, width: f32) -> Result<TextMetrics> {
        Ok(TextMetrics {
            width: (text.text.chars().count() as f32 * text.size_dip * 0.55).min(width),
            height: text.size_dip * 1.45,
        })
    }
    fn invalidate(&self, mount: MountKey, _rect: Option<RectDip>) -> Result<()> {
        self.pending
            .lock()
            .map_err(|_| Error::Backend("render invalidation mutex poisoned".into()))?
            .insert(mount);
        pecofence_platform::window::post_message(
            pecofence_platform::HWND(self.notify_hwnd as *mut core::ffi::c_void),
            crate::commands::WM_APP_PLUGIN_EVENT,
            0,
            0,
        );
        Ok(())
    }
}

/// Shared state behind [`HostTheme`]: the live [`ThemeSnapshot`] derived from
/// the fence window's [`pecofence_render::Theme`] plus the epoch that advances
/// whenever the derived tokens change.
pub(crate) struct ThemeHost {
    epoch: AtomicU64,
    current: Mutex<Arc<ThemeSnapshot>>,
}

impl ThemeHost {
    fn new() -> Self {
        Self {
            epoch: AtomicU64::new(1),
            current: Mutex::new(Arc::new(theme_snapshot_for(
                pecofence_render::ThemeMode::Dark,
                1,
            ))),
        }
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    pub(crate) fn snapshot(&self) -> Arc<ThemeSnapshot> {
        Arc::clone(&self.current.lock().expect("theme mutex"))
    }

    /// Re-derive the snapshot from the fence window's live `Theme`. The epoch
    /// (and therefore the published revision) advances only when the derived
    /// tokens actually change, so per-frame syncs stay free. Token comparison
    /// deliberately ignores the revision field: it is bookkeeping, not a token.
    pub(crate) fn sync(&self, theme: &pecofence_render::Theme) {
        let candidate = theme_snapshot_for(theme.mode, 0);
        let mut guard = self.current.lock().expect("theme mutex");
        if !same_tokens(&guard, &candidate) {
            let revision = self.epoch.fetch_add(1, Ordering::AcqRel) + 1;
            let mut next = candidate;
            next.revision = revision;
            *guard = Arc::new(next);
        }
    }
}

/// Process-wide theme host so fence-window paint bridges and the service
/// published to plugins observe the same state.
pub(crate) fn theme_host() -> &'static ThemeHost {
    static THEME_HOST: std::sync::OnceLock<ThemeHost> = std::sync::OnceLock::new();
    THEME_HOST.get_or_init(ThemeHost::new)
}

/// Semantic panel palette for a theme mode. The host design system owns these
/// values; plugins map business state onto the roles, never onto colours. Dark
/// values mirror the panel's original tuned palette so the re-wiring is a
/// visual no-op.
fn theme_snapshot_for(mode: pecofence_render::ThemeMode, revision: u64) -> ThemeSnapshot {
    use pecofence_render::ThemeMode::{Dark, Light};
    match mode {
        Dark => ThemeSnapshot {
            revision,
            high_contrast: false,
            text_primary: [0.949, 0.961, 0.98, 1.0],
            text_secondary: [0.725, 0.773, 0.839, 1.0],
            surface_panel: [0.125, 0.149, 0.188, 1.0],
            surface_subtle: [0.165, 0.2, 0.251, 1.0],
            stroke: [0.196, 0.267, 0.314, 1.0],
            accent: [0.545, 0.765, 1.0, 1.0],
            danger: [0.973, 0.443, 0.443, 1.0],
            warning: [0.984, 0.573, 0.235, 1.0],
            success: [0.29, 0.871, 0.533, 1.0],
            unknown: [0.725, 0.773, 0.839, 1.0],
            info: [0.545, 0.765, 1.0, 1.0],
        },
        Light => ThemeSnapshot {
            revision,
            high_contrast: false,
            text_primary: [0.059, 0.09, 0.133, 1.0],
            text_secondary: [0.278, 0.333, 0.412, 1.0],
            surface_panel: [0.957, 0.969, 0.973, 1.0],
            surface_subtle: [0.882, 0.906, 0.922, 1.0],
            stroke: [0.78, 0.839, 0.871, 1.0],
            accent: [0.0, 0.373, 0.722, 1.0],
            danger: [0.863, 0.148, 0.148, 1.0],
            warning: [0.918, 0.345, 0.043, 1.0],
            success: [0.086, 0.639, 0.29, 1.0],
            unknown: [0.392, 0.455, 0.545, 1.0],
            info: [0.0, 0.373, 0.722, 1.0],
        },
    }
}

/// Token equality excluding the revision bookkeeping field.
fn same_tokens(a: &ThemeSnapshot, b: &ThemeSnapshot) -> bool {
    a.high_contrast == b.high_contrast
        && a.text_primary == b.text_primary
        && a.text_secondary == b.text_secondary
        && a.surface_panel == b.surface_panel
        && a.surface_subtle == b.surface_subtle
        && a.stroke == b.stroke
        && a.accent == b.accent
        && a.danger == b.danger
        && a.warning == b.warning
        && a.success == b.success
        && a.unknown == b.unknown
        && a.info == b.info
}

struct HostTheme;
impl ThemeService for HostTheme {
    fn snapshot(&self) -> Result<Arc<ThemeSnapshot>> {
        Ok(theme_host().snapshot())
    }
}

struct HostDesktop;
impl DesktopService for HostDesktop {
    fn request_mode(&self, _instance: InstanceKey, _mode: Presentation) -> Result<()> {
        Ok(())
    }
}

struct PipeEvent {
    sequence: u64,
    snapshot_stamp: Option<SnapshotStamp>,
    scope: ScopeId,
    generation: u64,
    subscription: Token,
    bytes: Arc<[u8]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotStamp {
    pub session: spm_contracts::DaemonSessionId,
    pub revision: spm_contracts::Revision,
}

#[derive(Default)]
struct PipeInbox {
    snapshots: HashMap<(ScopeId, u64, Token), PipeEvent>,
    reliable: VecDeque<(PipeEvent, OwnedSemaphorePermit)>,
}

const RELIABLE_EVENT_CAPACITY: usize = 128;

struct PipeCommand {
    owner: ScopeId,
    generation: u64,
}

pub(crate) struct PipeState {
    next: AtomicU64,
    next_event: AtomicU64,
    notify_hwnd: isize,
    inbox: Mutex<PipeInbox>,
    reliable_slots: Arc<Semaphore>,
    commands: Mutex<HashMap<Token, PipeCommand>>,
}

impl PipeState {
    pub(crate) fn new(notify_hwnd: pecofence_platform::HWND) -> Self {
        Self {
            next: AtomicU64::new(1),
            next_event: AtomicU64::new(1),
            notify_hwnd: notify_hwnd.0 as isize,
            inbox: Mutex::new(PipeInbox::default()),
            reliable_slots: Arc::new(Semaphore::new(RELIABLE_EVENT_CAPACITY)),
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

    fn event(
        &self,
        scope: ScopeId,
        generation: u64,
        subscription: Token,
        bytes: Arc<[u8]>,
    ) -> PipeEvent {
        PipeEvent {
            sequence: self.next_event.fetch_add(1, Ordering::Relaxed),
            snapshot_stamp: None,
            scope,
            generation,
            subscription,
            bytes,
        }
    }

    fn notify(&self) {
        pecofence_platform::window::post_message(
            pecofence_platform::HWND(self.notify_hwnd as *mut core::ffi::c_void),
            crate::commands::WM_APP_PLUGIN_EVENT,
            0,
            0,
        );
    }

    pub(crate) fn publish_snapshot(
        &self,
        scope: ScopeId,
        generation: u64,
        subscription: Token,
        stamp: SnapshotStamp,
        bytes: Arc<[u8]>,
    ) {
        let mut event = self.event(scope, generation, subscription, bytes);
        event.snapshot_stamp = Some(stamp);
        if let Ok(mut inbox) = self.inbox.lock() {
            let key = (scope, generation, subscription);
            if inbox.snapshots.get(&key).is_some_and(|old| {
                old.snapshot_stamp.is_some_and(|old_stamp| {
                    old_stamp.session == stamp.session && old_stamp.revision >= stamp.revision
                })
            }) {
                return;
            }
            let wake = inbox.snapshots.is_empty() && inbox.reliable.is_empty();
            inbox.snapshots.insert(key, event);
            drop(inbox);
            if wake {
                self.notify();
            }
        }
    }

    pub(crate) async fn publish_reliable(
        &self,
        scope: ScopeId,
        generation: u64,
        subscription: Token,
        bytes: Arc<[u8]>,
    ) -> Result<()> {
        let permit = self
            .reliable_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| Error::Closed)?;
        let event = self.event(scope, generation, subscription, bytes);
        let mut inbox = self
            .inbox
            .lock()
            .map_err(|_| Error::Backend("IPC inbox mutex poisoned".into()))?;
        let wake = inbox.snapshots.is_empty() && inbox.reliable.is_empty();
        inbox.reliable.push_back((event, permit));
        drop(inbox);
        if wake {
            self.notify();
        }
        Ok(())
    }

    fn drain(&self) -> Vec<PipeEvent> {
        let Ok(mut inbox) = self.inbox.lock() else {
            return Vec::new();
        };
        let mut events: Vec<_> = inbox.snapshots.drain().map(|(_, event)| event).collect();
        events.extend(inbox.reliable.drain(..).map(|(event, _permit)| event));
        events.sort_unstable_by_key(|event| event.sequence);
        events
    }

    #[cfg(test)]
    pub(crate) fn drain_for_test(&self) -> Vec<(Token, Arc<[u8]>)> {
        self.drain()
            .into_iter()
            .map(|event| (event.subscription, event.bytes))
            .collect()
    }
}

struct PipeIpc {
    state: Arc<PipeState>,
    transport: crate::spm_transport::TransportHandle,
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
        self.transport
            .subscribe(subscription, owner, generation, project_query)
            .map_err(|_| Error::Backend("SPM transport stopped".into()))?;
        self.state
            .commands
            .lock()
            .map_err(|_| Error::Backend("IPC command mutex poisoned".into()))?
            .insert(subscription, PipeCommand { owner, generation });
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
        let subscription = commands
            .iter()
            .filter(|(_, command)| command.owner == owner && command.generation == generation)
            .map(|(subscription, _)| *subscription)
            .min_by_key(|subscription| subscription.0)
            .ok_or(Error::Revoked)?;
        let envelope: spm_contracts::Envelope =
            serde_json::from_slice(&payload).map_err(|error| Error::Invalid(error.to_string()))?;
        self.transport
            .request(subscription, envelope)
            .map_err(|_| Error::Backend("SPM transport unavailable or busy".into()))?;
        self.state.token()
    }
    fn cancel(&self, token: Token) -> Result<()> {
        self.state
            .commands
            .lock()
            .map_err(|_| Error::Backend("IPC command mutex poisoned".into()))?
            .remove(&token)
            .ok_or(Error::Revoked)?;
        self.transport
            .unsubscribe(token)
            .map_err(|_| Error::Backend("SPM transport stopped".into()))?;
        Ok(())
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

#[cfg(test)]
mod theme_host_tests {
    use super::*;

    // Tests build their own ThemeHost instead of the process-wide `theme_host()`
    // global: two tests sharing the global race on its epoch under the default
    // parallel test runner (a Light-mode sync in one test advanced the epoch
    // observed by the other).
    #[test]
    fn repeated_sync_of_same_theme_keeps_epoch_stable() {
        let host = ThemeHost::new();
        let theme = pecofence_render::Theme::dark();
        host.sync(&theme);
        let before = host.epoch();
        for _ in 0..100 {
            host.sync(&theme);
        }
        assert_eq!(
            host.epoch(),
            before,
            "identical tokens must not advance the epoch"
        );
        let first = host.snapshot();
        host.sync(&theme);
        assert_eq!(host.snapshot().as_ref(), first.as_ref());
    }

    #[test]
    fn mode_change_advances_epoch_and_revision() {
        let host = ThemeHost::new();
        host.sync(&pecofence_render::Theme::dark());
        let before = host.epoch();
        let other = match pecofence_render::Theme::dark().mode {
            pecofence_render::ThemeMode::Dark => pecofence_render::Theme::light(),
            _ => pecofence_render::Theme::dark(),
        };
        host.sync(&other);
        assert!(host.epoch() > before, "mode change must advance the epoch");
        assert_ne!(host.snapshot().revision, 0);
    }
}

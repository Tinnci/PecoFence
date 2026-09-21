//! Platform-independent contracts between the PecoFence host and in-process panels.
//!
//! The API deliberately exposes logical identifiers and DIPs, never HWNDs, COM objects, or
//! window procedures. It is a static Rust interface and is not a stable dynamic-library ABI.

use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceKey {
    pub id: u128,
    pub activation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MountKey {
    pub instance: InstanceKey,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Token(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RectDip {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectDip {
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Presentation {
    #[default]
    Workspace,
    Compact,
    Capsule,
}

impl Presentation {
    pub const fn default_size(self) -> (f32, f32) {
        match self {
            Self::Workspace => (1120.0, 760.0),
            Self::Compact => (480.0, 320.0),
            Self::Capsule => (360.0, 48.0),
        }
    }

    pub const fn minimum_size(self, text_scale: f32) -> (f32, f32) {
        match self {
            Self::Workspace => (480.0, 520.0),
            Self::Compact => (400.0, 280.0),
            Self::Capsule => (280.0, if text_scale >= 2.0 { 64.0 } else { 48.0 }),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Grouping {
    #[default]
    Single,
    Tabbed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Exposure {
    #[default]
    Desktop,
    Peek,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceBreakpoint {
    Wide,
    Medium,
    Narrow,
}

impl WorkspaceBreakpoint {
    pub fn for_width(width: f32) -> Self {
        if width >= 1040.0 {
            Self::Wide
        } else if width >= 760.0 {
            Self::Medium
        } else {
            Self::Narrow
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Closed,
    Revoked,
    Invalid(String),
    Backend(String),
    Exhausted,
    Duplicate,
    DependencyCycle,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => f.write_str("scope is closed"),
            Self::Revoked => f.write_str("capability is revoked"),
            Self::Invalid(message) | Self::Backend(message) => f.write_str(message),
            Self::Exhausted => f.write_str("generation counter exhausted"),
            Self::Duplicate => f.write_str("duplicate registration"),
            Self::DependencyCycle => f.write_str("service dependency cycle"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// The kernel-facing portion of a scope lease.
pub trait ScopeLease {
    fn id(&self) -> ScopeId;
    fn generation(&self) -> u64;
    fn is_open(&self) -> bool;
}

#[derive(Clone)]
pub struct ScopeHandle(Weak<dyn ScopeLease>);

impl ScopeHandle {
    #[doc(hidden)]
    pub fn from_lease(lease: &Rc<dyn ScopeLease>) -> Self {
        Self(Rc::downgrade(lease))
    }

    pub fn check(&self) -> Result<ScopeId> {
        let lease = self.0.upgrade().ok_or(Error::Closed)?;
        lease.is_open().then(|| lease.id()).ok_or(Error::Closed)
    }

    pub fn generation(&self) -> Result<u64> {
        let lease = self.0.upgrade().ok_or(Error::Closed)?;
        lease
            .is_open()
            .then(|| lease.generation())
            .ok_or(Error::Closed)
    }
}

/// Registry-owned service storage. Plugins only receive a weak [`Capability`].
pub struct ServiceCell<S: ?Sized> {
    generation: u64,
    ready: Cell<bool>,
    service: Box<S>,
}

impl<S: ?Sized> ServiceCell<S> {
    #[doc(hidden)]
    pub fn new(generation: u64, service: Box<S>) -> Self {
        Self {
            generation,
            ready: Cell::new(true),
            service,
        }
    }

    #[doc(hidden)]
    pub fn revoke(&self) {
        self.ready.set(false);
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
}

pub struct Capability<S: ?Sized> {
    cell: Weak<ServiceCell<S>>,
    generation: u64,
    consumer: ScopeHandle,
}

impl<S: ?Sized> Clone for Capability<S> {
    fn clone(&self) -> Self {
        Self {
            cell: self.cell.clone(),
            generation: self.generation,
            consumer: self.consumer.clone(),
        }
    }
}

impl<S: ?Sized> Capability<S> {
    #[doc(hidden)]
    pub fn new(cell: &Rc<ServiceCell<S>>, consumer: ScopeHandle) -> Self {
        Self {
            cell: Rc::downgrade(cell),
            generation: cell.generation,
            consumer,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn with<R>(&self, call: impl FnOnce(&S) -> Result<R>) -> Result<R> {
        self.consumer.check()?;
        let cell = self.cell.upgrade().ok_or(Error::Revoked)?;
        if !cell.ready.get() || cell.generation != self.generation {
            return Err(Error::Revoked);
        }
        call(&cell.service)
    }
}

#[derive(Clone, Debug)]
pub struct ThemeSnapshot {
    pub revision: u64,
    pub high_contrast: bool,
    pub text_primary: [f32; 4],
    pub surface_panel: [f32; 4],
    pub accent: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextSpec {
    pub text: String,
    pub size_dip: f32,
    pub weight: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug)]
pub struct Query {
    pub endpoint: String,
    pub payload: Arc<[u8]>,
}

#[derive(Clone, Debug)]
pub struct ExternalTarget {
    pub system: String,
    pub https_uri: String,
}

#[derive(Clone, Debug)]
pub struct VersionedValue {
    pub revision: u64,
    pub bytes: Arc<[u8]>,
}

pub trait RenderService {
    fn measure(&self, text: &TextSpec, width: f32) -> Result<TextMetrics>;
    fn invalidate(&self, mount: MountKey, rect: Option<RectDip>) -> Result<()>;
}

pub trait ThemeService {
    fn snapshot(&self) -> Result<Arc<ThemeSnapshot>>;
}

pub trait DesktopService {
    fn request_mode(&self, instance: InstanceKey, mode: Presentation) -> Result<()>;
}

pub trait IpcService {
    fn subscribe(&self, scope: &ScopeHandle, query: Query) -> Result<Token>;
    fn send(&self, scope: &ScopeHandle, payload: Arc<[u8]>) -> Result<Token>;
    fn cancel(&self, token: Token) -> Result<()>;
}

pub trait StorageService {
    fn read(&self, scope: &ScopeHandle, key: &str) -> Result<Token>;
    fn compare_and_set(
        &self,
        scope: &ScopeHandle,
        key: &str,
        expected: Option<u64>,
        bytes: Arc<[u8]>,
    ) -> Result<Token>;
}

pub trait NavigationService {
    fn open(&self, scope: &ScopeHandle, target: ExternalTarget) -> Result<Token>;
}

pub trait ClipboardService {
    fn write_text(&self, scope: &ScopeHandle, text: String) -> Result<Token>;
}

pub struct PluginContext {
    pub scope: ScopeHandle,
    pub render: Capability<dyn RenderService>,
    pub theme: Capability<dyn ThemeService>,
    pub desktop: Capability<dyn DesktopService>,
    pub ipc: Capability<dyn IpcService>,
    pub storage: Capability<dyn StorageService>,
    pub navigation: Option<Capability<dyn NavigationService>>,
    pub clipboard: Option<Capability<dyn ClipboardService>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub id: &'static str,
    pub api_major: u16,
    pub config_major: u16,
    pub required_services: &'static [&'static str],
}

#[derive(Clone, Debug)]
pub struct PanelConfig {
    pub version: u16,
    pub bytes: Arc<[u8]>,
}

pub struct CreatePanel {
    pub key: InstanceKey,
    pub config: PanelConfig,
}

pub struct MountContext {
    pub key: MountKey,
    pub scope: ScopeHandle,
    pub viewport: RectDip,
    pub dpi: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameInput {
    pub viewport: RectDip,
    pub presentation: Presentation,
    pub grouping: Grouping,
    pub exposure: Exposure,
    pub active: bool,
    pub text_scale: f32,
    pub theme_epoch: u64,
    pub device_epoch: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HitNode {
    pub id: u64,
    pub rect: RectDip,
    pub action: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticNode {
    pub id: u64,
    pub parent: Option<u64>,
    pub role: String,
    pub name: String,
    pub rect: RectDip,
    pub action: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutSnapshot {
    pub revision: u64,
    pub hits: Vec<HitNode>,
    pub semantics: Vec<SemanticNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameIdentity {
    pub mount_key: MountKey,
    pub frame_revision: u64,
    pub layout_revision: u64,
    pub theme_epoch: u64,
    pub device_epoch: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PointDip {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeStyle {
    pub rgba: [f32; 4],
    pub width: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub points: Vec<PointDip>,
    pub closed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u64);

pub trait Canvas {
    fn draw_rect(&mut self, rect: RectDip, stroke: StrokeStyle) -> Result<()>;
    fn fill_rect(&mut self, rect: RectDip, rgba: [f32; 4]) -> Result<()>;
    fn fill_rounded_rect(&mut self, rect: RectDip, radius: f32, rgba: [f32; 4]) -> Result<()>;
    fn stroke_path(&mut self, path: &Path, stroke: StrokeStyle) -> Result<()>;
    fn draw_text(&mut self, rect: RectDip, text: &TextSpec, rgba: [f32; 4]) -> Result<()>;
    fn draw_image(&mut self, image: ImageId, destination: RectDip) -> Result<()>;
    fn measure_text(&mut self, text: &TextSpec, width: f32) -> Result<TextMetrics>;
    fn push_clip(&mut self, rect: RectDip) -> Result<()>;
    fn pop_clip(&mut self) -> Result<()>;
}

pub trait PreparedFrame {
    fn identity(&self) -> FrameIdentity;
    fn hit_tree(&self) -> &[HitNode];
    fn semantics(&self) -> &[SemanticNode];
    fn paint(&self, canvas: &mut dyn Canvas) -> Result<()>;
}

pub enum PanelEvent {
    Invoke {
        action: u64,
        layout_revision: u64,
    },
    Snapshot {
        subscription: Token,
        bytes: Arc<[u8]>,
    },
    Completion {
        operation: Token,
        result: Result<Arc<[u8]>>,
    },
    ThemeChanged {
        revision: u64,
    },
    VisibilityChanged {
        visible: bool,
    },
    SuspendInput,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HostCommand {
    Invalidate,
    SetTitle(String),
    RequestMode(Presentation),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PanelUpdate {
    pub commands: Vec<HostCommand>,
    pub relayout: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Disabled,
    Deleted,
    Reconfigured,
    DependencyLost,
    Shutdown,
}

pub trait PanelProvider {
    fn descriptor(&self) -> ProviderDescriptor;
    fn validate(&self, config: &PanelConfig) -> Result<()>;
    fn create(&self, ctx: PluginContext, input: CreatePanel) -> Result<Box<dyn PanelInstance>>;
}

pub trait PanelInstance {
    fn mount(&mut self, ctx: MountContext) -> Result<()>;
    fn event(&mut self, event: PanelEvent) -> Result<PanelUpdate>;
    fn prepare_frame(&mut self, input: FrameInput) -> Result<Rc<dyn PreparedFrame>>;
    fn unmount(&mut self, key: MountKey);
    fn begin_stop(&mut self, reason: StopReason);
}

pub fn validate_provider(provider: &dyn PanelProvider, config: &PanelConfig) -> Result<()> {
    provider.validate(config)
}

pub fn stop_panel(panel: &mut dyn PanelInstance, reason: StopReason) {
    panel.begin_stop(reason);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lease;
    impl ScopeLease for Lease {
        fn id(&self) -> ScopeId {
            ScopeId(1)
        }
        fn generation(&self) -> u64 {
            2
        }
        fn is_open(&self) -> bool {
            true
        }
    }

    #[test]
    fn capability_is_generation_bound() {
        trait Value {
            fn value(&self) -> u32;
        }
        struct V;
        impl Value for V {
            fn value(&self) -> u32 {
                7
            }
        }
        let lease: Rc<dyn ScopeLease> = Rc::new(Lease);
        let service: Rc<ServiceCell<dyn Value>> = Rc::new(ServiceCell::new(3, Box::new(V)));
        let capability = Capability::new(&service, ScopeHandle::from_lease(&lease));
        assert_eq!(capability.with(|value| Ok(value.value())), Ok(7));
        service.revoke();
        assert_eq!(
            capability.with(|value| Ok(value.value())),
            Err(Error::Revoked)
        );
    }

    #[test]
    fn presentation_geometry_and_workspace_breakpoints_are_stable() {
        assert_eq!(Presentation::Workspace.default_size(), (1120.0, 760.0));
        assert_eq!(Presentation::Workspace.minimum_size(1.0), (480.0, 520.0));
        assert_eq!(Presentation::Compact.default_size(), (480.0, 320.0));
        assert_eq!(Presentation::Compact.minimum_size(1.0), (400.0, 280.0));
        assert_eq!(Presentation::Capsule.default_size(), (360.0, 48.0));
        assert_eq!(Presentation::Capsule.minimum_size(2.0), (280.0, 64.0));
        assert_eq!(
            WorkspaceBreakpoint::for_width(1040.0),
            WorkspaceBreakpoint::Wide
        );
        assert_eq!(
            WorkspaceBreakpoint::for_width(760.0),
            WorkspaceBreakpoint::Medium
        );
        assert_eq!(
            WorkspaceBreakpoint::for_width(759.0),
            WorkspaceBreakpoint::Narrow
        );
    }

    #[test]
    fn rendering_contracts_are_dyn_compatible() {
        fn accepts_canvas(_: &mut dyn Canvas) {}
        fn accepts_frame(_: &dyn PreparedFrame) {}
        let _ = accepts_canvas;
        let _ = accepts_frame;
    }
}

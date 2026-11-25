//! UIA fragment providers over immutable, thread-safe published data.
//! No COM callback borrows application UI state or waits for its message loop.
#![allow(non_snake_case)]
use crate::HWND;
use crate::uia_bindings::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use windows_core::{BSTR, HRESULT, IUnknown, Interface, implement, interface};

const OK: HRESULT = HRESULT(0);
const POINTER: HRESULT = HRESULT(0x80004003u32 as i32);
const FAIL: HRESULT = HRESULT(0x80004005u32 as i32);
const UNAVAILABLE: HRESULT = HRESULT(0x80040201u32 as i32);
const UNSUPPORTED: HRESULT = HRESULT(0x80040204u32 as i32);
const NOT_ENABLED: HRESULT = HRESULT(0x80040200u32 as i32);
pub const ACTION_MESSAGE: u32 = 0x8000 + 0x150;
const OUT_OF_MEMORY: HRESULT = HRESULT(0x8007000Eu32 as i32);

/// Physical screen coordinates, as required by UIA (never text-scaled DIPs).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Bounds {
    fn valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|n| n.is_finite())
            && self.width >= 0.0
            && self.height >= 0.0
            && (self.x + self.width).is_finite()
            && (self.y + self.height).is_finite()
    }
    fn contains(self, x: f64, y: f64) -> bool {
        self.width > 0.0
            && self.height > 0.0
            && x >= self.x
            && x < self.x + self.width
            && y >= self.y
            && y < self.y + self.height
    }
    fn raw(self) -> UiaRect {
        UiaRect {
            left: self.x,
            top: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// Integers following UiaAppendRuntimeId. Include namespace and all identity bits.
pub type NodeId = Vec<i32>;
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub role: String,
    pub name: String,
    pub bounds: Bounds,
    pub enabled: bool,
    /// Opaque host action identity. Only actual button actions advertise Invoke.
    pub invoke: Option<Vec<i32>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvokeRequest {
    pub id: NodeId,
    pub token: Vec<i32>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub name: String,
    pub bounds: Bounds,
    pub visible: bool,
    pub nodes: Vec<Node>,
}
struct Published {
    snapshot: Snapshot,
    by_id: HashMap<NodeId, usize>,
}
impl Published {
    fn new(snapshot: Snapshot) -> std::result::Result<Self, &'static str> {
        if !snapshot.bounds.valid() {
            return Err("invalid root bounds");
        }
        let mut by_id = HashMap::new();
        for (i, node) in snapshot.nodes.iter().enumerate() {
            if node.id.is_empty()
                || !node.bounds.valid()
                || node
                    .invoke
                    .as_ref()
                    .is_some_and(|token| token.is_empty() || node.role != "button")
                || by_id.insert(node.id.clone(), i).is_some()
            {
                return Err("invalid or duplicate accessibility node");
            }
        }
        for node in &snapshot.nodes {
            let mut visited = HashSet::new();
            let mut current = node.parent.as_ref();
            while let Some(parent) = current {
                if parent == &node.id || !visited.insert(parent) {
                    return Err("cyclic accessibility parent");
                }
                let Some(&index) = by_id.get(parent) else {
                    return Err("missing accessibility parent");
                };
                current = snapshot.nodes[index].parent.as_ref();
            }
        }
        Ok(Self { snapshot, by_id })
    }
    fn node(&self, id: &NodeId) -> std::result::Result<&Node, HRESULT> {
        self.by_id
            .get(id)
            .map(|&i| &self.snapshot.nodes[i])
            .ok_or(UNAVAILABLE)
    }
    fn navigate(
        &self,
        id: Option<&NodeId>,
        direction: i32,
    ) -> std::result::Result<Option<Option<NodeId>>, HRESULT> {
        let parent = id
            .map(|id| self.node(id).map(|n| n.parent.as_ref()))
            .transpose()?
            .flatten();
        let children: Vec<_> = self
            .snapshot
            .nodes
            .iter()
            .filter(|n| n.parent.as_ref() == id)
            .collect();
        Ok(match direction {
            0 => id.map(|_| parent.cloned()), // Parent (Some(None) denotes fragment root)
            1 | 2 if id.is_some() => {
                let siblings: Vec<_> = self
                    .snapshot
                    .nodes
                    .iter()
                    .filter(|n| n.parent.as_ref() == parent)
                    .collect();
                let index = siblings
                    .iter()
                    .position(|n| Some(&n.id) == id)
                    .ok_or(UNAVAILABLE)?;
                let next = if direction == 1 {
                    index.checked_add(1)
                } else {
                    index.checked_sub(1)
                };
                next.and_then(|i| siblings.get(i))
                    .map(|n| Some(n.id.clone()))
            }
            3 => children.first().map(|n| Some(n.id.clone())),
            4 => children.last().map(|n| Some(n.id.clone())),
            _ => None,
        })
    }
    fn hit(&self, x: f64, y: f64) -> Option<NodeId> {
        // Published order follows painting, independently of parent declaration order.
        self.snapshot
            .nodes
            .iter()
            .rev()
            .find(|n| n.bounds.contains(x, y))
            .map(|n| n.id.clone())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct InvokedEvent {
    id: NodeId,
    parents: Vec<NodeId>,
}
struct Shared {
    root: OnceLock<windows_core::Weak<RawRoot>>,
    hwnd: AtomicIsize,
    connected: AtomicBool,
    published: RwLock<Arc<Published>>,
    pending: Mutex<VecDeque<InvokeRequest>>,
    invoked: Mutex<Vec<InvokedEvent>>,
}
impl Shared {
    fn read(&self) -> std::result::Result<Arc<Published>, HRESULT> {
        if !self.connected.load(Ordering::Acquire) {
            return Err(UNAVAILABLE);
        }
        self.published.read().map(|v| v.clone()).map_err(|_| FAIL)
    }
}

#[derive(Clone)]
pub struct Publisher(Arc<Shared>);
impl Publisher {
    /// Record an accepted button activation while its committed semantics still exist.
    /// Delivery is deferred until the host has released its UI state borrow.
    pub fn queue_invoked(&self, id: &NodeId) -> bool {
        let Ok(published) = self.0.read() else {
            return false;
        };
        let Ok(node) = published.node(id) else {
            return false;
        };
        if !published.snapshot.visible || !node.enabled || node.invoke.is_none() {
            return false;
        }
        let mut parents = Vec::new();
        let mut parent = node.parent.as_ref();
        while let Some(id) = parent {
            parents.push(id.clone());
            parent = published
                .node(id)
                .ok()
                .and_then(|node| node.parent.as_ref());
        }
        let Ok(mut events) = self.0.invoked.lock() else {
            return false;
        };
        events.push(InvokedEvent {
            id: id.clone(),
            parents,
        });
        tracing::debug!(?id, "queued UIA Invoked event");
        true
    }
    /// Must be called outside UI state borrows; event delivery can call into UIA.
    pub fn flush_events(&self) {
        let events = self
            .0
            .invoked
            .lock()
            .map(|mut events| std::mem::take(&mut *events))
            .unwrap_or_default();
        if !self.is_connected() {
            return;
        }
        // SAFETY: plain UIA query, with no snapshot or queue lock held.
        let listening = unsafe { UiaClientsAreListening().as_bool() };
        if !events.is_empty() {
            tracing::debug!(listening, count = events.len(), "flushing UIA events");
        }
        if !listening {
            return;
        }
        for event in events {
            let provider: RawSimple = EventSource {
                shared: self.0.clone(),
                id: event.id,
                parents: event.parents,
            }
            .into();
            // SAFETY: UIA retains its own reference while delivering event handlers.
            // Identity methods remain valid for a removed node; property/action
            // methods still resolve against live data and report unavailability.
            let result = unsafe { UiaRaiseAutomationEvent(provider.as_raw(), 20009) };
            tracing::debug!(?result, "UIA Invoked event delivery result");
        }
    }
    pub fn take_invocations(&self) -> Vec<InvokeRequest> {
        self.0
            .pending
            .lock()
            .map(|mut queue| queue.drain(..).collect())
            .unwrap_or_default()
    }
    pub fn is_connected(&self) -> bool {
        self.0.connected.load(Ordering::Acquire)
    }
    /// Invalid snapshots disable the provider instead of exposing stale geometry.
    pub fn publish(&self, snapshot: Snapshot) -> std::result::Result<bool, &'static str> {
        let next = Published::new(snapshot);
        let error = next.as_ref().err().copied();
        let next = next.unwrap_or_else(|_| Published {
            snapshot: Snapshot::default(),
            by_id: HashMap::new(),
        });
        let mut guard = self
            .0
            .published
            .write()
            .map_err(|_| "poisoned accessibility snapshot")?;
        if !self.0.connected.load(Ordering::Acquire) {
            return Ok(false);
        }
        let changed = guard.snapshot != next.snapshot;
        if changed {
            *guard = Arc::new(next);
        }
        error.map_or(Ok(changed), Err)
    }
}

/// Owned by a window handler. Closing invalidates providers already held by clients.
pub struct WindowAccessibility {
    shared: Arc<Shared>,
    root: RawSimple,
}
impl Default for WindowAccessibility {
    fn default() -> Self {
        Self::new()
    }
}
impl WindowAccessibility {
    pub fn new() -> Self {
        let shared = Arc::new(Shared {
            root: OnceLock::new(),
            hwnd: AtomicIsize::new(0),
            connected: AtomicBool::new(true),
            pending: Mutex::new(VecDeque::new()),
            invoked: Mutex::new(Vec::new()),
            published: RwLock::new(Arc::new(Published {
                snapshot: Snapshot::default(),
                by_id: HashMap::new(),
            })),
        });
        let root: RawSimple = Root {
            shared: shared.clone(),
        }
        .into();
        if let Ok(fragment_root) = root.cast::<RawRoot>()
            && let Ok(weak) = fragment_root.downgrade()
        {
            let _ = shared.root.set(weak);
        }
        Self { shared, root }
    }
    pub fn publisher(&self) -> Publisher {
        Publisher(self.shared.clone())
    }
    pub fn handle_message(
        &self,
        hwnd: HWND,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> Option<isize> {
        if message == 0x0002 {
            // WM_DESTROY
            self.shared.connected.store(false, Ordering::Release);
            // SAFETY: notify UIA to release this HWND's provider event map as
            // documented for WM_DESTROY. No UI state is borrowed by this call.
            unsafe {
                UiaReturnRawElementProvider(hwnd.0, 0, 0, ptr::null_mut());
            }
        }
        if message != 0x003D || lparam as i32 != -25 {
            return None;
        } // WM_GETOBJECT, UiaRootObjectId
        if !self.shared.connected.load(Ordering::Acquire) {
            return Some(0);
        }
        let raw = hwnd.0 as isize;
        let previous =
            self.shared
                .hwnd
                .compare_exchange(0, raw, Ordering::AcqRel, Ordering::Acquire);
        if previous.is_err_and(|previous| previous != raw) {
            return Some(0);
        }
        // SAFETY: the root is retained for the window lifetime; UIA receives its
        // IRawElementProviderSimple pointer and takes its own COM references.
        Some(unsafe { UiaReturnRawElementProvider(hwnd.0, wparam, lparam, self.root.as_raw()) })
    }
}
impl Drop for WindowAccessibility {
    fn drop(&mut self) {
        self.shared.connected.store(false, Ordering::Release);
    }
}

// Explicit ABI declarations preserve S_OK/null for unsupported patterns and
// navigation endpoints. Result<Interface> projections cannot represent that case.
#[interface("d6dd68d1-86fd-4332-8666-9abedea2d24c")]
unsafe trait RawSimple: IUnknown {
    unsafe fn ProviderOptions(&self, out: *mut u32) -> HRESULT;
    unsafe fn GetPatternProvider(&self, pattern: i32, out: *mut *mut c_void) -> HRESULT;
    unsafe fn GetPropertyValue(&self, property: i32, out: *mut VARIANT) -> HRESULT;
    unsafe fn HostRawElementProvider(&self, out: *mut *mut c_void) -> HRESULT;
}
#[interface("f7063da8-8359-439c-9297-bbc5299a7d87")]
unsafe trait RawFragment: IUnknown {
    unsafe fn Navigate(&self, direction: i32, out: *mut *mut c_void) -> HRESULT;
    unsafe fn GetRuntimeId(&self, out: *mut *mut SAFEARRAY) -> HRESULT;
    unsafe fn BoundingRectangle(&self, out: *mut UiaRect) -> HRESULT;
    unsafe fn GetEmbeddedFragmentRoots(&self, out: *mut *mut SAFEARRAY) -> HRESULT;
    unsafe fn SetFocus(&self) -> HRESULT;
    unsafe fn FragmentRoot(&self, out: *mut *mut c_void) -> HRESULT;
}
#[interface("620ce2a5-ab8f-40a9-86cb-de3c75599b58")]
unsafe trait RawRoot: IUnknown {
    unsafe fn ElementProviderFromPoint(&self, x: f64, y: f64, out: *mut *mut c_void) -> HRESULT;
    unsafe fn GetFocus(&self, out: *mut *mut c_void) -> HRESULT;
}
#[interface("54fcb24b-e18e-47a2-b4d3-eccbe77599a2")]
unsafe trait RawInvoke: IUnknown {
    unsafe fn Invoke(&self) -> HRESULT;
}
#[implement(RawInvoke)]
struct Invocation {
    shared: Arc<Shared>,
    id: NodeId,
}

fn enqueue_invoke(shared: &Shared, id: &NodeId, notify: impl FnOnce() -> bool) -> HRESULT {
    let request = shared.read().and_then(|p| {
        let node = p.node(id)?;
        if !p.snapshot.visible || node.bounds.width == 0.0 || node.bounds.height == 0.0 {
            return Err(UNAVAILABLE);
        }
        if !node.enabled {
            return Err(NOT_ENABLED);
        }
        let token = node.invoke.clone().ok_or(UNSUPPORTED)?;
        Ok(InvokeRequest {
            id: id.clone(),
            token,
        })
    });
    let request = match request {
        Ok(request) => request,
        Err(error) => return error,
    };
    let Ok(mut queue) = shared.pending.lock() else {
        return FAIL;
    };
    if !shared.connected.load(Ordering::Acquire) {
        return UNAVAILABLE;
    }
    if queue.len() >= 64 {
        return OUT_OF_MEMORY;
    }
    queue.push_back(request);
    if notify() {
        OK
    } else {
        queue.pop_back();
        FAIL
    }
}
impl RawInvoke_Impl for Invocation_Impl {
    unsafe fn Invoke(&self) -> HRESULT {
        let hwnd = self.shared.hwnd.load(Ordering::Acquire);
        if hwnd == 0 {
            return UNAVAILABLE;
        }
        enqueue_invoke(&self.shared, &self.id, || {
            // SAFETY: post a payload-free notification; owned request data stays in the queue.
            unsafe {
                crate::bindings::PostMessageW(
                    Some(HWND(hwnd as *mut c_void)),
                    ACTION_MESSAGE,
                    crate::bindings::WPARAM(0),
                    crate::bindings::LPARAM(0),
                )
                .as_bool()
            }
        })
    }
}

unsafe fn pattern(
    shared: &Arc<Shared>,
    id: Option<&NodeId>,
    pattern_id: i32,
    out: *mut *mut c_void,
) -> HRESULT {
    // SAFETY: initialize typed COM output before inspecting element lifetime.
    let status = unsafe { empty_output(shared, id, out) };
    if status.is_err() || pattern_id != 10000 {
        return status;
    }
    let Some(id) = id else {
        return OK;
    };
    let supported = shared
        .read()
        .and_then(|p| p.node(id).map(|n| n.invoke.is_some()));
    match supported {
        Ok(true) => {
            let provider: RawInvoke = Invocation {
                shared: shared.clone(),
                id: id.clone(),
            }
            .into();
            // SAFETY: validated output receives one owned COM reference.
            unsafe { write(out, provider.into_raw()) }
        }
        Ok(false) => OK,
        Err(error) => error,
    }
}
#[implement(RawSimple, RawFragment, RawRoot)]
struct Root {
    shared: Arc<Shared>,
}
#[implement(RawSimple, RawFragment)]
struct Element {
    shared: Arc<Shared>,
    id: NodeId,
}

#[implement(RawSimple, RawFragment)]
struct EventSource {
    shared: Arc<Shared>,
    id: NodeId,
    parents: Vec<NodeId>,
}

fn root_fragment(shared: &Arc<Shared>) -> RawFragment {
    if let Some(root) = shared.root.get().and_then(|weak| weak.upgrade())
        && let Ok(fragment) = root.cast()
    {
        return fragment;
    }
    Root {
        shared: shared.clone(),
    }
    .into()
}
fn fragment(shared: &Arc<Shared>, id: Option<NodeId>) -> RawFragment {
    id.map_or_else(
        || root_fragment(shared),
        |id| {
            Element {
                shared: shared.clone(),
                id,
            }
            .into()
        },
    )
}
unsafe fn write<T>(out: *mut T, value: T) -> HRESULT {
    if out.is_null() {
        return POINTER;
    }
    // SAFETY: COM callers provide writable storage of the method's declared type.
    unsafe {
        out.write(value);
    }
    OK
}
fn property_variant(kind: u16, data: VARIANT_0_0_0) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VARTYPE(kind),
                Anonymous: data,
                ..Default::default()
            }),
        },
    }
}
fn string(value: &str) -> VARIANT {
    property_variant(
        8,
        VARIANT_0_0_0 {
            bstrVal: ManuallyDrop::new(BSTR::from(value)),
        },
    )
}
fn boolean(value: bool) -> VARIANT {
    property_variant(
        11,
        VARIANT_0_0_0 {
            boolVal: VARIANT_BOOL(if value { -1 } else { 0 }),
        },
    )
}
fn integer(value: i32) -> VARIANT {
    property_variant(3, VARIANT_0_0_0 { lVal: value })
}
fn control_type(role: &str) -> i32 {
    match role {
        "button" => 50000,
        "checkbox" => 50002,
        "textbox" | "edit" => 50004,
        "listitem" => 50007,
        "list" => 50008,
        "tab" => 50019,
        "text" | "heading" => 50020,
        "columnheader" => 50035,
        "group" => 50026,
        "titlebar" => 50037,
        _ => 50025,
    }
}
fn property(
    shared: &Shared,
    id: Option<&NodeId>,
    property: i32,
) -> std::result::Result<VARIANT, HRESULT> {
    let published = shared.read()?;
    let node = id.map(|id| published.node(id)).transpose()?;
    let snapshot = &published.snapshot;
    Ok(match property {
        30003 => integer(node.map_or(50033, |n| control_type(&n.role))),
        30005 => string(node.map_or(snapshot.name.as_str(), |n| n.name.as_str())),
        30011 => string(&id.map_or_else(
            || "window".into(),
            |id| id.iter().map(i32::to_string).collect::<Vec<_>>().join(":"),
        )),
        30031 => boolean(node.is_some_and(|n| n.invoke.is_some())),
        30024 => string("PecoFence"),
        30107 => string("PecoFence committed scene"),
        30008 | 30009 | 30019 => boolean(false), // no focus/action patterns advertised yet
        30010 => boolean(snapshot.visible && node.is_none_or(|n| n.enabled)),
        30016 | 30017 => boolean(true),
        30022 => boolean(
            !snapshot.visible
                || node.is_some_and(|n| n.bounds.width <= 0.0 || n.bounds.height <= 0.0),
        ),
        _ => VARIANT::default(),
    })
}
unsafe fn simple_property(
    shared: &Shared,
    id: Option<&NodeId>,
    property_id: i32,
    out: *mut VARIANT,
) -> HRESULT {
    if out.is_null() {
        return POINTER;
    }
    // SAFETY: initialize the COM out parameter even when the element disappeared.
    unsafe {
        out.write(VARIANT::default());
    }
    match property(shared, id, property_id) {
        Ok(value) => unsafe { write(out, value) }, // SAFETY: checked writable COM out parameter.
        Err(error) => error,
    }
}
unsafe fn navigate(
    shared: &Arc<Shared>,
    id: Option<&NodeId>,
    direction: i32,
    out: *mut *mut c_void,
) -> HRESULT {
    if out.is_null() {
        return POINTER;
    }
    // SAFETY: initialize the COM interface output; ownership transfers only on success.
    unsafe {
        *out = ptr::null_mut();
    }
    let result = shared.read().and_then(|p| p.navigate(id, direction));
    match result {
        Ok(Some(target)) => unsafe { write(out, fragment(shared, target).into_raw()) }, // SAFETY: checked COM output.
        Ok(None) => OK,
        Err(error) => error,
    }
}
unsafe fn bounds(shared: &Shared, id: Option<&NodeId>, out: *mut UiaRect) -> HRESULT {
    if out.is_null() {
        return POINTER;
    }
    // SAFETY: initialize writable COM output before any failure.
    unsafe {
        *out = UiaRect::default();
    }
    match shared
        .read()
        .and_then(|p| id.map_or(Ok(p.snapshot.bounds), |id| p.node(id).map(|n| n.bounds)))
    {
        Ok(value) => unsafe { write(out, value.raw()) }, // SAFETY: checked COM output.
        Err(error) => error,
    }
}
fn available(shared: &Shared, id: Option<&NodeId>) -> HRESULT {
    match shared
        .read()
        .and_then(|p| id.map_or(Ok(()), |id| p.node(id).map(|_| ())))
    {
        Ok(()) => OK,
        Err(error) => error,
    }
}
/// Initialize nullable COM outputs before checking the current element lifetime.
unsafe fn empty_output<T>(shared: &Shared, id: Option<&NodeId>, out: *mut *mut T) -> HRESULT {
    // SAFETY: write checks null and initializes the declared COM output type.
    let initialized = unsafe { write(out, ptr::null_mut()) };
    if initialized.is_err() {
        initialized
    } else {
        available(shared, id)
    }
}

unsafe fn runtime_id(id: &[i32], out: *mut *mut SAFEARRAY) -> HRESULT {
    if out.is_null() {
        return POINTER;
    }
    // SAFETY: oleaut32 owns SAFEARRAY allocation; caller receives it only after successful initialization.
    unsafe {
        *out = ptr::null_mut();
        let array = SafeArrayCreateVector(VARTYPE(3), 0, (id.len() + 1) as u32);
        if array.is_null() {
            return OUT_OF_MEMORY;
        }
        let mut data = ptr::null_mut();
        let result = SafeArrayAccessData(array, &mut data);
        if result.is_err() {
            let _ = SafeArrayDestroy(array);
            return result;
        }
        let values = std::slice::from_raw_parts_mut(data.cast::<i32>(), id.len() + 1);
        values[0] = 3; // UiaAppendRuntimeId
        values[1..].copy_from_slice(id);
        let result = SafeArrayUnaccessData(array);
        if result.is_err() {
            let _ = SafeArrayDestroy(array);
            return result;
        }
        *out = array;
        OK
    }
}

macro_rules! simple_impl {
    ($ty:ty, $id:expr, $event:expr) => {
        impl RawSimple_Impl for $ty {
            unsafe fn ProviderOptions(&self, out: *mut u32) -> HRESULT {
                // SAFETY: typed COM output, ServerSideProvider | UseComThreading.
                unsafe { write(out, 0x22) }
            }
            unsafe fn GetPatternProvider(&self, pattern_id: i32, out: *mut *mut c_void) -> HRESULT {
                // SAFETY: helper initializes output and preserves S_OK/null for unsupported patterns.
                unsafe { pattern(&self.shared, ($id)(self), pattern_id, out) }
            }
            unsafe fn GetPropertyValue(&self, property: i32, out: *mut VARIANT) -> HRESULT {
                // SAFETY: helper validates and initializes the COM out parameter.
                unsafe { simple_property(&self.shared, ($id)(self), property, out) }
            }
            unsafe fn HostRawElementProvider(&self, out: *mut *mut c_void) -> HRESULT {
                if out.is_null() {
                    return POINTER;
                }
                // SAFETY: initialize COM output. Only fragment roots are HWND-hosted.
                unsafe {
                    let status =
                        empty_output(&self.shared, if $event { None } else { ($id)(self) }, out);
                    if status.is_err() {
                        return status;
                    }
                    if ($id)(self).is_none() {
                        if !self.shared.connected.load(Ordering::Acquire) {
                            return UNAVAILABLE;
                        }
                        UiaHostProviderFromHwnd(
                            self.shared.hwnd.load(Ordering::Acquire) as *mut c_void,
                            out,
                        )
                    } else {
                        OK
                    }
                }
            }
        }
    };
}
fn root_id(_: &Root_Impl) -> Option<&NodeId> {
    None
}
fn element_id(v: &Element_Impl) -> Option<&NodeId> {
    Some(&v.id)
}
fn event_id(v: &EventSource_Impl) -> Option<&NodeId> {
    Some(&v.id)
}
simple_impl!(Root_Impl, root_id, false);
simple_impl!(Element_Impl, element_id, false);
simple_impl!(EventSource_Impl, event_id, true);

unsafe fn root_navigation(value: &Root_Impl, direction: i32, out: *mut *mut c_void) -> HRESULT {
    // SAFETY: delegate typed COM output validation to the common helper.
    unsafe { navigate(&value.shared, None, direction, out) }
}
unsafe fn element_navigation(
    value: &Element_Impl,
    direction: i32,
    out: *mut *mut c_void,
) -> HRESULT {
    // SAFETY: delegate typed COM output validation to the common helper.
    unsafe { navigate(&value.shared, Some(&value.id), direction, out) }
}
unsafe fn event_navigation(
    value: &EventSource_Impl,
    direction: i32,
    out: *mut *mut c_void,
) -> HRESULT {
    // SAFETY: initialize COM output and check window lifetime before traversing saved ancestry.
    let status = unsafe { empty_output(&value.shared, None, out) };
    if status.is_err() {
        return status;
    }
    if direction != 0 {
        return OK;
    }
    let target: RawFragment = if let Some((parent, rest)) = value.parents.split_first() {
        EventSource {
            shared: value.shared.clone(),
            id: parent.clone(),
            parents: rest.to_vec(),
        }
        .into()
    } else {
        root_fragment(&value.shared)
    };
    // SAFETY: output was checked; transfer one owned fragment reference.
    unsafe { write(out, target.into_raw()) }
}

macro_rules! fragment_impl {
    ($ty:ty,$id:expr,$event:expr,$nav:expr) => {
        impl RawFragment_Impl for $ty {
            unsafe fn Navigate(&self, direction: i32, out: *mut *mut c_void) -> HRESULT {
                // SAFETY: helper handles null checks, lifetime and interface ownership.
                unsafe { ($nav)(self, direction, out) }
            }
            unsafe fn GetRuntimeId(&self, out: *mut *mut SAFEARRAY) -> HRESULT {
                let id: Option<&NodeId> = ($id)(self);
                // SAFETY: initialize the output even if this element has expired.
                let status =
                    unsafe { empty_output(&self.shared, if $event { None } else { id }, out) };
                if status.is_err() {
                    return status;
                }
                // SAFETY: helpers initialize checked outputs; root identity is supplied by its HWND host.
                unsafe { id.map_or_else(|| write(out, ptr::null_mut()), |id| runtime_id(id, out)) }
            }
            unsafe fn BoundingRectangle(&self, out: *mut UiaRect) -> HRESULT {
                // SAFETY: helper validates the COM output.
                unsafe { bounds(&self.shared, ($id)(self), out) }
            }
            unsafe fn GetEmbeddedFragmentRoots(&self, out: *mut *mut SAFEARRAY) -> HRESULT {
                // SAFETY: no embedded fragments are exposed; valid S_OK/null endpoint.
                unsafe { empty_output(&self.shared, ($id)(self), out) }
            }
            unsafe fn SetFocus(&self) -> HRESULT {
                let status = available(&self.shared, ($id)(self));
                if status.is_err() { status } else { UNSUPPORTED }
            }
            unsafe fn FragmentRoot(&self, out: *mut *mut c_void) -> HRESULT {
                // SAFETY: initialize the output before validating the child identity.
                let status = unsafe {
                    empty_output(&self.shared, if $event { None } else { ($id)(self) }, out)
                };
                if status.is_err() {
                    return status;
                }
                let root: RawRoot = match root_fragment(&self.shared).cast() {
                    Ok(root) => root,
                    Err(error) => return error.code(),
                };
                // SAFETY: transferred COM reference to a checked output.
                if out.is_null() {
                    return POINTER;
                }
                unsafe { write(out, root.into_raw()) }
            }
        }
    };
}
fragment_impl!(Root_Impl, root_id, false, root_navigation);
fragment_impl!(Element_Impl, element_id, false, element_navigation);
fragment_impl!(EventSource_Impl, event_id, true, event_navigation);
impl RawRoot_Impl for Root_Impl {
    unsafe fn ElementProviderFromPoint(&self, x: f64, y: f64, out: *mut *mut c_void) -> HRESULT {
        if out.is_null() {
            return POINTER;
        }
        // SAFETY: initialize COM output before snapshot lookup.
        unsafe {
            *out = ptr::null_mut();
        }
        match self.shared.read() {
            Ok(p) if p.snapshot.visible && p.snapshot.bounds.contains(x, y) => {
                // SAFETY: checked output; interface ownership transfers to caller.
                unsafe { write(out, fragment(&self.shared, p.hit(x, y)).into_raw()) }
            }
            Ok(_) => OK,
            Err(error) => error,
        }
    }
    unsafe fn GetFocus(&self, out: *mut *mut c_void) -> HRESULT {
        // SAFETY: read-only initial provider does not report a focus pattern.
        unsafe { empty_output(&self.shared, None, out) }
    }
}

windows_core::link!("uiautomationcore.dll" "system" fn UiaReturnRawElementProvider(hwnd:*mut c_void,wparam:usize,lparam:isize,provider:*mut c_void)->isize);
windows_core::link!("uiautomationcore.dll" "system" fn UiaHostProviderFromHwnd(hwnd:*mut c_void,out:*mut *mut c_void)->HRESULT);

windows_core::link!("uiautomationcore.dll" "system" fn UiaClientsAreListening()->windows_core::BOOL);
windows_core::link!("uiautomationcore.dll" "system" fn UiaRaiseAutomationEvent(provider:*mut c_void,event:i32)->HRESULT);

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        let bounds = Bounds {
            x: -100.0,
            y: 20.0,
            width: 200.0,
            height: 100.0,
        };
        Snapshot {
            name: "栅栏".into(),
            bounds,
            visible: true,
            nodes: vec![
                Node {
                    id: vec![1],
                    parent: None,
                    role: "list".into(),
                    name: "文件".into(),
                    bounds,
                    enabled: true,
                    invoke: None,
                },
                Node {
                    id: vec![2, i32::MIN, i32::MAX],
                    parent: Some(vec![1]),
                    role: "listitem".into(),
                    name: "中文.txt".into(),
                    bounds,
                    enabled: true,
                    invoke: None,
                },
            ],
        }
    }
    fn child(window: &WindowAccessibility) -> RawFragment {
        fragment(&window.shared, Some(vec![2, i32::MIN, i32::MAX]))
    }

    #[test]
    fn heading_and_calendar_columns_map_to_native_text_and_header_items() {
        assert_eq!(control_type("heading"), 50020);
        assert_eq!(control_type("columnheader"), 50035);
        assert_eq!(control_type("extension-role"), 50025);
    }

    #[test]
    fn invalid_publications_clear_previous_geometry() {
        let window = WindowAccessibility::new();
        let publisher = window.publisher();
        for failure in 0..5 {
            publisher.publish(snapshot()).unwrap();
            let mut invalid = snapshot();
            match failure {
                0 => invalid.nodes[1].id = vec![1],
                1 => invalid.nodes[1].parent = Some(vec![999]),
                2 => invalid.nodes[0].parent = Some(invalid.nodes[1].id.clone()),
                3 => invalid.bounds.width = f64::INFINITY,
                _ => {
                    invalid.bounds.x = f64::MAX;
                    invalid.bounds.width = f64::MAX;
                }
            }
            assert!(publisher.publish(invalid).is_err());
            let current = window.shared.read().unwrap();
            assert!(!current.snapshot.visible);
            assert!(current.snapshot.nodes.is_empty());
        }
    }

    #[test]
    fn tree_navigation_and_paint_order_hit_agree() {
        let published = Published::new(snapshot()).unwrap();
        let id = vec![2, i32::MIN, i32::MAX];
        assert_eq!(published.navigate(None, 3).unwrap(), Some(Some(vec![1])));
        assert_eq!(published.navigate(Some(&vec![1]), 0).unwrap(), Some(None));
        assert_eq!(
            published.navigate(Some(&id), 0).unwrap(),
            Some(Some(vec![1]))
        );
        assert_eq!(published.navigate(Some(&id), 1).unwrap(), None);
        assert_eq!(published.hit(0.0, 30.0), Some(id));
        assert_eq!(published.hit(100.0, 30.0), None);
    }

    #[test]
    fn com_properties_and_runtime_ids_preserve_unicode_and_identity() {
        let window = WindowAccessibility::new();
        window.publisher().publish(snapshot()).unwrap();
        let fragment = child(&window);
        let simple: RawSimple = fragment.cast().unwrap();
        // SAFETY: writable typed COM output; SDK defines ServerSide=0x2 and UseComThreading=0x20.
        unsafe {
            let mut options = 0;
            assert_eq!(simple.ProviderOptions(&mut options), OK);
            assert_eq!(options, 0x22);
        }
        assert!(fragment.cast::<RawRoot>().is_err());
        assert!(window.root.cast::<RawRoot>().is_ok());
        // SAFETY: all COM outputs below are correctly typed writable locals;
        // returned VARIANT and SAFEARRAY allocations are freed exactly once.
        unsafe {
            let mut value = VARIANT::default();
            assert_eq!(simple.GetPropertyValue(30005, &mut value), OK);
            assert_eq!(value.Anonymous.Anonymous.vt.0, 8);
            assert_eq!(
                String::from_utf16(&value.Anonymous.Anonymous.Anonymous.bstrVal).unwrap(),
                "中文.txt"
            );
            assert!(VariantClear(&mut value).is_ok());
            let mut array = ptr::null_mut();
            assert_eq!(fragment.GetRuntimeId(&mut array), OK);
            let mut data = ptr::null_mut();
            assert!(SafeArrayAccessData(array, &mut data).is_ok());
            assert_eq!(
                std::slice::from_raw_parts(data.cast::<i32>(), 4),
                &[3, 2, i32::MIN, i32::MAX]
            );
            assert!(SafeArrayUnaccessData(array).is_ok());
            assert!(SafeArrayDestroy(array).is_ok());
            let mut pattern = ptr::dangling_mut::<c_void>();
            assert_eq!(simple.GetPatternProvider(10000, &mut pattern), OK);
            assert!(pattern.is_null());
            assert_eq!(simple.HostRawElementProvider(&mut pattern), OK);
            assert!(pattern.is_null());
        }
    }

    fn assert_unavailable(fragment: &RawFragment) {
        let simple: RawSimple = fragment.cast().unwrap();
        // SAFETY: outputs are writable locals. Failed calls must not transfer ownership.
        unsafe {
            let mut output = ptr::dangling_mut::<c_void>();
            assert_eq!(simple.GetPatternProvider(10000, &mut output), UNAVAILABLE);
            assert!(output.is_null());
            output = ptr::dangling_mut();
            assert_eq!(simple.HostRawElementProvider(&mut output), UNAVAILABLE);
            assert!(output.is_null());
            output = ptr::dangling_mut();
            assert_eq!(fragment.FragmentRoot(&mut output), UNAVAILABLE);
            assert!(output.is_null());
            let mut array = ptr::dangling_mut::<SAFEARRAY>();
            assert_eq!(fragment.GetRuntimeId(&mut array), UNAVAILABLE);
            assert!(array.is_null());
            array = ptr::dangling_mut();
            assert_eq!(fragment.GetEmbeddedFragmentRoots(&mut array), UNAVAILABLE);
            assert!(array.is_null());
            assert_eq!(fragment.SetFocus(), UNAVAILABLE);
            assert_eq!(fragment.GetRuntimeId(ptr::null_mut()), POINTER);
        }
    }

    #[test]
    fn held_com_references_expire_on_removal_and_window_close() {
        let window = WindowAccessibility::new();
        let publisher = window.publisher();
        publisher.publish(snapshot()).unwrap();
        let old_child = child(&window);
        publisher.publish(Snapshot::default()).unwrap();
        assert_unavailable(&old_child);
        publisher.publish(snapshot()).unwrap();
        let closed_child = child(&window);
        let root: RawRoot = window.root.cast().unwrap();
        drop(window);
        assert_unavailable(&closed_child);
        assert!(!publisher.is_connected());
        assert_eq!(publisher.publish(snapshot()), Ok(false));
        // SAFETY: a typed writable COM output is initialized on the failure path.
        unsafe {
            let mut output = ptr::dangling_mut::<c_void>();
            assert_eq!(root.GetFocus(&mut output), UNAVAILABLE);
            assert!(output.is_null());
        }
    }
    #[test]
    fn agile_com_reference_can_query_from_another_apartment() {
        let _sta = crate::com::StaGuard::init().unwrap();
        let window = WindowAccessibility::new();
        let publisher = window.publisher();
        publisher.publish(snapshot()).unwrap();
        let simple: RawSimple = child(&window).cast().unwrap();
        let agile = windows_core::AgileReference::new(&simple).unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _mta = crate::com::MtaGuard::init().unwrap();
            let simple = agile.resolve().unwrap();
            // SAFETY: local typed VARIANT outputs; successful values are cleared once.
            unsafe {
                let mut value = VARIANT::default();
                assert_eq!(simple.GetPropertyValue(30003, &mut value), OK);
                assert_eq!(value.Anonymous.Anonymous.Anonymous.lVal, 50007);
                assert!(VariantClear(&mut value).is_ok());
                ready_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
                assert_eq!(simple.GetPropertyValue(30003, &mut value), UNAVAILABLE);
                assert_eq!(value.Anonymous.Anonymous.vt.0, 0);
            }
        });
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        publisher.publish(Snapshot::default()).unwrap();
        resume_tx.send(()).unwrap();
        worker.join().unwrap();
    }
    fn invokable() -> Snapshot {
        let mut value = snapshot();
        value.nodes[1].role = "button".into();
        value.nodes[1].invoke = Some(vec![7, 0, 10, 0]);
        value
    }

    #[test]
    fn invoke_queue_preserves_accepted_identity_and_rolls_back_failed_notification() {
        let window = WindowAccessibility::new();
        let publisher = window.publisher();
        let mut value = invokable();
        let id = value.nodes[1].id.clone();
        publisher.publish(value.clone()).unwrap();
        assert_eq!(enqueue_invoke(&window.shared, &id, || true), OK);
        assert_eq!(enqueue_invoke(&window.shared, &id, || false), FAIL);
        value.nodes[1].invoke = Some(vec![8, 0, 11, 0]);
        publisher.publish(value).unwrap();
        let requests = publisher.take_invocations();
        assert_eq!(
            requests,
            vec![InvokeRequest {
                id: id.clone(),
                token: vec![7, 0, 10, 0]
            }]
        );
        assert!(publisher.take_invocations().is_empty());
        for _ in 0..64 {
            assert_eq!(enqueue_invoke(&window.shared, &id, || true), OK);
        }
        assert_eq!(
            enqueue_invoke(&window.shared, &id, || panic!("full queue must not notify")),
            OUT_OF_MEMORY
        );
        assert_eq!(publisher.take_invocations().len(), 64);
    }

    #[test]
    fn invoke_rejects_disabled_hidden_deleted_and_unsupported_targets() {
        let window = WindowAccessibility::new();
        let publisher = window.publisher();
        let mut value = invokable();
        let id = value.nodes[1].id.clone();
        value.nodes[1].enabled = false;
        publisher.publish(value.clone()).unwrap();
        assert_eq!(enqueue_invoke(&window.shared, &id, || true), NOT_ENABLED);
        assert_eq!(NOT_ENABLED.0 as u32, 0x80040200);
        assert_eq!(UNSUPPORTED.0 as u32, 0x80040204);
        value.nodes[1].enabled = true;
        value.nodes[1].invoke = None;
        publisher.publish(value.clone()).unwrap();
        assert_eq!(enqueue_invoke(&window.shared, &id, || true), UNSUPPORTED);
        value.nodes[1].invoke = Some(vec![1]);
        value.visible = false;
        publisher.publish(value).unwrap();
        assert_eq!(enqueue_invoke(&window.shared, &id, || true), UNAVAILABLE);
        publisher.publish(Snapshot::default()).unwrap();
        assert_eq!(enqueue_invoke(&window.shared, &id, || true), UNAVAILABLE);
        assert!(publisher.take_invocations().is_empty());
    }

    #[test]
    fn com_invoke_pattern_is_advertised_only_for_declared_button_actions() {
        let window = WindowAccessibility::new();
        window.publisher().publish(invokable()).unwrap();
        let simple: RawSimple = child(&window).cast().unwrap();
        // SAFETY: typed COM outputs; from_raw assumes ownership of the returned
        // reference, and each owned VARIANT is cleared exactly once.
        unsafe {
            let mut output = ptr::null_mut();
            assert_eq!(simple.GetPatternProvider(10000, &mut output), OK);
            assert!(!output.is_null());
            let invocation = RawInvoke::from_raw(output);
            let mut value = VARIANT::default();
            assert_eq!(simple.GetPropertyValue(30031, &mut value), OK);
            assert_eq!(value.Anonymous.Anonymous.Anonymous.boolVal.0, -1);
            assert!(VariantClear(&mut value).is_ok());
            window.publisher().publish(Snapshot::default()).unwrap();
            assert_eq!(invocation.Invoke(), UNAVAILABLE);
        }
    }
    #[test]
    fn event_identity_survives_node_removal_without_resurrecting_properties_or_actions() {
        let window = WindowAccessibility::new();
        let publisher = window.publisher();
        let value = invokable();
        let id = value.nodes[1].id.clone();
        publisher.publish(value).unwrap();
        assert!(publisher.queue_invoked(&id));
        assert!(!publisher.queue_invoked(&vec![1]));
        let source: RawFragment = EventSource {
            shared: window.shared.clone(),
            id: id.clone(),
            parents: vec![vec![1]],
        }
        .into();
        let simple: RawSimple = source.cast().unwrap();
        publisher.publish(Snapshot::default()).unwrap();
        assert_eq!(
            *window.shared.invoked.lock().unwrap(),
            vec![InvokedEvent {
                id,
                parents: vec![vec![1]]
            }]
        );
        // SAFETY: all outputs are writable locals. Each returned SAFEARRAY or
        // COM reference is released exactly once by its owning wrapper/API.
        unsafe {
            let mut array = ptr::null_mut();
            assert_eq!(source.GetRuntimeId(&mut array), OK);
            assert!(!array.is_null());
            assert!(SafeArrayDestroy(array).is_ok());
            let mut root = ptr::null_mut();
            assert_eq!(source.FragmentRoot(&mut root), OK);
            drop(RawRoot::from_raw(root));
            let mut value = VARIANT::default();
            assert_eq!(simple.GetPropertyValue(30005, &mut value), UNAVAILABLE);
            assert_eq!(value.Anonymous.Anonymous.vt.0, 0);
            let mut pattern = ptr::dangling_mut::<c_void>();
            assert_eq!(simple.GetPatternProvider(10000, &mut pattern), UNAVAILABLE);
            assert!(pattern.is_null());
            drop(window);
            assert_eq!(source.GetRuntimeId(&mut array), UNAVAILABLE);
            assert!(array.is_null());
        }
        publisher.flush_events(); // a closed window never calls out to UIA
        assert!(publisher.0.invoked.lock().unwrap().is_empty());
    }
    #[test]
    fn fragment_roots_preserve_the_windows_provider_com_identity() {
        let window = WindowAccessibility::new();
        let expected: IUnknown = window.root.cast().unwrap();
        assert!(
            window
                .shared
                .root
                .get()
                .and_then(|weak| weak.upgrade())
                .is_some()
        );
        let first: IUnknown = root_fragment(&window.shared).cast().unwrap();
        let second: IUnknown = root_fragment(&window.shared).cast().unwrap();
        assert_eq!(first, expected);
        assert_eq!(second, expected);
    }
}

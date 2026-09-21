use pecofence_plugin_api::{Error, Result, ScopeHandle, ScopeId, ScopeLease};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::{DrainReport, TaskSupervisor};

pub type Undo = Box<dyn FnOnce() -> Result<()>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    Root,
    Service,
    Provider,
    Instance,
    Mount,
    Gesture,
    Subscription,
    Window,
    Shell,
    Peek,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimePhase {
    Open,
    Stopping,
    Draining,
    Quarantined,
    Disposed,
}

pub struct LocalScope {
    id: ScopeId,
    generation: u64,
    phase: Cell<RuntimePhase>,
    closing: Cell<bool>,
    undos: RefCell<Vec<Undo>>,
    destroy_lifo: RefCell<Vec<Undo>>,
    errors: RefCell<Vec<Error>>,
}

impl ScopeLease for LocalScope {
    fn id(&self) -> ScopeId {
        self.id
    }
    fn generation(&self) -> u64 {
        self.generation
    }
    fn is_open(&self) -> bool {
        self.phase.get() == RuntimePhase::Open
    }
}

impl LocalScope {
    pub fn new(id: ScopeId, generation: u64) -> Rc<Self> {
        Rc::new(Self {
            id,
            generation,
            phase: Cell::new(RuntimePhase::Open),
            closing: Cell::new(false),
            undos: RefCell::new(Vec::new()),
            destroy_lifo: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
        })
    }

    pub fn id(&self) -> ScopeId {
        self.id
    }
    pub fn phase(&self) -> RuntimePhase {
        self.phase.get()
    }

    pub fn handle(this: &Rc<Self>) -> ScopeHandle {
        let lease: Rc<dyn ScopeLease> = this.clone();
        ScopeHandle::from_lease(&lease)
    }

    pub fn defer(&self, undo: Undo) -> Result<()> {
        if self.phase.get() == RuntimePhase::Open {
            self.undos.borrow_mut().push(undo);
            Ok(())
        } else {
            let result = undo();
            result.and(Err(Error::Closed))
        }
    }

    pub fn defer_destroy(&self, destroy: Undo) -> Result<()> {
        if self.phase.get() == RuntimePhase::Open {
            self.destroy_lifo.borrow_mut().push(destroy);
            Ok(())
        } else {
            let result = destroy();
            result.and(Err(Error::Closed))
        }
    }

    /// Creates a resource under a local guard and atomically transfers its inverse to this scope.
    pub fn acquire_and_track<T>(
        &self,
        acquire: impl FnOnce() -> Result<T>,
        release: impl FnOnce(T) -> Result<()> + 'static,
    ) -> Result<()>
    where
        T: 'static,
    {
        let resource = acquire()?;
        self.defer(Box::new(move || release(resource)))
    }

    pub fn mark_stopping(&self) {
        if self.phase.get() == RuntimePhase::Open {
            self.phase.set(RuntimePhase::Stopping);
        }
    }

    pub fn close_local(&self) {
        if matches!(
            self.phase.get(),
            RuntimePhase::Draining | RuntimePhase::Quarantined | RuntimePhase::Disposed
        ) {
            return;
        }
        if self.closing.replace(true) {
            return;
        }
        self.mark_stopping();
        let mut undos = std::mem::take(&mut *self.undos.borrow_mut());
        while let Some(undo) = undos.pop() {
            if let Err(error) = undo() {
                self.errors.borrow_mut().push(error);
            }
        }
        self.phase.set(RuntimePhase::Draining);
        self.closing.set(false);
    }

    pub fn finish_destroy(&self) {
        if self.phase.get() == RuntimePhase::Disposed {
            return;
        }
        let mut destroys = std::mem::take(&mut *self.destroy_lifo.borrow_mut());
        while let Some(destroy) = destroys.pop() {
            if let Err(error) = destroy() {
                self.errors.borrow_mut().push(error);
            }
        }
        self.phase.set(RuntimePhase::Disposed);
    }

    pub fn quarantine(&self) {
        if self.phase.get() == RuntimePhase::Draining {
            self.phase.set(RuntimePhase::Quarantined);
        }
    }

    pub fn take_errors(&self) -> Vec<Error> {
        std::mem::take(&mut *self.errors.borrow_mut())
    }
}

impl Drop for LocalScope {
    fn drop(&mut self) {
        self.close_local();
    }
}

struct ScopeRecord {
    kind: ScopeKind,
    parent: Option<ScopeId>,
    children: Vec<ScopeId>,
    local: Rc<LocalScope>,
}

pub struct ScopeTree {
    scopes: HashMap<ScopeId, ScopeRecord>,
    next_id: u64,
    next_generation: u64,
    root: ScopeId,
}

impl Default for ScopeTree {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopeTree {
    pub fn new() -> Self {
        let root = ScopeId(1);
        let local = LocalScope::new(root, 1);
        let scopes = HashMap::from([(
            root,
            ScopeRecord {
                kind: ScopeKind::Root,
                parent: None,
                children: Vec::new(),
                local,
            },
        )]);
        Self {
            scopes,
            next_id: 2,
            next_generation: 2,
            root,
        }
    }

    pub fn root(&self) -> ScopeId {
        self.root
    }

    pub fn create(&mut self, parent: ScopeId, kind: ScopeKind) -> Result<ScopeId> {
        if kind == ScopeKind::Root {
            return Err(Error::Invalid(
                "a second root scope is not permitted".into(),
            ));
        }
        let parent_record = self.scopes.get(&parent).ok_or(Error::Closed)?;
        if parent_record.local.phase() != RuntimePhase::Open {
            return Err(Error::Closed);
        }
        let id = ScopeId(self.next_id);
        self.next_id = self.next_id.checked_add(1).ok_or(Error::Exhausted)?;
        let generation = self.next_generation;
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        self.scopes.insert(
            id,
            ScopeRecord {
                kind,
                parent: Some(parent),
                children: Vec::new(),
                local: LocalScope::new(id, generation),
            },
        );
        self.scopes
            .get_mut(&parent)
            .expect("validated parent")
            .children
            .push(id);
        Ok(id)
    }

    pub fn kind(&self, id: ScopeId) -> Option<ScopeKind> {
        self.scopes.get(&id).map(|record| record.kind)
    }

    pub fn parent(&self, id: ScopeId) -> Option<ScopeId> {
        self.scopes.get(&id).and_then(|record| record.parent)
    }

    pub fn local(&self, id: ScopeId) -> Result<Rc<LocalScope>> {
        self.scopes
            .get(&id)
            .map(|record| record.local.clone())
            .ok_or(Error::Closed)
    }

    pub fn handle(&self, id: ScopeId) -> Result<ScopeHandle> {
        Ok(LocalScope::handle(&self.local(id)?))
    }

    fn postorder(&self, root: ScopeId, output: &mut Vec<ScopeId>) -> Result<()> {
        let record = self.scopes.get(&root).ok_or(Error::Closed)?;
        for child in &record.children {
            self.postorder(*child, output)?;
        }
        output.push(root);
        Ok(())
    }

    /// Phase one: closes every gate before invoking any undo, then runs child-first LIFO undo.
    pub fn begin_stop(&self, root: ScopeId) -> Result<Vec<ScopeId>> {
        let mut order = Vec::new();
        self.postorder(root, &mut order)?;
        for id in &order {
            self.scopes
                .get(id)
                .expect("postorder id")
                .local
                .mark_stopping();
        }
        for id in &order {
            self.scopes
                .get(id)
                .expect("postorder id")
                .local
                .close_local();
        }
        Ok(order)
    }

    pub fn finish_dispose(&self, root: ScopeId) -> Result<()> {
        let mut order = Vec::new();
        self.postorder(root, &mut order)?;
        for id in order {
            self.scopes
                .get(&id)
                .expect("postorder id")
                .local
                .finish_destroy();
        }
        Ok(())
    }

    /// Removes a fully disposed non-root subtree from the arena and its parent's child list.
    pub fn remove_disposed_subtree(&mut self, root: ScopeId) -> Result<usize> {
        if root == self.root || self.kind(root) == Some(ScopeKind::Service) {
            return Err(Error::Invalid(
                "root and service scopes cannot be removed".into(),
            ));
        }
        let mut order = Vec::new();
        self.postorder(root, &mut order)?;
        if order.iter().any(|id| {
            self.scopes
                .get(id)
                .is_some_and(|record| record.local.phase() != RuntimePhase::Disposed)
        }) {
            return Err(Error::Invalid("scope subtree is not disposed".into()));
        }
        let parent = self.scopes.get(&root).and_then(|record| record.parent);
        if let Some(parent) = parent.and_then(|id| self.scopes.get_mut(&id)) {
            parent.children.retain(|child| *child != root);
        }
        let count = order.len();
        for id in order {
            self.scopes.remove(&id);
        }
        Ok(count)
    }

    pub fn len(&self) -> usize {
        self.scopes.len()
    }

    /// Executes both stop phases for a scope subtree. Task handles remain owned by the
    /// supervisor on timeout and the corresponding scopes enter `Quarantined`.
    pub fn stop_and_drain(
        &self,
        root: ScopeId,
        supervisor: &mut TaskSupervisor,
        deadline: Instant,
    ) -> Result<DrainReport> {
        let order = self.begin_stop(root)?;
        let mut aggregate = DrainReport {
            joined: 0,
            pending: 0,
            native_pending: 0,
            quarantined: false,
        };
        for id in &order {
            let report = supervisor.drain_scope(*id, deadline);
            aggregate.joined += report.joined;
            aggregate.pending += report.pending;
            aggregate.native_pending += report.native_pending;
            aggregate.quarantined |= report.quarantined;
        }
        if aggregate.quarantined {
            for id in order {
                self.scopes
                    .get(&id)
                    .expect("postorder id")
                    .local
                    .quarantine();
            }
        } else {
            self.finish_dispose(root)?;
        }
        Ok(aggregate)
    }

    pub fn stop_and_drain_default(
        &self,
        root: ScopeId,
        supervisor: &mut TaskSupervisor,
    ) -> Result<DrainReport> {
        self.stop_and_drain(root, supervisor, Instant::now() + Duration::from_secs(5))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn effects_execute_in_reverse_registration_order() {
        let scope = LocalScope::new(ScopeId(1), 1);
        let calls = Rc::new(RefCell::new(Vec::new()));
        for value in 1..=3 {
            let calls = calls.clone();
            scope
                .defer(Box::new(move || {
                    calls.borrow_mut().push(value);
                    Ok(())
                }))
                .unwrap();
        }
        scope.close_local();
        assert_eq!(*calls.borrow(), [3, 2, 1]);
    }

    #[test]
    fn failed_registration_rolls_resource_back_immediately() {
        let scope = LocalScope::new(ScopeId(1), 1);
        scope.mark_stopping();
        let released = Rc::new(Cell::new(false));
        let released_for_undo = released.clone();
        let result = scope.acquire_and_track(
            || Ok(17),
            move |value| {
                assert_eq!(value, 17);
                released_for_undo.set(true);
                Ok(())
            },
        );
        assert_eq!(result, Err(Error::Closed));
        assert!(released.get());
    }

    #[test]
    fn closed_scope_rejects_children_and_effects() {
        let mut tree = ScopeTree::new();
        let plugin = tree.create(tree.root(), ScopeKind::Provider).unwrap();
        tree.begin_stop(plugin).unwrap();
        assert_eq!(tree.create(plugin, ScopeKind::Instance), Err(Error::Closed));
        let called = Rc::new(Cell::new(false));
        let copy = called.clone();
        assert_eq!(
            tree.local(plugin).unwrap().defer(Box::new(move || {
                copy.set(true);
                Ok(())
            })),
            Err(Error::Closed)
        );
        assert!(called.get());
    }

    #[test]
    fn descendants_are_closed_before_any_callback() {
        let mut tree = ScopeTree::new();
        let plugin = tree.create(tree.root(), ScopeKind::Provider).unwrap();
        let instance = tree.create(plugin, ScopeKind::Instance).unwrap();
        let instance_handle = tree.handle(instance).unwrap();
        tree.local(plugin)
            .unwrap()
            .defer(Box::new(move || {
                assert_eq!(instance_handle.check(), Err(Error::Closed));
                Ok(())
            }))
            .unwrap();
        tree.begin_stop(plugin).unwrap();
    }

    #[test]
    fn two_phase_stop_waits_for_supervised_tasks_before_disposal() {
        let mut tree = ScopeTree::new();
        let instance = tree.create(tree.root(), ScopeKind::Instance).unwrap();
        let generation = tree.handle(instance).unwrap().generation().unwrap();
        let mut supervisor = TaskSupervisor::new();
        supervisor
            .spawn(instance, generation, |cancel| {
                while !cancel.is_cancelled() {
                    std::thread::yield_now();
                }
            })
            .unwrap();
        let report = tree
            .stop_and_drain(
                instance,
                &mut supervisor,
                Instant::now() + Duration::from_secs(1),
            )
            .unwrap();
        assert!(!report.quarantined);
        assert_eq!(
            tree.local(instance).unwrap().phase(),
            RuntimePhase::Disposed
        );
    }
}

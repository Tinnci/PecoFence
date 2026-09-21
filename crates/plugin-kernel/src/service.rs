use pecofence_plugin_api::{Capability, Error, Result, ScopeHandle, ScopeId, ServiceCell};
use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Identity {
    namespace: &'static str,
    name: &'static str,
    api_major: u16,
    service_type: TypeId,
}

pub struct ServiceKey<S: ?Sized + 'static> {
    namespace: &'static str,
    name: &'static str,
    api_major: u16,
    marker: PhantomData<fn() -> S>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServiceDependency(Identity);

impl<S: ?Sized + 'static> Copy for ServiceKey<S> {}
impl<S: ?Sized + 'static> Clone for ServiceKey<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: ?Sized + 'static> ServiceKey<S> {
    pub const fn new(namespace: &'static str, name: &'static str, api_major: u16) -> Self {
        Self {
            namespace,
            name,
            api_major,
            marker: PhantomData,
        }
    }

    fn identity(self) -> Identity {
        Identity {
            namespace: self.namespace,
            name: self.name,
            api_major: self.api_major,
            service_type: TypeId::of::<S>(),
        }
    }

    pub fn dependency(self) -> ServiceDependency {
        ServiceDependency(self.identity())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceState {
    Ready,
    Revoking,
}

struct Entry {
    provider: ScopeId,
    generation: u64,
    state: ServiceState,
    cell: Box<dyn Any>,
    revoke: Box<dyn Fn()>,
    dependencies: Vec<Identity>,
    consumers: HashSet<ScopeId>,
}

/// Owns a revoked generation until every dependent consumer has crossed its drain barrier.
pub struct RevocationBarrier {
    identity: Identity,
    generation: u64,
    held_cell: Option<Box<dyn Any>>,
    pending_consumers: HashSet<ScopeId>,
}

impl RevocationBarrier {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn pending_consumers(&self) -> impl Iterator<Item = ScopeId> + '_ {
        self.pending_consumers.iter().copied()
    }
    pub fn acknowledge(&mut self, consumer: ScopeId) {
        self.pending_consumers.remove(&consumer);
        if self.pending_consumers.is_empty() {
            self.held_cell.take();
        }
    }
    pub fn is_complete(&self) -> bool {
        self.pending_consumers.is_empty()
    }
    pub fn service_name(&self) -> &'static str {
        self.identity.name
    }
}

#[derive(Default)]
pub struct ServiceRegistry {
    services: HashMap<Identity, Entry>,
    next_generation: u64,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            next_generation: 1,
        }
    }

    pub fn publish<S: ?Sized + 'static>(
        &mut self,
        key: ServiceKey<S>,
        provider: ScopeId,
        service: Box<S>,
    ) -> Result<u64> {
        self.publish_with_dependencies(key, provider, service, &[])
    }

    pub fn publish_with_dependencies<S: ?Sized + 'static>(
        &mut self,
        key: ServiceKey<S>,
        provider: ScopeId,
        service: Box<S>,
        dependencies: &[ServiceDependency],
    ) -> Result<u64> {
        let identity = key.identity();
        if self.services.contains_key(&identity) {
            return Err(Error::Duplicate);
        }
        let dependency_ids: Vec<_> = dependencies
            .iter()
            .map(|dependency| dependency.0.clone())
            .collect();
        if dependency_ids
            .iter()
            .any(|dependency| !self.services.contains_key(dependency))
        {
            return Err(Error::Revoked);
        }
        // Existing entries cannot depend on an unpublished key, so adding this node can only
        // form a cycle if it names itself directly.
        if dependency_ids
            .iter()
            .any(|dependency| dependency == &identity)
        {
            return Err(Error::DependencyCycle);
        }
        let generation = self.next_generation;
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        let cell: Rc<ServiceCell<S>> = Rc::new(ServiceCell::new(generation, service));
        let revoke_cell = cell.clone();
        self.services.insert(
            identity,
            Entry {
                provider,
                generation,
                state: ServiceState::Ready,
                cell: Box::new(cell),
                revoke: Box::new(move || revoke_cell.revoke()),
                dependencies: dependency_ids,
                consumers: HashSet::new(),
            },
        );
        Ok(generation)
    }

    pub fn resolve<S: ?Sized + 'static>(
        &mut self,
        key: ServiceKey<S>,
        consumer_scope: ScopeId,
        consumer: ScopeHandle,
    ) -> Result<Capability<S>> {
        consumer.check()?;
        let entry = self
            .services
            .get_mut(&key.identity())
            .ok_or(Error::Revoked)?;
        if entry.state != ServiceState::Ready {
            return Err(Error::Revoked);
        }
        let cell = entry
            .cell
            .downcast_ref::<Rc<ServiceCell<S>>>()
            .ok_or_else(|| {
                Error::Invalid("typed service key did not match stored service".into())
            })?;
        entry.consumers.insert(consumer_scope);
        Ok(Capability::new(cell, consumer))
    }

    pub fn state<S: ?Sized + 'static>(&self, key: ServiceKey<S>) -> Option<ServiceState> {
        self.services.get(&key.identity()).map(|entry| entry.state)
    }

    pub fn provider<S: ?Sized + 'static>(&self, key: ServiceKey<S>) -> Option<ScopeId> {
        self.services
            .get(&key.identity())
            .map(|entry| entry.provider)
    }

    /// Revokes the selected generation and every service that transitively depends on it.
    /// Returned barriers retain backend ownership until consumers acknowledge drain completion.
    pub fn revoke<S: ?Sized + 'static>(
        &mut self,
        key: ServiceKey<S>,
    ) -> Result<Vec<RevocationBarrier>> {
        let target = key.identity();
        if !self.services.contains_key(&target) {
            return Err(Error::Revoked);
        }
        let mut affected = HashSet::from([target.clone()]);
        loop {
            let before = affected.len();
            for (identity, entry) in &self.services {
                if entry
                    .dependencies
                    .iter()
                    .any(|dependency| affected.contains(dependency))
                {
                    affected.insert(identity.clone());
                }
            }
            if affected.len() == before {
                break;
            }
        }
        let mut barriers = Vec::new();
        for identity in affected {
            let mut entry = self
                .services
                .remove(&identity)
                .expect("affected service exists");
            entry.state = ServiceState::Revoking;
            (entry.revoke)();
            barriers.push(RevocationBarrier {
                identity,
                generation: entry.generation,
                held_cell: Some(entry.cell),
                pending_consumers: entry.consumers,
            });
        }
        Ok(barriers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LocalScope, ScopeTree};
    use pecofence_plugin_api::ScopeId;

    trait Number {
        fn get(&self) -> u32;
    }
    struct Seven;
    impl Number for Seven {
        fn get(&self) -> u32 {
            7
        }
    }
    const NUMBER: ServiceKey<dyn Number> = ServiceKey::new("host", "number", 1);

    #[test]
    fn dependency_resolution_and_revocation_barrier() {
        let tree = ScopeTree::new();
        let consumer = tree.root();
        let mut registry = ServiceRegistry::new();
        registry
            .publish(NUMBER, ScopeId(90), Box::new(Seven))
            .unwrap();
        let capability = registry
            .resolve(NUMBER, consumer, tree.handle(consumer).unwrap())
            .unwrap();
        assert_eq!(capability.with(|service| Ok(service.get())), Ok(7));
        let mut barriers = registry.revoke(NUMBER).unwrap();
        assert_eq!(
            capability.with(|service| Ok(service.get())),
            Err(Error::Revoked)
        );
        assert_eq!(barriers.len(), 1);
        assert!(!barriers[0].is_complete());
        barriers[0].acknowledge(consumer);
        assert!(barriers[0].is_complete());
        let _ = LocalScope::new(ScopeId(2), 2);
    }
}

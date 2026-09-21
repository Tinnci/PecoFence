use pecofence_plugin_api::{Error, Result, ScopeHandle};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

type Callback<T> = Box<dyn FnMut(&T, &EventRegistration<T>)>;

struct Entry<T> {
    generation: u64,
    owner: ScopeHandle,
    alive: Rc<Cell<bool>>,
    callback: Option<Callback<T>>,
}

struct Inner<T> {
    next_id: u64,
    entries: HashMap<u64, Entry<T>>,
}

pub struct EventSink<T>(Rc<RefCell<Inner<T>>>);

impl<T> Clone for EventSink<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

pub struct EventRegistration<T> {
    sink: Weak<RefCell<Inner<T>>>,
    id: u64,
    generation: u64,
    alive: Rc<Cell<bool>>,
}

impl<T> Clone for EventRegistration<T> {
    fn clone(&self) -> Self {
        Self {
            sink: self.sink.clone(),
            id: self.id,
            generation: self.generation,
            alive: self.alive.clone(),
        }
    }
}

impl<T: 'static> EventSink<T> {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(Inner {
            next_id: 1,
            entries: HashMap::new(),
        })))
    }

    pub fn register(
        &self,
        owner: ScopeHandle,
        callback: impl FnMut(&T, &EventRegistration<T>) + 'static,
    ) -> Result<EventRegistration<T>> {
        let generation = owner.generation()?;
        let mut inner = self.0.borrow_mut();
        let id = inner.next_id;
        inner.next_id = inner.next_id.checked_add(1).ok_or(Error::Exhausted)?;
        let alive = Rc::new(Cell::new(true));
        inner.entries.insert(
            id,
            Entry {
                generation,
                owner,
                alive: alive.clone(),
                callback: Some(Box::new(callback)),
            },
        );
        Ok(EventRegistration {
            sink: Rc::downgrade(&self.0),
            id,
            generation,
            alive,
        })
    }

    pub fn emit(&self, event: &T) {
        let ids: Vec<u64> = self.0.borrow().entries.keys().copied().collect();
        for id in ids {
            let extracted = {
                let mut inner = self.0.borrow_mut();
                let Some(entry) = inner.entries.get_mut(&id) else {
                    continue;
                };
                if !entry.alive.get() || entry.owner.generation() != Ok(entry.generation) {
                    inner.entries.remove(&id);
                    continue;
                }
                entry
                    .callback
                    .take()
                    .map(|callback| (callback, entry.generation, entry.alive.clone()))
            };
            let Some((mut callback, generation, alive)) = extracted else {
                continue;
            };
            let registration = EventRegistration {
                sink: Rc::downgrade(&self.0),
                id,
                generation,
                alive: alive.clone(),
            };
            callback(event, &registration);
            let mut inner = self.0.borrow_mut();
            if alive.get()
                && inner.entries.get(&id).is_some_and(|entry| {
                    entry.generation == generation && entry.owner.generation() == Ok(generation)
                })
            {
                inner.entries.get_mut(&id).expect("checked entry").callback = Some(callback);
            } else {
                inner.entries.remove(&id);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.0.borrow().entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T: 'static> Default for EventSink<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> EventRegistration<T> {
    pub fn revoke(&self) {
        self.alive.set(false);
        if let Some(sink) = self.sink.upgrade() {
            sink.borrow_mut().entries.remove(&self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::LocalScope;
    use pecofence_plugin_api::ScopeId;

    #[test]
    fn callback_can_revoke_itself_without_reinsertion() {
        let owner = LocalScope::new(ScopeId(1), 8);
        let sink = EventSink::new();
        let count = Rc::new(Cell::new(0));
        let count_in_callback = count.clone();
        let registration = sink
            .register(LocalScope::handle(&owner), move |_, self_token| {
                count_in_callback.set(count_in_callback.get() + 1);
                self_token.revoke();
            })
            .unwrap();
        // Keep the caller token alive; self-revocation must still remove the route.
        sink.emit(&());
        sink.emit(&());
        assert_eq!(count.get(), 1);
        assert!(sink.is_empty());
        drop(registration);
    }
}

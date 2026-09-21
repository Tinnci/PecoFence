use pecofence_plugin_api::{Error, Result, ScopeId, Token};
use std::{
    cell::RefCell,
    collections::HashSet,
    rc::{Rc, Weak},
    time::Duration,
};

#[derive(Clone, Copy, Debug)]
pub struct PollBudget {
    pub max_events: usize,
    pub max_duration: Duration,
}
impl Default for PollBudget {
    fn default() -> Self {
        Self {
            max_events: 64,
            max_duration: Duration::from_millis(2),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DrainState {
    Pending {
        callbacks: usize,
        cleanup: usize,
        native: usize,
    },
    Complete,
}

#[derive(Default)]
struct LedgerInner {
    next: u64,
    callbacks: HashSet<Token>,
    cleanup: HashSet<Token>,
    native: HashSet<Token>,
}

#[derive(Clone, Default)]
pub struct DrainLedger(Rc<RefCell<LedgerInner>>);

impl DrainLedger {
    fn allocate(&self) -> Result<Token> {
        let mut inner = self.0.borrow_mut();
        inner.next = inner.next.checked_add(1).ok_or(Error::Exhausted)?;
        Ok(Token(inner.next))
    }
    pub fn enter_callback(&self, _owner: ScopeId) -> Result<CallbackGuard> {
        let token = self.allocate()?;
        self.0.borrow_mut().callbacks.insert(token);
        Ok(CallbackGuard {
            ledger: Rc::downgrade(&self.0),
            token,
        })
    }
    pub fn begin_cleanup(&self, _owner: ScopeId) -> Result<CleanupGuard> {
        let token = self.allocate()?;
        self.0.borrow_mut().cleanup.insert(token);
        Ok(CleanupGuard {
            ledger: Rc::downgrade(&self.0),
            token,
        })
    }
    pub fn register_native(&self) -> Result<Token> {
        let token = self.allocate()?;
        self.0.borrow_mut().native.insert(token);
        Ok(token)
    }
    pub fn record_native_completion(&self, token: Token) -> bool {
        self.0.borrow_mut().native.remove(&token)
    }
    pub fn poll(&self) -> DrainState {
        let inner = self.0.borrow();
        if inner.callbacks.is_empty() && inner.cleanup.is_empty() && inner.native.is_empty() {
            DrainState::Complete
        } else {
            DrainState::Pending {
                callbacks: inner.callbacks.len(),
                cleanup: inner.cleanup.len(),
                native: inner.native.len(),
            }
        }
    }
}

pub struct CallbackGuard {
    ledger: Weak<RefCell<LedgerInner>>,
    token: Token,
}
impl Drop for CallbackGuard {
    fn drop(&mut self) {
        if let Some(ledger) = self.ledger.upgrade() {
            ledger.borrow_mut().callbacks.remove(&self.token);
        }
    }
}
pub struct CleanupGuard {
    ledger: Weak<RefCell<LedgerInner>>,
    token: Token,
}
impl Drop for CleanupGuard {
    fn drop(&mut self) {
        if let Some(ledger) = self.ledger.upgrade() {
            ledger.borrow_mut().cleanup.remove(&self.token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guards_hold_drain_until_drop_and_native_completion_is_idempotent() {
        let ledger = DrainLedger::default();
        let callback = ledger.enter_callback(ScopeId(1)).unwrap();
        let cleanup = ledger.begin_cleanup(ScopeId(1)).unwrap();
        let native = ledger.register_native().unwrap();
        assert!(matches!(
            ledger.poll(),
            DrainState::Pending {
                callbacks: 1,
                cleanup: 1,
                native: 1
            }
        ));
        assert!(ledger.record_native_completion(native));
        assert!(!ledger.record_native_completion(native));
        drop(callback);
        drop(cleanup);
        assert_eq!(ledger.poll(), DrainState::Complete);
    }
}

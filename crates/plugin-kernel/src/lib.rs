//! Cordis-style lifecycle kernel for in-process PecoFence plugins.

mod activation;
mod drain;
mod event;
mod panel;
mod scope;
mod service;
mod supervisor;

pub use activation::{ActivationFailure, ActivationTransaction};
pub use drain::{CallbackGuard, CleanupGuard, DrainLedger, DrainState, PollBudget};
pub use event::{EventRegistration, EventSink};
pub use panel::{InstanceRecord, PanelManager};
pub use scope::{LocalScope, RuntimePhase, ScopeKind, ScopeTree, Undo};
pub use service::{
    RevocationBarrier, ServiceDependency, ServiceKey, ServiceRegistry, ServiceState,
};
pub use supervisor::{CancellationToken, DrainReport, TaskSupervisor};

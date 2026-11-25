//! Cordis-style lifecycle kernel for in-process PecoFence plugins.

mod activation;
mod scope;
mod service;
mod supervisor;

pub use activation::{ActivationFailure, ActivationTransaction};
pub use scope::{LocalScope, RuntimePhase, ScopeKind, ScopeTree, Undo};
pub use service::{
    RevocationBarrier, ServiceDependency, ServiceKey, ServiceRegistry, ServiceState,
};
pub use supervisor::{CancellationToken, DrainReport, TaskSupervisor};

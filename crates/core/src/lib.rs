//! Platform-independent core: workspace ownership, rule engine and persistence.
//! This crate must never depend on Windows APIs so it can be unit-tested anywhere.

pub mod brand;
pub mod config_store;
pub mod date_group;
pub mod geometry;
pub mod i18n;
pub mod model;
pub mod portal;
pub mod rules;
pub mod settings_protocol;
pub mod workspace;

pub use config_store::{BackupStatus, ConfigStore, FreshReason, LoadOutcome, SaveReceipt};
pub use date_group::{CivilDate, DateBucket, date_bucket};
pub use model::*;
pub use rules::{Cond, Decision, ItemFacts, Rule, RuleSet, StrOp, Target, TypeCategory};
pub use workspace::{TabDetach, Transition, Workspace, WorkspaceError};

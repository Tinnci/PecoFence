//! Owned folder-read contracts. No persistence, window handles or platform interfaces.

use crate::FenceId;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalRead {
    pub fence: FenceId,
    /// Source lifetime; changes on navigation/recreation, not on same-path notifications.
    pub generation: u64,
    /// Never reused within a runtime. Correlates one dispatched read and its terminal result.
    pub request_id: u64,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalEntry {
    pub path: PathBuf,
    pub display_name: String,
    pub is_folder: bool,
    pub attributes: u32,
    pub mtime: i64,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadFailure {
    Worker(String),
    Apartment(String),
    Open(String),
    Traversal(String),
    Metadata(String),
    DisplayName(String),
    Limit { max_entries: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortalOutcome {
    Complete(Vec<PortalEntry>),
    /// Entries are diagnostic only, never substituted for the last complete snapshot.
    Partial {
        entries: Vec<PortalEntry>,
        failure: ReadFailure,
    },
    Unavailable(ReadFailure),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalResult {
    pub request: PortalRead,
    pub outcome: PortalOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortalHealth {
    Loading,
    Ready,
    Stale(ReadFailure),
}

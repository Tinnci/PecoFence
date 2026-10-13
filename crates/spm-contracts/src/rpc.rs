use crate::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    Multiplexing,
    Pagination,
    RefreshEvents,
    Navigation,
    Briefing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RpcMethod {
    Hello,
    GetCapabilities,
    Subscribe,
    Unsubscribe,
    QueryPage,
    Refresh,
    ResolveNavigation,
    BuildBriefing,
    Ping,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportLimits {
    pub max_frame_bytes: u32,
    pub max_page_size: u16,
    pub max_subscriptions: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloRequest {
    pub client_build: String,
    pub protocol_major: u16,
    pub protocol_minor: u16,
    pub features: BTreeSet<Feature>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloResponse {
    pub daemon_build: String,
    pub daemon_session: DaemonSessionId,
    pub protocol_major: u16,
    pub protocol_minor: u16,
    pub features: BTreeSet<Feature>,
    pub limits: TransportLimits,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetCapabilitiesRequest {}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigEntry {
    pub id: String,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetCapabilitiesResponse {
    pub query_views: BTreeSet<ViewKind>,
    pub methods: BTreeSet<RpcMethod>,
    pub config_entry: Option<ConfigEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscribeRequest {
    pub query: ProjectQuery,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscribeResponse {
    pub subscription_id: SubscriptionId,
    pub accepted_query: ProjectQuery,
    pub initial_revision: Revision,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsubscribeRequest {
    pub subscription_id: SubscriptionId,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsubscribeResponse {
    pub removed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryPageRequest {
    pub subscription_id: SubscriptionId,
    pub revision: Revision,
    pub cursor: Option<PageCursor>,
    pub page_size: u16,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryPageResponse {
    pub revision: Revision,
    pub items: Vec<WorkItem>,
    pub next_cursor: Option<PageCursor>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshRequest {
    pub project_id: ProjectId,
    pub delivery_scope_id: DeliveryScopeId,
    pub idempotency_key: IdempotencyKey,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshDisposition {
    Accepted,
    Coalesced,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshResponse {
    pub operation_id: OperationId,
    pub disposition: RefreshDisposition,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveNavigationRequest {
    pub source: SourceRef,
    pub revision: Revision,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavigationTarget {
    pub system: SourceSystem,
    pub configured_origin_id: String,
    pub relative_record_path: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveNavigationResponse {
    pub targets: Vec<NavigationTarget>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildBriefingRequest {
    pub project_id: ProjectId,
    pub delivery_scope_id: DeliveryScopeId,
    pub revision: Revision,
    pub locale: String,
    pub timezone: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceMarker {
    pub label: String,
    pub source_refs: Vec<SourceRef>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildBriefingResponse {
    pub briefing_id: BriefingId,
    pub revision: Revision,
    pub text: String,
    pub html: Option<String>,
    pub evidence_markers: Vec<EvidenceMarker>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PingRequest {
    pub correlation_id: Uuid,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PingResponse {
    pub correlation_id: Uuid,
    pub daemon_session: DaemonSessionId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Request {
    Hello(HelloRequest),
    GetCapabilities(GetCapabilitiesRequest),
    Subscribe(SubscribeRequest),
    Unsubscribe(UnsubscribeRequest),
    QueryPage(QueryPageRequest),
    Refresh(RefreshRequest),
    ResolveNavigation(ResolveNavigationRequest),
    BuildBriefing(BuildBriefingRequest),
    Ping(PingRequest),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "result", rename_all = "snake_case")]
pub enum Response {
    Hello(HelloResponse),
    GetCapabilities(GetCapabilitiesResponse),
    Subscribe(SubscribeResponse),
    Unsubscribe(UnsubscribeResponse),
    QueryPage(QueryPageResponse),
    Refresh(RefreshResponse),
    ResolveNavigation(ResolveNavigationResponse),
    BuildBriefing(BuildBriefingResponse),
    Ping(PingResponse),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshOutcome {
    Completed,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationCompleted {
    pub operation_id: OperationId,
    pub outcome: RefreshOutcome,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
// Snapshot payloads legitimately dwarf control events; this enum is serialized
// immediately on the wire and boxing would churn every consumer's matches.
#[allow(clippy::large_enum_variant)]
pub enum Event {
    Snapshot(ProjectSnapshot),
    OperationCompleted(OperationCompleted),
}

pub type Extensions = BTreeMap<String, serde_json::Value>;

impl Request {
    pub fn method(&self) -> RpcMethod {
        match self {
            Self::Hello(_) => RpcMethod::Hello,
            Self::GetCapabilities(_) => RpcMethod::GetCapabilities,
            Self::Subscribe(_) => RpcMethod::Subscribe,
            Self::Unsubscribe(_) => RpcMethod::Unsubscribe,
            Self::QueryPage(_) => RpcMethod::QueryPage,
            Self::Refresh(_) => RpcMethod::Refresh,
            Self::ResolveNavigation(_) => RpcMethod::ResolveNavigation,
            Self::BuildBriefing(_) => RpcMethod::BuildBriefing,
            Self::Ping(_) => RpcMethod::Ping,
        }
    }
    pub fn validate(&self) -> Result<(), ContractError> {
        if let Self::QueryPage(q) = self {
            if q.revision.0 == 0 {
                return Err(ContractError::InvalidField {
                    field: "revision",
                    reason: "page revision must be nonzero".into(),
                });
            }
            if !(1..=MAX_PAGE_SIZE).contains(&q.page_size) {
                return Err(ContractError::InvalidField {
                    field: "page_size",
                    reason: format!("must be 1..={MAX_PAGE_SIZE}"),
                });
            }
        }
        Ok(())
    }
}

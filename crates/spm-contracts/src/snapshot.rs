use crate::{
    BaselineId, ContractError, DaemonSessionId, DeliveryScopeId, ProjectId, RecordId, Revision,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceSystem {
    Jira,
    Meegle,
    Gerrit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    Complete,
    Partial,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceErrorCode {
    Unavailable,
    PermissionDenied,
    RateLimited,
    InvalidData,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEvidence {
    pub source: SourceSystem,
    pub tenant: String,
    pub observed_at: DateTime<Utc>,
    pub source_revision: Option<String>,
    pub watermark: Option<String>,
    pub coverage: CoverageState,
    pub page_complete: bool,
    pub permission_complete: bool,
    pub freshness_ttl_secs: u32,
    pub error: Option<SourceErrorCode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Satisfied,
    Unsatisfied,
    Unknown,
    NotApplicable,
}

pub type GateState = GateStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Applicability {
    Applicable,
    NotApplicable,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RationalThreshold {
    pub p: u64,
    pub q: u64,
}

impl RationalThreshold {
    pub fn new(p: u64, q: u64) -> Result<Self, ContractError> {
        if q == 0 {
            return Err(ContractError::InvalidField {
                field: "threshold.q",
                reason: "must be greater than zero".into(),
            });
        }
        Ok(Self { p, q })
    }
    pub fn validate(self) -> Result<(), ContractError> {
        Self::new(self.p, self.q).map(|_| ())
    }
}

pub type Ratio = RationalThreshold;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverallGate {
    pub state: GateStatus,
    pub unsatisfied_count: u64,
    pub unknown_count: u64,
    pub applicable_count: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateEvaluation {
    pub gate_id: String,
    pub label: String,
    pub state: GateStatus,
    pub applicability: Applicability,
    pub n: Option<u64>,
    pub v: Option<u64>,
    pub threshold: Option<RationalThreshold>,
    pub required: Option<u64>,
    pub gap: Option<u64>,
    pub deadline: Option<DateTime<Utc>>,
    pub reason_codes: Vec<String>,
    pub evidence_refs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRef {
    pub system: SourceSystem,
    pub tenant: String,
    pub project: String,
    pub record_kind: String,
    pub record_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemKind {
    Defect,
    Task,
    Obligation,
    Verification,
    Change,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationState {
    Confirmed,
    Candidate,
    Rejected,
    Unrelated,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkItem {
    pub record_id: RecordId,
    pub kind: WorkItemKind,
    pub title: String,
    pub owner: Option<String>,
    pub due_at: Option<DateTime<Utc>>,
    pub next_step: Option<String>,
    pub source_refs: Vec<SourceRef>,
    pub relation_state: RelationState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MilestoneSummary {
    pub id: String,
    pub name: String,
    pub due_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSummary {
    pub total: u64,
    pub severe_open: u64,
    pub pending_verification: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomerObligationSummary {
    pub total: u64,
    pub accepted: u64,
    pub pending: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationSummary {
    pub total: u64,
    pub passed: u64,
    pub failed: u64,
    pub unknown: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeSummary {
    pub total: u64,
    pub merged: u64,
    pub open: u64,
    pub unknown: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrendBasis {
    CohortFixed,
    LiveScope,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrendPoint {
    pub at: DateTime<Utc>,
    pub total: Option<u64>,
    pub completed: Option<u64>,
    pub plan: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrendSummary {
    pub basis: TrendBasis,
    pub points: Vec<TrendPoint>,
    pub has_plan_line: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSnapshot {
    pub project_id: ProjectId,
    pub delivery_scope_id: DeliveryScopeId,
    pub baseline_id: Option<BaselineId>,
    pub policy_revision: u64,
    pub daemon_session: DaemonSessionId,
    pub revision: Revision,
    pub computed_at: DateTime<Utc>,
    pub project_timezone: String,
    pub freshness_ttl_secs: u32,
    pub sources: Vec<SourceEvidence>,
    pub overall_gate: OverallGate,
    pub gates: Vec<GateEvaluation>,
    pub next_milestone: Option<MilestoneSummary>,
    pub work_summary: WorkSummary,
    pub preview_items: Vec<WorkItem>,
    pub customer_obligations: CustomerObligationSummary,
    pub verification: VerificationSummary,
    pub merge: MergeSummary,
    pub trend: TrendSummary,
}

impl ProjectSnapshot {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.project_timezone.trim().is_empty() || !self.project_timezone.contains('/') {
            return Err(ContractError::InvalidField {
                field: "project_timezone",
                reason: "expected an IANA timezone name".into(),
            });
        }
        for gate in &self.gates {
            if let Some(threshold) = gate.threshold {
                threshold.validate()?;
            }
        }
        Ok(())
    }
}

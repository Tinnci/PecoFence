//! IPD issue tracker: reads and local judgments. SPM computes every number and
//! returns its inputs; PecoFence presents. Review and judgment requests only record a
//! local decision in people-cli. Nothing here writes to Lark, Meegle, Gerrit or Jenkins.
use crate::{ContractError, ProjectId, SourceSystem};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_ISSUE_LIMIT: u16 = 200;
pub const MAX_ISSUE_TITLE: usize = 256;
pub const MAX_ISSUE_TEXT: usize = 512;
pub const MAX_ISSUE_NAME: usize = 64;
pub const MAX_ISSUE_KEY: usize = 128;
pub const MAX_ISSUE_TRACE: usize = 8;
pub const MAX_ISSUE_QUEUES: usize = 5;
pub const MAX_ISSUE_SOURCES: usize = 4;
pub const MAX_ISSUE_MESSAGES: usize = 20;
pub const MAX_ISSUE_EVIDENCE: usize = 8;
pub const MAX_ISSUE_PROPOSALS: usize = 20;
pub const MAX_ISSUE_PEOPLE: usize = 12;
pub const MAX_ISSUE_RELATIONS: usize = 4;
pub const MAX_ISSUE_HANDLES: usize = 4;
pub const MAX_ISSUE_WHO: usize = 5;
pub const MAX_ISSUE_REASONS: usize = 6;
pub const MAX_ISSUE_SUGGESTIONS: usize = 8;
pub const MAX_ISSUE_CRITERIA: usize = 8;
pub const MAX_ISSUE_CONTRIBUTORS: usize = 100;
pub const MAX_GATE_TRACK: usize = 16;
pub const MAX_WHAT_IF_STEPS: u16 = 20;
pub const MAX_SEARCH_LIMIT: u16 = 50;
pub const MAX_SEARCH_QUERY: usize = 256;
pub const MAX_SEARCH_SNIPPET: usize = 512;
pub const MAX_SEARCH_SPANS: usize = 16;

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn invalid(reason: impl Into<String>) -> ContractError {
    ContractError::InvalidField {
        field: "issues",
        reason: reason.into(),
    }
}

fn plain(s: &str, max: usize) -> bool {
    s.len() <= max && !s.chars().any(char::is_control)
}

fn non_empty_plain(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.trim() == s && plain(s, max)
}

/// `R-102`, `D-2291`, `K-014`, `A-063`: one to four capital letters, a dash, one to eight digits.
pub fn validate_issue_id(id: &str) -> Result<(), ContractError> {
    let Some((prefix, digits)) = id.split_once('-') else {
        return Err(invalid("issue id must look like R-102"));
    };
    if !(1..=4).contains(&prefix.len())
        || !prefix.bytes().all(|b| b.is_ascii_uppercase())
        || !(1..=8).contains(&digits.len())
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid("issue id must look like R-102"));
    }
    Ok(())
}

/// Keys name a subject or object of a suggestion: `issue:R-102`, `record:<id>`,
/// `defect:D-2291` or `person:<entity>`.
pub fn validate_suggestion_key(key: &str) -> Result<(), ContractError> {
    let Some((prefix, value)) = key.split_once(':') else {
        return Err(invalid("suggestion key needs a prefix"));
    };
    if !matches!(prefix, "issue" | "record" | "defect" | "person")
        || !non_empty_plain(value, MAX_ISSUE_KEY)
    {
        return Err(invalid("unsupported suggestion key"));
    }
    Ok(())
}

fn finite_non_negative(v: f64) -> bool {
    v.is_finite() && v >= 0.0
}

/// IPD item types. Requirements keep the IR → SR → AR trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum IssueKind {
    Ar,
    Sr,
    Ir,
    Dts,
    Risk,
    Ai,
}

/// Defect severity. Weights are configured per project, not fixed here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefectSeverity {
    Fatal,
    Major,
    Normal,
    Hint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueQueue {
    NeedsMyReply,
    AwaitingAssignmentConfirmation,
    BlocksNextGate,
    LegacyPending,
    SourceConflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentState {
    Confirmed,
    Proposed,
    Conflict,
    Unspecified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskState {
    Open,
    Closed,
    Accepted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyState {
    Requested,
    Approved,
    Rejected,
}

/// Gates in IPD order. TR1-TR6 are technical reviews.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateKind {
    Cdcp,
    Pdcp,
    Tr1,
    Tr2,
    Tr3,
    Tr4,
    Tr5,
    Tr6,
    Adcp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatePhase {
    Concept,
    Plan,
    Development,
    Verification,
    Release,
    Lifecycle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateTrackState {
    Passed,
    Current,
    Upcoming,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriterionKind {
    DiThreshold,
    NoOpenFatalOrMajor,
    LegacyApproved,
    ArOwnerConfirmed,
    HighRisksClosedOrAccepted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    Unknown,
    Confirmed,
    Disputed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentBasis {
    ExplicitStatement,
    Unspecified,
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Proposed,
    Confirmed,
    LocalOnly,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRole {
    Request,
    Reply,
    Forward,
    Correction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentVerdict {
    Confirm,
    LocalOnly,
    Reject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionVerdict {
    Accept,
    Dismiss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchReason {
    Token,
    Link,
    ReplyChain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchPrecision {
    Exact,
    Snippet,
    IndexSubstring,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonRelation {
    PdtManager,
    PdtRepresentative,
    DevelopmentRepresentative,
    TestRepresentative,
    ProposedOwner,
    Mentioned,
    Participant,
}

/// Why a person is a candidate to ask. Counts come from the source; nothing is inferred here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonKind {
    ChangesInArea,
    DiscussionInArea,
    MentionedInThread,
    SharedLabel,
    RecentActivity,
    ExplicitAssignment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionKind {
    RelatedDiscussion,
    DuplicateCandidate,
    WhoToAsk,
    MissingIpdInfo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionOrigin {
    /// Semantic neighbour from local embeddings; a similarity, never a confidence.
    Semantic,
    /// Same module within a time window, computed by SPM from the item fields.
    SameModuleAndTime,
    /// Ranked by people-cli `who`.
    PeopleWho,
    /// A fixed IPD entry condition checked by SPM.
    Rule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingInfoKind {
    AcceptanceCriteria,
    ConfirmedOwner,
    TraceToRequirement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Lexical,
    Semantic,
    Hybrid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchScope {
    All,
    Issues,
    Messages,
    People,
    Changes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    Issue,
    Message,
    Person,
    Change,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchChannel {
    Lexical,
    Semantic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchWhy {
    ExactTerm,
    SameMeaning,
    LinkedToIssue,
    Participant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackReason {
    NoEligibleEmbeddingCache,
}

/// A person as the source names them, with the IPD roles this project gives them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuePerson {
    pub entity_id: String,
    pub display_name: String,
    #[serde(default)]
    pub persona: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub relations: Vec<PersonRelation>,
    #[serde(default)]
    pub handles: Vec<SourceValue>,
}

/// A value and the system that reported it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceValue {
    pub source: SourceSystem,
    pub value: String,
}

/// Two systems disagree about one field, for example the owner in Meegle and in Lark.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConflict {
    pub first: SourceValue,
    pub second: SourceValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueTrace {
    #[serde(default)]
    pub parents: Vec<String>,
    #[serde(default)]
    pub children: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskDetail {
    #[serde(default)]
    pub probability: Option<RiskLevel>,
    #[serde(default)]
    pub impact: Option<RiskLevel>,
    pub state: RiskState,
    #[serde(default)]
    pub mitigation: Option<String>,
}

/// One item of the issue tracker. `id` is the business token the user sees (R-102, D-2291).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueItem {
    pub id: String,
    pub kind: IssueKind,
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub severity: Option<DefectSeverity>,
    #[serde(default)]
    pub owner: Option<String>,
    pub assignment: AssignmentState,
    #[serde(default)]
    pub sources: Vec<SourceSystem>,
    #[serde(default)]
    pub conflict: Option<SourceConflict>,
    #[serde(default)]
    pub due_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub overdue: bool,
    #[serde(default)]
    pub queues: Vec<IssueQueue>,
    #[serde(default)]
    pub blocks_gate: Option<GateKind>,
    #[serde(default)]
    pub risk: Option<RiskDetail>,
    #[serde(default)]
    pub legacy: Option<LegacyState>,
    #[serde(default)]
    pub trace: Option<IssueTrace>,
    /// Estimated effort from the source. Absent means unknown; what-if sorts unknown last.
    #[serde(default)]
    pub effort_hours: Option<f64>,
}

impl IssueItem {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_issue_id(&self.id)?;
        if !non_empty_plain(&self.title, MAX_ISSUE_TITLE)
            || !non_empty_plain(&self.status, MAX_ISSUE_NAME)
            || self
                .owner
                .as_ref()
                .is_some_and(|o| !non_empty_plain(o, MAX_ISSUE_NAME))
        {
            return Err(invalid("issue text fields are empty or too long"));
        }
        if (self.kind == IssueKind::Dts) != self.severity.is_some() {
            return Err(invalid("severity is required for DTS and only for DTS"));
        }
        if self.overdue && self.due_at.is_none() {
            return Err(invalid("overdue needs a due date"));
        }
        if self.sources.len() > MAX_ISSUE_SOURCES || self.queues.len() > MAX_ISSUE_QUEUES {
            return Err(invalid("issue sources or queues exceed bounds"));
        }
        let unique: BTreeSet<_> = self.queues.iter().collect();
        if unique.len() != self.queues.len() {
            return Err(invalid("duplicate issue queue"));
        }
        let has = |q| self.queues.contains(&q);
        if has(IssueQueue::SourceConflict) != self.conflict.is_some() {
            return Err(invalid("source conflict queue must match the conflict"));
        }
        if self.blocks_gate.is_some() != has(IssueQueue::BlocksNextGate) {
            return Err(invalid("blocks_gate must match the blocking queue"));
        }
        if let Some(c) = &self.conflict {
            if c.first.value == c.second.value || c.first.source == c.second.source {
                return Err(invalid("a conflict needs two different values"));
            }
            for v in [&c.first, &c.second] {
                if !non_empty_plain(&v.value, MAX_ISSUE_NAME) {
                    return Err(invalid("conflict value is empty or too long"));
                }
            }
        }
        if self.risk.is_some() != (self.kind == IssueKind::Risk) {
            return Err(invalid("risk detail belongs to RISK items only"));
        }
        if self
            .risk
            .as_ref()
            .and_then(|r| r.mitigation.as_ref())
            .is_some_and(|m| !plain(m, MAX_ISSUE_TEXT))
        {
            return Err(invalid("risk mitigation is too long"));
        }
        if let Some(t) = &self.trace {
            if t.parents.len() > MAX_ISSUE_TRACE || t.children.len() > MAX_ISSUE_TRACE {
                return Err(invalid("trace exceeds bounds"));
            }
            for id in t.parents.iter().chain(&t.children) {
                validate_issue_id(id)?;
            }
        }
        if self
            .effort_hours
            .is_some_and(|e| !(e.is_finite() && e > 0.0))
        {
            return Err(invalid("effort must be a positive number of hours"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueCount {
    pub queue: IssueQueue,
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryIssuesRequest {
    pub project_id: ProjectId,
    #[serde(default)]
    pub queue: Option<IssueQueue>,
    #[serde(default)]
    pub kind: Option<IssueKind>,
    pub limit: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryIssuesResponse {
    #[serde(default)]
    pub observed_through: Option<DateTime<Utc>>,
    #[serde(default)]
    pub items: Vec<IssueItem>,
    #[serde(default)]
    pub queue_counts: Vec<QueueCount>,
    #[serde(default)]
    pub truncated: bool,
}

/// Explicit refresh of assignment proposals from the people-cli numbered-reply producer. It is
/// the only issue request that writes, and only to the local people database. Reads never run it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshIssuesRequest {
    pub project_id: ProjectId,
}

/// `people_configured` is false when no people-cli is set up; nothing ran in that case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshIssuesResponse {
    pub people_configured: bool,
    #[serde(default)]
    pub statements: u32,
    #[serde(default)]
    pub proposals_created: u32,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetIssueRequest {
    pub project_id: ProjectId,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateImpact {
    pub next_gate: GateKind,
    pub di_contribution: f64,
    #[serde(default)]
    pub criteria: Vec<CriterionKind>,
}

/// A Lark or Meegle message linked to an issue. `excerpt` follows the source's retention tier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueMessage {
    pub record_id: String,
    pub revision: u64,
    pub occurred_at: DateTime<Utc>,
    pub author: String,
    #[serde(default)]
    pub excerpt: Option<String>,
    pub reasons: Vec<MatchReason>,
    /// Absent for a reply-chain match, which has no text precision of its own.
    #[serde(default)]
    pub precision: Option<MatchPrecision>,
    #[serde(default)]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub participants: Vec<IssuePerson>,
    /// True when a parent message is referenced but was not stored.
    #[serde(default)]
    pub missing_context: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub record_id: String,
    pub revision: u64,
    pub role: EvidenceRole,
}

/// A proposal from the numbered-reply producer. Authority and acceptance stay `unknown`
/// until a source records them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignmentProposal {
    pub id: String,
    pub item_id: String,
    pub assignee: String,
    pub basis: AssignmentBasis,
    pub status: ProposalStatus,
    #[serde(default)]
    pub statement_author: Option<String>,
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
    pub authority: Standing,
    pub acceptance: Standing,
    #[serde(default)]
    pub statement_at: Option<DateTime<Utc>>,
    /// When the user reviewed the proposal; present exactly when it is no longer proposed.
    #[serde(default)]
    pub reviewed_at: Option<DateTime<Utc>>,
}

impl AssignmentProposal {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_issue_id(&self.item_id)?;
        if !non_empty_plain(&self.id, MAX_ISSUE_KEY)
            || !non_empty_plain(&self.assignee, MAX_ISSUE_NAME)
            || self
                .statement_author
                .as_ref()
                .is_some_and(|a| !non_empty_plain(a, MAX_ISSUE_NAME))
        {
            return Err(invalid("proposal text fields are empty or too long"));
        }
        if self.evidence.len() > MAX_ISSUE_EVIDENCE {
            return Err(invalid("proposal evidence exceeds bounds"));
        }
        if self.basis == AssignmentBasis::ExplicitStatement && self.evidence.is_empty() {
            return Err(invalid("an explicit assignment needs evidence"));
        }
        if (self.status == ProposalStatus::Proposed) == self.reviewed_at.is_some() {
            return Err(invalid(
                "reviewed_at is present exactly when the proposal was reviewed",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewAssignmentRequest {
    pub project_id: ProjectId,
    pub proposal_id: String,
    pub verdict: AssignmentVerdict,
    #[serde(default)]
    pub note: Option<String>,
}

impl ReviewAssignmentRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        if !non_empty_plain(&self.proposal_id, MAX_ISSUE_KEY) {
            return Err(invalid("proposal id is empty or too long"));
        }
        if self
            .note
            .as_ref()
            .is_some_and(|n| !plain(n, MAX_ISSUE_TEXT))
        {
            return Err(invalid("review note is too long"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JudgeSuggestionRequest {
    pub project_id: ProjectId,
    pub kind: SuggestionKind,
    pub subject_key: String,
    pub object_key: String,
    pub verdict: SuggestionVerdict,
    #[serde(default)]
    pub note: Option<String>,
}

impl JudgeSuggestionRequest {
    /// Only related discussion and duplicate candidates have people-cli judgments.
    /// Who-to-ask and missing-info suggestions are not recorded as decisions.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_suggestion_key(&self.subject_key)?;
        validate_suggestion_key(&self.object_key)?;
        fn prefix(k: &str) -> &str {
            k.split_once(':').map_or("", |(p, _)| p)
        }
        match self.kind {
            SuggestionKind::RelatedDiscussion => {
                if prefix(&self.subject_key) != "issue" || prefix(&self.object_key) != "record" {
                    return Err(invalid(
                        "related discussion judges issue:… against record:…",
                    ));
                }
            }
            SuggestionKind::DuplicateCandidate => {
                if prefix(&self.subject_key) != "defect"
                    || prefix(&self.object_key) != "defect"
                    || self.subject_key == self.object_key
                {
                    return Err(invalid("duplicate candidates pair two different defects"));
                }
            }
            SuggestionKind::WhoToAsk | SuggestionKind::MissingIpdInfo => {
                return Err(invalid("this suggestion kind has no judgment"));
            }
        }
        if self
            .note
            .as_ref()
            .is_some_and(|n| !plain(n, MAX_ISSUE_TEXT))
        {
            return Err(invalid("judgment note is too long"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestionJudgment {
    pub kind: SuggestionKind,
    pub subject_key: String,
    pub object_key: String,
    pub verdict: SuggestionVerdict,
    pub decided_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WhoToAsk {
    pub person: IssuePerson,
    /// people-cli `who` score in 0..=1. It ranks candidates; it is not a confidence.
    pub score: f64,
    pub reasons: Vec<ReasonKind>,
    #[serde(default)]
    pub count: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSuggestion {
    pub kind: SuggestionKind,
    pub origin: SuggestionOrigin,
    /// Judgment key pair, as accepted by `JudgeSuggestionRequest`. Absent for rule suggestions.
    pub subject_key: String,
    #[serde(default)]
    pub object_key: Option<String>,
    pub title: String,
    #[serde(default)]
    pub similarity: Option<f64>,
    #[serde(default)]
    pub reasons: Vec<ReasonKind>,
    #[serde(default)]
    pub missing: Option<MissingInfoKind>,
    /// A related artifact a rule points to, such as SR-031 for reusable acceptance criteria.
    #[serde(default)]
    pub reference_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueDetail {
    pub item: IssueItem,
    pub gate_impact: GateImpact,
    #[serde(default)]
    pub messages: Vec<IssueMessage>,
    #[serde(default)]
    pub proposals: Vec<AssignmentProposal>,
    #[serde(default)]
    pub people: Vec<IssuePerson>,
    #[serde(default)]
    pub who_to_ask: Vec<WhoToAsk>,
    #[serde(default)]
    pub suggestions: Vec<IssueSuggestion>,
    #[serde(default)]
    pub suggestions_dismissed: u32,
    /// Rejected proposals are hidden from `proposals` and counted here.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub proposals_rejected: u32,
    /// The proposal read hit its bound; some assignments may be missing from `proposals`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub proposals_truncated: bool,
    #[serde(default)]
    pub truncated: bool,
}

impl IssueDetail {
    pub fn validate(&self) -> Result<(), ContractError> {
        self.item.validate()?;
        if self.messages.len() > MAX_ISSUE_MESSAGES
            || self.proposals.len() > MAX_ISSUE_PROPOSALS
            || self.people.len() > MAX_ISSUE_PEOPLE
            || self.who_to_ask.len() > MAX_ISSUE_WHO
            || self.suggestions.len() > MAX_ISSUE_SUGGESTIONS
        {
            return Err(invalid("issue detail list exceeds bounds"));
        }
        for m in &self.messages {
            if !non_empty_plain(&m.record_id, MAX_ISSUE_KEY)
                || m.reasons.is_empty()
                || m.excerpt
                    .as_ref()
                    .is_some_and(|e| !plain(e, MAX_ISSUE_TEXT))
            {
                return Err(invalid("invalid linked message"));
            }
        }
        for p in &self.proposals {
            p.validate()?;
            if p.item_id != self.item.id {
                return Err(invalid("proposal belongs to another item"));
            }
        }
        for p in &self.people {
            validate_person(p)?;
        }
        for w in &self.who_to_ask {
            validate_person(&w.person)?;
            if !(0.0..=1.0).contains(&w.score) || w.reasons.len() > MAX_ISSUE_REASONS {
                return Err(invalid("who-to-ask score or reasons out of bounds"));
            }
        }
        for s in &self.suggestions {
            validate_suggestion(s)?;
        }
        if !finite_non_negative(self.gate_impact.di_contribution) {
            return Err(invalid("DI contribution must be finite and non-negative"));
        }
        Ok(())
    }
}

fn validate_person(p: &IssuePerson) -> Result<(), ContractError> {
    if !non_empty_plain(&p.entity_id, MAX_ISSUE_KEY)
        || !non_empty_plain(&p.display_name, MAX_ISSUE_NAME)
        || p.persona
            .as_ref()
            .is_some_and(|s| !plain(s, MAX_ISSUE_NAME))
        || p.title.as_ref().is_some_and(|s| !plain(s, MAX_ISSUE_TITLE))
        || p.relations.len() > MAX_ISSUE_RELATIONS
        || p.handles.len() > MAX_ISSUE_HANDLES
        || p.handles
            .iter()
            .any(|h| !non_empty_plain(&h.value, MAX_ISSUE_NAME))
    {
        return Err(invalid("invalid person reference"));
    }
    Ok(())
}

fn validate_suggestion(s: &IssueSuggestion) -> Result<(), ContractError> {
    validate_suggestion_key(&s.subject_key)?;
    if let Some(o) = &s.object_key {
        validate_suggestion_key(o)?;
    }
    if !non_empty_plain(&s.title, MAX_ISSUE_TITLE)
        || s.reasons.len() > MAX_ISSUE_REASONS
        || s.similarity.is_some_and(|v| !(0.0..=1.0).contains(&v))
        || s.reference_id
            .as_ref()
            .is_some_and(|r| validate_issue_id(r).is_err())
    {
        return Err(invalid("invalid suggestion"));
    }
    let judged = matches!(
        s.kind,
        SuggestionKind::RelatedDiscussion | SuggestionKind::DuplicateCandidate
    );
    if judged && s.object_key.is_none() {
        return Err(invalid("judged suggestion needs an object key"));
    }
    if (s.kind == SuggestionKind::MissingIpdInfo) != s.missing.is_some() {
        return Err(invalid("missing-info suggestions name what is missing"));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetGateReadinessRequest {
    pub project_id: ProjectId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateTrackEntry {
    pub gate: GateKind,
    pub phase: GatePhase,
    pub state: GateTrackState,
    #[serde(default)]
    pub due_at: Option<DateTime<Utc>>,
}

/// One entry criterion with the number it was judged on. `contributing` names the items
/// behind the value so the UI can list them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub kind: CriterionKind,
    pub value: f64,
    pub threshold: f64,
    pub pass: bool,
    #[serde(default)]
    pub contributing: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefectWeights {
    pub fatal: f64,
    pub major: f64,
    pub normal: f64,
    pub hint: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeverityCount {
    pub severity: DefectSeverity,
    pub count: u32,
    pub contribution: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiBreakdown {
    pub value: f64,
    pub weights: DefectWeights,
    #[serde(default)]
    pub by_severity: Vec<SeverityCount>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateReadiness {
    #[serde(default)]
    pub observed_through: Option<DateTime<Utc>>,
    pub track: Vec<GateTrackEntry>,
    pub next_gate: GateKind,
    #[serde(default)]
    pub next_gate_due: Option<DateTime<Utc>>,
    pub criteria: Vec<Criterion>,
    /// True only when every criterion passes.
    pub ready: bool,
    pub di: DiBreakdown,
    #[serde(default)]
    pub contributing: Vec<String>,
}

impl GateReadiness {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.track.len() > MAX_GATE_TRACK
            || self.criteria.len() > MAX_ISSUE_CRITERIA
            || self.contributing.len() > MAX_ISSUE_CONTRIBUTORS
            || self
                .track
                .iter()
                .filter(|t| t.state == GateTrackState::Current)
                .count()
                > 1
        {
            return Err(invalid("gate readiness exceeds bounds"));
        }
        if !finite_non_negative(self.di.value) {
            return Err(invalid("DI must be finite and non-negative"));
        }
        for c in &self.criteria {
            if !c.value.is_finite()
                || !c.threshold.is_finite()
                || c.contributing.len() > MAX_ISSUE_CONTRIBUTORS
                || c.contributing
                    .iter()
                    .any(|id| validate_issue_id(id).is_err())
            {
                return Err(invalid("criterion values out of bounds"));
            }
        }
        if self.ready != self.criteria.iter().all(|c| c.pass) {
            return Err(invalid("ready must equal all criteria passing"));
        }
        if self.di.by_severity.len() > 4 {
            return Err(invalid("too many severity rows"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateWhatIfRequest {
    pub project_id: ProjectId,
    pub gate: GateKind,
    pub max_steps: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WhatIfStep {
    pub item_id: String,
    pub severity: DefectSeverity,
    pub di_reduction: f64,
    #[serde(default)]
    pub effort_hours: Option<f64>,
    pub di_after: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateWhatIf {
    pub gate: GateKind,
    pub start_di: f64,
    pub threshold: f64,
    pub steps: Vec<WhatIfStep>,
    pub reaches_threshold: bool,
    /// Open defects with no effort estimate. They are sorted last.
    pub unknown_effort: u32,
}

impl GateWhatIf {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.steps.len() > usize::from(MAX_WHAT_IF_STEPS)
            || !finite_non_negative(self.start_di)
            || !finite_non_negative(self.threshold)
        {
            return Err(invalid("what-if exceeds bounds"));
        }
        let mut di = self.start_di;
        for s in &self.steps {
            validate_issue_id(&s.item_id)?;
            if !finite_non_negative(s.di_reduction)
                || s.effort_hours.is_some_and(|e| !(e.is_finite() && e > 0.0))
                || (di - s.di_reduction - s.di_after).abs() > 1e-6
            {
                return Err(invalid("what-if step does not follow from DI"));
            }
            di = s.di_after;
        }
        if self.reaches_threshold != (di <= self.threshold) {
            return Err(invalid("reaches_threshold disagrees with the final DI"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchWorkspaceRequest {
    pub project_id: ProjectId,
    pub query: String,
    pub mode: SearchMode,
    pub scope: SearchScope,
    pub limit: u16,
}

impl SearchWorkspaceRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        if !non_empty_plain(&self.query, MAX_SEARCH_QUERY) {
            return Err(invalid("search query is empty or too long"));
        }
        if !(1..=MAX_SEARCH_LIMIT).contains(&self.limit) {
            return Err(invalid("search limit out of bounds"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextSpan {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalHit {
    pub rank: u32,
    #[serde(default)]
    pub spans: Vec<TextSpan>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticHit {
    pub rank: u32,
    /// Cosine similarity of the best-matching segment. It explains ranking and is never
    /// shown as confidence or proof.
    pub similarity: f64,
    /// Span of the best-matching segment inside the snippet, when the segment is shown.
    #[serde(default)]
    pub span: Option<TextSpan>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchResult {
    pub kind: SearchKind,
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub channels: Vec<SearchChannel>,
    /// Reciprocal-rank fusion score (k = 60). Only orders results.
    pub score: f64,
    #[serde(default)]
    pub lexical: Option<LexicalHit>,
    #[serde(default)]
    pub semantic: Option<SemanticHit>,
    pub why: Vec<SearchWhy>,
    #[serde(default)]
    pub linked_issue: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchFallback {
    pub reason: FallbackReason,
    /// True when the semantic channel needs full source text that the policy does not retain.
    #[serde(default)]
    pub requires_source_text: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchWorkspaceResponse {
    pub mode_requested: SearchMode,
    pub mode_used: SearchMode,
    #[serde(default)]
    pub fallback: Option<SearchFallback>,
    #[serde(default)]
    pub results: Vec<SearchResult>,
    #[serde(default)]
    pub truncated: bool,
}

impl SearchWorkspaceResponse {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.results.len() > usize::from(MAX_SEARCH_LIMIT) {
            return Err(invalid("too many search results"));
        }
        if self.fallback.is_some() != (self.mode_used != self.mode_requested) {
            return Err(invalid("fallback must match the mode actually used"));
        }
        if self.fallback.is_some() && self.mode_used != SearchMode::Lexical {
            return Err(invalid("a fallback runs lexical search"));
        }
        for r in &self.results {
            if !non_empty_plain(&r.title, MAX_ISSUE_TITLE)
                || !plain(&r.snippet, MAX_SEARCH_SNIPPET)
                || r.id.is_empty()
                || r.id.len() > MAX_ISSUE_KEY
                || r.channels.is_empty()
                || !r.score.is_finite()
                || r.why.is_empty()
            {
                return Err(invalid("invalid search result"));
            }
            if r.channels.contains(&SearchChannel::Lexical) != r.lexical.is_some()
                || r.channels.contains(&SearchChannel::Semantic) != r.semantic.is_some()
            {
                return Err(invalid("channels must match the channel hits"));
            }
            let mut spans: Vec<TextSpan> = r.lexical.iter().flat_map(|l| l.spans.clone()).collect();
            if spans.len() > MAX_SEARCH_SPANS {
                return Err(invalid("too many matched spans"));
            }
            spans.extend(r.semantic.iter().filter_map(|s| s.span));
            for s in spans {
                if s.start >= s.end || s.end as usize > r.snippet.len() {
                    return Err(invalid("span outside the snippet"));
                }
            }
            if r.semantic
                .as_ref()
                .is_some_and(|s| !(-1.0..=1.0).contains(&s.similarity))
            {
                return Err(invalid("similarity outside -1..=1"));
            }
        }
        Ok(())
    }
}

impl QueryIssuesRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        if !(1..=MAX_ISSUE_LIMIT).contains(&self.limit) {
            return Err(invalid("issue limit out of bounds"));
        }
        Ok(())
    }
}

impl QueryIssuesResponse {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.items.len() > usize::from(MAX_ISSUE_LIMIT)
            || self.queue_counts.len() > 5
            || self
                .queue_counts
                .iter()
                .map(|q| q.count)
                .any(|c| c > 100_000)
        {
            return Err(invalid("issue page exceeds bounds"));
        }
        self.items.iter().try_for_each(IssueItem::validate)
    }
}

impl GetIssueRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_issue_id(&self.id)
    }
}

impl RefreshIssuesResponse {
    pub fn validate(&self) -> Result<(), ContractError> {
        if !self.people_configured && (self.statements != 0 || self.proposals_created != 0) {
            return Err(invalid("refresh reports counts without people-cli"));
        }
        Ok(())
    }
}

impl GateWhatIfRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        if !(1..=MAX_WHAT_IF_STEPS).contains(&self.max_steps) {
            return Err(invalid("what-if steps out of bounds"));
        }
        Ok(())
    }
}

impl SuggestionJudgment {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_suggestion_key(&self.subject_key)?;
        validate_suggestion_key(&self.object_key)
    }
}

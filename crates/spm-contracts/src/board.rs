use crate::{
    BaselineId, ContractError, DaemonSessionId, DeliveryScopeId, PageCursor, ProjectId, RecordId,
    RelationState, Revision, SourceEvidence, SourceRef, SourceSystem, SubscriptionId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryColumn {
    Todo,
    Doing,
    Done,
    Unclassified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardStage {
    NotStarted,
    Implementation,
    Review,
    IntegrationBuild,
    Regression,
    Acceptance,
    Done,
    Unmapped,
}

impl BoardStage {
    pub fn column(self) -> SummaryColumn {
        match self {
            Self::NotStarted => SummaryColumn::Todo,
            Self::Implementation
            | Self::Review
            | Self::IntegrationBuild
            | Self::Regression
            | Self::Acceptance => SummaryColumn::Doing,
            Self::Done => SummaryColumn::Done,
            Self::Unmapped => SummaryColumn::Unclassified,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageBasis {
    WorkflowNode,
    ActivityEvidence,
    Unmapped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageOverlay {
    Blocked,
    Waiting,
    WorkflowLag,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageProjection {
    pub stage: BoardStage,
    pub basis: StageBasis,
    pub overlays: BTreeSet<StageOverlay>,
    pub raw_node: Option<String>,
    pub explanation: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowState {
    pub source_state: String,
    pub node_key: Option<String>,
    pub source_time: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Swimlane {
    pub project: String,
    pub model: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountRef {
    pub system: SourceSystem,
    pub tenant: String,
    pub account_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonRef {
    pub person_id: Option<String>,
    pub account: Option<AccountRef>,
    pub display_name: String,
}

impl PersonRef {
    fn validate(&self) -> Result<(), ContractError> {
        if self.person_id.is_none() && self.account.is_none() {
            return Err(invalid("person", "needs a person id or a source account"));
        }
        if self.display_name.trim().is_empty() {
            return Err(invalid("person.display_name", "must not be empty"));
        }
        Ok(())
    }

    /// Same person when both resolve to one people-cli person, else same account.
    pub fn same_as(&self, other: &PersonRef) -> bool {
        match (&self.person_id, &other.person_id) {
            (Some(a), Some(b)) => a == b,
            _ => self.account.is_some() && self.account == other.account,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    ChangeOwner,
    PatchsetAuthor,
    PatchsetUploader,
    ReviewerListed,
    Approval,
    Submitter,
    Assigned,
    Accepted,
    Verified,
    HistoricalContributor,
    ImplementationContact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipBasis {
    SourceField,
    IssueField,
    ExplicitStatement,
    Derived,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipStatus {
    Current,
    Superseded,
    Retracted,
    Conflicting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatementKind {
    NumberedReply,
    Direct,
    Correction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Authority {
    Unknown,
    Recognized,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatementInfo {
    pub author: PersonRef,
    pub kind: StatementKind,
    pub authority: Authority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CarriedOver {
    No,
    Inferred,
    Reported,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalDetail {
    pub patchset: u32,
    pub label: String,
    pub value: i16,
    pub carried_over: CarriedOver,
    pub post_submit: bool,
    pub self_approval: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relationship {
    pub id: String,
    pub kind: RelationshipKind,
    pub person: PersonRef,
    pub subject: SourceRef,
    pub basis: RelationshipBasis,
    pub derivation: Option<String>,
    pub status: RelationshipStatus,
    pub source_time: Option<DateTime<Utc>>,
    pub observed_at: DateTime<Utc>,
    pub evidence: Vec<String>,
    pub supersedes: Option<String>,
    pub statement: Option<StatementInfo>,
    pub approval: Option<ApprovalDetail>,
}

impl Relationship {
    fn validate(&self) -> Result<(), ContractError> {
        if self.id.is_empty() {
            return Err(invalid("relationship.id", "must not be empty"));
        }
        self.person.validate()?;
        if self.evidence.is_empty() || self.evidence.iter().any(String::is_empty) {
            return Err(invalid(
                "relationship.evidence",
                "needs evidence references",
            ));
        }
        if (self.kind == RelationshipKind::Approval) != self.approval.is_some() {
            return Err(invalid(
                "relationship.approval",
                "present exactly for approval relationships",
            ));
        }
        if (self.basis == RelationshipBasis::ExplicitStatement) != self.statement.is_some() {
            return Err(invalid(
                "relationship.statement",
                "present exactly for explicit statements",
            ));
        }
        if let Some(statement) = &self.statement {
            statement.author.validate()?;
            // An allocation statement is only meaningful beside the task definition it targets.
            if self.kind == RelationshipKind::Assigned && self.evidence.len() < 2 {
                return Err(invalid(
                    "relationship.evidence",
                    "an assignment statement needs the task definition and the statement",
                ));
            }
        }
        if (self.basis == RelationshipBasis::Derived) != self.derivation.is_some() {
            return Err(invalid(
                "relationship.derivation",
                "present exactly for derived relationships",
            ));
        }
        let derived_only = matches!(
            self.kind,
            RelationshipKind::HistoricalContributor | RelationshipKind::ImplementationContact
        );
        if derived_only && self.basis != RelationshipBasis::Derived {
            return Err(invalid(
                "relationship.basis",
                "historical contributors and implementation contacts are derived",
            ));
        }
        if self.kind == RelationshipKind::Assigned
            && !matches!(
                self.basis,
                RelationshipBasis::IssueField | RelationshipBasis::ExplicitStatement
            )
        {
            return Err(invalid(
                "relationship.basis",
                "an assignment comes from an issue field or an explicit statement",
            ));
        }
        if self.supersedes.as_deref() == Some(self.id.as_str()) {
            return Err(invalid(
                "relationship.supersedes",
                "cannot supersede itself",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressState {
    NotStarted,
    InProgress,
    Passed,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeProgress {
    pub linked: u32,
    pub merged: u32,
    pub open: u32,
    pub abandoned: u32,
    pub relation: RelationState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardProgress {
    pub code: CodeProgress,
    pub build: ProgressState,
    pub verification: ProgressState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blocker {
    pub reason: String,
    pub dependency: Option<SourceRef>,
    pub contact: Option<PersonRef>,
    pub since: Option<DateTime<Utc>>,
    #[serde(default)]
    pub expected_at: Option<DateTime<Utc>>,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NextAction {
    pub text: String,
    pub owner: Option<PersonRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conflict {
    pub field: String,
    pub relationship_ids: Vec<String>,
    pub explanation: String,
}

/// Backend-owned delivery dates. The panel must not infer a schedule.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardSchedule {
    pub due: DateTime<Utc>,
    #[serde(default)]
    pub baseline_due: Option<DateTime<Utc>>,
    pub basis: SourceRef,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageSpan {
    pub stage: BoardStage,
    pub from: DateTime<Utc>,
    pub to: Option<DateTime<Utc>>,
}

/// Bounded pointers into GetCardEvidence, never copies of evidence text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventMark {
    pub at: DateTime<Utc>,
    pub system: SourceSystem,
    pub evidence_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Milestone {
    pub id: String,
    pub label: String,
    pub at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardCard {
    pub card_id: RecordId,
    pub source: SourceRef,
    pub business_number: Option<String>,
    pub title: String,
    pub swimlane: Swimlane,
    pub workflow: Option<WorkflowState>,
    pub column: SummaryColumn,
    pub stage: StageProjection,
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub schedule: Option<CardSchedule>,
    #[serde(default)]
    pub stage_history: Vec<StageSpan>,
    #[serde(default)]
    pub event_marks: Vec<EventMark>,
    pub current_assignee: Option<String>,
    pub relationships: Vec<Relationship>,
    pub progress: CardProgress,
    pub blockers: Vec<Blocker>,
    pub next_action: Option<NextAction>,
    pub conflicts: Vec<Conflict>,
    pub evidence_count: u32,
    pub observed_through: BTreeMap<SourceSystem, DateTime<Utc>>,
}

impl BoardCard {
    pub fn relationship(&self, id: &str) -> Option<&Relationship> {
        self.relationships.iter().find(|r| r.id == id)
    }

    pub fn assignee(&self) -> Option<&Relationship> {
        self.current_assignee
            .as_deref()
            .and_then(|id| self.relationship(id))
    }

    pub fn related(&self, kind: RelationshipKind) -> impl Iterator<Item = &Relationship> {
        self.relationships.iter().filter(move |r| r.kind == kind)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.title.trim().is_empty() {
            return Err(invalid("card.title", "must not be empty"));
        }
        let mut ids = BTreeSet::new();
        for relationship in &self.relationships {
            relationship.validate()?;
            if !ids.insert(relationship.id.as_str()) {
                return Err(invalid("relationship.id", "duplicated within a card"));
            }
        }
        for relationship in &self.relationships {
            if let Some(previous) = &relationship.supersedes {
                if !ids.contains(previous.as_str()) {
                    return Err(invalid("relationship.supersedes", "unknown relationship"));
                }
            }
        }
        self.validate_stage()?;
        for (i, span) in self.stage_history.iter().enumerate() {
            if span.to.is_some_and(|to| to <= span.from) {
                return Err(invalid(
                    "stage_history",
                    "a span must have positive duration",
                ));
            }
            if i > 0 && self.stage_history[i - 1].to.is_none_or(|to| to > span.from) {
                return Err(invalid(
                    "stage_history",
                    "must be ordered and non-overlapping; open span is last",
                ));
            }
        }
        if self.event_marks.len() > 12
            || self
                .event_marks
                .iter()
                .any(|m| m.evidence_id.trim().is_empty())
        {
            return Err(invalid(
                "event_marks",
                "at most 12 nonempty evidence pointers",
            ));
        }
        if let Some(id) = &self.current_assignee {
            let assignee = self
                .relationship(id)
                .ok_or_else(|| invalid("current_assignee", "unknown relationship"))?;
            if assignee.kind != RelationshipKind::Assigned
                || assignee.status != RelationshipStatus::Current
            {
                return Err(invalid(
                    "current_assignee",
                    "must name a current assignment",
                ));
            }
        }
        let conflicting = self
            .relationships
            .iter()
            .any(|r| r.status == RelationshipStatus::Conflicting);
        if conflicting && self.conflicts.is_empty() {
            return Err(invalid(
                "conflicts",
                "conflicting relationships need a conflict",
            ));
        }
        for conflict in &self.conflicts {
            if conflict.relationship_ids.is_empty()
                || conflict
                    .relationship_ids
                    .iter()
                    .any(|id| !ids.contains(id.as_str()))
            {
                return Err(invalid("conflict.relationship_ids", "unknown relationship"));
            }
        }
        for contact in self.blockers.iter().filter_map(|b| b.contact.as_ref()) {
            contact.validate()?;
        }
        Ok(())
    }

    fn validate_stage(&self) -> Result<(), ContractError> {
        let stage = &self.stage;
        let blocked = stage.overlays.contains(&StageOverlay::Blocked);
        if self.column != stage.stage.column() {
            return Err(invalid(
                "card.column",
                "differs from the stage's summary column",
            ));
        }
        if (stage.basis == StageBasis::Unmapped) != (stage.stage == BoardStage::Unmapped) {
            return Err(invalid(
                "stage.basis",
                "unmapped basis exactly for unmapped stage",
            ));
        }
        if stage.stage == BoardStage::Unmapped && stage.raw_node.is_none() {
            return Err(invalid(
                "stage.raw_node",
                "unmapped stage keeps the source node",
            ));
        }
        // A merge, build or vote can show that work started; only the workflow finishes it.
        if self.column == SummaryColumn::Done && stage.basis != StageBasis::WorkflowNode {
            return Err(invalid(
                "stage.basis",
                "only the workflow can finish a card",
            ));
        }
        if stage.basis == StageBasis::ActivityEvidence
            && !stage.overlays.contains(&StageOverlay::WorkflowLag)
        {
            return Err(invalid(
                "stage.overlays",
                "activity ahead of the workflow is flagged as workflow lag",
            ));
        }
        if blocked != !self.blockers.is_empty() {
            return Err(invalid(
                "stage.overlays",
                "blocked exactly when blockers exist",
            ));
        }
        let waits = blocked || stage.overlays.contains(&StageOverlay::Waiting);
        if waits && self.column != SummaryColumn::Doing {
            return Err(invalid(
                "stage.overlays",
                "blocked and waiting work stays in progress",
            ));
        }
        if matches!(self.column, SummaryColumn::Doing | SummaryColumn::Done)
            && self.started_at.is_none()
        {
            return Err(invalid("started_at", "started work records its start"));
        }
        if self.column == SummaryColumn::Todo && self.started_at.is_some() {
            return Err(invalid("started_at", "unstarted work has no start"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnCounts {
    pub todo: u64,
    pub doing: u64,
    pub done: u64,
    pub unclassified: u64,
    pub blocked: u64,
}

impl ColumnCounts {
    fn add(&mut self, card: &BoardCard) {
        match card.column {
            SummaryColumn::Todo => self.todo += 1,
            SummaryColumn::Doing => self.doing += 1,
            SummaryColumn::Done => self.done += 1,
            SummaryColumn::Unclassified => self.unclassified += 1,
        }
        if card.stage.overlays.contains(&StageOverlay::Blocked) {
            self.blocked += 1;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaneSummary {
    pub swimlane: Swimlane,
    pub counts: ColumnCounts,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardSnapshot {
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
    pub lanes: Vec<LaneSummary>,
    pub totals_complete: bool,
    pub cards: Vec<BoardCard>,
    #[serde(default)]
    pub milestones: Vec<Milestone>,
    pub next_cursor: Option<PageCursor>,
}

impl BoardSnapshot {
    pub fn card(&self, business_number: &str) -> Option<&BoardCard> {
        self.cards
            .iter()
            .find(|c| c.business_number.as_deref() == Some(business_number))
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.project_timezone.trim().is_empty() || !self.project_timezone.contains('/') {
            return Err(invalid(
                "project_timezone",
                "expected an IANA timezone name",
            ));
        }
        validate_cards(&self.cards)?;
        let mut milestone_ids = BTreeSet::new();
        for milestone in &self.milestones {
            if milestone.id.trim().is_empty()
                || milestone.label.trim().is_empty()
                || !milestone_ids.insert(&milestone.id)
            {
                return Err(invalid(
                    "milestones",
                    "unique nonempty ids and nonempty labels required",
                ));
            }
        }
        let mut lanes = BTreeSet::new();
        for lane in &self.lanes {
            if !lanes.insert(&lane.swimlane) {
                return Err(invalid("lanes", "duplicated swimlane"));
            }
        }
        if self.cards.iter().any(|c| !lanes.contains(&c.swimlane)) {
            return Err(invalid("card.swimlane", "not listed in lanes"));
        }
        let all_sources_complete = self.sources.iter().all(|s| {
            s.coverage == crate::CoverageState::Complete && s.page_complete && s.error.is_none()
        });
        if self.totals_complete && !all_sources_complete {
            return Err(invalid(
                "totals_complete",
                "requires complete coverage from every source",
            ));
        }
        // Counts of a single, final page must agree with its cards.
        if self.totals_complete && self.next_cursor.is_none() {
            let mut counted: BTreeMap<&Swimlane, ColumnCounts> = BTreeMap::new();
            for card in &self.cards {
                counted.entry(&card.swimlane).or_default().add(card);
            }
            for lane in &self.lanes {
                let actual = counted.remove(&lane.swimlane).unwrap_or_default();
                if actual != lane.counts {
                    return Err(invalid("lanes.counts", "differ from the cards"));
                }
            }
        }
        Ok(())
    }
}

/// One current card per requirement in a delivery scope.
pub fn validate_cards(cards: &[BoardCard]) -> Result<(), ContractError> {
    let mut ids = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for card in cards {
        card.validate()?;
        if !ids.insert(&card.card_id) {
            return Err(invalid("card_id", "duplicated in the board"));
        }
        let source = (
            card.source.system,
            card.source.tenant.as_str(),
            card.source.project.as_str(),
            card.source.record_kind.as_str(),
            card.source.record_id.as_str(),
        );
        if !sources.insert(source) {
            return Err(invalid("card.source", "one requirement has one card"));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardPageResponse {
    pub revision: Revision,
    pub cards: Vec<BoardCard>,
    pub next_cursor: Option<PageCursor>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetCardEvidenceRequest {
    pub subscription_id: SubscriptionId,
    pub card_id: RecordId,
    pub revision: Revision,
    pub cursor: Option<PageCursor>,
    pub page_size: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceItem {
    pub id: String,
    pub source: SourceRef,
    pub field_locator: Option<String>,
    pub source_time: Option<DateTime<Utc>>,
    pub observed_at: DateTime<Utc>,
    pub summary: String,
    pub relationship_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardEvidenceResponse {
    pub card_id: RecordId,
    pub revision: Revision,
    pub items: Vec<EvidenceItem>,
    pub next_cursor: Option<PageCursor>,
}

impl CardEvidenceResponse {
    pub fn validate(&self) -> Result<(), ContractError> {
        let mut ids = BTreeSet::new();
        let mut previous: Option<DateTime<Utc>> = None;
        for item in &self.items {
            if item.id.is_empty() || !ids.insert(item.id.as_str()) {
                return Err(invalid("evidence.id", "must be unique and non-empty"));
            }
            let at = item.source_time.unwrap_or(item.observed_at);
            if previous.is_some_and(|p| at < p) {
                return Err(invalid("evidence.items", "must be ordered by time"));
            }
            previous = Some(at);
        }
        Ok(())
    }
}

fn invalid(field: &'static str, reason: &str) -> ContractError {
    ContractError::InvalidField {
        field,
        reason: reason.into(),
    }
}

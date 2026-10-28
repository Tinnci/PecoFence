//! Generic, bounded delivery-plan projection. No private scheduling algorithms or source policies.
use crate::{BaselineId, ContractError, DeliveryScopeId, ProjectId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const MAX_PLAN_PAGE: usize = 200;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetDeliveryPlanRequest {
    pub project_id: ProjectId,
    pub delivery_scope_id: DeliveryScopeId,
    pub plan_id: String,
    pub plan_revision: Option<u64>,
    pub offset: u32,
    pub limit: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanWorkState {
    Todo,
    Doing,
    Done,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanReadiness {
    Satisfied,
    Unsatisfied,
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanTaskView {
    pub id: String,
    pub label: String,
    pub lane: String,
    pub state: PlanWorkState,
    pub start: Option<u32>,
    pub finish: Option<u32>,
    pub actual_start: Option<u32>,
    pub actual_finish: Option<u32>,
    pub conditional: bool,
    pub critical: bool,
    pub driving: bool,
    pub blocked: bool,
    pub unscheduled_reason: Option<String>,
    pub dependencies: Vec<String>,
    pub resources: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanMilestoneView {
    pub id: String,
    pub label: String,
    pub target: Option<u32>,
    pub baseline: Option<u32>,
    pub forecast: Option<u32>,
    pub actual: Option<u32>,
    pub conditional: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanGateView {
    pub id: String,
    pub readiness: PlanReadiness,
    pub reasons: Vec<String>,
    pub reason_count: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanResourceWait {
    pub resource: String,
    pub before: String,
    pub after: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanBufferView {
    pub id: String,
    pub after: String,
    pub before: Option<String>,
    pub slots: u32,
    pub consumed: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanFlowLane {
    pub lane: String,
    pub wip: u32,
    pub blocked_wip: u32,
    pub limit: Option<u32>,
    pub oldest_wip: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryPlanView {
    pub schema_version: u32,
    pub project_id: ProjectId,
    pub delivery_scope_id: DeliveryScopeId,
    pub plan_id: String,
    pub plan_revision: u64,
    pub baseline_id: BaselineId,
    pub epoch_utc: DateTime<Utc>,
    pub slot_minutes: u32,
    pub status_at: u32,
    pub horizon: u32,
    pub conditional: bool,
    pub finish: Option<u32>,
    pub protected_finish: Option<u32>,
    pub tasks: Vec<PlanTaskView>,
    pub total_tasks: u32,
    pub offset: u32,
    pub next_offset: Option<u32>,
    pub milestones: Vec<PlanMilestoneView>,
    pub gates: Vec<PlanGateView>,
    pub resource_waits: Vec<PlanResourceWait>,
    pub buffers: Vec<PlanBufferView>,
    pub flow: Vec<PlanFlowLane>,
    pub warnings: Vec<String>,
}
fn invalid(reason: &str) -> ContractError {
    ContractError::InvalidField {
        field: "delivery_plan",
        reason: reason.into(),
    }
}
fn text(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}
impl GetDeliveryPlanRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        if !text(&self.plan_id)
            || self.plan_revision == Some(0)
            || self.limit == 0
            || usize::from(self.limit) > MAX_PLAN_PAGE
            || self.offset > 2000
            || self.offset > 0 && self.plan_revision.is_none()
        {
            return Err(invalid(
                "invalid scoped page request; continuation requires exact plan revision",
            ));
        }
        Ok(())
    }
}
impl DeliveryPlanView {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema_version != 1
            || self.plan_revision == 0
            || !text(&self.plan_id)
            || self.horizon == 0
            || self.horizon > 100000
            || self.status_at > self.horizon
            || self.slot_minutes == 0
            || self.slot_minutes > 1440
            || self.tasks.len() > MAX_PLAN_PAGE
            || self.total_tasks > 2000
            || self.offset > self.total_tasks
            || self.offset + self.tasks.len() as u32 > self.total_tasks
            || self.milestones.len() > 2000
            || self.gates.len() > 2000
            || self.flow.len() > 2000
            || self.resource_waits.len() > 32000
            || self.buffers.len() > 2000
            || self.warnings.len() > 64
        {
            return Err(invalid("projection bounds"));
        }
        let expected = self.offset + self.tasks.len() as u32;
        if self.next_offset != ((expected < self.total_tasks).then_some(expected)) {
            return Err(invalid("continuation mismatch"));
        }
        if self.tasks.is_empty() && self.next_offset.is_some() {
            return Err(invalid("empty nonterminal page"));
        }
        if self
            .epoch_utc
            .checked_add_signed(chrono::Duration::minutes(
                i64::from(self.horizon) * i64::from(self.slot_minutes),
            ))
            .is_none()
        {
            return Err(invalid("calendar overflow"));
        }
        if !self.conditional
            && (self.tasks.iter().any(|t| t.conditional)
                || self.milestones.iter().any(|m| m.conditional))
        {
            return Err(invalid("conditional summary mismatch"));
        }
        let time = |v: Option<u32>| v.is_none_or(|n| n <= self.horizon);
        if !time(self.finish) || !time(self.protected_finish) {
            return Err(invalid("finish outside horizon"));
        }
        let mut ids = BTreeSet::new();
        for t in &self.tasks {
            if !text(&t.id)
                || !text(&t.label)
                || !text(&t.lane)
                || !ids.insert(&t.id)
                || !time(t.start)
                || !time(t.finish)
                || !time(t.actual_start)
                || !time(t.actual_finish)
                || t.start.is_some() != t.finish.is_some()
                || t.start.zip(t.finish).is_some_and(|(a, b)| a > b)
                || t.actual_start
                    .zip(t.actual_finish)
                    .is_some_and(|(a, b)| a > b)
                || t.dependencies.len() > 2000
                || t.resources.len() > 64
                || t.dependencies
                    .iter()
                    .chain(t.resources.iter())
                    .any(|v| !text(v))
                || t.unscheduled_reason.as_deref().is_some_and(|v| !text(v))
            {
                return Err(invalid("task projection"));
            }
            if t.state == PlanWorkState::Todo
                && (t.actual_start.is_some() || t.actual_finish.is_some())
                || t.state == PlanWorkState::Doing
                    && (t.actual_start.is_none()
                        || t.actual_finish.is_some()
                        || t.actual_start.is_some_and(|n| n > self.status_at))
            {
                return Err(invalid("workflow actuals"));
            }
            if t.state == PlanWorkState::Done
                && (t.actual_finish.is_none()
                    || t.actual_start.is_none()
                    || t.actual_finish.is_some_and(|n| n > self.status_at))
            {
                return Err(invalid("completed actuals"));
            }
        }
        for m in &self.milestones {
            if !text(&m.id)
                || !text(&m.label)
                || ![m.target, m.baseline, m.forecast, m.actual]
                    .into_iter()
                    .all(time)
            {
                return Err(invalid("milestone projection"));
            }
        }
        for g in &self.gates {
            if !text(&g.id)
                || g.reasons.len() > 8
                || g.reason_count < g.reasons.len() as u32
                || g.reasons.iter().any(|v| !text(v))
            {
                return Err(invalid("gate projection"));
            }
        }
        if self
            .resource_waits
            .iter()
            .any(|r| !text(&r.resource) || !text(&r.before) || !text(&r.after))
            || self.buffers.iter().any(|b| {
                !text(&b.id)
                    || !text(&b.after)
                    || b.before.as_deref().is_some_and(|v| !text(v))
                    || b.slots > 100000
            })
            || self
                .flow
                .iter()
                .any(|f| !text(&f.lane) || f.blocked_wip > f.wip || f.wip > 2000)
            || self.warnings.iter().any(|v| !text(v))
        {
            return Err(invalid("summary projection"));
        }
        if serde_json::to_vec(self)
            .map_err(|_| invalid("serialization"))?
            .len()
            > 3 * 1024 * 1024
        {
            return Err(invalid("projection byte limit"));
        }
        Ok(())
    }
}

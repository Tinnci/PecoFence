//! Read-only CI observations and local watchlist edits. ETA is projected by SPM.
use crate::{ContractError, SourceSystem};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_CI_JOBS: usize = 50;
pub const MAX_CI_BRANCHES: usize = 20;
pub const MAX_CI_BUILDS: usize = 100;
pub const MAX_CI_CHANGES: usize = 20;
pub const MAX_CI_STAGE_LABEL: usize = 64;
pub const MAX_CI_NOTE: usize = 512;
pub const MAX_CI_HISTORY: usize = 20;
pub const MAX_CI_STEPS: usize = 64;
pub const MAX_CI_MILESTONES: usize = 8;
/// Bytes of console text shown in one log window; the window is a byte range of the full log.
pub const MAX_CI_LOG_WINDOW: usize = 8192;
fn invalid(reason: &str) -> ContractError {
    ContractError::InvalidField {
        field: "ci_watch",
        reason: reason.into(),
    }
}
fn bounded(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
pub fn validate_ci_job(job: &str) -> Result<(), ContractError> {
    if !bounded(job, 512)
        || job.split('/').any(|s| {
            s.is_empty() || matches!(s, "." | "..") || s.contains(['\\', '%', ';', '?', '#'])
        })
    {
        return Err(invalid("unsafe Jenkins job path"));
    }
    Ok(())
}
pub fn validate_ci_branch(project: &str, branch: &str) -> Result<(), ContractError> {
    for s in [project, branch] {
        if !bounded(s, 200)
            || s.starts_with('-')
            || s.contains("..")
            || s.contains("//")
            || s.ends_with('/')
            || s.ends_with('.')
            || s.ends_with(".lock")
            || s.split('/')
                .any(|part| part.is_empty() || part.starts_with('.'))
            || !s
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_.-".contains(&b))
        {
            return Err(invalid("unsafe project:branch"));
        }
    }
    Ok(())
}
pub fn validate_ci_query(query: &str) -> Result<(), ContractError> {
    if query.len() > 256 || query.chars().any(char::is_control) || query.matches(':').count() > 1 {
        return Err(invalid("suggestion query must be at most 256 bytes"));
    }
    Ok(())
}

/// What a watched job means for the delivery pipeline. `Other` is the unspecified default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiJobRole {
    Verify,
    Daily,
    Release,
    #[default]
    Other,
}
/// The six delivery stages of one change, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiStageKind {
    Submit,
    Verify,
    Review,
    Merge,
    DailyBuild,
    Release,
}
impl CiStageKind {
    pub const ALL: [Self; 6] = [
        Self::Submit,
        Self::Verify,
        Self::Review,
        Self::Merge,
        Self::DailyBuild,
        Self::Release,
    ];
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiStageState {
    Done,
    Running,
    Failed,
    Warn,
    #[default]
    Pending,
}
/// One stage of a change. `label` is a short SPM-derived token such as `PS3`, `× #913`
/// or `约 8 分`; `entered_at` stays `None` when the source does not show when the stage began.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiStage {
    pub kind: CiStageKind,
    pub state: CiStageState,
    pub label: String,
    #[serde(default)]
    pub entered_at: Option<DateTime<Utc>>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiRatio {
    pub passed: u32,
    pub total: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiUser {
    #[serde(default)]
    pub jenkins_user: String,
    #[serde(default)]
    pub gerrit_user: String,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiBuildState {
    Queued,
    Running,
    Success,
    Failure,
    Unstable,
    Aborted,
    #[default]
    NotBuilt,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CiTrigger {
    User {
        id: String,
    },
    Gerrit {
        change: u64,
        patchset: u64,
    },
    Timer,
    Upstream {
        job: String,
        number: u64,
    },
    #[default]
    Other,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiBuild {
    pub job: String,
    #[serde(default)]
    pub number: Option<u64>,
    pub url_origin: String,
    pub path: String,
    pub state: CiBuildState,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub elapsed_ms: u64,
    #[serde(default)]
    pub estimated_ms: Option<u64>,
    #[serde(default)]
    pub eta_at: Option<DateTime<Utc>>,
    /// Backend-projected time remaining, for presentation without UI ETA arithmetic.
    #[serde(default)]
    pub remaining_ms: Option<u64>,
    #[serde(default)]
    pub overdue: bool,
    #[serde(default)]
    pub queue_reason: Option<String>,
    #[serde(default)]
    pub node: Option<String>,
    #[serde(default)]
    pub trigger: CiTrigger,
    #[serde(default)]
    pub commit_ids: Vec<String>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchHealth {
    Ok,
    NoPermission,
    NotFound,
    #[default]
    Stale,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceHealth {
    pub source: SourceSystem,
    #[serde(default)]
    pub observed_at: Option<DateTime<Utc>>,
    pub health: WatchHealth,
    #[serde(default)]
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildResultMark {
    pub number: u64,
    pub state: CiBuildState,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    /// Commits in the build's changeSets, for comparing a failure with the last success.
    #[serde(default)]
    pub commit_ids: Vec<String>,
    #[serde(default)]
    pub url_origin: String,
    #[serde(default)]
    pub path: String,
}

/// One step of a build. Only present when the build record has steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiStepState {
    Done,
    Failed,
    #[default]
    Pending,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiStep {
    pub name: String,
    pub state: CiStepState,
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
}

/// A byte window of a console log. `text` is exactly `end - start` bytes of the log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogWindow {
    pub text: String,
    pub start: u64,
    pub end: u64,
    pub total: u64,
}

/// Triage for the selected failed build: its last success, steps when known, and the first-error log window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureTriage {
    pub build: CiBuild,
    #[serde(default)]
    pub last_success: Option<BuildResultMark>,
    #[serde(default)]
    pub steps: Vec<CiStep>,
    #[serde(default)]
    pub log: Option<LogWindow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiMilestoneKind {
    CodeFreeze,
    Release,
}

/// A dated line on the release-train axis. Only set from a known source, never inferred.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiMilestone {
    pub kind: CiMilestoneKind,
    pub label: String,
    pub at: DateTime<Utc>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchedJob {
    pub id: String,
    pub job: String,
    #[serde(default)]
    pub role: CiJobRole,
    #[serde(default)]
    pub only_mine: bool,
    #[serde(default)]
    pub notify_on_failure: bool,
    #[serde(default)]
    pub last: Option<CiBuild>,
    #[serde(default)]
    pub history: Vec<BuildResultMark>,
    #[serde(default)]
    pub median_ms: Option<u64>,
    #[serde(default)]
    pub health: WatchHealth,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiChangeStatus {
    Open,
    Merged,
    Abandoned,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiChange {
    pub number: u64,
    pub subject: String,
    pub owner: String,
    pub patchset: u64,
    #[serde(default)]
    pub revision: String,
    pub status: CiChangeStatus,
    #[serde(default)]
    pub labels: BTreeMap<String, i8>,
    #[serde(default)]
    pub mergeable: Option<bool>,
    /// Embedded bounded build reference: immutable identity plus its observed status.
    #[serde(default)]
    pub ci: Option<CiBuild>,
    /// Six stages in order when present; empty for sources that do not derive them.
    #[serde(default)]
    pub pipeline: Vec<CiStage>,
    /// One line for the stage that needs attention, such as the failing build or next step.
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub note_state: Option<CiStageState>,
    pub url_origin: String,
    pub path: String,
}
/// A daily or release job's current, last finished and next scheduled build.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseTrainEntry {
    pub job: String,
    pub role: CiJobRole,
    #[serde(default)]
    pub current: Option<CiBuild>,
    #[serde(default)]
    pub last_finished: Option<CiBuild>,
    /// Only set when the schedule is known; never inferred.
    #[serde(default)]
    pub scheduled_at: Option<DateTime<Utc>>,
    /// Merged changes on watched branches not seen in this job's observed builds.
    #[serde(default)]
    pub pending_merges: Option<u32>,
    #[serde(default)]
    pub pending_mine: Option<u32>,
    /// Passed out of finished observed builds.
    #[serde(default)]
    pub recent: Option<CiRatio>,
    #[serde(default)]
    pub median_ms: Option<u64>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiKpis {
    #[serde(default)]
    pub submit_to_merge_median_ms: Option<u64>,
    #[serde(default)]
    pub merge_to_release_median_ms: Option<u64>,
    /// Passed out of finished verify-role builds in the last seven days.
    #[serde(default)]
    pub verify_pass_7d: Option<CiRatio>,
    #[serde(default)]
    pub watched_total: u32,
    #[serde(default)]
    pub watched_failing: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchedBranch {
    pub id: String,
    pub project: String,
    pub branch: String,
    #[serde(default)]
    pub only_mine: bool,
    #[serde(default)]
    pub include_merged: bool,
    #[serde(default)]
    pub changes: Vec<CiChange>,
    #[serde(default)]
    pub truncated: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiWatchSnapshot {
    #[serde(default)]
    pub observed_through: Option<DateTime<Utc>>,
    #[serde(default)]
    pub user: CiUser,
    #[serde(default)]
    pub my_builds: Vec<CiBuild>,
    #[serde(default)]
    pub jobs: Vec<WatchedJob>,
    #[serde(default)]
    pub branches: Vec<WatchedBranch>,
    #[serde(default)]
    pub sources: Vec<SourceHealth>,
    #[serde(default)]
    pub release_train: Vec<ReleaseTrainEntry>,
    #[serde(default)]
    pub kpis: CiKpis,
    #[serde(default)]
    pub milestones: Vec<CiMilestone>,
    #[serde(default)]
    pub triage: Option<FailureTriage>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetCiWatchRequest {
    #[serde(default)]
    pub refresh: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestCiWatchRequest {
    pub query: String,
    /// `seeds` asks for the first-run list (recent jobs and open-change branches) and takes no
    /// query. Absent means an ordinary search.
    #[serde(default)]
    pub mode: CiSuggestMode,
    /// A board source (a Gerrit change or a Jenkins build) to resolve to the one watchlist edit
    /// that follows it. Takes no query and is a search, not a seed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<crate::SourceRef>,
}
/// Whether a suggestion request searches for a term or seeds an empty watchlist.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiSuggestMode {
    #[default]
    Search,
    Seeds,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiSuggestionKind {
    Job,
    Branch,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiSuggestion {
    pub kind: CiSuggestionKind,
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub already_watched: bool,
    /// For seeds: how many of the user's builds (a job) or open changes (a branch) back it,
    /// counted over the last seven days for builds. Absent for ordinary search results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiSuggestions {
    #[serde(default)]
    pub items: Vec<CiSuggestion>,
    #[serde(default)]
    pub truncated: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobWatch {
    pub id: String,
    pub job: String,
    #[serde(default)]
    pub role: CiJobRole,
    #[serde(default)]
    pub only_mine: bool,
    #[serde(default)]
    pub notify_on_failure: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchWatch {
    pub id: String,
    pub project: String,
    pub branch: String,
    #[serde(default)]
    pub only_mine: bool,
    #[serde(default)]
    pub include_merged: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiWatchlist {
    #[serde(default)]
    pub jobs: Vec<JobWatch>,
    #[serde(default)]
    pub branches: Vec<BranchWatch>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiWatchFlag {
    OnlyMine,
    NotifyOnFailure,
    IncludeMerged,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum UpdateCiWatchlistRequest {
    AddJob {
        job: String,
    },
    AddBranch {
        project: String,
        branch: String,
    },
    Remove {
        id: String,
    },
    Toggle {
        id: String,
        flag: CiWatchFlag,
        value: bool,
    },
    SetRole {
        id: String,
        role: CiJobRole,
    },
}
impl CiWatchlist {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.jobs.len() > MAX_CI_JOBS || self.branches.len() > MAX_CI_BRANCHES {
            return Err(invalid("watchlist bounds exceeded"));
        }
        let mut ids = BTreeSet::new();
        let mut jobs = BTreeSet::new();
        let mut branches = BTreeSet::new();
        for j in &self.jobs {
            validate_ci_job(&j.job)?;
            if !bounded(&j.id, 768) || !ids.insert(&j.id) || !jobs.insert(&j.job) {
                return Err(invalid("duplicate job or ID"));
            }
        }
        for b in &self.branches {
            validate_ci_branch(&b.project, &b.branch)?;
            if !bounded(&b.id, 768)
                || !ids.insert(&b.id)
                || !branches.insert((&b.project, &b.branch))
            {
                return Err(invalid("duplicate branch or ID"));
            }
        }
        Ok(())
    }
    pub fn apply(&mut self, request: UpdateCiWatchlistRequest) -> Result<(), ContractError> {
        request.validate()?;
        let mut next = self.clone();
        match request {
            UpdateCiWatchlistRequest::AddJob { job } => {
                if next.jobs.iter().any(|j| j.job == job) {
                    return Err(invalid("job already watched"));
                }
                next.jobs.push(JobWatch {
                    id: format!("job:{job}"),
                    job,
                    role: CiJobRole::Other,
                    only_mine: false,
                    notify_on_failure: false,
                });
            }
            UpdateCiWatchlistRequest::AddBranch { project, branch } => {
                if next
                    .branches
                    .iter()
                    .any(|b| b.project == project && b.branch == branch)
                {
                    return Err(invalid("branch already watched"));
                }
                next.branches.push(BranchWatch {
                    id: format!("branch:{project}:{branch}"),
                    project,
                    branch,
                    only_mine: false,
                    include_merged: false,
                });
            }
            UpdateCiWatchlistRequest::Remove { id } => {
                let n = next.jobs.len() + next.branches.len();
                next.jobs.retain(|j| j.id != id);
                next.branches.retain(|b| b.id != id);
                if n == next.jobs.len() + next.branches.len() {
                    return Err(invalid("unknown watch ID"));
                }
            }
            UpdateCiWatchlistRequest::SetRole { id, role } => {
                let job = next
                    .jobs
                    .iter_mut()
                    .find(|j| j.id == id)
                    .ok_or_else(|| invalid("unknown watch ID"))?;
                job.role = role;
            }
            UpdateCiWatchlistRequest::Toggle { id, flag, value } => {
                if let Some(j) = next.jobs.iter_mut().find(|j| j.id == id) {
                    match flag {
                        CiWatchFlag::OnlyMine => j.only_mine = value,
                        CiWatchFlag::NotifyOnFailure => j.notify_on_failure = value,
                        _ => return Err(invalid("flag does not apply to job")),
                    }
                } else if let Some(b) = next.branches.iter_mut().find(|b| b.id == id) {
                    match flag {
                        CiWatchFlag::OnlyMine => b.only_mine = value,
                        CiWatchFlag::IncludeMerged => b.include_merged = value,
                        _ => return Err(invalid("flag does not apply to branch")),
                    }
                } else {
                    return Err(invalid("unknown watch ID"));
                }
            }
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
}
impl UpdateCiWatchlistRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::AddJob { job } => validate_ci_job(job),
            Self::AddBranch { project, branch } => validate_ci_branch(project, branch),
            Self::Remove { id } | Self::Toggle { id, .. } | Self::SetRole { id, .. } => {
                if bounded(id, 768) {
                    Ok(())
                } else {
                    Err(invalid("invalid watch ID"))
                }
            }
        }
    }
}
fn navigation(origin: &str, path: &str) -> Result<(), ContractError> {
    if !bounded(origin, 64)
        || !origin
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        || !bounded(path, 2048)
        || !path.starts_with('/')
        || path[1..]
            .split('/')
            .any(|s| s.is_empty() || matches!(s, "." | ".."))
        || path.contains(['?', '#', '\\'])
    {
        return Err(invalid("invalid CI navigation"));
    }
    Ok(())
}
impl CiBuild {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_ci_job(&self.job)?;
        navigation(&self.url_origin, &self.path)?;
        if (self.state == CiBuildState::Queued) != self.number.is_none()
            || self.number == Some(0)
            || self.commit_ids.len() > 100
            || self.commit_ids.iter().any(|id| {
                !(7..=64).contains(&id.len()) || !id.bytes().all(|b| b.is_ascii_hexdigit())
            })
            || self
                .queue_reason
                .as_ref()
                .is_some_and(|s| !bounded(s, 2048))
            || self.node.as_ref().is_some_and(|s| !bounded(s, 256))
        {
            return Err(invalid("invalid CI build identity or metadata"));
        }
        if self.eta_at.is_some()
            && (self.state != CiBuildState::Running
                || self.overdue
                || self.estimated_ms.is_none_or(|e| e == 0))
            || self.overdue
                && (self.state != CiBuildState::Running
                    || self
                        .estimated_ms
                        .is_none_or(|e| e == 0 || self.elapsed_ms <= e))
        {
            return Err(invalid("inconsistent CI ETA"));
        }
        match &self.trigger {
            CiTrigger::User { id } if !bounded(id, 256) => Err(invalid("invalid trigger user")),
            CiTrigger::Gerrit { change, patchset } if *change == 0 || *patchset == 0 => {
                Err(invalid("invalid Gerrit trigger"))
            }
            CiTrigger::Upstream { job, number } => {
                validate_ci_job(job)?;
                if *number == 0 {
                    Err(invalid("invalid upstream build"))
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }
}
impl CiWatchSnapshot {
    pub fn watchlist(&self) -> CiWatchlist {
        CiWatchlist {
            jobs: self
                .jobs
                .iter()
                .map(|j| JobWatch {
                    id: j.id.clone(),
                    job: j.job.clone(),
                    role: j.role,
                    only_mine: j.only_mine,
                    notify_on_failure: j.notify_on_failure,
                })
                .collect(),
            branches: self
                .branches
                .iter()
                .map(|b| BranchWatch {
                    id: b.id.clone(),
                    project: b.project.clone(),
                    branch: b.branch.clone(),
                    only_mine: b.only_mine,
                    include_merged: b.include_merged,
                })
                .collect(),
        }
    }
    pub fn apply_watchlist(&mut self, list: &CiWatchlist) {
        self.jobs = list
            .jobs
            .iter()
            .map(|entry| {
                let mut j = self
                    .jobs
                    .iter()
                    .find(|j| j.id == entry.id)
                    .cloned()
                    .unwrap_or(WatchedJob {
                        id: entry.id.clone(),
                        job: entry.job.clone(),
                        role: entry.role,
                        only_mine: entry.only_mine,
                        notify_on_failure: entry.notify_on_failure,
                        last: None,
                        history: vec![],
                        median_ms: None,
                        health: WatchHealth::Stale,
                    });
                j.role = entry.role;
                j.only_mine = entry.only_mine;
                j.notify_on_failure = entry.notify_on_failure;
                j
            })
            .collect();
        self.branches = list
            .branches
            .iter()
            .map(|entry| {
                let mut b = self
                    .branches
                    .iter()
                    .find(|b| b.id == entry.id)
                    .cloned()
                    .unwrap_or(WatchedBranch {
                        id: entry.id.clone(),
                        project: entry.project.clone(),
                        branch: entry.branch.clone(),
                        only_mine: entry.only_mine,
                        include_merged: entry.include_merged,
                        changes: vec![],
                        truncated: false,
                    });
                b.only_mine = entry.only_mine;
                b.include_merged = entry.include_merged;
                b
            })
            .collect();
    }
    pub fn validate(&self) -> Result<(), ContractError> {
        self.watchlist().validate()?;
        if self.my_builds.len() > MAX_CI_BUILDS
            || self.sources.len() > 2
            || self.user.jenkins_user.len() > 256
            || self.user.gerrit_user.len() > 256
        {
            return Err(invalid("CI snapshot bounds exceeded"));
        }
        let mut sources = BTreeSet::new();
        for s in &self.sources {
            if !matches!(s.source, SourceSystem::Jenkins | SourceSystem::Gerrit)
                || !sources.insert(s.source)
                || s.error.as_ref().is_some_and(|e| !bounded(e, 1024))
            {
                return Err(invalid("invalid CI source health"));
            }
        }
        for b in &self.my_builds {
            b.validate()?;
        }
        if self.release_train.len() > MAX_CI_JOBS {
            return Err(invalid("release train bounds exceeded"));
        }
        for entry in &self.release_train {
            entry.validate()?;
        }
        if let Some(r) = &self.kpis.verify_pass_7d {
            r.validate()?;
        }
        if self.kpis.watched_failing > self.kpis.watched_total {
            return Err(invalid("CI KPI failing exceeds watched"));
        }
        if self.milestones.len() > MAX_CI_MILESTONES
            || self.milestones.iter().any(|m| !bounded(&m.label, 64))
        {
            return Err(invalid("invalid CI milestones"));
        }
        if let Some(t) = &self.triage {
            t.validate()?;
        }
        for j in &self.jobs {
            if j.history.len() > MAX_CI_HISTORY
                || j.history.iter().any(|h| {
                    h.number == 0
                        || h.state == CiBuildState::Queued
                        || h.commit_ids.len() > MAX_CI_CHANGES
                        || (!h.path.is_empty() && navigation(&h.url_origin, &h.path).is_err())
                })
            {
                return Err(invalid("invalid CI history"));
            }
            if let Some(b) = &j.last {
                b.validate()?;
                if b.job != j.job {
                    return Err(invalid("job/build mismatch"));
                }
            }
        }
        for branch in &self.branches {
            if branch.changes.len() > MAX_CI_CHANGES {
                return Err(invalid("too many branch changes"));
            }
            let mut seen = BTreeSet::new();
            for c in &branch.changes {
                navigation(&c.url_origin, &c.path)?;
                if c.number == 0
                    || c.patchset == 0
                    || !seen.insert(c.number)
                    || !bounded(&c.subject, 1024)
                    || c.owner.len() > 256
                    || c.labels.iter().any(|(k, v)| {
                        !matches!(k.as_str(), "Verified" | "Code-Review") || !(-2..=2).contains(v)
                    })
                {
                    return Err(invalid("invalid CI change"));
                }
                c.validate_pipeline()?;
                if let Some(b) = &c.ci {
                    b.validate()?;
                    if !(!c.revision.is_empty() && b.commit_ids.contains(&c.revision)
                        || matches!(b.trigger,CiTrigger::Gerrit{change,patchset} if change==c.number && patchset==c.patchset))
                    {
                        return Err(invalid("CI build does not match current patchset"));
                    }
                }
            }
        }
        Ok(())
    }
}
impl CiChange {
    fn validate_pipeline(&self) -> Result<(), ContractError> {
        if !self.pipeline.is_empty() {
            let shaped = self.pipeline.len() == CiStageKind::ALL.len()
                && self
                    .pipeline
                    .iter()
                    .zip(CiStageKind::ALL)
                    .all(|(stage, kind)| {
                        stage.kind == kind && bounded(&stage.label, MAX_CI_STAGE_LABEL)
                    });
            if !shaped {
                return Err(invalid(
                    "CI pipeline must list six labelled stages in order",
                ));
            }
        }
        if self.note.as_ref().is_some_and(|n| !bounded(n, MAX_CI_NOTE))
            || (self.note.is_none() && self.note_state.is_some())
        {
            return Err(invalid("invalid CI change note"));
        }
        Ok(())
    }
}
impl FailureTriage {
    fn validate(&self) -> Result<(), ContractError> {
        self.build.validate()?;
        if self.build.state != CiBuildState::Failure {
            return Err(invalid("triage build must be a failure"));
        }
        if let Some(mark) = &self.last_success {
            if mark.number == 0 || mark.state != CiBuildState::Success {
                return Err(invalid("triage last success must be a success"));
            }
        }
        if self.steps.len() > MAX_CI_STEPS || self.steps.iter().any(|s| !bounded(&s.name, 256)) {
            return Err(invalid("invalid CI steps"));
        }
        if let Some(log) = &self.log {
            if log.text.len() > MAX_CI_LOG_WINDOW
                || log.start > log.end
                || log.end > log.total
                || log.text.len() as u64 != log.end - log.start
            {
                return Err(invalid("invalid CI log window"));
            }
        }
        Ok(())
    }
}
impl CiRatio {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.passed > self.total {
            return Err(invalid("CI ratio passed exceeds total"));
        }
        Ok(())
    }
}
impl ReleaseTrainEntry {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_ci_job(&self.job)?;
        if !matches!(self.role, CiJobRole::Daily | CiJobRole::Release) {
            return Err(invalid("release train role must be daily or release"));
        }
        if let Some(b) = &self.current {
            b.validate()?;
            if b.job != self.job || !matches!(b.state, CiBuildState::Running | CiBuildState::Queued)
            {
                return Err(invalid("release train current build mismatch"));
            }
        }
        if let Some(b) = &self.last_finished {
            b.validate()?;
            if b.job != self.job || matches!(b.state, CiBuildState::Running | CiBuildState::Queued)
            {
                return Err(invalid("release train finished build mismatch"));
            }
        }
        if let Some(r) = &self.recent {
            r.validate()?;
            if r.total as usize > MAX_CI_HISTORY {
                return Err(invalid("release train history bound exceeded"));
            }
        }
        if self.pending_mine.unwrap_or(0) > self.pending_merges.unwrap_or(u32::MAX) {
            return Err(invalid("pending mine exceeds pending merges"));
        }
        Ok(())
    }
}
impl SuggestCiWatchRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        if let Some(source) = &self.source {
            if self.mode != CiSuggestMode::Search || !self.query.is_empty() {
                return Err(invalid("a source resolves alone, without a query or seeds"));
            }
            if !matches!(
                source.system,
                crate::SourceSystem::Gerrit | crate::SourceSystem::Jenkins
            ) {
                return Err(invalid("only Gerrit and Jenkins sources can be followed"));
            }
            return validate_ci_query(&self.query);
        }
        match self.mode {
            CiSuggestMode::Seeds if !self.query.is_empty() => {
                Err(invalid("seed suggestions take no query"))
            }
            _ => validate_ci_query(&self.query),
        }
    }
}
impl CiSuggestions {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.items.len() > 20 {
            return Err(invalid("too many CI suggestions"));
        }
        for i in &self.items {
            if !bounded(&i.label, 512) {
                return Err(invalid("invalid CI suggestion"));
            }
            match i.kind {
                CiSuggestionKind::Job => validate_ci_job(&i.value)?,
                CiSuggestionKind::Branch => {
                    let (p, b) = i
                        .value
                        .split_once(':')
                        .ok_or_else(|| invalid("missing project:branch"))?;
                    validate_ci_branch(p, b)?;
                }
            }
        }
        Ok(())
    }
}

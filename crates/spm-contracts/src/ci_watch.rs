//! Read-only CI observations and local watchlist edits. ETA is projected by SPM.
use crate::{ContractError, SourceSystem};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_CI_JOBS: usize = 50;
pub const MAX_CI_BRANCHES: usize = 20;
pub const MAX_CI_BUILDS: usize = 100;
pub const MAX_CI_CHANGES: usize = 20;
pub const MAX_CI_HISTORY: usize = 10;
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
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchedJob {
    pub id: String,
    pub job: String,
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
    pub url_origin: String,
    pub path: String,
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
            Self::Remove { id } | Self::Toggle { id, .. } => {
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
                        only_mine: entry.only_mine,
                        notify_on_failure: entry.notify_on_failure,
                        last: None,
                        history: vec![],
                        median_ms: None,
                        health: WatchHealth::Stale,
                    });
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
        for j in &self.jobs {
            if j.history.len() > MAX_CI_HISTORY
                || j.history
                    .iter()
                    .any(|h| h.number == 0 || h.state == CiBuildState::Queued)
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

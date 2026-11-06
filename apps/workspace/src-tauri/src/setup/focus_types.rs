//! The shapes `spm focus` returns, shared with the frontend through ts-rs.
use super::*;

/// One repository's share of a model's changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct RepoRow {
    pub project: String,
    #[ts(type = "number")]
    pub changes: u64,
    #[ts(type = "number")]
    pub merged: u64,
    #[ts(type = "number")]
    pub open: u64,
    #[ts(type = "number")]
    pub authors: u64,
    #[ts(type = "number")]
    pub flagged: u64,
    pub days: Vec<u32>,
    pub sample_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct MineReason {
    pub key: String,
    pub points: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct MyIssue {
    pub source: String,
    pub id: String,
    pub display_id: String,
    pub title: String,
    pub url: String,
    pub status: String,
    pub group: String,
    pub severity: String,
    pub model: String,
    pub followed: bool,
    pub score: u32,
    pub reasons: Vec<MineReason>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct MineSource {
    pub source: String,
    #[ts(type = "number")]
    pub count: u64,
    pub error: String,
}

/// What is assigned to the signed-in user, most urgent first.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct Mine {
    pub computed_at: String,
    pub sources: Vec<MineSource>,
    pub items: Vec<MyIssue>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct BackupSummary {
    pub path: String,
    #[ts(type = "number")]
    pub models: u64,
    #[ts(type = "number")]
    pub items: u64,
}

/// What restoring a backup would do; nothing changes until it is applied.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct BackupPlan {
    pub created_at: String,
    #[ts(type = "number")]
    pub added: u64,
    #[ts(type = "number")]
    pub replaced: u64,
    #[ts(type = "number")]
    pub kept: u64,
    #[ts(type = "number")]
    pub items: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct BackupRestored {
    #[ts(type = "number")]
    pub models: u64,
    #[ts(type = "number")]
    pub items: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct SearchHit {
    /// item or change
    #[serde(rename = "type")]
    pub kind: String,
    pub model: String,
    pub id: String,
    pub title: String,
    pub status: String,
    pub owner: String,
    pub snippet: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct SearchResults {
    #[ts(type = "number")]
    pub total: u64,
    pub results: Vec<SearchHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ReleaseLine {
    pub id: String,
    pub label: String,
    pub lifecycle: String,
    pub phase: String,
    pub branches: Vec<String>,
    pub jobs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SourceSegment {
    pub branch: String,
    #[ts(type = "number")]
    pub first_build: u64,
    #[ts(type = "number")]
    pub last_build: u64,
    pub note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct JobSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub history: Option<Vec<SourceSegment>>,
    pub branch: String,
    pub previous: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub since_build: Option<u64>,
    pub note: String,
}
/// A followed model as `spm focus` stores it; SPM validates every value.
/// A named sprint with its own dates, both YYYY-MM-DD and inclusive.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Iteration {
    pub name: String,
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FocusModel {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub legacy: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub aliases: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub iterations: Option<Vec<Iteration>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub job_sources: Option<BTreeMap<String, JobSource>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub branch_jobs: Option<BTreeMap<String, Vec<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub release_lines: Option<Vec<ReleaseLine>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ignored_jobs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tracking: Option<TrackingConfig>,
    pub code: String,
    #[serde(default)]
    pub issues: Vec<ScopeQuery>,
    #[serde(default)]
    pub branches: Vec<String>,
    /// Jenkins jobs by full name; empty reads the model's Project_Daily_Build folder.
    #[serde(default)]
    pub jobs: Vec<String>,
    /// Rule → action: revert, feature_after_freeze, customer_feedback, other_model,
    /// no_issue_id, automation → alert, track, log, ignore.
    #[serde(default)]
    pub rules: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub freeze: Option<String>,
    pub targets: FocusTargets,
    #[serde(default)]
    pub accepted: Vec<u32>,
    #[serde(default)]
    pub tracked: Vec<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Candidate {
    pub name: String,
    pub count: u32,
    pub picked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scope: Option<ScopeQuery>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Suggestions {
    pub code: String,
    pub stem: String,
    pub meegle: Vec<Candidate>,
    pub jira: Vec<Candidate>,
    pub branches: Vec<Candidate>,
    pub rule_counts: std::collections::BTreeMap<String, u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BranchStrip {
    pub branch: String,
    /// main, ir, pb, mp or old, from the name.
    pub phase: String,
    pub total: u32,
    pub days: Vec<u32>,
    /// Changes on the branch in 需要留意.
    pub flagged: u32,
}
/// What a model's settings surface over the last summary's changes.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PreviewSide {
    pub flagged: u32,
    pub alerts: u32,
    pub rule_counts: std::collections::BTreeMap<String, u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Preview {
    pub before: PreviewSide,
    pub after: PreviewSide,
    pub entering: Vec<Flagged>,
    pub leaving: Vec<Flagged>,
    pub changes: u32,
}
/// A change on one branch with what the rules made of it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Listed {
    pub number: u32,
    pub subject: String,
    pub project: String,
    pub branch: String,
    pub owner: String,
    pub status: String,
    pub created: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub created_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    pub rules: Vec<String>,
    pub issues: Vec<String>,
    /// flagged, earlier, tracked, accepted, logged, ignored or clean.
    pub state: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BuildRun {
    pub number: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub result: Option<String>,
    pub building: bool,
    /// Start, epoch milliseconds.
    #[ts(type = "number")]
    pub timestamp: i64,
    #[ts(type = "number")]
    pub duration: u64,
    #[ts(type = "number")]
    pub estimated: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cause: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_parameters: Option<BTreeMap<String, String>>,
}
/// A build parameter that differs from the build before.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ParamChange {
    pub build: u32,
    pub name: String,
    #[ts(type = "unknown")]
    pub before: serde_json::Value,
    #[ts(type = "unknown")]
    pub after: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct JobView {
    pub job: String,
    pub name: String,
    pub phase: String,
    pub disabled: bool,
    pub builds: Vec<BuildRun>,
    #[ts(type = "Record<string, unknown>")]
    pub parameters: std::collections::BTreeMap<String, serde_json::Value>,
    pub changes: Vec<ParamChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct QueuedBuild {
    #[ts(type = "number")]
    pub id: u64,
    pub job: String,
    #[ts(type = "number | null")]
    pub since: Option<i64>,
    pub reason: Option<String>,
    pub blocked: bool,
    pub buildable: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CapacityTask {
    #[ts(type = "number")]
    pub id: u64,
    pub job: String,
    pub label: Option<String>,
    #[ts(type = "number | null")]
    pub since: Option<i64>,
    pub blocked: bool,
    pub buildable: bool,
    pub concurrent: Option<bool>,
    #[ts(type = "number | null")]
    pub low: Option<u64>,
    #[ts(type = "number | null")]
    pub high: Option<u64>,
    pub samples: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CapacitySlot {
    pub node: String,
    pub labels: Vec<String>,
    pub online: bool,
    pub idle: bool,
    pub job: Option<String>,
    #[ts(type = "number | null")]
    pub started: Option<i64>,
    #[ts(type = "number | null")]
    pub low: Option<u64>,
    #[ts(type = "number | null")]
    pub high: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BuildCapacity {
    pub observed_at: String,
    pub slots: Vec<CapacitySlot>,
    pub tasks: Vec<CapacityTask>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BuildQueue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capacity: Option<BuildCapacity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capacity_error: Option<String>,
    pub server_count: usize,
    pub observed_at: String,
    pub items: Vec<QueuedBuild>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Builds {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub folder: Option<String>,
    pub jobs: Vec<JobView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub queue: Option<BuildQueue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub queue_error: Option<String>,
    pub computed_at: String,
    pub errors: Vec<String>,
}
/// A change the model's rules surfaced.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Flagged {
    pub number: u32,
    pub subject: String,
    pub project: String,
    pub branch: String,
    pub owner: String,
    pub status: String,
    pub created: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    pub rules: Vec<String>,
    pub issues: Vec<String>,
    pub tracked: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct IssueLink {
    pub id: String,
    pub url: String,
    pub resolved: bool,
    pub severe: bool,
    pub fatal: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ModelSummary {
    pub code: String,
    pub targets: FocusTargets,
    pub issues: u32,
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub issue_links: Option<Vec<IssueLink>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub measures: Option<Measures>,
    pub new_issues: u32,
    pub days: Vec<u32>,
    pub days_from: String,
    pub week_changes: u32,
    pub week_merged: u32,
    pub branches: Vec<BranchStrip>,
    pub flagged: Vec<Flagged>,
    pub rule_counts: std::collections::BTreeMap<String, u32>,
    pub rules: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub freeze: Option<String>,
    pub errors: Vec<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct FocusSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub computed_at: Option<String>,
    pub models: Vec<ModelSummary>,
}
/// How a model's issues and requirements are classified; SPM validates it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct TrackingConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub jira_software_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub jira_requirement_jql: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub jira_software_values: Option<Vec<String>>,
}
/// A software issue or requirement in a model's scope.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkItem {
    /// The source's own labels: Jira's labels, 飞书项目's 标签.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub labels: Option<Vec<String>>,
    /// What part of the product it touches: Jira's 领域, 飞书项目's 组件.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub domains: Option<Vec<String>>,
    pub id: String,
    /// The source server the item lives on; empty in reads from before instances.
    #[serde(default)]
    pub instance: String,
    pub display_id: String,
    pub title: String,
    pub url: String,
    pub source: String,
    pub scope_id: String,
    pub status: String,
    /// pending, active, review, testing, blocked, done, cancelled or unknown.
    pub group: String,
    pub resolution: String,
    pub owner: String,
    pub severity: String,
    pub updated: String,
    pub software_evidence: String,
    /// Requirements: customer or internal.
    #[serde(default)]
    pub origin: String,
    /// internet for lockscreen poster, ads, Push and analytics work.
    #[serde(default)]
    pub origin_kind: String,
    #[serde(default)]
    pub origin_evidence: String,
    /// Gerrit changes whose subjects name the item.
    #[serde(default)]
    pub commits: Vec<u32>,
    #[serde(default)]
    pub activity: ItemActivity,
    /// Tags, corrections, notes and the next check kept on this machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub local: Option<LocalLayer>,
}
/// Whether an item is moving: status age, the last human comment, versions, reopens.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct ItemActivity {
    pub created: String,
    pub status_since: String,
    /// history, created, field or observed.
    pub status_since_source: String,
    pub last_comment: Option<Comment>,
    pub comment_count: u32,
    pub progress: String,
    pub found_versions: Vec<String>,
    pub target_versions: Vec<String>,
    pub reopened: u32,
    pub last_activity: String,
    /// When the code bot last said a change was merged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub merged_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Comment {
    pub at: String,
    pub by: String,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ChangeFiles {
    pub change: u32,
    /// The full commit message; absent when Gerrit did not return it.
    #[serde(default)]
    pub message: Option<String>,
    /// Gerrit's review history: uploads, votes, merge, abandon, comments.
    #[serde(default)]
    pub timeline: Option<Vec<ChangeEvent>>,
    pub files: Vec<ChangeFile>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ChangeEvent {
    pub at: String,
    pub by: String,
    pub bot: bool,
    /// upload, review, merged, abandoned, restored or comment
    pub kind: String,
    pub patch_set: Option<u32>,
    pub votes: Vec<(String, String)>,
    pub text: String,
}
/// One file of a change's current patchset.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ChangeFile {
    pub path: String,
    /// ADDED, MODIFIED, DELETED, RENAMED, COPIED or REWRITE
    pub kind: String,
    pub insertions: u32,
    pub deletions: u32,
}
/// One file's unified diff, from its `diff --git` line.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ChangeDiff {
    pub change: u32,
    pub path: String,
    pub patch_set: Option<u32>,
    pub diff: String,
    pub truncated: bool,
}
/// One line of the shared log.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LogEntry {
    pub ts: String,
    pub level: String,
    pub app: String,
    pub event: String,
    pub detail: String,
    /// Ties the entries of one user action together; empty for plain text logs.
    pub run_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LogCount {
    pub total: u32,
    pub problems: u32,
}
/// Every tool's log merged, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Logs {
    pub entries: Vec<LogEntry>,
    pub apps: std::collections::BTreeMap<String, LogCount>,
    pub truncated: bool,
}
/// One value a calendar event rests on, and where to find it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CalOrigin {
    pub role: String,
    pub document: String,
    pub place: String,
    pub field: String,
    pub value: String,
    pub url: String,
}
/// One dated stage or milestone on the calendar.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CalEvent {
    pub id: String,
    pub source: String,
    pub key: String,
    pub stage: String,
    pub milestone: bool,
    pub project: String,
    pub model: String,
    pub legacy: bool,
    pub start: String,
    pub end: String,
    pub status: String,
    pub version: String,
    pub customer: String,
    pub owner: String,
    pub note: String,
    #[serde(default)]
    pub origins: Vec<CalOrigin>,
}
/// How one calendar source read.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CalSource {
    pub source: String,
    pub count: u32,
    pub error: String,
}
/// Everything scheduled across projects.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Calendar {
    pub computed_at: String,
    pub configured: bool,
    pub sources: Vec<CalSource>,
    pub events: Vec<CalEvent>,
}
/// One thing waiting on the signed-in user.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct NeedItem {
    pub kind: String,
    pub title: String,
    pub model: String,
    pub id: String,
    pub url: String,
    pub since: String,
}
/// The queue of what cannot move until the user does.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Needs {
    pub computed_at: String,
    pub items: Vec<NeedItem>,
    pub error: String,
}
/// When one release gate closes at the recent pace.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct GateForecast {
    pub gate: String,
    pub target_pct: u32,
    pub gap: u32,
    pub resolved_per_week: f64,
    pub arrived_per_week: f64,
    pub state: String,
    pub likely: Option<String>,
    pub safe: Option<String>,
    /// The gap at each week's end, oldest first; the last is now.
    pub history: Vec<i32>,
}
/// When a model reaches its gates, against its freeze date.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Forecast {
    pub code: String,
    pub freeze: Option<String>,
    pub state: String,
    pub limiting: String,
    pub likely: Option<String>,
    pub safe: Option<String>,
    pub late_days: Option<i32>,
    pub gates: Vec<GateForecast>,
}
/// One timed step of an action.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TraceSpan {
    pub app: String,
    pub name: String,
    pub verb: String,
    /// In the tree; a span's children follow it.
    pub depth: u32,
    pub offset_ms: u32,
    pub ms: u32,
    /// Time not covered by children.
    pub self_ms: u32,
    /// ok, failed, running, or lost when its process ended first.
    pub status: String,
    pub code: String,
    /// The stretches the whole action had to wait for, from the trace's start.
    pub critical: Vec<(u32, u32)>,
}
/// One action across Workspace, spm, tools and bridges.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Trace {
    pub id: String,
    pub run_id: String,
    pub app: String,
    pub name: String,
    pub started: String,
    pub ms: u32,
    pub running: bool,
    pub failed: u32,
    pub spans: Vec<TraceSpan>,
}
/// Recent actions, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Traces {
    pub traces: Vec<Trace>,
    pub truncated: bool,
}
/// A model's platform: vendor, Android and kernel from its code, the chip as named.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Platform {
    pub code: String,
    pub vendor: String,
    pub chip: String,
    #[serde(default)]
    pub baseband: String,
    #[serde(default)]
    pub modem: Vec<String>,
    #[serde(default)]
    pub modem_repos: Vec<String>,
    #[serde(default)]
    pub modem_changes: usize,
    pub android: Vec<String>,
    pub kernel: Vec<String>,
    pub evidence: Vec<String>,
}
/// A value and every place that states it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProfileValue {
    pub value: String,
    pub origins: Vec<CalOrigin>,
}
/// One answer about a model; empty while its sources disagree.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProfileField {
    pub field: String,
    pub value: String,
    pub values: Vec<ProfileValue>,
    pub conflict: bool,
}
/// One model as every view should describe it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ModelProfile {
    pub code: String,
    pub aliases: Vec<String>,
    pub suggested_aliases: Vec<String>,
    pub platform: Platform,
    pub fields: Vec<ProfileField>,
    /// development, release, ota or maintenance.
    #[serde(default)]
    pub phase: String,
    /// you, board, plan or code.
    #[serde(default)]
    pub phase_from: String,
    pub read_at: String,
}
/// One item on its way into an OTA.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OtaItem {
    pub id: String,
    pub display_id: String,
    pub source: String,
    pub instance: String,
    pub kind: String,
    pub title: String,
    pub url: String,
    pub status: String,
    pub owner: String,
    pub severity: String,
    pub labels: Vec<String>,
    pub tags: Vec<String>,
    pub stage: String,
    pub must: bool,
    pub customer: bool,
    pub late: bool,
    pub reason: String,
}
/// One OTA: when it ships, what it carries and how far that has come.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OtaPlan {
    pub version: String,
    pub pre1: String,
    pub pre2: String,
    pub official: String,
    pub rollout: String,
    pub state: String,
    pub total: usize,
    pub merged: usize,
    pub verified: usize,
    pub late: usize,
    pub must_open: usize,
    pub items: Vec<OtaItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ModelOta {
    pub code: String,
    pub plans: Vec<OtaPlan>,
    pub undecided: Vec<OtaItem>,
    pub left_out: Vec<OtaItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OtaReport {
    pub models: Vec<ModelOta>,
    pub errors: Vec<String>,
}
/// Gerrit changes bound to an item, and items like it with how they were fixed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Related {
    #[serde(default)]
    pub platform: Option<Platform>,
    pub item: String,
    #[serde(default)]
    pub instance: String,
    pub changes: Vec<RelatedChange>,
    pub similar: Vec<SimilarItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RelatedChange {
    pub change: u32,
    /// The model whose summary read the change; None when none did.
    pub model: Option<String>,
    pub subject: Option<String>,
    pub status: Option<String>,
    pub branch: Option<String>,
    pub project: Option<String>,
    pub owner: Option<String>,
    pub url: Option<String>,
    pub evidence: Vec<LinkEvidence>,
}
/// Why an item and a change belong together: subject, message, field or flow.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LinkEvidence {
    pub kind: String,
    pub detail: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SimilarItem {
    pub model: String,
    pub kind: String,
    pub item: SimilarWorkItem,
    pub score: f64,
    /// The score weighed by what the two models' platforms share.
    #[serde(default)]
    pub rank: f64,
    #[serde(default)]
    pub platform: Option<Platform>,
    /// vendor, chip, kernel or android
    #[serde(default)]
    pub same_platform: Vec<String>,
    pub shared: Vec<String>,
    pub changes: Vec<RelatedChange>,
    pub cause: Option<String>,
    pub plan: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SimilarWorkItem {
    pub source: String,
    pub instance: String,
    pub id: String,
    pub display_id: String,
    pub title: String,
    pub url: String,
    pub status: String,
    pub group: String,
    pub owner: String,
    pub severity: String,
}
/// A 飞书项目 item's 流转记录, oldest first.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ItemFlow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fetched_at: Option<String>,
    pub item: String,
    pub events: Vec<FlowEvent>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SourceField {
    pub key: String,
    pub name: String,
    pub kind: String,
    pub value: String,
    pub available: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SourceFields {
    pub item: String,
    pub address: String,
    pub item_updated: String,
    pub fetched_at: String,
    pub fields: Vec<SourceField>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct FlowFailure {
    pub item: String,
    pub address: Option<String>,
    pub code: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct FlowEvent {
    pub at: String,
    pub by: String,
    pub bot: bool,
    /// created, status, assign, field or comment
    pub kind: String,
    pub field: String,
    pub from: String,
    pub to: String,
    pub text: String,
}
/// Every file's diff from one read; files missing after a cut are read one by one.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ChangeDiffs {
    pub change: u32,
    pub patch_set: Option<u32>,
    pub files: std::collections::BTreeMap<String, String>,
    pub truncated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AnnotatedItem {
    /// issues or requirements
    pub kind: String,
    pub item: WorkItem,
}
/// One item's local history: each change to its layer and each status change seen here.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ItemHistory {
    #[serde(default)]
    pub instance: String,
    pub local: Option<LocalLayer>,
    pub events: Vec<LocalEvent>,
    pub transitions: Vec<Transition>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LocalEvent {
    pub at: String,
    /// tags, correct, uncorrect, note, next_check or import
    pub kind: String,
    #[ts(type = "Record<string, unknown>")]
    pub payload: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Transition {
    /// None when the item was first seen.
    pub from: Option<String>,
    pub to: String,
    pub at: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct LocalLayer {
    pub tags: Vec<String>,
    pub corrections: Vec<Correction>,
    pub notes: Vec<Note>,
    pub next_check: Option<String>,
}
/// A local value with the remote value it replaced; `state` is active, resolved or conflict.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Correction {
    pub field: String,
    pub value: String,
    pub remote: String,
    pub reason: String,
    pub at: String,
    pub state: String,
    pub remote_now: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Note {
    pub at: String,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DomainValue {
    pub value: String,
    pub count: u32,
    pub software: bool,
}
/// The Jira field that decides software, with every value seen.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Classification {
    pub field: String,
    pub field_name: String,
    pub configured: bool,
    pub values: Vec<DomainValue>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TrackingSource {
    pub id: String,
    pub source: String,
    pub label: String,
    pub filter: String,
    pub software_filter: String,
    pub total: u32,
    pub matched: u32,
    pub excluded: u32,
    pub unclassified: u32,
    pub updated_at: Option<String>,
    pub stale: bool,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub classification: Option<Classification>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkItems {
    pub code: String,
    pub kind: String,
    pub checked_at: String,
    pub sources: Vec<TrackingSource>,
    pub items: Vec<WorkItem>,
}
#[derive(Debug, Default, Serialize, Deserialize, TS)]
pub struct FocusData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub build_evidence: Option<BuildEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub work_items: Option<WorkItems>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub local: Option<LocalLayer>,
    /// After an annotation: the item as the last read shows it, with the new layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub work_item: Option<AnnotatedItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub history: Option<ItemHistory>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub change_files: Option<ChangeFiles>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub change_diff: Option<ChangeDiff>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub change_diffs: Option<ChangeDiffs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub flow: Option<ItemFlow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub related: Option<Related>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub platforms: Option<Vec<Platform>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub profiles: Option<Vec<ModelProfile>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ota: Option<OtaReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub logs: Option<Logs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub traces: Option<Traces>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub search: Option<SearchResults>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mine: Option<Mine>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub needs: Option<Needs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub forecasts: Option<Vec<Forecast>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub calendar: Option<Calendar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repos: Option<Vec<RepoRow>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub backup: Option<BackupSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub backup_plan: Option<BackupPlan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub backup_restored: Option<BackupRestored>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub flows: Option<Vec<ItemFlow>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub flow_errors: Option<Vec<FlowFailure>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_fields: Option<SourceFields>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub models: Option<Vec<FocusModel>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub suggestions: Option<Suggestions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub summary: Option<FocusSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub preview: Option<Preview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub changes: Option<Vec<Listed>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub builds: Option<Builds>,
}

/// Structured evidence from at most 320 KiB of console output.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BuildEvidence {
    pub job: String,
    pub number: u32,
    pub inspected_at: String,
    pub windows: Vec<LogWindow>,
    #[ts(type = "number | null")]
    pub total_bytes: Option<u64>,
    pub software_number: Option<String>,
    pub system_manifest: Option<String>,
    pub vendor_manifest: Option<String>,
    pub vendor_skipped: bool,
    pub archive_commit: Option<String>,
    pub archive_repository: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LogWindow {
    #[ts(type = "number")]
    pub start: u64,
    #[ts(type = "number")]
    pub bytes: u64,
}

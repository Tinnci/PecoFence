//! SPM project setup through the owner CLI: `spm setup import-template` and
//! `spm setup status`. Workspace never writes project configuration itself.
#![cfg_attr(not(windows), allow(dead_code))]
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Largest template accepted for the private copy; real templates are a few KiB.
pub const TEMPLATE_LIMIT: u64 = 1024 * 1024;

#[derive(Debug, Serialize, TS)]
pub struct SetupFailure {
    pub code: String,
    /// Owner's message for template and import errors; absent for runtime failures.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
}
impl SetupFailure {
    pub fn new(code: impl AsRef<str>) -> Self {
        let code = match code.as_ref() {
            "spm_missing"
            | "spm_update_required"
            | "spm_timeout"
            | "spm_invalid_response"
            | "setup_busy"
            | "template_cancelled"
            | "template_unreadable"
            | "template_too_large"
            | "template_stale"
            | "sign_in_unsupported"
            | "invalid_input" => code.as_ref(),
            _ => "spm_unavailable",
        };
        Self {
            code: code.into(),
            message: None,
        }
    }
    fn owner(code: &str, message: Option<String>) -> Self {
        let code = match code {
            "TEMPLATE_INVALID" => "template_invalid",
            "IMPORT_CONFLICT" => "import_conflict",
            "IMPORT_IO" => "import_io",
            "SIGN_IN_HOST_UNKNOWN" => "sign_in_host_unknown",
            "SIGN_IN_FAILED" => "sign_in_failed",
            "SIGN_IN_UNSUPPORTED" => "sign_in_unsupported",
            "INVALID_INPUT" => "invalid_input",
            "NOT_INSTALLED" => "tool_missing",
            "SIGNED_OUT" => "signed_out",
            "SOURCE_FAILED" => "source_failed",
            "SCOPE_EMPTY" => "scope_empty",
            "SCOPE_TOO_LARGE" => "scope_too_large",
            "SCOPE_INCOMPLETE" => "scope_incomplete",
            "MAPPING_UNDETECTED" => "mapping_undetected",
            "QUESTIONS_OPEN" => "questions_open",
            "DRAFT_CHANGED" => "draft_changed",
            "TOOL_OUTDATED" => "tool_outdated",
            "PROJECT_INVALID" => "project_invalid",
            "ACTIVATE_UNAVAILABLE" => "activate_unavailable",
            "ACTIVATE_NOT_READY" => "activate_not_ready",
            "ACTIVATE_FAILED" => "activate_failed",
            "ACTIVATE_TIMEOUT" => "activate_timeout",
            "NOT_FOLLOWED" => "not_followed",
            _ => return Self::new("spm_invalid_response"),
        };
        Self {
            code: code.into(),
            message: message.map(|m| m.chars().take(500).collect()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct TemplateInfo {
    pub id: String,
    pub version: String,
    pub title: String,
    pub sha256: String,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct PlanFile {
    pub role: String,
    pub path: String,
    /// create, unchanged, replace or conflict
    pub action: String,
}
#[derive(Debug, Default, Serialize, Deserialize, TS)]
pub struct ProjectCheck {
    pub valid: bool,
    #[serde(default)]
    pub ready: bool,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ImportPlan {
    pub template: TemplateInfo,
    pub project: String,
    pub scope: String,
    pub mode: String,
    pub directory: String,
    pub files: Vec<PlanFile>,
    pub applicable: bool,
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub backups: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub check: Option<ProjectCheck>,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ImportedProject {
    /// Absent when a directory name is not valid UTF-8.
    pub project: Option<String>,
    pub scope: Option<String>,
    pub directory: String,
    #[serde(default)]
    pub check: ProjectCheck,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ProjectStatus {
    pub config_root: String,
    pub projects: Vec<ImportedProject>,
}

/// One source tool's login as observed by `spm connections` (ADR-0004).
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct Connection {
    /// meegle, jira, gerrit or jenkins
    pub source: String,
    pub tool: String,
    /// connected, signed_out, not_configured, unreachable, not_installed or error
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub code: Option<String>,
    #[serde(default)]
    pub sign_in: bool,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ConnectionReport {
    pub checked_at: String,
    pub connections: Vec<Connection>,
}
#[derive(Deserialize)]
struct SignedIn {
    checked_at: String,
    connection: Connection,
}

#[derive(Deserialize)]
struct Envelope {
    ok: bool,
    #[serde(default)]
    data: serde_json::Value,
    #[serde(default)]
    error: Option<OwnerError>,
}
#[derive(Deserialize)]
struct OwnerError {
    code: String,
    #[serde(default)]
    message: Option<String>,
}

fn envelope(code: Option<i32>, stdout: &[u8]) -> Result<Envelope, SetupFailure> {
    // clap exits 2 for an unknown subcommand: an spm older than `setup`.
    if code == Some(2) {
        return Err(SetupFailure::new("spm_update_required"));
    }
    if !matches!(code, Some(0 | 1)) {
        return Err(SetupFailure::new("spm_unavailable"));
    }
    serde_json::from_slice(stdout).map_err(|_| SetupFailure::new("spm_invalid_response"))
}

pub fn decode_plan(code: Option<i32>, stdout: &[u8]) -> Result<ImportPlan, SetupFailure> {
    let envelope = envelope(code, stdout)?;
    if !envelope.ok {
        let error = envelope
            .error
            .ok_or(SetupFailure::new("spm_invalid_response"))?;
        return Err(SetupFailure::owner(&error.code, error.message));
    }
    serde_json::from_value(envelope.data).map_err(|_| SetupFailure::new("spm_invalid_response"))
}

/// NO_PROJECT and PROJECT_INVALID still carry the listing; the UI reads the checks.
pub fn decode_status(code: Option<i32>, stdout: &[u8]) -> Result<ProjectStatus, SetupFailure> {
    let envelope = envelope(code, stdout)?;
    if !envelope.ok
        && !envelope
            .error
            .as_ref()
            .is_some_and(|e| e.code == "NO_PROJECT" || e.code == "PROJECT_INVALID")
    {
        return Err(SetupFailure::new("spm_invalid_response"));
    }
    serde_json::from_value(envelope.data).map_err(|_| SetupFailure::new("spm_invalid_response"))
}

pub fn decode_connections(
    code: Option<i32>,
    stdout: &[u8],
) -> Result<ConnectionReport, SetupFailure> {
    let envelope = envelope(code, stdout)?;
    if !envelope.ok {
        return Err(SetupFailure::new("spm_invalid_response"));
    }
    serde_json::from_value(envelope.data).map_err(|_| SetupFailure::new("spm_invalid_response"))
}

/// A completed sign-in returns the rechecked connection, reported like a status row.
pub fn decode_sign_in(code: Option<i32>, stdout: &[u8]) -> Result<ConnectionReport, SetupFailure> {
    let envelope = envelope(code, stdout)?;
    if !envelope.ok {
        let error = envelope
            .error
            .ok_or(SetupFailure::new("spm_invalid_response"))?;
        return Err(SetupFailure::owner(&error.code, error.message));
    }
    let signed: SignedIn = serde_json::from_value(envelope.data)
        .map_err(|_| SetupFailure::new("spm_invalid_response"))?;
    Ok(ConnectionReport {
        checked_at: signed.checked_at,
        connections: vec![signed.connection],
    })
}

/// The executable named by an installer shim such as `"C:\...\spm.exe" %*`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Space {
    pub project_key: String,
    pub simple_name: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkItemType {
    pub type_key: String,
    pub api_name: String,
    pub name: String,
    pub issue_like: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Group {
    pub field: String,
    pub name: String,
    pub target_type: String,
    pub target_name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Item {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub count: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DayActivity {
    pub date: String,
    pub created: u32,
    pub merged: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BranchCount {
    pub branch: String,
    pub count: u32,
}
/// A year of Gerrit changes per day for a repository and/or branch.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Activity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub branch: Option<String>,
    pub from: String,
    pub to: String,
    pub days: Vec<DayActivity>,
    pub total: u32,
    pub merged: u32,
    pub open: u32,
    pub abandoned: u32,
    pub branches: Vec<BranchCount>,
}
/// A Jira version or starred filter with its issue count inside the project.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct JiraGroup {
    pub name: String,
    pub jql: String,
    pub count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub released: Option<bool>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct JiraProject {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub count: u32,
}
/// What `spm discover` returned; exactly one list is present.
#[derive(Debug, Default, Serialize, Deserialize, TS)]
pub struct Discovery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub spaces: Option<Vec<Space>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub types: Option<Vec<WorkItemType>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub groups: Option<Vec<Group>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub items: Option<Vec<Item>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub projects: Option<Vec<JiraProject>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub jira_groups: Option<Vec<JiraGroup>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gerrit_projects: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub activity: Option<Activity>,
    /// Items of the group type holding no issue, when items were counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub without_issues: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MeegleFilter {
    pub field: String,
    pub field_name: String,
    /// One or more group items forming one scope.
    pub values: Vec<FilterValue>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FilterValue {
    pub id: String,
    pub label: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
// SPM rejects unknown scope fields itself; ts-rs cannot express deny_unknown_fields on an enum.
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ScopeQuery {
    Meegle {
        space: String,
        type_key: String,
        work_item_type: String,
        type_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        filter: Option<MeegleFilter>,
    },
    Jira {
        project: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        jql: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        label: Option<String>,
    },
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Answers {
    #[serde(default)]
    pub severities: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub stages: std::collections::BTreeMap<String, String>,
}
/// The user's choices, passed to `spm setup draft` on stdin; SPM validates every value.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DraftChoices {
    pub project: String,
    pub scope_id: String,
    pub scope: ScopeQuery,
    pub phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub milestone_date: Option<String>,
    pub severe_target_pct: u32,
    pub overall_target_pct: u32,
    pub fatal_max: u32,
    #[serde(default)]
    pub scope_complete: bool,
    #[serde(default)]
    pub answers: Answers,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Question {
    pub field: String,
    pub value: String,
    /// Issues in the scope holding this value; only those block creation.
    pub count: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ValueCount {
    pub value: String,
    pub count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mapped: Option<String>,
}
/// The gate inputs of the scope; targets only move the thresholds.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Measures {
    pub severe_total: u32,
    pub severe_resolved: u32,
    pub overall_total: u32,
    pub overall_resolved: u32,
    pub fatal_open: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Breakdown {
    pub severities: Vec<ValueCount>,
    pub statuses: Vec<ValueCount>,
    pub resolutions: Vec<ValueCount>,
    pub measures: Measures,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DraftMapping {
    pub severity_from: String,
    pub severities: std::collections::BTreeMap<String, String>,
    pub stages: std::collections::BTreeMap<String, String>,
    pub dispositions: std::collections::BTreeMap<String, String>,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct Draft {
    pub template: TemplateInfo,
    pub project: String,
    pub scope: String,
    pub directory: String,
    pub files: Vec<PlanFile>,
    pub applicable: bool,
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub backups: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub check: Option<ProjectCheck>,
    pub summary: String,
    pub count: u32,
    pub questions: Vec<Question>,
    pub mapping: DraftMapping,
    pub breakdown: Breakdown,
}

/// A gate of the daemon's last snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct GateSummary {
    pub gate_id: String,
    pub state: String,
    pub n: u32,
    pub v: u32,
    pub gap: u32,
}
/// A source value the running project's mapping does not cover.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Unmapped {
    pub field: String,
    pub value: String,
    pub count: u32,
}
/// The background service's health (`spmd --health`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ServiceHealth {
    pub state: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_poll: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_commit: Option<String>,
    #[serde(default)]
    #[ts(type = "number")]
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    #[serde(default)]
    pub gates: Vec<GateSummary>,
    #[serde(default)]
    pub unmapped: Vec<Unmapped>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Activation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub health: Option<ServiceHealth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub waited_secs: Option<u64>,
}
/// `spm setup activate` for a drafted project; identifiers are directory-safe.
pub fn activate_args<'a>(project: &'a str, scope: &'a str) -> Option<Vec<&'a str>> {
    let safe = |s: &str| {
        (1..=64).contains(&s.len())
            && s.starts_with(|c: char| c.is_ascii_alphanumeric())
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    };
    (safe(project) && safe(scope))
        .then(|| vec!["setup", "activate", "--project", project, "--scope", scope])
}
pub fn decode_activation(code: Option<i32>, stdout: &[u8]) -> Result<Activation, SetupFailure> {
    owner_data(code, stdout)
}
pub fn decode_service(code: Option<i32>, stdout: &[u8]) -> Result<ServiceHealth, SetupFailure> {
    owner_data(code, stdout)
}
pub const STAGES: [&str; 8] = [
    "Open",
    "Implementing",
    "Reviewing",
    "Regression",
    "Closed",
    "WaitInfo",
    "Track",
    "Ccb",
];
/// `spm setup remap … --apply` for stage answers; values are the source's own text.
pub fn remap_args(
    project: &str,
    scope: &str,
    stages: &std::collections::BTreeMap<String, String>,
) -> Option<Vec<String>> {
    let mut args = activate_args(project, scope)?
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    args[1] = "remap".into();
    if stages.is_empty() || stages.len() > 50 {
        return None;
    }
    for (value, stage) in stages {
        if value.trim().is_empty()
            || value.chars().count() > 120
            || value.chars().any(char::is_control)
            || !STAGES.contains(&stage.as_str())
        {
            return None;
        }
        args.push("--stage".into());
        args.push(format!("{value}={stage}"));
    }
    args.push("--apply".into());
    Some(args)
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FocusTargets {
    pub severe_pct: u32,
    pub overall_pct: u32,
    pub fatal_max: u32,
}
/// A followed model as `spm focus` stores it; SPM validates every value.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FocusModel {
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
pub struct Builds {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub folder: Option<String>,
    pub jobs: Vec<JobView>,
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
pub struct ModelSummary {
    pub code: String,
    pub targets: FocusTargets,
    pub issues: u32,
    pub sources: Vec<String>,
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
#[derive(Debug, Default, Serialize, Deserialize, TS)]
pub struct FocusData {
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

/// A Gerrit branch name as the frontend may pass it.
pub fn valid_branch(branch: &str) -> bool {
    (1..=200).contains(&branch.len())
        && branch.starts_with(|c: char| c.is_ascii_alphanumeric())
        && branch
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./+".contains(&b))
}

/// A model code: letters, digits, `-` and `_`.
pub fn valid_code(code: &str) -> bool {
    (2..=32).contains(&code.len())
        && code.starts_with(|c: char| c.is_ascii_alphanumeric())
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
/// Arguments for `spm focus`; `None` when a value is malformed.
pub fn focus_args(
    action: &str,
    code: Option<&str>,
    change: Option<u32>,
    review: Option<&str>,
    branch: Option<&str>,
) -> Option<Vec<String>> {
    let code = || code.filter(|c| valid_code(c)).map(str::to_owned);
    let mut args: Vec<String> = vec!["focus".into()];
    match action {
        "list" => args.push("list".into()),
        "save" => args.push("save".into()),
        "summary" => args.push("summary".into()),
        "cached" => args.extend(["summary".into(), "--cached".into()]),
        "suggest" => args.extend(["suggest".into(), "--code".into(), code()?]),
        "remove" => args.extend(["remove".into(), "--code".into(), code()?]),
        "refresh" => args.extend(["summary".into(), "--code".into(), code()?]),
        "preview" => args.push("preview".into()),
        "changes" => args.extend([
            "changes".into(),
            "--code".into(),
            code()?,
            "--branch".into(),
            branch.filter(|b| valid_branch(b))?.into(),
        ]),
        "builds" => args.extend(["builds".into(), "--code".into(), code()?]),
        "builds_cached" => {
            args.extend(["builds".into(), "--code".into(), code()?, "--cached".into()])
        }
        "review" => {
            let review = review.filter(|r| ["accept", "track", "untrack"].contains(r))?;
            args.extend([
                "review".into(),
                "--code".into(),
                code()?,
                "--change".into(),
                change?.to_string(),
                "--action".into(),
                review.into(),
            ]);
        }
        _ => return None,
    }
    Some(args)
}
pub fn decode_focus(code: Option<i32>, stdout: &[u8]) -> Result<FocusData, SetupFailure> {
    owner_data(code, stdout)
}

/// Keys the frontend may pass to `spm discover`: Meegle space, type and field keys.
pub fn valid_key(key: &str) -> bool {
    (1..=64).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
/// What a discovery list may name: Meegle keys and a Jira project.
#[derive(Debug, Default, Clone, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DiscoverArgs {
    #[serde(default)]
    #[ts(optional)]
    pub space: Option<String>,
    /// A Meegle type key: the issue type for `groups`, the group's target type for `items`.
    #[serde(default)]
    #[ts(optional)]
    pub kind: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub search: Option<String>,
    /// `items`: count issues of this type key per group, through `field`.
    #[serde(default)]
    #[ts(optional)]
    pub count_type: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub field: Option<String>,
    /// `jira_versions` and `jira_filters`.
    #[serde(default)]
    #[ts(optional)]
    pub project: Option<String>,
    /// `gerrit_activity`: a repository path and/or a branch.
    #[serde(default)]
    #[ts(optional)]
    pub repo: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub branch: Option<String>,
}

/// A Gerrit repository path or branch name: letters, digits and /._-+@.
pub fn valid_gerrit_name(name: &str) -> bool {
    (1..=200).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || "/._-+@".contains(c))
}

/// A Jira project key such as PLV.
pub fn valid_project(key: &str) -> bool {
    (1..=20).contains(&key.len())
        && key.starts_with(|c: char| c.is_ascii_uppercase())
        && key
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// Arguments for one discovery list; `None` when an argument is malformed.
pub fn discover_args<'a>(list: &str, a: &'a DiscoverArgs) -> Option<Vec<&'a str>> {
    let key = |k: &'a Option<String>| k.as_deref().filter(|k| valid_key(k));
    let project = || a.project.as_deref().filter(|p| valid_project(p));
    let space = key(&a.space);
    let kind = key(&a.kind);
    let search = a.search.as_deref();
    let mut args = vec!["discover"];
    match list {
        "spaces" => args.push("spaces"),
        "jira_projects" => args.push("jira-projects"),
        "jira_versions" => args.extend(["jira-versions", "--project", project()?]),
        "jira_filters" => args.extend(["jira-filters", "--project", project()?]),
        "gerrit_projects" => args.push("gerrit-projects"),
        "gerrit_activity" => {
            args.push("gerrit-activity");
            let name = |n: &'a Option<String>| n.as_deref().filter(|n| valid_gerrit_name(n));
            match (&a.repo, &a.branch) {
                (None, None) => return None,
                (repo, branch) => {
                    if repo.is_some() {
                        args.extend(["--project", name(repo)?]);
                    }
                    if branch.is_some() {
                        args.extend(["--branch", name(branch)?]);
                    }
                }
            }
        }
        "types" => args.extend(["types", "--space", space?]),
        "groups" => args.extend(["groups", "--space", space?, "--type", kind?]),
        "items" => {
            args.extend(["items", "--space", space?, "--type", kind?]);
            match (&a.count_type, &a.field) {
                (Some(_), Some(_)) => args.extend([
                    "--count-type",
                    key(&a.count_type)?,
                    "--field",
                    key(&a.field)?,
                ]),
                (None, None) => {}
                _ => return None,
            }
            if let Some(search) = search.map(str::trim).filter(|s| !s.is_empty()) {
                if search.chars().count() > 60 || search.chars().any(char::is_control) {
                    return None;
                }
                args.extend(["--search", search]);
            }
        }
        _ => return None,
    }
    Some(args)
}
pub fn draft_args(apply: bool, expect: Option<&str>) -> Option<Vec<&str>> {
    let mut args = vec!["setup", "draft"];
    if apply {
        let sha = expect.filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))?;
        args.extend(["--apply", "--replace", "--expect", sha]);
    }
    Some(args)
}

fn owner_data<T: serde::de::DeserializeOwned>(
    code: Option<i32>,
    stdout: &[u8],
) -> Result<T, SetupFailure> {
    let envelope = envelope(code, stdout)?;
    if !envelope.ok {
        let error = envelope
            .error
            .ok_or(SetupFailure::new("spm_invalid_response"))?;
        return Err(SetupFailure::owner(&error.code, error.message));
    }
    serde_json::from_value(envelope.data).map_err(|_| SetupFailure::new("spm_invalid_response"))
}
pub fn decode_discovery(code: Option<i32>, stdout: &[u8]) -> Result<Discovery, SetupFailure> {
    owner_data(code, stdout)
}
pub fn decode_draft(code: Option<i32>, stdout: &[u8]) -> Result<Draft, SetupFailure> {
    owner_data(code, stdout)
}

pub fn shim_target(shim: &str) -> Option<PathBuf> {
    shim.lines().find_map(|line| {
        let rest = line.trim().strip_prefix('"')?;
        let (path, tail) = rest.split_once('"')?;
        (tail.trim() == "%*"
            && path.to_ascii_lowercase().ends_with("\\spm.exe")
            && !path.chars().any(|c| c.is_control() || c == '%'))
        .then(|| PathBuf::from(path))
    })
}

/// `TINNCI_SPM_EXE`, else the target of `~\.local\bin\spm.cmd`, else `~\.local\bin\spm.exe`.
pub fn locate(
    override_path: Option<PathBuf>,
    home: Option<&Path>,
) -> Result<PathBuf, SetupFailure> {
    let exists = |path: &Path| path.is_absolute() && path.is_file();
    if let Some(path) = override_path {
        return if exists(&path) {
            Ok(path)
        } else {
            Err(SetupFailure::new("spm_missing"))
        };
    }
    let bin = home
        .ok_or(SetupFailure::new("spm_missing"))?
        .join(".local")
        .join("bin");
    let shim = std::fs::read(bin.join("spm.cmd"))
        .ok()
        .filter(|bytes| bytes.len() < 4096)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| shim_target(&text))
        .filter(|path| exists(path));
    shim.or_else(|| Some(bin.join("spm.exe")).filter(|path| exists(path)))
        .ok_or(SetupFailure::new("spm_missing"))
}

/// Copies the chosen template into Workspace's own directory, so the file that is
/// applied is byte-for-byte the file that was planned.
pub fn snapshot(source: &Path, directory: &Path) -> Result<PathBuf, SetupFailure> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(source)
        .and_then(|file| file.take(TEMPLATE_LIMIT + 1).read_to_end(&mut bytes))
        .map_err(|_| SetupFailure::new("template_unreadable"))?;
    if bytes.len() as u64 > TEMPLATE_LIMIT {
        return Err(SetupFailure::new("template_too_large"));
    }
    std::fs::create_dir_all(directory).map_err(|_| SetupFailure::new("template_unreadable"))?;
    let copy = directory.join("pending.template.json");
    std::fs::write(&copy, bytes).map_err(|_| SetupFailure::new("template_unreadable"))?;
    Ok(copy)
}

pub fn import_args(template: &str, apply: bool, replace: bool) -> Vec<&str> {
    let mut args = vec!["setup", "import-template", template];
    if apply {
        args.push("--apply");
        if replace {
            args.push("--replace");
        }
    }
    args
}

pub fn types() -> String {
    let config = ts_rs::Config::default();
    format!(
        "// Generated by cargo run -- --export-types; do not edit.\n{}",
        [
            SetupFailure::decl(&config),
            TemplateInfo::decl(&config),
            PlanFile::decl(&config),
            ProjectCheck::decl(&config),
            ImportPlan::decl(&config),
            ImportedProject::decl(&config),
            ProjectStatus::decl(&config),
            Connection::decl(&config),
            ConnectionReport::decl(&config),
            Space::decl(&config),
            WorkItemType::decl(&config),
            Group::decl(&config),
            Item::decl(&config),
            JiraProject::decl(&config),
            JiraGroup::decl(&config),
            DayActivity::decl(&config),
            BranchCount::decl(&config),
            Activity::decl(&config),
            Discovery::decl(&config),
            DiscoverArgs::decl(&config),
            MeegleFilter::decl(&config),
            FilterValue::decl(&config),
            ScopeQuery::decl(&config),
            Answers::decl(&config),
            DraftChoices::decl(&config),
            Question::decl(&config),
            ValueCount::decl(&config),
            Measures::decl(&config),
            Breakdown::decl(&config),
            DraftMapping::decl(&config),
            Draft::decl(&config),
            GateSummary::decl(&config),
            Unmapped::decl(&config),
            ServiceHealth::decl(&config),
            FocusTargets::decl(&config),
            FocusModel::decl(&config),
            Candidate::decl(&config),
            Suggestions::decl(&config),
            BranchStrip::decl(&config),
            PreviewSide::decl(&config),
            Preview::decl(&config),
            Listed::decl(&config),
            BuildRun::decl(&config),
            ParamChange::decl(&config),
            JobView::decl(&config),
            Builds::decl(&config),
            Flagged::decl(&config),
            ModelSummary::decl(&config),
            FocusSummary::decl(&config),
            FocusData::decl(&config),
            Activation::decl(&config),
        ]
        .map(|decl| format!("export {decl}\n"))
        .concat()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan_json(ok: bool) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"schema_version":"1.0","ok":ok,
            "data":{"template":{"id":"t","version":"1.0","title":"T","sha256":"ab"},
                "project":"P","scope":"S","mode":"live","directory":"C:\\cfg\\P\\S",
                "database":{"path":"C:\\cfg\\P\\S\\spm_data.db","exists":false},
                "files":[{"role":"live_config","path":"C:\\cfg\\P\\S\\live.json","action":"conflict"}],
                "applicable":false,"applied":false},
            "error": if ok { serde_json::Value::Null } else {
                serde_json::json!({"code":"IMPORT_CONFLICT","message":"live.json differs","retryable":false})
            }}))
        .unwrap()
    }
    #[test]
    fn discovery_and_draft_arguments_are_validated() {
        let with = |f: fn(&mut DiscoverArgs)| {
            let mut a = DiscoverArgs::default();
            f(&mut a);
            a
        };
        let items = with(|a| {
            a.space = Some("efzuhc".into());
            a.kind = Some("693ff1c8".into());
            a.search = Some(" E67 ".into());
            a.count_type = Some("6942c93f".into());
            a.field = Some("field_b86050".into());
        });
        assert_eq!(
            discover_args("items", &items).unwrap(),
            [
                "discover",
                "items",
                "--space",
                "efzuhc",
                "--type",
                "693ff1c8",
                "--count-type",
                "6942c93f",
                "--field",
                "field_b86050",
                "--search",
                "E67"
            ]
        );
        let half = with(|a| {
            a.space = Some("efzuhc".into());
            a.kind = Some("693ff1c8".into());
            a.field = Some("field_b86050".into());
        });
        assert!(discover_args("items", &half).is_none());
        assert_eq!(
            discover_args("spaces", &DiscoverArgs::default()).unwrap(),
            ["discover", "spaces"]
        );
        let camera = with(|a| {
            a.repo = Some("packages/apps/DreamCamera2".into());
            a.branch = Some("mtk_sys_OS3.4_A16".into());
        });
        assert_eq!(
            discover_args("gerrit_activity", &camera).unwrap(),
            [
                "discover",
                "gerrit-activity",
                "--project",
                "packages/apps/DreamCamera2",
                "--branch",
                "mtk_sys_OS3.4_A16"
            ]
        );
        assert!(discover_args("gerrit_activity", &DiscoverArgs::default()).is_none());
        let injected = with(|a| a.repo = Some("x OR project:y".into()));
        assert!(discover_args("gerrit_activity", &injected).is_none());
        let plv = with(|a| a.project = Some("PLV".into()));
        assert_eq!(
            discover_args("jira_versions", &plv).unwrap(),
            ["discover", "jira-versions", "--project", "PLV"]
        );
        let lower = with(|a| a.project = Some("plv; rm".into()));
        assert!(discover_args("jira_filters", &lower).is_none());
        assert!(discover_args("types", &with(|a| a.space = Some("efz uhc".into()))).is_none());
        assert!(discover_args("groups", &with(|a| a.space = Some("efzuhc".into()))).is_none());
        assert!(discover_args("rm", &DiscoverArgs::default()).is_none());
        let newline = with(|a| {
            a.space = Some("a".into());
            a.kind = Some("b".into());
            a.search = Some("x\ny".into());
        });
        assert!(discover_args("items", &newline).is_none());
        assert_eq!(draft_args(false, None).unwrap(), ["setup", "draft"]);
        let sha = "a".repeat(64);
        assert_eq!(
            draft_args(true, Some(&sha)).unwrap(),
            [
                "setup",
                "draft",
                "--apply",
                "--replace",
                "--expect",
                sha.as_str()
            ]
        );
        assert!(draft_args(true, None).is_none());
        assert!(draft_args(true, Some("--config-root")).is_none());
    }
    #[test]
    fn focus_arguments_and_data() {
        assert_eq!(
            focus_args("review", Some("E6767SM"), Some(80153), Some("accept"), None).unwrap(),
            [
                "focus", "review", "--code", "E6767SM", "--change", "80153", "--action", "accept"
            ]
        );
        assert_eq!(
            focus_args("cached", None, None, None, None).unwrap(),
            ["focus", "summary", "--cached"]
        );
        assert_eq!(
            focus_args(
                "changes",
                Some("E6767SM"),
                None,
                None,
                Some("E6767SM_SYS_A16_MP")
            )
            .unwrap(),
            [
                "focus",
                "changes",
                "--code",
                "E6767SM",
                "--branch",
                "E6767SM_SYS_A16_MP"
            ]
        );
        assert!(
            focus_args(
                "changes",
                Some("E6767SM"),
                None,
                None,
                Some("--config-root")
            )
            .is_none()
        );
        assert!(focus_args("changes", Some("E6767SM"), None, None, None).is_none());
        assert_eq!(
            focus_args("builds_cached", Some("E6767SM"), None, None, None).unwrap(),
            ["focus", "builds", "--code", "E6767SM", "--cached"]
        );
        assert!(focus_args("suggest", Some("E6751 SM"), None, None, None).is_none());
        assert!(focus_args("review", Some("E6767SM"), Some(1), Some("delete"), None).is_none());
        assert!(focus_args("rm", None, None, None, None).is_none());
        let summary = br#"{"ok":true,"data":{"summary":{"computed_at":"2026-11-01T11:00:00Z","models":[{"code":"E6767SM",
            "targets":{"severe_pct":95,"overall_pct":85,"fatal_max":0},"issues":342,"sources":["x"],
            "measures":{"severe_total":100,"severe_resolved":96,"overall_total":342,"overall_resolved":281,"fatal_open":0},
            "new_issues":24,"days":[0,1],"days_from":"2026-08-10","week_changes":13,"week_merged":12,
            "branches":[{"branch":"E6767SM_SYS_A16_MP","phase":"mp","total":314,"days":[0,1],"flagged":2}],
            "flagged":[{"number":80020,"subject":"s","project":"p","branch":"b","owner":"o","status":"NEW","created":"2026-10-30","rules":["feature_after_freeze"],"issues":[],"tracked":false}],
            "rule_counts":{"revert":17},"rules":{"revert":"alert"},"freeze":"2026-10-12","errors":[]}]}}}"#;
        let data = decode_focus(Some(0), summary).unwrap();
        let model = &data.summary.unwrap().models[0];
        assert_eq!((model.new_issues, model.flagged[0].number), (24, 80020));
        let builds = br#"{"ok":true,"data":{"builds":{"code":"E6767SM","folder":"Project_Daily_Build/Project_E6767SM",
            "computed_at":"2026-11-02T01:00:00Z","errors":[],"jobs":[{"job":"Project_Daily_Build/Project_E6767SM/E6767SM_IR",
            "name":"E6767SM_IR","phase":"ir","disabled":false,"builds":[{"number":17,"result":"SUCCESS","building":false,
            "timestamp":1790602588731,"duration":12330139,"estimated":9786231,"cause":"Started by user luyushan","node":"fast_build_server-97"}],
            "parameters":{"BRANCH":"E6767SM_SYS_A16_IR","CLEAN":true},"changes":[{"build":17,"name":"BRANCH","before":"a","after":"b"}]}]}}}"#;
        let builds = decode_focus(Some(0), builds).unwrap().builds.unwrap();
        assert_eq!(builds.jobs[0].builds[0].timestamp, 1790602588731);
        assert_eq!(builds.jobs[0].parameters["CLEAN"], serde_json::json!(true));
        let model: Result<FocusModel, _> = serde_json::from_value(serde_json::json!({
            "code":"E6751SM","targets":{"severe_pct":95,"overall_pct":85,"fatal_max":0},"token":"x"}));
        assert!(model.is_err());
    }
    #[test]
    fn remap_arguments_and_service_health() {
        let answers = [("To Be Confirmed".to_string(), "Open".to_string())].into();
        assert_eq!(
            remap_args("E6751SM", "main", &answers).unwrap(),
            [
                "setup",
                "remap",
                "--project",
                "E6751SM",
                "--scope",
                "main",
                "--stage",
                "To Be Confirmed=Open",
                "--apply"
            ]
        );
        let bad = [("x".to_string(), "Deleted".to_string())].into();
        assert!(remap_args("E6751SM", "main", &bad).is_none());
        assert!(remap_args("E6751SM", "main", &Default::default()).is_none());
        let health = br#"{"ok":true,"data":{"mode":"live","state":"running","project":"E6751SM","scope":"main","revision":2,
            "gates":[{"gate_id":"severe","state":"satisfied","n":115,"v":111,"gap":0}],
            "unmapped":[{"field":"stage","value":"Communication","count":3}]}}"#;
        let health = decode_service(Some(0), health).unwrap();
        assert_eq!(health.project.as_deref(), Some("E6751SM"));
        assert_eq!(health.unmapped[0].count, 3);
    }
    #[test]
    fn activation_arguments_and_results() {
        assert_eq!(
            activate_args("E6767SM", "main").unwrap(),
            [
                "setup",
                "activate",
                "--project",
                "E6767SM",
                "--scope",
                "main"
            ]
        );
        assert!(activate_args("../x", "main").is_none());
        assert!(activate_args("E6767SM", "--dry-run").is_none());
        let done = br#"{"ok":true,"data":{"dry_run":false,"health":{"mode":"live","state":"ready","last_commit":"2026-11-01T10:00:00Z","revision":2},"waited_secs":21}}"#;
        let activation = decode_activation(Some(0), done).unwrap();
        assert_eq!(activation.health.unwrap().revision, 2);
        let late = br#"{"ok":false,"data":null,"error":{"code":"ACTIVATE_TIMEOUT","message":"no snapshot within 300 s"}}"#;
        assert_eq!(
            decode_activation(Some(1), late).unwrap_err().code,
            "activate_timeout"
        );
    }
    #[test]
    fn decodes_drafts_and_discovery_errors() {
        let draft = serde_json::json!({"schema_version":"1.0","ok":true,"data":{
            "template":{"id":"E6767SM-main","version":"2026-11-01","title":"E6767SM main","sha256":"ab"},
            "project":"E6767SM","scope":"main","mode":"live","directory":"C:\\cfg\\E6767SM\\main",
            "database":{"path":"x","exists":false},
            "files":[{"role":"scope_query","path":"C:\\cfg\\scope.json","action":"create"}],
            "applicable":true,"applied":false,"summary":"E6767SM · main：342 个问题。","count":342,
            "questions":[{"field":"stage","value":"Communication","count":0}],
            "mapping":{"severity_from":"优先级","severities":{"致命":"Fatal"},"stages":{},"dispositions":{}},
            "breakdown":{"severities":[{"value":"严重","count":100,"mapped":"Severe"}],
                "statuses":[{"value":"Communication","count":0}],"resolutions":[],
                "measures":{"severe_total":100,"severe_resolved":96,"overall_total":342,"overall_resolved":281,"fatal_open":0}}}});
        let draft = decode_draft(Some(0), &serde_json::to_vec(&draft).unwrap()).unwrap();
        assert_eq!(
            (draft.count, draft.questions[0].value.as_str()),
            (342, "Communication")
        );
        assert_eq!(draft.breakdown.measures.severe_resolved, 96);
        assert!(draft.breakdown.statuses[0].mapped.is_none());
        let signed_out = br#"{"ok":false,"data":null,"error":{"code":"SIGNED_OUT","message":"run meegle auth login"}}"#;
        assert_eq!(
            decode_discovery(Some(1), signed_out).unwrap_err().code,
            "signed_out"
        );
        let spaces = br#"{"ok":true,"data":{"spaces":[{"project_key":"6","simple_name":"efzuhc","name":"IPD"}]}}"#;
        assert_eq!(
            decode_discovery(Some(0), spaces).unwrap().spaces.unwrap()[0].name,
            "IPD"
        );
        let choices: Result<DraftChoices, _> = serde_json::from_value(serde_json::json!({
            "project":"P","scope_id":"main","scope":{"source":"jira","project":"PLV"},"argv":["x"],
            "phase":"TR4","severe_target_pct":95,"overall_target_pct":85,"fatal_max":0}));
        assert!(choices.is_err());
    }
    #[test]
    fn decodes_plans_and_maps_owner_errors() {
        let plan = decode_plan(Some(0), &plan_json(true)).unwrap();
        assert_eq!(plan.files[0].action, "conflict");
        assert!(!plan.applicable && plan.check.is_none());
        let conflict = decode_plan(Some(1), &plan_json(false)).unwrap_err();
        assert_eq!(conflict.code, "import_conflict");
        assert_eq!(conflict.message.as_deref(), Some("live.json differs"));
        let invalid = br#"{"ok":false,"data":null,"error":{"code":"TEMPLATE_INVALID","message":"unknown field `x`"}}"#;
        assert_eq!(
            decode_plan(Some(1), invalid).unwrap_err().code,
            "template_invalid"
        );
        let unknown = br#"{"ok":false,"error":{"code":"SECRET_LEAK","message":"token"}}"#;
        let failure = decode_plan(Some(1), unknown).unwrap_err();
        assert_eq!(failure.code, "spm_invalid_response");
        assert!(failure.message.is_none());
        assert_eq!(
            decode_plan(Some(2), b"error: unrecognized subcommand")
                .unwrap_err()
                .code,
            "spm_update_required"
        );
        assert_eq!(
            decode_plan(Some(0), b"not json").unwrap_err().code,
            "spm_invalid_response"
        );
        assert_eq!(decode_plan(None, b"").unwrap_err().code, "spm_unavailable");
    }
    /// Envelopes captured from `spm setup` (spm 47f5d44), paths rewritten.
    #[test]
    fn decodes_recorded_owner_output() {
        macro_rules! fixture {
            ($name:literal) => {
                include_bytes!(concat!("../tests/fixtures/", $name))
            };
        }
        let plan = decode_plan(Some(0), fixture!("plan.json")).unwrap();
        assert!(plan.applicable && !plan.applied && plan.files.len() == 3);
        assert!(plan.files.iter().all(|f| f.action == "create"));
        let applied = decode_plan(Some(0), fixture!("applied.json")).unwrap();
        let check = applied.check.unwrap();
        assert!(applied.applied && check.valid && check.ready);
        let conflict = decode_plan(Some(0), fixture!("plan-conflict.json")).unwrap();
        assert!(!conflict.applicable && conflict.files.iter().any(|f| f.action == "conflict"));
        let refused = decode_plan(Some(1), fixture!("apply-conflict.json")).unwrap_err();
        assert_eq!(refused.code, "import_conflict");
        let replaced = decode_plan(Some(0), fixture!("replaced.json")).unwrap();
        assert_eq!(replaced.backups.len(), 1);
        let invalid = decode_plan(Some(1), fixture!("template-invalid.json")).unwrap_err();
        assert_eq!(invalid.code, "template_invalid");
        assert!(invalid.message.unwrap().contains("schema_version"));
        assert!(
            decode_status(Some(1), fixture!("status-empty.json"))
                .unwrap()
                .projects
                .is_empty()
        );
        let broken = decode_status(Some(1), fixture!("status-invalid.json")).unwrap();
        assert!(!broken.projects[0].check.valid);
        let status = decode_status(Some(0), fixture!("status.json")).unwrap();
        assert_eq!(status.projects[0].scope.as_deref(), Some("TEST_SCOPE"));
        assert!(status.projects[0].check.valid);
    }
    #[test]
    fn decodes_connections_and_sign_in_results() {
        let report = decode_connections(
            Some(0),
            include_bytes!("../tests/fixtures/connections.json"),
        )
        .unwrap();
        let states: Vec<_> = report
            .connections
            .iter()
            .map(|c| (c.source.as_str(), c.state.as_str()))
            .collect();
        assert_eq!(
            states,
            [
                ("meegle", "connected"),
                ("jira", "connected"),
                ("gerrit", "connected"),
                ("jenkins", "unreachable")
            ]
        );
        assert!(report.connections[0].sign_in && report.connections[0].account.is_none());
        assert_eq!(
            decode_connections(Some(2), b"error: unrecognized subcommand")
                .unwrap_err()
                .code,
            "spm_update_required"
        );
        let signed = br#"{"ok":true,"data":{"checked_at":"t","connection":{"source":"meegle","tool":"meegle","state":"connected","sign_in":true}}}"#;
        assert_eq!(
            decode_sign_in(Some(0), signed).unwrap().connections[0].state,
            "connected"
        );
        let host = br#"{"ok":false,"data":null,"error":{"code":"SIGN_IN_HOST_UNKNOWN","message":"run `meegle auth login` once"}}"#;
        assert_eq!(
            decode_sign_in(Some(1), host).unwrap_err().code,
            "sign_in_host_unknown"
        );
    }
    #[test]
    fn status_keeps_listing_for_empty_and_invalid_projects() {
        let empty = br#"{"ok":false,"data":{"config_root":"C:\\cfg","projects":[]},"error":{"code":"NO_PROJECT","message":"none"}}"#;
        assert!(decode_status(Some(1), empty).unwrap().projects.is_empty());
        let invalid = br#"{"ok":false,"data":{"config_root":"C:\\cfg","projects":[{"project":"P","scope":"S","directory":"d","template":null,"check":{"valid":false,"ready":false,"error":"bad"}}]},"error":{"code":"PROJECT_INVALID","message":"x"}}"#;
        assert!(
            !decode_status(Some(1), invalid).unwrap().projects[0]
                .check
                .valid
        );
        let ready = br#"{"ok":true,"data":{"config_root":"C:\\cfg","projects":[{"project":"P","scope":"S","directory":"d","check":{"valid":true,"ready":true}}]}}"#;
        assert!(
            decode_status(Some(0), ready).unwrap().projects[0]
                .check
                .ready
        );
        let other = br#"{"ok":false,"data":{},"error":{"code":"IMPORT_IO","message":"x"}}"#;
        assert_eq!(
            decode_status(Some(1), other).unwrap_err().code,
            "spm_invalid_response"
        );
    }
    #[test]
    fn reads_only_literal_shim_targets() {
        let shim = "@echo off\r\n\"C:\\Users\\A B\\AppData\\Local\\Tinnci\\SPM\\versions\\0.1.0-abc\\spm.exe\" %*\r\n";
        assert_eq!(
            shim_target(shim).unwrap(),
            PathBuf::from(
                "C:\\Users\\A B\\AppData\\Local\\Tinnci\\SPM\\versions\\0.1.0-abc\\spm.exe"
            )
        );
        for bad in [
            "\"C:\\tools\\other.exe\" %*",
            "\"%LOCALAPPDATA%\\spm.exe\" %*",
            "\"C:\\spm.exe\" setup & calc",
            "C:\\spm.exe %*",
        ] {
            assert!(shim_target(bad).is_none(), "{bad}");
        }
    }
    #[test]
    fn locates_override_then_shim_then_plain_exe() {
        let root = std::env::temp_dir().join(format!("workspace-spm-{}", std::process::id()));
        let bin = root.join(".local").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        assert_eq!(locate(None, Some(&root)).unwrap_err().code, "spm_missing");
        std::fs::write(bin.join("spm.exe"), b"").unwrap();
        assert_eq!(locate(None, Some(&root)).unwrap(), bin.join("spm.exe"));
        assert_eq!(
            locate(Some(root.join("absent.exe")), Some(&root))
                .unwrap_err()
                .code,
            "spm_missing"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn snapshot_copies_bounded_templates_and_apply_flags_are_literal() {
        let root = std::env::temp_dir().join(format!("workspace-template-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("project.template.json");
        std::fs::write(&source, b"{\"kind\":\"spm.project-template\"}").unwrap();
        let copy = snapshot(&source, &root.join("cache")).unwrap();
        assert_eq!(
            std::fs::read(&copy).unwrap(),
            std::fs::read(&source).unwrap()
        );
        std::fs::write(&source, vec![b' '; TEMPLATE_LIMIT as usize + 1]).unwrap();
        assert_eq!(
            snapshot(&source, &root.join("cache")).unwrap_err().code,
            "template_too_large"
        );
        assert_eq!(
            snapshot(&root.join("missing"), &root).unwrap_err().code,
            "template_unreadable"
        );
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(
            import_args("t.json", false, true),
            ["setup", "import-template", "t.json"]
        );
        assert_eq!(
            import_args("t.json", true, true),
            ["setup", "import-template", "t.json", "--apply", "--replace"]
        );
    }
}

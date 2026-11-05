//! SPM project setup through the owner CLI: `spm setup import-template` and
//! `spm setup status`. Workspace never writes project configuration itself.
#![cfg_attr(not(windows), allow(dead_code))]
mod focus_types;
pub use focus_types::*;
mod args;
pub use args::*;
mod locate;
pub use locate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    /// Saved issue scope and targets for opening the existing model workbench.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub workspace: Option<FocusModel>,
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
    pub sample_url: Option<String>,
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
#[cfg(test)]
mod tests;

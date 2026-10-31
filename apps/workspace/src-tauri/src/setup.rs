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
            | "sign_in_unsupported" => code.as_ref(),
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
    #[serde(default)]
    pub missing_environment: Vec<String>,
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
        assert!(applied.applied && check.valid && !check.ready);
        assert!(check.missing_environment.contains(&"JIRA_PAT".to_string()));
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
        let ready = br#"{"ok":true,"data":{"config_root":"C:\\cfg","projects":[{"project":"P","scope":"S","directory":"d","check":{"valid":true,"ready":false,"missing_environment":["JIRA_PAT"]}}]}}"#;
        assert_eq!(
            decode_status(Some(0), ready).unwrap().projects[0]
                .check
                .missing_environment,
            ["JIRA_PAT"]
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

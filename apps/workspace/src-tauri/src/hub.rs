use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, TS)]
pub struct LoadFailure {
    pub code: String,
    pub observed_at_ms: f64,
}
impl LoadFailure {
    pub fn new(code: impl AsRef<str>) -> Self {
        let code = match code.as_ref() {
            "wsl_missing"
            | "wsl_timeout"
            | "wsl_unavailable"
            | "wsl_distribution_missing"
            | "wsl_distribution_unavailable"
            | "cli_unavailable"
            | "cli_missing"
            | "hub_update_required"
            | "hub_invalid_install_path"
            | "hub_timeout"
            | "hub_invalid_response"
            | "hub_incompatible_version"
            | "hub_invalid_scope"
            | "hub_output_limit"
            | "hub_busy" => code.as_ref(),
            _ => "hub_unavailable",
        };
        Self {
            code: code.into(),
            observed_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as f64,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct Observation {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exit_code: Option<i32>,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct Step {
    pub id: String,
    pub title: String,
    pub why: String,
    pub state: String,
    #[serde(default)]
    pub needs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub observation: Option<Observation>,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct Component {
    pub id: String,
    pub name: String,
    pub group: String,
    pub summary: String,
    pub host: String,
    pub lifecycle: String,
    pub state: String,
    pub installed: bool,
    #[serde(default)]
    pub checkable: bool,
    pub issues: Vec<String>,
    pub setup: Vec<Step>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<Observation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub checked_at: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct HubReport {
    pub schema_version: u32,
    pub observed_at: String,
    pub checked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scope: Option<String>,
    pub components: Vec<Component>,
    pub unmanifested: Vec<String>,
}
#[derive(Deserialize)]
struct SourceEnvelope {
    schema_version: String,
    #[serde(rename = "ok")]
    _ok: bool,
    data: serde_json::Value,
}

pub fn decode(bytes: &[u8]) -> Result<HubReport, String> {
    let envelope: SourceEnvelope =
        serde_json::from_slice(bytes).map_err(|_| "hub_invalid_response")?;
    if envelope.schema_version != "1.0" || envelope.data["schema_version"] != 1 {
        return Err("hub_incompatible_version".into());
    }
    let mut data = envelope.data;
    let rows = data
        .get_mut("components")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or("hub_invalid_response")?;
    if rows.len() > 128 {
        return Err("hub_response_limit".into());
    }
    for row in rows {
        if !row.is_object() {
            return Err("hub_invalid_response".into());
        }
        // Paths and executable argv stay in the owner CLI, not in the webview.
        let installed = row["installed"]
            .as_object()
            .is_some_and(|m| m.values().any(|v| v.is_string()));
        row["installed"] = installed.into();
        row["checkable"] = (row["lifecycle"] != "paused"
            && (row
                .get("status")
                .is_some_and(|s| s.is_object() && s["state"] != "invalid")
                || row["setup"].as_array().is_some_and(|steps| {
                    steps
                        .iter()
                        .any(|step| step["check"].is_object() && step["state"] != "planned")
                })))
        .into();
        if let Some(steps) = row
            .get_mut("setup")
            .and_then(serde_json::Value::as_array_mut)
        {
            if steps.len() > 20 {
                return Err("hub_response_limit".into());
            }
            for step in steps {
                if !step.is_object() {
                    return Err("hub_invalid_response".into());
                }
                if ["done", "unknown", "action_needed"]
                    .contains(&step["state"].as_str().unwrap_or(""))
                {
                    step["observation"] = serde_json::json!({
                        "state": step["state"], "reason": step["reason"],
                        "code": step["code"], "exit_code": step["exit_code"],
                    });
                }
            }
        }
        if row["state"] == "invalid" {
            for (key, value) in [
                ("name", "配置需要修复"),
                ("group", "desktop"),
                ("summary", "组件清单无效"),
                ("host", "both"),
                ("lifecycle", "active"),
            ] {
                if row.get(key).is_none() {
                    row[key] = value.into();
                }
            }
            if row.get("setup").is_none() {
                row["setup"] = serde_json::json!([]);
            }
        }
    }
    serde_json::from_value(data).map_err(|_| "hub_invalid_response".into())
}

pub fn valid_component(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        && id.as_bytes()[0].is_ascii_alphanumeric()
}
pub fn decode_scoped(bytes: &[u8], component: Option<&str>) -> Result<HubReport, String> {
    let report = decode(bytes)?;
    if let Some(id) = component {
        if report.scope.as_deref() != Some(id) {
            return Err("hub_update_required".into());
        }
        if report.components.len() != 1 || report.components[0].id != id {
            return Err("hub_invalid_response".into());
        }
    } else if report.scope.is_some() {
        return Err("hub_invalid_response".into());
    }
    Ok(report)
}

pub fn types() -> String {
    format!(
        "// Generated by cargo run -- --export-types; do not edit.\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\n",
        Observation::decl(&ts_rs::Config::default()),
        Step::decl(&ts_rs::Config::default()),
        Component::decl(&ts_rs::Config::default()),
        HubReport::decl(&ts_rs::Config::default()),
        LoadFailure::decl(&ts_rs::Config::default())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projects_only_the_ui_contract_and_accepts_invalid_manifest_rows() {
        let data = serde_json::json!({"schema_version":"1.0", "ok":false,"data":{
            "schema_version":1,"observed_at":"2026-01-01T00:00:00Z","checked":false,
            "components":[{"id":"tool","installed":{"tool":"/private/path"},"state":"invalid","issues":["bad manifest"]}],"unmanifested":[]}});
        let report = decode(&serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(report.components[0].installed);
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(!serialized.contains("/private/path"));
        assert_eq!(report.components[0].state, "invalid");
        assert!(decode(br#"{"schema_version":"9","data":{}}"#).is_err());
    }
    #[test]
    fn scoped_reports_preserve_diagnostics_without_exposing_owner_commands() {
        let source = serde_json::json!({"schema_version":"1.0","ok":true,"data":{
            "schema_version":1,"scope":"tool","observed_at":"now","checked":true,"unmanifested":[],
            "components":[{"id":"tool","name":"Tool","group":"delivery","summary":"Tool","host":"wsl","lifecycle":"active","state":"action_needed","installed":{"tool":"/private/path"},"issues":[],"depends_on":["dependency"],"checked_at":"now", "setup":[{"id":"route","title":"Route","why":"Access","needs":[],"state":"action_needed","code":"proxy_unavailable","exit_code":1,"check":{"argv":["private-command"]}}]}]
        }});
        let bytes = serde_json::to_vec(&source).unwrap();
        let report = decode_scoped(&bytes, Some("tool")).unwrap();
        assert!(report.components[0].checkable);
        assert_eq!(
            report.components[0].setup[0]
                .observation
                .as_ref()
                .unwrap()
                .code
                .as_deref(),
            Some("proxy_unavailable")
        );
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(!serialized.contains("private-command"));
        assert!(!serialized.contains("/private/path"));
        assert!(decode_scoped(&bytes, Some("other")).is_err());
        assert!(decode_scoped(&bytes, None).is_err());
        let mut old = source;
        old["data"]["scope"] = serde_json::Value::Null;
        assert_eq!(
            decode_scoped(&serde_json::to_vec(&old).unwrap(), Some("tool")).unwrap_err(),
            "hub_update_required"
        );
    }
    #[test]
    fn only_literal_component_identifiers_are_accepted() {
        for id in ["spm", "jenkins-bridge", "tool1"] {
            assert!(valid_component(id));
        }
        for id in ["", "--check", "UPPER", "tool;cmd", "a/b", "tool\n"] {
            assert!(!valid_component(id));
        }
    }
    #[test]
    fn malformed_containers_fail_without_panicking() {
        for data in [
            serde_json::json!([]),
            serde_json::json!({"schema_version":1,"components":[null]}),
            serde_json::json!({"schema_version":1,"components":[{"setup":[false]}]}),
        ] {
            let bytes = serde_json::to_vec(
                &serde_json::json!({"schema_version":"1.0","ok":true,"data":data}),
            )
            .unwrap();
            assert!(decode(&bytes).is_err());
        }
    }
    #[test]
    fn generated_types_are_current() {
        assert_eq!(include_str!("../../src/contracts/hub.ts"), types());
    }
}

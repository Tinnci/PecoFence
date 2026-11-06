use super::*;
use std::io::Read;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, deny_unknown_fields)]
pub struct LogPolicy {
    pub schema_version: u32,
    pub retention_days: u32,
    pub max_total_mib: u32,
    pub daily_info_mib: u32,
    pub daily_max_mib: u32,
    pub daemon_file_mib: u32,
    pub daemon_files_per_stream: u32,
}
impl Default for LogPolicy {
    fn default() -> Self {
        Self {
            schema_version: 1,
            retention_days: 14,
            max_total_mib: 200,
            daily_info_mib: 20,
            daily_max_mib: 40,
            daemon_file_mib: 20,
            daemon_files_per_stream: 14,
        }
    }
}
impl LogPolicy {
    pub fn valid(&self) -> bool {
        self.schema_version == 1
            && (1..=365).contains(&self.retention_days)
            && (1..=4096).contains(&self.max_total_mib)
            && (1..=256).contains(&self.daily_info_mib)
            && (1..=512).contains(&self.daily_max_mib)
            && (1..=256).contains(&self.daemon_file_mib)
            && (1..=100).contains(&self.daemon_files_per_stream)
            && self.daily_info_mib <= self.daily_max_mib
            && self.daily_max_mib <= self.max_total_mib
    }
}
/// Runtime readers preserve logs if a policy is malformed. Missing settings use defaults.
pub fn local_log_policy(directory: &Path) -> Option<LogPolicy> {
    let path = directory.parent()?.join("logging.json");
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(LogPolicy::default()),
        Err(_) => return None,
    };
    let mut bytes = vec![];
    file.take(16 * 1024 + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 16 * 1024 {
        return None;
    }
    let policy: LogPolicy =
        serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes)).ok()?;
    policy.valid().then_some(policy)
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct LogUsage {
    pub files: usize,
    #[ts(type = "number")]
    pub bytes: u64,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct LoggingSettings {
    pub policy: LogPolicy,
    pub defaults: LogPolicy,
    pub settings_path: String,
    pub directory: String,
    pub daemon_directory: String,
    pub usage: LogUsage,
    pub daemon_usage: LogUsage,
}
pub fn logging_args(action: &str, policy: Option<&LogPolicy>) -> Option<Vec<String>> {
    if !matches!(action, "show" | "set" | "reset") {
        return None;
    }
    let mut args: Vec<String> = ["settings", "logging", action]
        .into_iter()
        .map(String::from)
        .collect();
    if action == "set" {
        let policy = policy.filter(|p| p.valid())?;
        for (flag, value) in [
            ("--retention-days", policy.retention_days),
            ("--max-total-mib", policy.max_total_mib),
            ("--daily-info-mib", policy.daily_info_mib),
            ("--daily-max-mib", policy.daily_max_mib),
            ("--daemon-file-mib", policy.daemon_file_mib),
            ("--daemon-files-per-stream", policy.daemon_files_per_stream),
        ] {
            args.extend([flag.into(), value.to_string()]);
        }
    } else if policy.is_some() {
        return None;
    }
    Some(args)
}
pub fn decode_logging(code: Option<i32>, bytes: &[u8]) -> Result<LoggingSettings, SetupFailure> {
    #[derive(Deserialize)]
    struct Data {
        logging: LoggingSettings,
    }
    let data: Data = owner_data(code, bytes)?;
    if !data.logging.policy.valid() || !data.logging.defaults.valid() {
        return Err(SetupFailure::new("spm_invalid_response"));
    }
    Ok(data.logging)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logging_routes_are_allowlisted_and_validate_the_entire_policy() {
        let mut policy = LogPolicy::default();
        assert!(logging_args("show", None).is_some());
        assert!(logging_args("delete", None).is_none());
        assert!(logging_args("reset", Some(&policy)).is_none());
        assert!(logging_args("set", None).is_none());
        let args = logging_args("set", Some(&policy)).unwrap();
        assert_eq!(&args[..3], &["settings", "logging", "set"]);
        assert_eq!(args.len(), 15);
        policy.retention_days = 0;
        assert!(logging_args("set", Some(&policy)).is_none());
        policy.retention_days = 365;
        policy.daily_max_mib = 10;
        assert!(!policy.valid());
    }
    #[test]
    fn owner_envelope_is_decoded_and_malformed_or_old_cli_responses_are_reported() {
        let data = serde_json::json!({"ok":true,"data":{"logging":{
            "policy":LogPolicy::default(),"defaults":LogPolicy::default(),"settings_path":"logging.json",
            "directory":"logs","daemon_directory":"daemon","usage":{"files":1,"bytes":20},"daemon_usage":{"files":2,"bytes":30}
        }}});
        let bytes = serde_json::to_vec(&data).unwrap();
        assert_eq!(
            decode_logging(Some(0), &bytes)
                .unwrap()
                .policy
                .retention_days,
            14
        );
        assert_eq!(
            decode_logging(Some(2), b"").unwrap_err().code,
            "spm_update_required"
        );
        assert!(decode_logging(Some(0), b"{}").is_err());
        let mut invalid = data;
        invalid["data"]["logging"]["policy"]["retention_days"] = 0.into();
        assert!(decode_logging(Some(0), &serde_json::to_vec(&invalid).unwrap()).is_err());
    }
}

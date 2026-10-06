use crate::ContractError;
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointIdentity {
    pub user_sid: String,
    pub windows_session_id: u32,
    pub instance: Option<String>,
}

// The instance suffix only changes transport addressing, not the v2 wire format or protocol version.
pub fn v2_pipe_name(identity: &EndpointIdentity) -> Result<String, ContractError> {
    let sid = &identity.user_sid;
    if sid.len() > 184
        || !sid.starts_with("S-")
        || !sid[2..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-')
        || sid[2..].is_empty()
    {
        return Err(ContractError::InvalidField {
            field: "user_sid",
            reason: "expected an S- prefixed numeric SID of at most 184 bytes".into(),
        });
    }
    let mut name = format!(
        r"\\.\pipe\pecofence.spmd.v2.{}.{}",
        sid, identity.windows_session_id
    );
    if let Some(instance) = identity
        .instance
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let mut hasher = DefaultHasher::new();
        instance.hash(&mut hasher);
        name.push_str(&format!(".i-{:016x}", hasher.finish()));
    }
    Ok(name)
}

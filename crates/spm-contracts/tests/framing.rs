use spm_contracts::*;

#[test]
fn frame_roundtrip_is_bounded_and_exact() {
    let frame = encode_frame(&vec!["value"]).unwrap();
    assert_eq!(decode_frame::<Vec<String>>(&frame).unwrap(), vec!["value"]);
    let mut trailing = frame.clone();
    trailing.push(0);
    assert!(matches!(
        decode_frame::<Vec<String>>(&trailing),
        Err(ContractError::TrailingBytes)
    ));
    assert!(matches!(
        validate_declared_length(0u32.to_le_bytes()),
        Err(ContractError::InvalidFrameLength(0))
    ));
    assert!(validate_declared_length((MAX_FRAME_BYTES as u32).to_le_bytes()).is_ok());
    assert!(validate_declared_length((MAX_FRAME_BYTES as u32 + 1).to_le_bytes()).is_err());
}

#[test]
fn endpoint_uses_sid_and_windows_session() {
    let name = v2_pipe_name(&EndpointIdentity {
        user_sid: "S-1-5-21-42".into(),
        windows_session_id: 7,
        instance: None,
    })
    .unwrap();
    assert_eq!(name, r"\\.\pipe\pecofence.spmd.v2.S-1-5-21-42.7");
    assert!(v2_pipe_name(&EndpointIdentity {
        user_sid: "user".into(),
        windows_session_id: 7,
        instance: None,
    })
    .is_err());
}

fn identity(instance: Option<&str>) -> EndpointIdentity {
    EndpointIdentity {
        user_sid: "S-1-5-21-42".into(),
        windows_session_id: 7,
        instance: instance.map(str::to_owned),
    }
}

#[test]
fn default_identity_matches_legacy_pipe_name() {
    assert_eq!(
        v2_pipe_name(&identity(None)).unwrap(),
        r"\\.\pipe\pecofence.spmd.v2.S-1-5-21-42.7"
    );
}

#[test]
fn instance_appends_stable_slug() {
    let first = v2_pipe_name(&identity(Some("alpha"))).unwrap();
    assert_eq!(first, v2_pipe_name(&identity(Some("alpha"))).unwrap());
    assert_ne!(first, v2_pipe_name(&identity(Some("beta"))).unwrap());
    assert!(first.starts_with(&format!("{}.", v2_pipe_name(&identity(None)).unwrap())));
    assert_eq!(first.rsplit('.').next().unwrap().len(), 18);
}

#[test]
fn whitespace_instance_treated_as_default() {
    let default = v2_pipe_name(&identity(None)).unwrap();
    assert_eq!(v2_pipe_name(&identity(Some(" \t "))).unwrap(), default);
    assert_eq!(v2_pipe_name(&identity(Some(""))).unwrap(), default);
    assert_eq!(
        v2_pipe_name(&identity(Some(" alpha "))).unwrap(),
        v2_pipe_name(&identity(Some("alpha"))).unwrap()
    );
}

#[test]
fn slug_is_pipe_safe() {
    let name = v2_pipe_name(&identity(Some("<>:\\/?*| unicode 🚀"))).unwrap();
    let slug = name.rsplit('.').next().unwrap();
    assert!(slug
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'));
    assert!(name
        .strip_prefix(r"\\.\pipe\")
        .unwrap()
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-'));
}

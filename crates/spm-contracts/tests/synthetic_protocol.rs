// New public verification cases use deliberately fictional data, not backend
// configuration, business policies or production snapshots.
use spm_contracts::*;

fn snapshot() -> ProjectSnapshot {
    serde_json::from_str(include_str!("../fixtures/snapshots/fictional-example.json")).unwrap()
}

#[test]
fn fictional_snapshot_roundtrips_all_gate_states() {
    let mut snapshot = snapshot();
    assert_eq!(
        snapshot.gates[0].threshold,
        Some(RationalThreshold { p: 3, q: 4 })
    );
    for state in [
        GateStatus::Satisfied,
        GateStatus::Unsatisfied,
        GateStatus::Unknown,
        GateStatus::NotApplicable,
    ] {
        snapshot.overall_gate.state = state;
        snapshot.gates[0].state = state;
        snapshot.validate().unwrap();
        let encoded = canonical_json(&snapshot).unwrap();
        assert_eq!(
            serde_json::from_str::<ProjectSnapshot>(&encoded).unwrap(),
            snapshot
        );
    }
}

#[test]
fn snapshot_event_identity_and_direction_are_checked() {
    let snapshot = snapshot();
    let mut envelope = Envelope {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        daemon_session: Some(snapshot.daemon_session),
        request_id: None,
        subscription_id: Some(SubscriptionId::new()),
        revision: Some(snapshot.revision),
        body: Body::Event(Event::Snapshot(snapshot)),
    };
    envelope.validate(Direction::ServerToClient).unwrap();
    assert!(envelope.validate(Direction::ClientToServer).is_err());
    let encoded = encode_frame(&envelope).unwrap();
    assert_eq!(decode_frame::<Envelope>(&encoded).unwrap(), envelope);
    envelope.revision = Some(Revision(2));
    assert!(envelope.validate(Direction::ServerToClient).is_err());
}

#[test]
fn identifiers_cursors_and_page_boundaries_keep_upstream_rules() {
    assert!(ProjectId::new("fictional-project").is_ok());
    for value in ["", " untrimmed", "untrimmed "] {
        assert!(ProjectId::new(value).is_err());
        assert!(serde_json::from_value::<ProjectId>(serde_json::json!(value)).is_err());
    }
    assert!(ProjectId::new("x".repeat(256)).is_ok());
    assert!(ProjectId::new("x".repeat(257)).is_err());
    assert!(PageCursor::new("synthetic_cursor-1").is_ok());
    for value in ["", "cursor=", "cursor/"] {
        assert!(PageCursor::new(value).is_err());
    }
    let mut page = QueryPageRequest {
        subscription_id: SubscriptionId::new(),
        revision: Revision(1),
        cursor: None,
        page_size: 1,
    };
    for size in [1, MAX_PAGE_SIZE] {
        page.page_size = size;
        Request::QueryPage(page.clone()).validate().unwrap();
    }
    page.page_size = 0;
    assert!(Request::QueryPage(page.clone()).validate().is_err());
    page.page_size = 1;
    page.revision = Revision(0);
    assert!(Request::QueryPage(page).validate().is_err());
}

#[test]
fn retryability_and_truncated_frames_keep_upstream_rules() {
    let mut error = ErrorDto {
        code: ErrorCode::Unavailable,
        request_id: Some(RequestId::new()),
        retryable: true,
        message: "Fictional unavailable service".into(),
        details: None,
    };
    error.validate().unwrap();
    error.code = ErrorCode::PermissionDenied;
    assert!(error.validate().is_err());
    let frame = encode_frame(&vec!["synthetic"]).unwrap();
    assert!(matches!(
        decode_frame::<Vec<String>>(&frame[..3]),
        Err(ContractError::TruncatedFrame)
    ));
    assert!(matches!(
        decode_frame::<Vec<String>>(&frame[..frame.len() - 1]),
        Err(ContractError::TruncatedFrame)
    ));
}

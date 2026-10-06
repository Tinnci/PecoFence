use spm_contracts::*;

#[test]
fn hello_cannot_claim_a_daemon_session() {
    let mut envelope: Envelope =
        serde_json::from_str(include_str!("../fixtures/rpc/hello-request.json")).unwrap();
    envelope.daemon_session = Some(DaemonSessionId::new());
    assert!(envelope.validate(Direction::ClientToServer).is_err());
}

#[test]
fn all_gate_states_are_distinct() {
    let states = [
        GateStatus::Satisfied,
        GateStatus::Unsatisfied,
        GateStatus::Unknown,
        GateStatus::NotApplicable,
    ];
    let encoded: Vec<_> = states
        .into_iter()
        .map(|state| serde_json::to_string(&state).unwrap())
        .collect();
    let unique: std::collections::BTreeSet<_> = encoded.iter().collect();
    assert_eq!(unique.len(), 4);
}

use spm_contracts::*;

#[test]
fn unknown_critical_enum_is_rejected() {
    assert!(serde_json::from_str::<GateEvaluation>(include_str!(
        "../fixtures/invalid/unknown-gate-state.json"
    ))
    .is_err());
}

#[test]
fn request_semantics_reject_oversize_pages() {
    let request: Request =
        serde_json::from_str(include_str!("../fixtures/invalid/page-size-501.json")).unwrap();
    assert!(request.validate().is_err());
}

#[test]
fn threshold_denominator_must_be_positive() {
    assert!(RationalThreshold::new(1, 0).is_err());
}

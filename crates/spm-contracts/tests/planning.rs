use spm_contracts::*;
fn view() -> DeliveryPlanView {
    serde_json::from_value(serde_json::json!({"schema_version":1,"project_id":"DEMO","delivery_scope_id":"release-a","plan_id":"plan-a","plan_revision":1,"baseline_id":"baseline-a","epoch_utc":"2026-10-28T00:00:00Z","slot_minutes":60,"status_at":0,"horizon":100,"conditional":false,"finish":4,"protected_finish":6,"tasks":[{"id":"task-a","label":"Task A","lane":"model-a","state":"todo","start":0,"finish":4,"actual_start":null,"actual_finish":null,"conditional":false,"critical":true,"driving":true,"blocked":false,"unscheduled_reason":null,"dependencies":[],"resources":["lab"]}],"total_tasks":1,"offset":0,"next_offset":null,"milestones":[],"gates":[],"resource_waits":[],"buffers":[],"flow":[],"warnings":[]})).unwrap()
}
#[test]
fn bounded_projection_round_trips_without_private_solver_types() {
    let v = view();
    v.validate().unwrap();
    let raw = serde_json::to_vec(&v).unwrap();
    let restored: DeliveryPlanView = serde_json::from_slice(&raw).unwrap();
    assert_eq!(v, restored);
}
#[test]
fn plan_pages_pin_plan_revision_and_have_exact_continuation() {
    let mut q:GetDeliveryPlanRequest=serde_json::from_value(serde_json::json!({"project_id":"DEMO","delivery_scope_id":"release-a","plan_id":"plan-a","plan_revision":null,"offset":1,"limit":100})).unwrap();
    assert!(q.validate().is_err());
    q.plan_revision = Some(1);
    q.validate().unwrap();
    q.limit = 201;
    assert!(q.validate().is_err());
    let mut v = view();
    v.next_offset = Some(1);
    assert!(v.validate().is_err());
}
#[test]
fn duplicate_tasks_unknown_fields_and_fake_actuals_are_rejected() {
    let mut v = view();
    v.tasks.push(v.tasks[0].clone());
    v.total_tasks = 2;
    assert!(v.validate().is_err());
    let mut v = view();
    v.tasks[0].state = PlanWorkState::Done;
    assert!(v.validate().is_err());
    let mut raw = serde_json::to_value(view()).unwrap();
    raw["solver_private_field"] = true.into();
    assert!(serde_json::from_value::<DeliveryPlanView>(raw).is_err());
}
#[test]
fn plan_envelopes_validate_before_display() {
    let mut v = view();
    v.tasks[0].finish = Some(101);
    let envelope = Envelope {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        daemon_session: Some(DaemonSessionId::new()),
        request_id: Some(RequestId::new()),
        subscription_id: None,
        revision: None,
        body: Body::Response(Response::GetDeliveryPlan(v)),
    };
    assert!(envelope.validate(Direction::ServerToClient).is_err());
}

#[test]
fn delivery_method_is_minor_one_and_cannot_leak_into_minor_zero_envelopes() {
    assert_eq!(RpcMethod::GetDeliveryPlan.minimum_minor(), 1);
    assert_eq!(RpcMethod::GetCapabilities.minimum_minor(), 0);
    let q:GetDeliveryPlanRequest=serde_json::from_value(serde_json::json!({"project_id":"DEMO","delivery_scope_id":"release-a","plan_id":"plan-a","plan_revision":null,"offset":0,"limit":100})).unwrap();
    let mut envelope = Envelope {
        protocol_major: 2,
        protocol_minor: 0,
        daemon_session: Some(DaemonSessionId::new()),
        request_id: Some(RequestId::new()),
        subscription_id: None,
        revision: None,
        body: Body::Request(Request::GetDeliveryPlan(q)),
    };
    assert!(envelope.validate(Direction::ClientToServer).is_err());
    envelope.protocol_minor = 1;
    envelope.validate(Direction::ClientToServer).unwrap();
}

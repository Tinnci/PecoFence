use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use spm_contracts::*;

/// Serde writes every `Option` as `null` and every default list as `[]`; the fixtures omit
/// absent and empty optional fields. Both sides drop nulls and empty lists before comparing,
/// so the check is about the model, not the spelling.
fn strip_nulls(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(_, v)| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
                .map(|(k, v)| (k, strip_nulls(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(strip_nulls).collect()),
        other => other,
    }
}

fn golden<T>(fixture: &str)
where
    T: Serialize + DeserializeOwned + std::fmt::Debug + PartialEq,
{
    let typed: T = serde_json::from_str(fixture).expect("fixture parses");
    let wire = canonical_json(&typed).unwrap();
    let again: T = serde_json::from_str(&wire).expect("canonical output parses");
    assert_eq!(typed, again, "round trip changed the value");
    let fixture_value: Value = serde_json::from_str(fixture).unwrap();
    let wire_value: Value = serde_json::from_str(&wire).unwrap();
    assert_eq!(strip_nulls(fixture_value), strip_nulls(wire_value));
}

fn assert_rejected<T>(fixture: &str, validate: impl Fn(&T) -> Result<(), ContractError>)
where
    T: DeserializeOwned,
{
    match serde_json::from_str::<T>(fixture) {
        Err(_) => {}
        Ok(value) => assert!(validate(&value).is_err(), "fixture should be rejected"),
    }
}

#[test]
fn issue_responses_round_trip_and_match_their_fixtures() {
    golden::<IssueDetail>(include_str!("../fixtures/issues/get-issue-r102.json"));
    golden::<GateReadiness>(include_str!("../fixtures/issues/gate-readiness-tr5.json"));
    golden::<GateWhatIf>(include_str!("../fixtures/issues/gate-whatif-tr5.json"));
    golden::<SearchWorkspaceResponse>(include_str!("../fixtures/issues/search-hybrid.json"));
}

#[test]
fn issue_requests_round_trip_and_validate() {
    for fixture in [
        include_str!("../fixtures/issues/query-issues-request.json"),
        include_str!("../fixtures/issues/judge-duplicate-request.json"),
        include_str!("../fixtures/issues/search-request.json"),
        include_str!("../fixtures/issues/review-assignment-request.json"),
        include_str!("../fixtures/issues/refresh-issues-request.json"),
    ] {
        golden::<Request>(fixture);
        let request: Request = serde_json::from_str(fixture).unwrap();
        request.validate().unwrap();
    }
}

#[test]
fn the_golden_detail_is_internally_consistent() {
    let detail: IssueDetail =
        serde_json::from_str(include_str!("../fixtures/issues/get-issue-r102.json")).unwrap();
    detail.validate().unwrap();
    // Gale's reply is the reason the proposal exists; without it the status stays unspecified.
    assert_eq!(
        detail.proposals[0].basis,
        AssignmentBasis::ExplicitStatement
    );
    assert_eq!(detail.proposals[0].authority, Standing::Unknown);
    assert_eq!(detail.proposals[0].acceptance, Standing::Unknown);
}

#[test]
fn the_golden_readiness_matches_the_documented_entry_numbers() {
    let readiness: GateReadiness =
        serde_json::from_str(include_str!("../fixtures/issues/gate-readiness-tr5.json")).unwrap();
    readiness.validate().unwrap();
    assert!(!readiness.ready);
    assert_eq!(readiness.next_gate, GateKind::Tr5);
    assert_eq!(readiness.criteria[0].value, 14.5);
    assert_eq!(readiness.criteria[0].threshold, 10.0);
    assert_eq!(
        readiness
            .di
            .by_severity
            .iter()
            .map(|s| s.contribution)
            .sum::<f64>(),
        14.5
    );
}

#[test]
fn negative_issue_fixtures_fail() {
    assert_rejected::<IssueItem>(
        include_str!("../fixtures/issues/invalid-dts-without-severity.json"),
        IssueItem::validate,
    );
    assert_rejected::<IssueItem>(
        include_str!("../fixtures/issues/invalid-issue-id.json"),
        IssueItem::validate,
    );
    assert_rejected::<Request>(
        include_str!("../fixtures/issues/invalid-judge-who-to-ask.json"),
        Request::validate,
    );
    assert_rejected::<Request>(
        include_str!("../fixtures/issues/invalid-query-limit.json"),
        Request::validate,
    );
    assert_rejected::<GateReadiness>(
        include_str!("../fixtures/issues/invalid-gate-ready-flag.json"),
        GateReadiness::validate,
    );
    assert_rejected::<GateWhatIf>(
        include_str!("../fixtures/issues/invalid-whatif-arithmetic.json"),
        GateWhatIf::validate,
    );
    assert_rejected::<SearchWorkspaceResponse>(
        include_str!("../fixtures/issues/invalid-search-span.json"),
        SearchWorkspaceResponse::validate,
    );
    assert_rejected::<SearchWorkspaceResponse>(
        include_str!("../fixtures/issues/invalid-search-fallback.json"),
        SearchWorkspaceResponse::validate,
    );
}

#[test]
fn unknown_fields_are_rejected_on_issue_types() {
    let mut value: Value =
        serde_json::from_str(include_str!("../fixtures/issues/get-issue-r102.json")).unwrap();
    value["item"]["owner_typo"] = Value::from("Gale");
    assert!(serde_json::from_value::<IssueDetail>(value).is_err());
}

#[test]
fn issue_ids_follow_the_business_token_shape() {
    for ok in ["R-102", "D-2291", "K-014", "A-063", "AR-1", "SR-031"] {
        validate_issue_id(ok).unwrap_or_else(|_| panic!("{ok} should pass"));
    }
    for bad in [
        "r-102",
        "R102",
        "R-",
        "-102",
        "R-123456789",
        "ABCDE-1",
        "R-1 ",
        "R-1x",
        "",
    ] {
        assert!(validate_issue_id(bad).is_err(), "{bad} should fail");
    }
}

#[test]
fn judgment_routing_accepts_only_the_two_judged_kinds() {
    let project = ProjectId::new("orion-ir").unwrap();
    let judge = |kind, subject: &str, object: &str| JudgeSuggestionRequest {
        project_id: project.clone(),
        kind,
        subject_key: subject.into(),
        object_key: object.into(),
        verdict: SuggestionVerdict::Accept,
        note: None,
    };
    judge(
        SuggestionKind::RelatedDiscussion,
        "issue:R-102",
        "record:lark:msg:0b7",
    )
    .validate()
    .unwrap();
    judge(
        SuggestionKind::DuplicateCandidate,
        "defect:D-2301",
        "defect:D-2291",
    )
    .validate()
    .unwrap();
    assert!(
        judge(SuggestionKind::RelatedDiscussion, "record:x", "issue:R-102")
            .validate()
            .is_err()
    );
    assert!(judge(
        SuggestionKind::DuplicateCandidate,
        "defect:D-2291",
        "defect:D-2291"
    )
    .validate()
    .is_err());
    assert!(judge(
        SuggestionKind::DuplicateCandidate,
        "issue:R-102",
        "defect:D-2291"
    )
    .validate()
    .is_err());
    assert!(
        judge(SuggestionKind::WhoToAsk, "issue:R-102", "person:entity:kai")
            .validate()
            .is_err()
    );
    assert!(
        judge(SuggestionKind::MissingIpdInfo, "issue:R-102", "issue:R-102")
            .validate()
            .is_err()
    );
    for bad in ["issue", "other:R-102", "issue:", "issue:R-102\n"] {
        assert!(validate_suggestion_key(bad).is_err(), "{bad:?} should fail");
    }
}

#[test]
fn review_requests_bound_their_notes() {
    let mut review = ReviewAssignmentRequest {
        project_id: ProjectId::new("orion-ir").unwrap(),
        proposal_id: "proposal:R-102:gale".into(),
        verdict: AssignmentVerdict::Confirm,
        note: Some("ok".into()),
    };
    review.validate().unwrap();
    review.note = Some("x".repeat(MAX_ISSUE_TEXT + 1));
    assert!(review.validate().is_err());
    review.note = None;
    review.proposal_id = "  ".into();
    assert!(review.validate().is_err());
}

#[test]
fn list_and_scalar_bounds_are_enforced() {
    let project = ProjectId::new("orion-ir").unwrap();
    let query = |limit| QueryIssuesRequest {
        project_id: project.clone(),
        queue: None,
        kind: None,
        limit,
    };
    query(1).validate().unwrap();
    query(MAX_ISSUE_LIMIT).validate().unwrap();
    assert!(query(0).validate().is_err());
    assert!(query(MAX_ISSUE_LIMIT + 1).validate().is_err());

    let search = |query: String, limit| SearchWorkspaceRequest {
        project_id: project.clone(),
        query,
        mode: SearchMode::Hybrid,
        scope: SearchScope::All,
        limit,
    };
    search("搜索框".into(), MAX_SEARCH_LIMIT)
        .validate()
        .unwrap();
    assert!(search(String::new(), 5).validate().is_err());
    assert!(search("x".repeat(MAX_SEARCH_QUERY + 1), 5)
        .validate()
        .is_err());
    assert!(search("a\nb".into(), 5).validate().is_err());
    assert!(search("a".into(), MAX_SEARCH_LIMIT + 1).validate().is_err());

    let what_if = |max_steps| GateWhatIfRequest {
        project_id: project.clone(),
        gate: GateKind::Tr5,
        max_steps,
    };
    what_if(MAX_WHAT_IF_STEPS).validate().unwrap();
    assert!(what_if(0).validate().is_err());
    assert!(what_if(MAX_WHAT_IF_STEPS + 1).validate().is_err());

    let mut detail: IssueDetail =
        serde_json::from_str(include_str!("../fixtures/issues/get-issue-r102.json")).unwrap();
    let message = detail.messages[0].clone();
    detail.messages = vec![message; MAX_ISSUE_MESSAGES + 1];
    assert!(detail.validate().is_err());
}

#[test]
fn item_consistency_rules_hold() {
    let base: IssueDetail =
        serde_json::from_str(include_str!("../fixtures/issues/get-issue-r102.json")).unwrap();
    let mut item = base.item;
    item.validate().unwrap();

    item.queues.push(IssueQueue::SourceConflict);
    assert!(item.validate().is_err(), "conflict queue needs a conflict");
    item.queues.pop();

    item.blocks_gate = Some(GateKind::Tr5);
    assert!(
        item.validate().is_err(),
        "blocks_gate needs the blocking queue"
    );
    item.blocks_gate = None;

    item.overdue = true;
    assert!(item.validate().is_err(), "overdue needs a due date");
    item.overdue = false;

    item.effort_hours = Some(0.0);
    assert!(item.validate().is_err(), "effort must be positive");
    item.effort_hours = None;

    item.risk = Some(RiskDetail {
        probability: Some(RiskLevel::High),
        impact: None,
        state: RiskState::Open,
        mitigation: None,
    });
    assert!(
        item.validate().is_err(),
        "risk detail is for RISK items only"
    );
}

#[test]
fn what_if_and_readiness_reject_inconsistent_totals() {
    let mut readiness: GateReadiness =
        serde_json::from_str(include_str!("../fixtures/issues/gate-readiness-tr5.json")).unwrap();
    readiness.ready = true;
    assert!(
        readiness.validate().is_err(),
        "ready needs every criterion to pass"
    );
    for criterion in &mut readiness.criteria {
        criterion.pass = true;
    }
    readiness.validate().unwrap();
    readiness.criteria[4].pass = false;
    assert!(
        readiness.validate().is_err(),
        "one failing criterion blocks ready"
    );

    let mut what_if: GateWhatIf =
        serde_json::from_str(include_str!("../fixtures/issues/gate-whatif-tr5.json")).unwrap();
    what_if.reaches_threshold = false;
    assert!(what_if.validate().is_err());
    let mut long = what_if.clone();
    long.steps = vec![long.steps[0].clone(); usize::from(MAX_WHAT_IF_STEPS) + 1];
    assert!(long.validate().is_err());
}

#[test]
fn search_results_reject_channel_mismatches_and_fake_similarity() {
    let mut response: SearchWorkspaceResponse =
        serde_json::from_str(include_str!("../fixtures/issues/search-hybrid.json")).unwrap();
    response.validate().unwrap();
    response.results[0].channels = vec![SearchChannel::Lexical];
    assert!(
        response.validate().is_err(),
        "semantic hit without the channel"
    );
    response.results[0].channels = vec![SearchChannel::Lexical, SearchChannel::Semantic];
    response.results[0].semantic.as_mut().unwrap().similarity = 1.5;
    assert!(response.validate().is_err(), "similarity outside -1..=1");
}

#[test]
fn request_methods_map_to_their_rpc_names() {
    let project = ProjectId::new("orion-ir").unwrap();
    let cases: Vec<(Request, RpcMethod)> = vec![
        (
            Request::GetGateReadiness(GetGateReadinessRequest {
                project_id: project.clone(),
            }),
            RpcMethod::GetGateReadiness,
        ),
        (
            Request::GetIssue(GetIssueRequest {
                project_id: project.clone(),
                id: "R-102".into(),
            }),
            RpcMethod::GetIssue,
        ),
        (
            Request::GateWhatIf(GateWhatIfRequest {
                project_id: project.clone(),
                gate: GateKind::Tr5,
                max_steps: 3,
            }),
            RpcMethod::GateWhatIf,
        ),
        (
            Request::RefreshIssues(RefreshIssuesRequest {
                project_id: project.clone(),
            }),
            RpcMethod::RefreshIssues,
        ),
    ];
    for (request, method) in cases {
        assert_eq!(request.method(), method);
        request.validate().unwrap();
    }
}

#[test]
fn refresh_reports_no_counts_without_people_cli() {
    let refused: RefreshIssuesResponse =
        serde_json::from_str(r#"{"people_configured":false,"statements":2,"proposals_created":1}"#)
            .unwrap();
    assert!(refused.validate().is_err(), "counts without people-cli");
    let demo: RefreshIssuesResponse =
        serde_json::from_str(r#"{"people_configured":false}"#).unwrap();
    demo.validate().unwrap();
    let ran: RefreshIssuesResponse = serde_json::from_str(
        r#"{"people_configured":true,"statements":3,"proposals_created":2,"truncated":false}"#,
    )
    .unwrap();
    ran.validate().unwrap();
}

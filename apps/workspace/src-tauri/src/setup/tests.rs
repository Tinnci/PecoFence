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
/// Every action `src/api.ts` lets the frontend send, with plausible arguments.
fn sample(action: &str) -> FocusRequest {
    FocusRequest {
        action: action.into(),
        code: Some("E6765GDL".into()),
        job: Some("folder/model_A16".into()),
        build: Some(23),
        change: Some(80481),
        review: Some("accept".into()),
        branch: Some("E6765GDL_MP".into()),
        source: Some(
            if action.starts_with("flow") {
                "meegle"
            } else {
                "jira"
            }
            .into(),
        ),
        item: Some(
            if action.starts_with("flow") || action.starts_with("source_fields") {
                "7140117552"
            } else {
                "PLV-1509"
            }
            .into(),
        ),
        path: Some("buildspec/P.spec".into()),
        items: Some(vec!["7140117552".into(), "7128060102".into()]),
        addresses: Some(if action == "flow" || action.starts_with("source_fields") {
            vec!["https://project.feishu.cn/efzuhc/wentiguanli/detail/7140117552".into()]
        } else {
            vec![
                "https://project.feishu.cn/efzuhc/wentiguanli/detail/7140117552".into(),
                "https://project.feishu.cn/efzuhc/wentiguanli/detail/7128060102".into(),
            ]
        }),
        instance: Some("192.168.11.130:8080".into()),
        chip: Some("MT6769 · Helio G81".into()),
        baseband: None,
        query: Some("words".into()),
    }
}

#[test]
fn search_takes_words_but_never_an_option() {
    let request = |query: &str| FocusRequest {
        action: "search".into(),
        query: Some(query.into()),
        ..Default::default()
    };
    assert_eq!(route_args(&request(" ESM2-647 ")).unwrap()[2], "ESM2-647");
    assert!(route_args(&request("--config-root")).is_none());
    assert!(route_args(&request("  ")).is_none());
}

#[test]
fn platform_updates_preserve_absent_fields_and_validate_both_names() {
    let mut r = sample("chip");
    r.chip = None;
    r.baseband = Some(" MOLY.LR12A.R3.MP ".into());
    let args = route_args(&r).unwrap();
    assert!(args.contains(&"--baseband=MOLY.LR12A.R3.MP".into()));
    assert!(!args.iter().any(|a| a.starts_with("--chip=")));
    r.chip = Some("-chip".into());
    r.baseband = Some(String::new());
    let args = route_args(&r).unwrap();
    assert!(args.contains(&"--chip=-chip".into()));
    assert!(args.contains(&"--baseband=".into()));
    r.baseband = Some("bad\nname".into());
    assert!(route_args(&r).is_none());
    r.baseband = Some("x".repeat(81));
    assert!(route_args(&r).is_none());
    r.chip = None;
    r.baseband = None;
    assert!(route_args(&r).is_none());
}

#[test]
fn older_platform_payloads_default_modem_details() {
    let p: Platform = serde_json::from_value(serde_json::json!({
        "code": "P6700LV", "vendor": "Unisoc", "chip": "", "android": ["A16"],
        "kernel": [], "evidence": []
    }))
    .unwrap();
    assert!(p.baseband.is_empty());
    assert!(p.modem.is_empty());
    assert!(p.modem_repos.is_empty());
    assert_eq!(p.modem_changes, 0);
}

#[test]
fn every_focus_action_the_frontend_sends_is_routed() {
    let api = include_str!("../../../src/api.ts");
    let start = api
        .find("export async function focus(")
        .expect("focus in api.ts");
    let union = &api[start..start + api[start..].find("args:").unwrap()];
    let actions: Vec<&str> = union.split('"').skip(1).step_by(2).collect();
    assert!(actions.len() > 20, "found {actions:?}");
    for action in actions {
        // save and preview carry their model on stdin; their arguments need nothing else.
        assert!(
            route_args(&sample(action)).is_some(),
            "the frontend sends {action} but the focus command does not route it"
        );
    }
    assert!(route_args(&sample("no_such_action")).is_none());
}

#[test]
fn annotate_and_history_carry_the_items_instance() {
    let args = route_args(&sample("annotate")).unwrap();
    assert!(
        args.windows(2)
            .any(|w| w == ["--instance", "192.168.11.130:8080"])
    );
    let mut bad = sample("history");
    bad.instance = Some("--x".into());
    assert!(route_args(&bad).is_none());
    assert!(valid_instance("project.feishu.cn/efzuhc"));
    assert!(!valid_instance("a b"));
}

#[test]
fn evidence_arguments_are_numeric_and_literal() {
    assert!(build_evidence_args("P6700LV", "folder/model_A16", 23).is_some());
    for job in [
        "--job",
        "folder/../x",
        "folder//x",
        "https://ci/job",
        "folder/%2f",
        "x;whoami",
    ] {
        assert!(build_evidence_args("P6700LV", job, 23).is_none());
    }
    assert!(build_evidence_args("P6700LV", "folder/model", 0).is_none());
    assert_eq!(
        diff_args("P6700LV", 80481, Some("buildspec/P6700LV.spec"), false).unwrap()[6..],
        ["--path", "buildspec/P6700LV.spec"]
    );
    assert_eq!(diff_args("P6700LV", 80481, None, false).unwrap().len(), 6);
    assert_eq!(
        diff_args("P6700LV", 80481, None, true)
            .unwrap()
            .last()
            .map(String::as_str),
        Some("--all")
    );
    for bad in ["--raw", "/etc/passwd", "a/../b", "", "a\nb"] {
        assert!(diff_args("P6700LV", 1, Some(bad), false).is_none(), "{bad}");
    }
    assert!(diff_args("P6700LV", 0, None, false).is_none());
    assert_eq!(
        flow_args("E6765GDL", &["1".into(), "2".into()]).unwrap()[4..],
        ["--item", "1", "--item", "2"]
    );
    assert!(flow_args("E6765GDL", &["--x".into()]).is_none());
    assert!(flow_args("E6765GDL", &[]).is_none());
}

#[test]
fn discovery_and_draft_arguments_are_validated() {
    let with = |f: fn(&mut DiscoverArgs)| {
        let mut a = DiscoverArgs::default();
        f(&mut a);
        a
    };
    let items = with(|a| {
        a.space = Some("efzuhc".into());
        a.kind = Some("693ff1c8".into());
        a.search = Some(" E67 ".into());
        a.count_type = Some("6942c93f".into());
        a.field = Some("field_b86050".into());
    });
    assert_eq!(
        discover_args("items", &items).unwrap(),
        [
            "discover",
            "items",
            "--space",
            "efzuhc",
            "--type",
            "693ff1c8",
            "--count-type",
            "6942c93f",
            "--field",
            "field_b86050",
            "--search",
            "E67"
        ]
    );
    let half = with(|a| {
        a.space = Some("efzuhc".into());
        a.kind = Some("693ff1c8".into());
        a.field = Some("field_b86050".into());
    });
    assert!(discover_args("items", &half).is_none());
    assert_eq!(
        discover_args("spaces", &DiscoverArgs::default()).unwrap(),
        ["discover", "spaces"]
    );
    let camera = with(|a| {
        a.repo = Some("packages/apps/DreamCamera2".into());
        a.branch = Some("mtk_sys_OS3.4_A16".into());
    });
    assert_eq!(
        discover_args("gerrit_activity", &camera).unwrap(),
        [
            "discover",
            "gerrit-activity",
            "--project",
            "packages/apps/DreamCamera2",
            "--branch",
            "mtk_sys_OS3.4_A16"
        ]
    );
    assert!(discover_args("gerrit_activity", &DiscoverArgs::default()).is_none());
    let injected = with(|a| a.repo = Some("x OR project:y".into()));
    assert!(discover_args("gerrit_activity", &injected).is_none());
    let plv = with(|a| a.project = Some("PLV".into()));
    assert_eq!(
        discover_args("jira_versions", &plv).unwrap(),
        ["discover", "jira-versions", "--project", "PLV"]
    );
    let lower = with(|a| a.project = Some("plv; rm".into()));
    assert!(discover_args("jira_filters", &lower).is_none());
    assert!(discover_args("types", &with(|a| a.space = Some("efz uhc".into()))).is_none());
    assert!(discover_args("groups", &with(|a| a.space = Some("efzuhc".into()))).is_none());
    assert!(discover_args("rm", &DiscoverArgs::default()).is_none());
    let newline = with(|a| {
        a.space = Some("a".into());
        a.kind = Some("b".into());
        a.search = Some("x\ny".into());
    });
    assert!(discover_args("items", &newline).is_none());
    assert_eq!(draft_args(false, None).unwrap(), ["setup", "draft"]);
    let sha = "a".repeat(64);
    assert_eq!(
        draft_args(true, Some(&sha)).unwrap(),
        [
            "setup",
            "draft",
            "--apply",
            "--replace",
            "--expect",
            sha.as_str()
        ]
    );
    assert!(draft_args(true, None).is_none());
    assert!(draft_args(true, Some("--config-root")).is_none());
}
#[test]
fn focus_arguments_and_data() {
    assert_eq!(
        focus_args(
            "review",
            Some("E6767SM"),
            Some(80153),
            Some("accept"),
            None,
            None
        )
        .unwrap(),
        [
            "focus", "review", "--code", "E6767SM", "--change", "80153", "--action", "accept"
        ]
    );
    assert_eq!(
        focus_args("cached", None, None, None, None, None).unwrap(),
        ["focus", "summary", "--cached"]
    );
    assert_eq!(
        focus_args(
            "changes",
            Some("E6767SM"),
            None,
            None,
            Some("E6767SM_SYS_A16_MP"),
            None
        )
        .unwrap(),
        [
            "focus",
            "changes",
            "--code",
            "E6767SM",
            "--branch",
            "E6767SM_SYS_A16_MP"
        ]
    );
    assert!(
        focus_args(
            "changes",
            Some("E6767SM"),
            None,
            None,
            Some("--config-root"),
            None
        )
        .is_none()
    );
    assert!(focus_args("changes", Some("E6767SM"), None, None, None, None).is_none());
    assert_eq!(
        focus_args(
            "annotate",
            None,
            None,
            None,
            None,
            Some(("jira", "PLV-1509"))
        )
        .unwrap(),
        [
            "focus", "annotate", "--source", "jira", "--item", "PLV-1509"
        ]
    );
    assert_eq!(
        focus_args(
            "annotate",
            Some("P6700LV"),
            None,
            None,
            None,
            Some(("jira", "PLV-1509"))
        )
        .unwrap()[6..],
        ["--code", "P6700LV"]
    );
    assert_eq!(
        focus_args(
            "history",
            None,
            None,
            None,
            None,
            Some(("meegle", "7146988888"))
        )
        .unwrap(),
        [
            "focus",
            "history",
            "--source",
            "meegle",
            "--item",
            "7146988888"
        ]
    );
    assert_eq!(
        focus_args("requirements_full", Some("P6700LV"), None, None, None, None)
            .unwrap()
            .last()
            .map(String::as_str),
        Some("--full")
    );
    assert!(focus_args("issues_fast", Some("P6700LV"), None, None, None, None).is_none());
    assert!(focus_args("annotate", None, None, None, None, Some(("svn", "1"))).is_none());
    assert!(focus_args("annotate", None, None, None, None, Some(("jira", "--x"))).is_none());
    assert_eq!(
        focus_args("builds_cached", Some("E6767SM"), None, None, None, None).unwrap(),
        ["focus", "builds", "--code", "E6767SM", "--cached"]
    );
    assert!(focus_args("suggest", Some("E6751 SM"), None, None, None, None).is_none());
    assert!(
        focus_args(
            "review",
            Some("E6767SM"),
            Some(1),
            Some("delete"),
            None,
            None
        )
        .is_none()
    );
    assert!(focus_args("rm", None, None, None, None, None).is_none());
    let summary = br#"{"ok":true,"data":{"summary":{"computed_at":"2026-11-01T11:00:00Z","models":[{"code":"E6767SM",
            "targets":{"severe_pct":95,"overall_pct":85,"fatal_max":0},"issues":342,"sources":["x"],
            "measures":{"severe_total":100,"severe_resolved":96,"overall_total":342,"overall_resolved":281,"fatal_open":0},
            "new_issues":24,"days":[0,1],"days_from":"2026-08-10","week_changes":13,"week_merged":12,
            "branches":[{"branch":"E6767SM_SYS_A16_MP","phase":"mp","total":314,"days":[0,1],"flagged":2}],
            "flagged":[{"number":80020,"subject":"s","project":"p","branch":"b","owner":"o","status":"NEW","created":"2026-10-30","rules":["feature_after_freeze"],"issues":[],"tracked":false}],
            "rule_counts":{"revert":17},"rules":{"revert":"alert"},"freeze":"2026-10-12","errors":[]}]}}}"#;
    let data = decode_focus(Some(0), summary).unwrap();
    let model = &data.summary.unwrap().models[0];
    assert_eq!((model.new_issues, model.flagged[0].number), (24, 80020));
    let builds = br#"{"ok":true,"data":{"builds":{"code":"E6767SM","folder":"Project_Daily_Build/Project_E6767SM",
            "computed_at":"2026-11-02T01:00:00Z","errors":[],"jobs":[{"job":"Project_Daily_Build/Project_E6767SM/E6767SM_IR",
            "name":"E6767SM_IR","phase":"ir","disabled":false,"builds":[{"number":17,"result":"SUCCESS","building":false,
            "timestamp":1790602588731,"duration":12330139,"estimated":9786231,"cause":"Started by user luyushan","node":"fast_build_server-97"}],
            "parameters":{"BRANCH":"E6767SM_SYS_A16_IR","CLEAN":true},"changes":[{"build":17,"name":"BRANCH","before":"a","after":"b"}]}]}}}"#;
    let builds = decode_focus(Some(0), builds).unwrap().builds.unwrap();
    assert_eq!(builds.jobs[0].builds[0].timestamp, 1790602588731);
    assert_eq!(builds.jobs[0].parameters["CLEAN"], serde_json::json!(true));
    let model: Result<FocusModel, _> = serde_json::from_value(serde_json::json!({
            "code":"E6751SM","targets":{"severe_pct":95,"overall_pct":85,"fatal_max":0},"token":"x"}));
    assert!(model.is_err());
}
#[test]
fn remap_arguments_and_service_health() {
    let answers = [("To Be Confirmed".to_string(), "Open".to_string())].into();
    assert_eq!(
        remap_args("E6751SM", "main", &answers).unwrap(),
        [
            "setup",
            "remap",
            "--project",
            "E6751SM",
            "--scope",
            "main",
            "--stage",
            "To Be Confirmed=Open",
            "--apply"
        ]
    );
    let bad = [("x".to_string(), "Deleted".to_string())].into();
    assert!(remap_args("E6751SM", "main", &bad).is_none());
    assert!(remap_args("E6751SM", "main", &Default::default()).is_none());
    let health = br#"{"ok":true,"data":{"mode":"live","state":"running","project":"E6751SM","scope":"main","revision":2,
            "gates":[{"gate_id":"severe","state":"satisfied","n":115,"v":111,"gap":0}],
            "unmapped":[{"field":"stage","value":"Communication","count":3}]}}"#;
    let health = decode_service(Some(0), health).unwrap();
    assert_eq!(health.project.as_deref(), Some("E6751SM"));
    assert_eq!(health.unmapped[0].count, 3);
}
#[test]
fn activation_arguments_and_results() {
    assert_eq!(
        activate_args("E6767SM", "main").unwrap(),
        [
            "setup",
            "activate",
            "--project",
            "E6767SM",
            "--scope",
            "main"
        ]
    );
    assert!(activate_args("../x", "main").is_none());
    assert!(activate_args("E6767SM", "--dry-run").is_none());
    let done = br#"{"ok":true,"data":{"dry_run":false,"health":{"mode":"live","state":"ready","last_commit":"2026-11-01T10:00:00Z","revision":2},"waited_secs":21}}"#;
    let activation = decode_activation(Some(0), done).unwrap();
    assert_eq!(activation.health.unwrap().revision, 2);
    let late = br#"{"ok":false,"data":null,"error":{"code":"ACTIVATE_TIMEOUT","message":"no snapshot within 300 s"}}"#;
    assert_eq!(
        decode_activation(Some(1), late).unwrap_err().code,
        "activate_timeout"
    );
}
#[test]
fn decodes_drafts_and_discovery_errors() {
    let draft = serde_json::json!({"schema_version":"1.0","ok":true,"data":{
            "template":{"id":"E6767SM-main","version":"2026-11-01","title":"E6767SM main","sha256":"ab"},
            "project":"E6767SM","scope":"main","mode":"live","directory":"C:\\cfg\\E6767SM\\main",
            "database":{"path":"x","exists":false},
            "files":[{"role":"scope_query","path":"C:\\cfg\\scope.json","action":"create"}],
            "applicable":true,"applied":false,"summary":"E6767SM · main：342 个问题。","count":342,
            "questions":[{"field":"stage","value":"Communication","count":0}],
            "mapping":{"severity_from":"优先级","severities":{"致命":"Fatal"},"stages":{},"dispositions":{}},
            "breakdown":{"severities":[{"value":"严重","count":100,"mapped":"Severe"}],
                "statuses":[{"value":"Communication","count":0}],"resolutions":[],
                "measures":{"severe_total":100,"severe_resolved":96,"overall_total":342,"overall_resolved":281,"fatal_open":0}}}});
    let draft = decode_draft(Some(0), &serde_json::to_vec(&draft).unwrap()).unwrap();
    assert_eq!(
        (draft.count, draft.questions[0].value.as_str()),
        (342, "Communication")
    );
    assert_eq!(draft.breakdown.measures.severe_resolved, 96);
    assert!(draft.breakdown.statuses[0].mapped.is_none());
    let signed_out = br#"{"ok":false,"data":null,"error":{"code":"SIGNED_OUT","message":"run meegle auth login"}}"#;
    assert_eq!(
        decode_discovery(Some(1), signed_out).unwrap_err().code,
        "signed_out"
    );
    let spaces = br#"{"ok":true,"data":{"spaces":[{"project_key":"6","simple_name":"efzuhc","name":"IPD"}]}}"#;
    assert_eq!(
        decode_discovery(Some(0), spaces).unwrap().spaces.unwrap()[0].name,
        "IPD"
    );
    let choices: Result<DraftChoices, _> = serde_json::from_value(serde_json::json!({
            "project":"P","scope_id":"main","scope":{"source":"jira","project":"PLV"},"argv":["x"],
            "phase":"TR4","severe_target_pct":95,"overall_target_pct":85,"fatal_max":0}));
    assert!(choices.is_err());
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
            include_bytes!(concat!("../../tests/fixtures/", $name))
        };
    }
    let plan = decode_plan(Some(0), fixture!("plan.json")).unwrap();
    assert!(plan.applicable && !plan.applied && plan.files.len() == 3);
    assert!(plan.files.iter().all(|f| f.action == "create"));
    let applied = decode_plan(Some(0), fixture!("applied.json")).unwrap();
    let check = applied.check.unwrap();
    assert!(applied.applied && check.valid && check.ready);
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
        include_bytes!("../../tests/fixtures/connections.json"),
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
    let ready = br#"{"ok":true,"data":{"config_root":"C:\\cfg","projects":[{"project":"P","scope":"S","directory":"d","check":{"valid":true,"ready":true}}]}}"#;
    assert!(
        decode_status(Some(0), ready).unwrap().projects[0]
            .check
            .ready
    );
    let other = br#"{"ok":false,"data":{},"error":{"code":"IMPORT_IO","message":"x"}}"#;
    assert_eq!(
        decode_status(Some(1), other).unwrap_err().code,
        "spm_invalid_response"
    );
}
#[test]
fn status_preserves_project_workbench_scope_and_targets() {
    let response = serde_json::json!({
        "ok": true,
        "data": {"config_root": "cfg", "projects": [{
            "project": "MODEL", "scope": "main", "directory": "cfg/MODEL/main",
            "check": {"valid": true, "ready": true},
            "workspace": {
                "code": "MODEL", "issues": [{"source": "jira", "project": "TEST"}],
                "branches": [], "jobs": [], "rules": {},
                "targets": {"severe_pct": 95, "overall_pct": 85, "fatal_max": 0},
                "accepted": [], "tracked": []
            }
        }]}
    });
    let parsed = decode_status(Some(0), &serde_json::to_vec(&response).unwrap()).unwrap();
    let seed = parsed.projects[0].workspace.as_ref().unwrap();
    assert_eq!(seed.code, "MODEL");
    assert_eq!(seed.issues.len(), 1);
    assert_eq!(seed.targets.severe_pct, 95);
}

#[test]
fn reads_only_literal_shim_targets() {
    let shim = "@echo off\r\n\"C:\\Users\\A B\\AppData\\Local\\Tinnci\\SPM\\versions\\0.1.0-abc\\spm.exe\" %*\r\n";
    assert_eq!(
        shim_target(shim).unwrap(),
        PathBuf::from("C:\\Users\\A B\\AppData\\Local\\Tinnci\\SPM\\versions\\0.1.0-abc\\spm.exe")
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

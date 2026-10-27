use spm_contracts::*;

#[test]
fn golden_ci_snapshot_round_trip_and_additive_defaults() {
    let data = include_str!("../fixtures/ci_watch/snapshot.json");
    let snapshot: CiWatchSnapshot = serde_json::from_str(data).unwrap();
    snapshot.validate().unwrap();
    let wire = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(snapshot, serde_json::from_str(&wire).unwrap());
    let empty: CiWatchSnapshot = serde_json::from_str("{}").unwrap();
    empty.validate().unwrap();
    for request in [
        Request::GetCiWatch(GetCiWatchRequest::default()),
        Request::SuggestCiWatch(SuggestCiWatchRequest {
            query: "orion/car".into(),
            mode: CiSuggestMode::Search,
            source: None,
        }),
        Request::UpdateCiWatchlist(UpdateCiWatchlistRequest::AddJob {
            job: "folder/job".into(),
        }),
    ] {
        request.validate().unwrap();
        assert_eq!(
            request,
            serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap()
        );
    }
}
#[test]
fn negative_ci_fixtures_fail_validation() {
    for data in [
        include_str!("../fixtures/ci_watch/invalid-history-bound.json"),
        include_str!("../fixtures/ci_watch/invalid-changes-bound.json"),
        include_str!("../fixtures/ci_watch/invalid-wrong-patchset.json"),
        include_str!("../fixtures/ci_watch/invalid-queued-number.json"),
        include_str!("../fixtures/ci_watch/invalid-raw-navigation.json"),
        include_str!("../fixtures/ci_watch/invalid-pipeline-order.json"),
        include_str!("../fixtures/ci_watch/invalid-release-train-role.json"),
        include_str!("../fixtures/ci_watch/invalid-log-window-bytes.json"),
        include_str!("../fixtures/ci_watch/invalid-triage-not-failure.json"),
    ] {
        assert!(serde_json::from_str::<CiWatchSnapshot>(data)
            .unwrap()
            .validate()
            .is_err());
    }
}
#[test]
fn watchlist_validation_and_atomic_local_edits() {
    let mut list = CiWatchlist::default();
    for job in ["/a", "a/", "a//b", "a/../b", "a%2Fb", "a?build", "a\\b"] {
        assert!(list
            .apply(UpdateCiWatchlistRequest::AddJob { job: job.into() })
            .is_err());
    }
    for (p, b) in [
        ("p", "main owner:self"),
        ("p", "../main"),
        ("p:x", "main"),
        ("p", "main.lock"),
    ] {
        assert!(validate_ci_branch(p, b).is_err());
    }
    list.apply(UpdateCiWatchlistRequest::AddJob {
        job: "folder/a b".into(),
    })
    .unwrap();
    let prior = list.clone();
    assert!(list
        .apply(UpdateCiWatchlistRequest::AddJob {
            job: "folder/a b".into()
        })
        .is_err());
    assert_eq!(list, prior);
    list.apply(UpdateCiWatchlistRequest::Toggle {
        id: list.jobs[0].id.clone(),
        flag: CiWatchFlag::OnlyMine,
        value: true,
    })
    .unwrap();
    assert!(list.jobs[0].only_mine);
    assert!(list
        .apply(UpdateCiWatchlistRequest::Toggle {
            id: list.jobs[0].id.clone(),
            flag: CiWatchFlag::IncludeMerged,
            value: true
        })
        .is_err());
    list.apply(UpdateCiWatchlistRequest::Remove {
        id: list.jobs[0].id.clone(),
    })
    .unwrap();
    assert!(list.jobs.is_empty());
    for i in 0..MAX_CI_JOBS {
        list.apply(UpdateCiWatchlistRequest::AddJob {
            job: format!("job-{i}"),
        })
        .unwrap();
    }
    assert!(list
        .apply(UpdateCiWatchlistRequest::AddJob {
            job: "overflow".into()
        })
        .is_err());
}

#[test]
fn pipeline_release_train_and_role_contract() {
    let snapshot: CiWatchSnapshot =
        serde_json::from_str(include_str!("../fixtures/ci_watch/snapshot.json")).unwrap();
    snapshot.validate().unwrap();
    assert_eq!(snapshot.release_train.len(), 2);
    assert_eq!(snapshot.kpis.watched_failing, 1);
    let change = &snapshot.branches[0].changes[0];
    assert_eq!(change.pipeline.len(), CiStageKind::ALL.len());
    assert_eq!(change.pipeline[1].state, CiStageState::Failed);
    // Additive defaults: an older payload without the new fields still validates.
    let old: CiWatchSnapshot =
        serde_json::from_str(r#"{"jobs":[{"id":"job:a","job":"a"}]}"#).unwrap();
    old.validate().unwrap();
    assert_eq!(old.jobs[0].role, CiJobRole::Other);
    assert!(old.release_train.is_empty());
    assert_eq!(old.kpis, CiKpis::default());
    // Role edits are local and only apply to a job.
    let mut list = CiWatchlist::default();
    list.apply(UpdateCiWatchlistRequest::AddJob {
        job: "orion/a".into(),
    })
    .unwrap();
    let id = list.jobs[0].id.clone();
    list.apply(UpdateCiWatchlistRequest::SetRole {
        id: id.clone(),
        role: CiJobRole::Verify,
    })
    .unwrap();
    assert_eq!(list.jobs[0].role, CiJobRole::Verify);
    assert!(list
        .apply(UpdateCiWatchlistRequest::SetRole {
            id: "branch:x:y".into(),
            role: CiJobRole::Daily
        })
        .is_err());
    let mut snap = snapshot.clone();
    snap.apply_watchlist(&list);
    assert_eq!(snap.watchlist().jobs[0].role, CiJobRole::Verify);
}

#[test]
fn seed_requests_take_no_query_and_suggestion_counts_are_additive() {
    use spm_contracts::{CiSuggestMode, CiSuggestion, CiSuggestionKind, SuggestCiWatchRequest};
    let seeds = SuggestCiWatchRequest {
        query: String::new(),
        mode: CiSuggestMode::Seeds,
        source: None,
    };
    assert!(seeds.validate().is_ok());
    let with_query = SuggestCiWatchRequest {
        query: "orion".into(),
        mode: CiSuggestMode::Seeds,
        source: None,
    };
    assert!(with_query.validate().is_err());
    // An older client omits the mode and the count; both default.
    let old: SuggestCiWatchRequest = serde_json::from_str(r#"{"query":"orion"}"#).unwrap();
    assert_eq!(old.mode, CiSuggestMode::Search);
    let suggestion: CiSuggestion =
        serde_json::from_str(r#"{"kind":"job","value":"orion/a","label":"orion/a"}"#).unwrap();
    assert_eq!(suggestion.kind, CiSuggestionKind::Job);
    assert_eq!(suggestion.count, None);
}

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

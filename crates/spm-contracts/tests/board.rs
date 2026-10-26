use spm_contracts::*;

const AFTER_REPLY: &str = include_str!("../fixtures/board/orion-after-reply.json");
const AFTER_MEEGLE: &str = include_str!("../fixtures/board/orion-after-meegle.json");
const R102_EVIDENCE: &str = include_str!("../fixtures/board/orion-r102-evidence.json");

fn board(text: &str) -> BoardSnapshot {
    let board: BoardSnapshot = serde_json::from_str(text).unwrap();
    board.validate().unwrap();
    board
}

fn assignee<'a>(board: &'a BoardSnapshot, number: &str) -> Option<&'a Relationship> {
    board.card(number).unwrap().assignee()
}

fn person(relationship: &Relationship) -> &str {
    relationship.person.person_id.as_deref().unwrap()
}

#[test]
fn fixtures_round_trip_without_loss() {
    for text in [AFTER_REPLY, AFTER_MEEGLE] {
        let parsed: BoardSnapshot = serde_json::from_str(text).unwrap();
        let original: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            canonical_json(&parsed).unwrap(),
            canonical_json(&original).unwrap()
        );
    }
    let evidence: CardEvidenceResponse = serde_json::from_str(R102_EVIDENCE).unwrap();
    evidence.validate().unwrap();
    let original: serde_json::Value = serde_json::from_str(R102_EVIDENCE).unwrap();
    assert_eq!(
        canonical_json(&evidence).unwrap(),
        canonical_json(&original).unwrap()
    );
}

#[test]
fn numbered_reply_resolves_three_tasks_and_leaves_task_two_open() {
    let board = board(AFTER_REPLY);
    assert_eq!(board.cards.len(), 4);
    for (number, expected) in [
        ("ORION-R-101", "p:casey"),
        ("ORION-R-103", "p:gale"),
        ("ORION-R-104", "p:morgan"),
    ] {
        let assignment = assignee(&board, number).unwrap();
        assert_eq!(person(assignment), expected);
        assert_eq!(assignment.basis, RelationshipBasis::ExplicitStatement);
        let statement = assignment.statement.as_ref().unwrap();
        assert_eq!(statement.author.person_id.as_deref(), Some("p:gale"));
        assert_eq!(statement.authority, Authority::Unknown);
        assert_eq!(assignment.evidence, ["ev:l-request", "ev:l-reply"]);
    }
    let r102 = board.card("ORION-R-102").unwrap();
    assert!(r102.assignee().is_none());
    assert_eq!(r102.related(RelationshipKind::Assigned).count(), 0);
    let contact = r102
        .related(RelationshipKind::ImplementationContact)
        .next()
        .unwrap();
    assert_eq!(person(contact), "p:morgan");
    assert_eq!(contact.basis, RelationshipBasis::Derived);
}

#[test]
fn historical_contribution_stays_separate_from_assignment() {
    let board = board(AFTER_REPLY);
    let r101 = board.card("ORION-R-101").unwrap();
    assert_eq!(person(r101.assignee().unwrap()), "p:casey");
    let historical: Vec<_> = r101
        .related(RelationshipKind::HistoricalContributor)
        .map(person)
        .collect();
    assert_eq!(historical, ["p:morgan"]);
    assert_eq!(
        r101.related(RelationshipKind::ChangeOwner)
            .map(person)
            .collect::<Vec<_>>(),
        ["p:morgan"]
    );
    // Reviewed and submitted by Gale; neither makes Gale the assignee.
    assert!(r101
        .related(RelationshipKind::ReviewerListed)
        .chain(r101.related(RelationshipKind::Submitter))
        .all(|r| person(r) == "p:gale"));
    // A candidate merged change does not start the requirement.
    assert_eq!(r101.column, SummaryColumn::Todo);
    assert_eq!(r101.progress.code.merged, 1);
    assert_eq!(r101.progress.code.relation, RelationState::Candidate);
}

#[test]
fn copied_vote_is_scoped_to_its_patch_set() {
    let board = board(AFTER_REPLY);
    let r101 = board.card("ORION-R-101").unwrap();
    let votes: Vec<_> = r101
        .related(RelationshipKind::Approval)
        .map(|r| {
            let approval = r.approval.as_ref().unwrap();
            (approval.patchset, approval.value, approval.carried_over)
        })
        .collect();
    assert_eq!(
        votes,
        [(1, 2, CarriedOver::No), (2, 2, CarriedOver::Inferred)]
    );
}

#[test]
fn issue_assignment_becomes_current_and_keeps_the_original_statement() {
    let before = board(AFTER_REPLY);
    let after = board(AFTER_MEEGLE);
    assert!(after.revision > before.revision);
    for number in ["ORION-R-101", "ORION-R-102"] {
        let assignment = assignee(&after, number).unwrap();
        assert_eq!(person(assignment), "p:casey");
        assert_eq!(assignment.basis, RelationshipBasis::IssueField);
        let card = after.card(number).unwrap();
        assert_eq!(card.column, SummaryColumn::Doing);
        assert_eq!(card.stage.stage, BoardStage::Implementation);
        assert_eq!(card.stage.basis, StageBasis::WorkflowNode);
    }
    let r101 = after.card("ORION-R-101").unwrap();
    let lark = r101.relationship("rel-101-lark").unwrap();
    assert_eq!(lark.status, RelationshipStatus::Current);
    assert_eq!(
        lark,
        before
            .card("ORION-R-101")
            .unwrap()
            .relationship("rel-101-lark")
            .unwrap()
    );
    assert!(lark.source_time < r101.assignee().unwrap().source_time);

    let r102 = after.card("ORION-R-102").unwrap();
    assert_eq!(
        r102.related(RelationshipKind::ImplementationContact)
            .map(person)
            .collect::<Vec<_>>(),
        ["p:morgan"]
    );
    for number in ["ORION-R-103", "ORION-R-104"] {
        let (old, new) = (before.card(number).unwrap(), after.card(number).unwrap());
        assert_eq!(old.relationships, new.relationships);
        assert_eq!(old.current_assignee, new.current_assignee);
        assert_eq!(new.column, SummaryColumn::Todo);
    }
}

#[test]
fn lazy_evidence_explains_the_unassigned_task() {
    let after = board(AFTER_MEEGLE);
    let evidence: CardEvidenceResponse = serde_json::from_str(R102_EVIDENCE).unwrap();
    let card = after.card("ORION-R-102").unwrap();
    assert_eq!(evidence.card_id, card.card_id);
    assert_eq!(evidence.revision, after.revision);
    assert_eq!(evidence.items.len() as u32, card.evidence_count);
    for item in &evidence.items {
        for id in &item.relationship_ids {
            assert!(card.relationship(id).is_some(), "{id}");
        }
    }
}

fn rejects(text: &str, mutate: impl FnOnce(&mut BoardSnapshot)) {
    let mut board: BoardSnapshot = serde_json::from_str(text).unwrap();
    mutate(&mut board);
    assert!(board.validate().is_err());
}

fn card_mut<'a>(board: &'a mut BoardSnapshot, number: &str) -> &'a mut BoardCard {
    board
        .cards
        .iter_mut()
        .find(|c| c.business_number.as_deref() == Some(number))
        .unwrap()
}

#[test]
fn a_merge_or_build_cannot_finish_a_card() {
    rejects(AFTER_MEEGLE, |board| {
        let card = card_mut(board, "ORION-R-101");
        card.column = SummaryColumn::Done;
        card.stage.stage = BoardStage::Done;
        card.stage.basis = StageBasis::ActivityEvidence;
        card.stage.overlays.insert(StageOverlay::WorkflowLag);
    });
}

#[test]
fn activity_ahead_of_the_workflow_is_flagged() {
    rejects(AFTER_MEEGLE, |board| {
        card_mut(board, "ORION-R-101").stage.basis = StageBasis::ActivityEvidence;
    });
}

#[test]
fn column_must_follow_the_stage() {
    rejects(AFTER_MEEGLE, |board| {
        card_mut(board, "ORION-R-103").column = SummaryColumn::Doing;
    });
}

#[test]
fn a_historical_contributor_cannot_fill_the_assignee_slot() {
    rejects(AFTER_REPLY, |board| {
        card_mut(board, "ORION-R-102").current_assignee = Some("rel-102-history-morgan".into());
    });
    rejects(AFTER_REPLY, |board| {
        let card = card_mut(board, "ORION-R-102");
        let contact = card
            .relationships
            .iter_mut()
            .find(|r| r.kind == RelationshipKind::ImplementationContact)
            .unwrap();
        contact.basis = RelationshipBasis::IssueField;
        contact.derivation = None;
    });
}

#[test]
fn an_assignment_statement_needs_the_task_definition() {
    rejects(AFTER_REPLY, |board| {
        let card = card_mut(board, "ORION-R-103");
        card.relationships[0].evidence = vec!["ev:l-reply".into()];
    });
}

#[test]
fn a_superseded_assignment_cannot_stay_current() {
    rejects(AFTER_REPLY, |board| {
        let card = card_mut(board, "ORION-R-104");
        card.relationships[0].status = RelationshipStatus::Superseded;
    });
}

#[test]
fn conflicting_rows_need_a_visible_conflict() {
    rejects(AFTER_MEEGLE, |board| {
        let card = card_mut(board, "ORION-R-101");
        let lark = card
            .relationships
            .iter_mut()
            .find(|r| r.id == "rel-101-lark")
            .unwrap();
        lark.status = RelationshipStatus::Conflicting;
    });
}

#[test]
fn blocked_work_lists_its_blocker_and_stays_in_progress() {
    rejects(AFTER_MEEGLE, |board| {
        card_mut(board, "ORION-R-101")
            .stage
            .overlays
            .insert(StageOverlay::Blocked);
    });
    rejects(AFTER_MEEGLE, |board| {
        let card = card_mut(board, "ORION-R-103");
        card.stage.overlays.insert(StageOverlay::Blocked);
        card.blockers.push(Blocker {
            reason: "Waiting for the regional feature decision".into(),
            dependency: None,
            contact: None,
            since: None,
            expected_at: None,
            evidence: vec!["ev:l-reply".into()],
        });
    });
}

#[test]
fn one_requirement_has_one_card() {
    rejects(AFTER_REPLY, |board| {
        let duplicate = board.cards[0].clone();
        board.cards.push(duplicate);
    });
    rejects(AFTER_REPLY, |board| {
        let mut copy = board.cards[0].clone();
        copy.card_id = RecordId::new("meegle:acme/orion/story/copy").unwrap();
        board.cards.push(copy);
    });
}

#[test]
fn complete_totals_agree_with_cards_and_coverage() {
    rejects(AFTER_MEEGLE, |board| board.lanes[0].counts.doing = 3);
    rejects(AFTER_MEEGLE, |board| {
        board.sources[1].coverage = CoverageState::Partial;
    });
    let mut partial: BoardSnapshot = serde_json::from_str(AFTER_MEEGLE).unwrap();
    partial.sources[1].coverage = CoverageState::Partial;
    partial.totals_complete = false;
    partial.lanes[0].counts.doing = 9;
    partial.validate().unwrap();
}

#[test]
fn board_event_identity_matches_its_envelope() {
    let board = board(AFTER_MEEGLE);
    let mut envelope = Envelope {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        daemon_session: Some(board.daemon_session),
        request_id: None,
        subscription_id: Some(SubscriptionId::new()),
        revision: Some(board.revision),
        body: Body::Event(Event::Board(board)),
    };
    envelope.validate(Direction::ServerToClient).unwrap();
    envelope.revision = Some(Revision(1));
    assert!(envelope.validate(Direction::ServerToClient).is_err());
}

#[test]
fn evidence_requests_are_bounded_and_revision_pinned() {
    let request = |revision, page_size| {
        Request::GetCardEvidence(GetCardEvidenceRequest {
            subscription_id: SubscriptionId::new(),
            card_id: RecordId::new("meegle:acme/orion/story/88232").unwrap(),
            revision: Revision(revision),
            cursor: None,
            page_size,
        })
    };
    assert_eq!(request(57, 50).method(), RpcMethod::GetCardEvidence);
    request(57, 50).validate().unwrap();
    assert!(request(0, 50).validate().is_err());
    assert!(request(57, MAX_PAGE_SIZE + 1).validate().is_err());
}

#[test]
fn evidence_is_time_ordered() {
    let mut evidence: CardEvidenceResponse = serde_json::from_str(R102_EVIDENCE).unwrap();
    evidence.items.swap(0, 3);
    assert!(evidence.validate().is_err());
}

#[test]
fn delivery_fields_default_for_older_payloads() {
    let mut value: serde_json::Value = serde_json::from_str(AFTER_REPLY).unwrap();
    value.as_object_mut().unwrap().remove("milestones");
    for card in value["cards"].as_array_mut().unwrap() {
        for key in ["schedule", "stage_history", "event_marks"] {
            card.as_object_mut().unwrap().remove(key);
        }
    }
    let parsed: BoardSnapshot = serde_json::from_value(value).unwrap();
    parsed.validate().unwrap();
    assert!(parsed.milestones.is_empty());
    assert!(parsed
        .cards
        .iter()
        .all(|c| c.schedule.is_none() && c.stage_history.is_empty() && c.event_marks.is_empty()));
}

#[test]
fn delivery_spans_are_ordered_nonoverlapping_and_open_only_at_end() {
    for text in [
        include_str!("../fixtures/invalid/board-stage-overlap.json"),
        include_str!("../fixtures/invalid/board-event-marks-13.json"),
    ] {
        let parsed: BoardSnapshot = serde_json::from_str(text).unwrap();
        assert!(parsed.validate().is_err());
    }
    let mut b = board(AFTER_REPLY);
    let c = &mut b.cards[0];
    let mut span = c.stage_history[0].clone();
    span.to = Some(span.from + chrono::Duration::days(2));
    c.stage_history = vec![span.clone(), span.clone()];
    assert!(c.validate().is_err());
    c.stage_history[1].from += chrono::Duration::days(2);
    c.stage_history[1].to = None;
    c.validate().unwrap();
    c.stage_history[0].to = Some(c.stage_history[0].from);
    assert!(c.validate().is_err());
    c.stage_history.clear();
    c.event_marks = vec![
        EventMark {
            at: span.from,
            system: SourceSystem::Lark,
            evidence_id: "e".into()
        };
        12
    ];
    c.validate().unwrap();
    c.event_marks[0].evidence_id.clear();
    assert!(c.validate().is_err());
}

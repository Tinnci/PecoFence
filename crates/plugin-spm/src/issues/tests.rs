use super::text::Lang;
use super::*;

const ZH: Lang = Lang { zh: true };
const EN: Lang = Lang { zh: false };

fn detail() -> IssueDetail {
    serde_json::from_str(include_str!(
        "../../../spm-contracts/fixtures/issues/get-issue-r102.json"
    ))
    .unwrap()
}

fn readiness() -> GateReadiness {
    serde_json::from_str(include_str!(
        "../../../spm-contracts/fixtures/issues/gate-readiness-tr5.json"
    ))
    .unwrap()
}

fn what_if() -> GateWhatIf {
    serde_json::from_str(include_str!(
        "../../../spm-contracts/fixtures/issues/gate-whatif-tr5.json"
    ))
    .unwrap()
}

#[test]
fn queue_chips_keep_a_fixed_order_and_zero_fill_missing_queues() {
    let counts = vec![QueueCount {
        queue: IssueQueue::NeedsMyReply,
        count: 2,
    }];
    let chips = queue_chips(ZH, &counts, GateKind::Tr5);
    assert_eq!(chips.len(), 5);
    assert_eq!(chips[0].count, 2);
    assert!(chips[1..].iter().all(|c| c.count == 0));
    assert_eq!(chips[2].label, "阻塞 TR5");
    assert_eq!(
        queue_chips(EN, &counts, GateKind::Tr5)[2].label,
        "Blocks TR5"
    );
}

#[test]
fn type_tags_and_severity_words_are_text_not_colour() {
    assert_eq!(EN.kind(IssueKind::Dts), "DTS");
    assert_eq!(ZH.severity(DefectSeverity::Major), "严重");
    assert_eq!(EN.severity(DefectSeverity::Fatal), "Fatal");
}

#[test]
fn phase_strip_marks_the_current_gate_with_its_word() {
    let strip = phase_strip(ZH, &readiness().track);
    let current: Vec<_> = strip.iter().filter(|c| c.current).collect();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].gate, "TR5");
    assert_eq!(current[0].state, "当前");
    assert_eq!(strip.iter().filter(|c| c.state == "已过").count(), 6);
}

#[test]
fn readiness_bar_reports_di_threshold_criteria_and_verdict_as_words() {
    let view = readiness_view(ZH, &readiness());
    assert_eq!(view.gate, "TR5");
    assert_eq!(view.di_text, "DI 14.5 · ≤ 10");
    assert_eq!(view.verdict, "未就绪");
    assert!(view.bar_threshold > 0.0 && view.bar_threshold < 1.0);
    assert_eq!(view.criteria.len(), 5);
    assert!(view.criteria.iter().all(|c| !c.pass));
    assert_eq!(view.criteria[0].pass_word, "未通过");
}

#[test]
fn what_if_lists_each_step_with_the_di_after_it() {
    let w = what_if();
    let view = what_if_view(EN, &w);
    assert_eq!(view.lines.len(), w.steps.len());
    assert_eq!(view.start, "DI 14.5");
    assert_eq!(view.summary, "Reaches the threshold");
    assert_eq!(view.lines[0].index, 1);
    assert!(view.lines.iter().all(|l| l.di_after.starts_with("DI ")));
    assert!(view.unknown_effort_note.is_some());
}

#[test]
fn proposal_actions_map_to_one_verdict_each() {
    let project = ProjectId::new("orion-ir").unwrap();
    let proposal = &detail()
        .proposals
        .clone()
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("fixture has a proposal"));
    let confirm = review_request(&project, proposal, ProposalAction::ConfirmMeegle).unwrap();
    assert_eq!(confirm.verdict, AssignmentVerdict::Confirm);
    assert_eq!(
        review_request(&project, proposal, ProposalAction::LocalOnly)
            .unwrap()
            .verdict,
        AssignmentVerdict::LocalOnly
    );
    assert_eq!(
        review_request(&project, proposal, ProposalAction::NotThis)
            .unwrap()
            .verdict,
        AssignmentVerdict::Reject
    );
    assert!(review_request(&project, proposal, ProposalAction::ReplyInLark).is_none());
    assert_eq!(
        confirmation_note(ZH, false),
        Some("Meegle 更新暂不可用；确认已在本地记录。")
    );
    assert_eq!(confirmation_note(ZH, true), None);
}

#[test]
fn cursor_clamps_at_both_ends_and_is_empty_for_empty_lists() {
    let c = Cursor::new(3);
    assert_eq!(c.index(), Some(0));
    assert_eq!(c.up().index(), Some(0));
    assert_eq!(c.down().down().down().index(), Some(2));
    assert_eq!(c.end().index(), Some(2));
    assert_eq!(c.end().home().index(), Some(0));
    assert_eq!(c.end().resized(1).index(), Some(0));
    assert_eq!(Cursor::new(0).down().index(), None);
}

#[test]
fn suggestion_strip_and_dismissed_note_come_from_the_detail() {
    let d = detail();
    let strip = suggestion_strip(ZH, &d).unwrap();
    assert!(strip.starts_with("建议 "));
    assert!(strip.ends_with(&format!("查看 {} 条", d.suggestions.len())));
    assert_eq!(
        dismissed_note(ZH, 1).as_deref(),
        Some("已忽略 1 条不再出现")
    );
    assert_eq!(dismissed_note(ZH, 0), None);
}

#[test]
fn highlight_splits_snippets_and_drops_invalid_spans() {
    let text = "R-102 reply 设计";
    let lex = [
        TextSpan { start: 0, end: 5 },
        TextSpan { start: 6, end: 11 },
    ];
    let segs = highlight(text, &lex, None);
    assert_eq!(
        segs[0],
        Segment {
            text: "R-102".into(),
            mark: Mark::Lexical
        }
    );
    assert_eq!(segs[1].mark, Mark::Plain);
    assert_eq!(segs[2].text, "reply");
    // A span inside a multi-byte character is dropped, not split.
    let bad = [TextSpan { start: 17, end: 18 }];
    assert_eq!(highlight(text, &bad, None).len(), 1);
    let semantic = TextSpan { start: 12, end: 18 };
    let segs = highlight(text, &[], Some(&semantic));
    assert_eq!(segs.last().unwrap().mark, Mark::Semantic);
}

#[test]
fn fallback_notice_says_lexical_and_source_text_when_required() {
    let fallback = SearchFallback {
        reason: FallbackReason::NoEligibleEmbeddingCache,
        requires_source_text: true,
    };
    let notice = fallback_notice(EN, &fallback).unwrap();
    assert!(notice.contains("lexical") || notice.contains("Semantic"));
    assert!(notice.contains("Source text required"));
}

#[test]
fn similarity_is_labelled_as_not_a_confidence() {
    assert!(similarity_text(ZH, 0.8234).starts_with("0.82 · "));
    assert!(similarity_text(ZH, 0.8234).contains("不是置信度"));
}

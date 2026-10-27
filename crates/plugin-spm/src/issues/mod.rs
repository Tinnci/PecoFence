//! IPD issue tracker presentation. SPM computes queues, gate readiness, DI, the what-if path
//! and assignment proposals. This module turns those responses into the labels, rows, actions
//! and key moves the 问题 workspace draws. Status is always a word, never colour alone, and
//! nothing here writes to Lark, Meegle, Gerrit or Jenkins.
pub mod layout;
pub mod panel;
pub mod text;

pub use panel::{
    Config, DemoBundle, IssuesPlugin, PROVIDER_ID, Panel, WORKSPACE_WIDTH, demo_bundle,
    demo_config, live_config,
};

use spm_contracts::*;
use text::Lang;

const QUEUES: [IssueQueue; 5] = [
    IssueQueue::NeedsMyReply,
    IssueQueue::AwaitingAssignmentConfirmation,
    IssueQueue::BlocksNextGate,
    IssueQueue::LegacyPending,
    IssueQueue::SourceConflict,
];

pub fn gate_name(gate: GateKind) -> &'static str {
    match gate {
        GateKind::Cdcp => "CDCP",
        GateKind::Pdcp => "PDCP",
        GateKind::Tr1 => "TR1",
        GateKind::Tr2 => "TR2",
        GateKind::Tr3 => "TR3",
        GateKind::Tr4 => "TR4",
        GateKind::Tr5 => "TR5",
        GateKind::Tr6 => "TR6",
        GateKind::Adcp => "ADCP",
    }
}

/// A queue chip. Queues the response does not list show a count of zero, never a gap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueChip {
    pub queue: IssueQueue,
    pub label: String,
    pub count: u32,
}

pub fn queue_chips(lang: Lang, counts: &[QueueCount], next_gate: GateKind) -> Vec<QueueChip> {
    let gate = gate_name(next_gate);
    QUEUES
        .iter()
        .map(|&queue| QueueChip {
            queue,
            label: lang.queue(queue, gate),
            count: counts
                .iter()
                .find(|c| c.queue == queue)
                .map_or(0, |c| c.count),
        })
        .collect()
}

/// One cell of the IPD phase strip. The current gate is marked by its word, not only its colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StripCell {
    pub gate: &'static str,
    pub phase: &'static str,
    pub state: &'static str,
    pub current: bool,
    pub due: Option<String>,
}

pub fn phase_strip(lang: Lang, track: &[GateTrackEntry]) -> Vec<StripCell> {
    track
        .iter()
        .map(|entry| StripCell {
            gate: gate_name(entry.gate),
            phase: lang.phase(entry.phase),
            state: lang.track(entry.state),
            current: entry.state == GateTrackState::Current,
            due: entry.due_at.map(|d| d.format("%Y-%m-%d").to_string()),
        })
        .collect()
}

/// Numbers print with one decimal only when they have a fraction.
pub fn number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CriterionLine {
    pub kind: CriterionKind,
    pub label: &'static str,
    pub value: String,
    pub threshold: String,
    pub pass: bool,
    pub pass_word: &'static str,
}

/// The TR5 readiness bar. `bar_value` and `bar_threshold` are fractions of the larger of the
/// two, so the threshold mark always sits inside the bar.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadinessView {
    pub gate: &'static str,
    pub di: f64,
    pub threshold: f64,
    pub di_text: String,
    pub bar_value: f64,
    pub bar_threshold: f64,
    pub verdict: &'static str,
    pub criteria: Vec<CriterionLine>,
}

pub fn readiness_view(lang: Lang, readiness: &GateReadiness) -> ReadinessView {
    let threshold = readiness
        .criteria
        .iter()
        .find(|c| c.kind == CriterionKind::DiThreshold)
        .map_or(0.0, |c| c.threshold);
    let di = readiness.di.value;
    let scale = di.max(threshold).max(f64::MIN_POSITIVE);
    let gate = gate_name(readiness.next_gate);
    ReadinessView {
        gate,
        di,
        threshold,
        di_text: format!("DI {} · ≤ {}", number(di), number(threshold)),
        bar_value: (di / scale).clamp(0.0, 1.0),
        bar_threshold: (threshold / scale).clamp(0.0, 1.0),
        verdict: if readiness.ready {
            lang.t("Ready for the gate")
        } else {
            lang.t("Not ready")
        },
        criteria: readiness
            .criteria
            .iter()
            .map(|c| CriterionLine {
                kind: c.kind,
                label: lang.criterion(c.kind),
                value: number(c.value),
                threshold: number(c.threshold),
                pass: c.pass,
                pass_word: lang.t(if c.pass { "Pass" } else { "Fail" }),
            })
            .collect(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhatIfLine {
    pub index: usize,
    pub item: String,
    pub severity: &'static str,
    pub reduction: String,
    pub effort: String,
    pub di_after: String,
}

/// The TR5 what-if as a list of steps; each line carries the DI after that step.
#[derive(Clone, Debug, PartialEq)]
pub struct WhatIfView {
    pub start: String,
    pub threshold: String,
    pub summary: &'static str,
    pub lines: Vec<WhatIfLine>,
    pub unknown_effort_note: Option<String>,
}

pub fn what_if_view(lang: Lang, what_if: &GateWhatIf) -> WhatIfView {
    let lines = what_if
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| WhatIfLine {
            index: i + 1,
            item: step.item_id.clone(),
            severity: lang.severity(step.severity),
            reduction: format!("−{}", number(step.di_reduction)),
            effort: match step.effort_hours {
                Some(h) if lang.zh => format!("{} 小时", number(h)),
                Some(h) => format!("{} h", number(h)),
                None => lang.t("Unknown effort, listed last").into(),
            },
            di_after: format!("DI {}", number(step.di_after)),
        })
        .collect();
    WhatIfView {
        start: format!("DI {}", number(what_if.start_di)),
        threshold: format!("≤ {}", number(what_if.threshold)),
        summary: lang.t(if what_if.reaches_threshold {
            "Reaches the threshold"
        } else {
            "Does not reach the threshold"
        }),
        lines,
        unknown_effort_note: (what_if.unknown_effort > 0).then(|| {
            if lang.zh {
                format!("{} 条未估工时，已排在最后", what_if.unknown_effort)
            } else {
                format!(
                    "{} unknown-effort items, listed last",
                    what_if.unknown_effort
                )
            }
        }),
    }
}

/// The actions on an assignment proposal. Each maps to one `ReviewAssignment` verdict, except
/// "Reply in Lark", which only opens the conversation through navigation origins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalAction {
    ConfirmMeegle,
    LocalOnly,
    NotThis,
    ReplyInLark,
}

pub const PROPOSAL_ACTIONS: [ProposalAction; 4] = [
    ProposalAction::ConfirmMeegle,
    ProposalAction::LocalOnly,
    ProposalAction::NotThis,
    ProposalAction::ReplyInLark,
];

impl ProposalAction {
    pub fn label(self, lang: Lang) -> &'static str {
        lang.t(match self {
            Self::ConfirmMeegle => "Confirm and update Meegle",
            Self::LocalOnly => "Record locally only",
            Self::NotThis => "Not this",
            Self::ReplyInLark => "Reply in Lark",
        })
    }

    /// The verdict this action sends; `None` for actions that never reach `ReviewAssignment`.
    pub fn verdict(self) -> Option<AssignmentVerdict> {
        match self {
            Self::ConfirmMeegle => Some(AssignmentVerdict::Confirm),
            Self::LocalOnly => Some(AssignmentVerdict::LocalOnly),
            Self::NotThis => Some(AssignmentVerdict::Reject),
            Self::ReplyInLark => None,
        }
    }
}

/// The request for an action, or `None` when the action only navigates.
pub fn review_request(
    project: &ProjectId,
    proposal: &AssignmentProposal,
    action: ProposalAction,
) -> Option<ReviewAssignmentRequest> {
    action.verdict().map(|verdict| ReviewAssignmentRequest {
        project_id: project.clone(),
        proposal_id: proposal.id.clone(),
        verdict,
        note: None,
    })
}

/// Shown after a confirmation when the Meegle confirmation window is not there yet.
pub fn confirmation_note(lang: Lang, meegle_update_available: bool) -> Option<&'static str> {
    (!meegle_update_available).then(|| {
        lang.t("Meegle update is not available yet; the confirmation is recorded locally.")
    })
}

/// A roving cursor for the lists. Arrow keys move it, Home and End jump, and it never leaves
/// the list: an empty list has no index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    index: usize,
    len: usize,
}

impl Cursor {
    pub fn new(len: usize) -> Self {
        Self { index: 0, len }
    }

    pub fn index(self) -> Option<usize> {
        (self.len > 0).then_some(self.index)
    }

    /// Keeps the cursor on a valid row when the list changes length.
    pub fn resized(self, len: usize) -> Self {
        Self {
            index: self.index.min(len.saturating_sub(1)),
            len,
        }
    }

    pub fn down(self) -> Self {
        Self {
            index: (self.index + 1).min(self.len.saturating_sub(1)),
            ..self
        }
    }

    pub fn up(self) -> Self {
        Self {
            index: self.index.saturating_sub(1),
            ..self
        }
    }

    pub fn home(self) -> Self {
        Self { index: 0, ..self }
    }

    pub fn end(self) -> Self {
        Self {
            index: self.len.saturating_sub(1),
            ..self
        }
    }
}

/// The one-line suggestion strip for an open issue. `None` when there is nothing to suggest.
pub fn suggestion_strip(lang: Lang, detail: &IssueDetail) -> Option<String> {
    let first = detail.suggestions.first()?;
    let n = detail.suggestions.len();
    Some(format!(
        "{} {} · {} {n} {}",
        lang.t("Suggested"),
        first.title,
        lang.t("See"),
        lang.t("items")
    ))
}

/// "已忽略 N 条不再出现" from the judgment memory; `None` when nothing was dismissed.
pub fn dismissed_note(lang: Lang, dismissed: u32) -> Option<String> {
    (dismissed > 0).then(|| {
        if lang.zh {
            format!("已忽略 {dismissed} 条不再出现")
        } else {
            format!("{} suggestions dismissed", dismissed)
        }
    })
}

/// Similarity is shown beside a result as a number with its meaning, never as a confidence.
pub fn similarity_text(lang: Lang, similarity: f64) -> String {
    format!(
        "{:.2} · {}",
        similarity,
        lang.t("Semantic similarity, not a confidence score")
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Plain,
    /// Lexical hits are underlined with a solid line.
    Lexical,
    /// The semantic segment is marked with a dotted underline.
    Semantic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub mark: Mark,
}

/// Splits a snippet at byte spans from the response. Spans that fall outside the snippet, or
/// off a character boundary, are dropped rather than guessed at. Overlaps keep the earlier span.
pub fn highlight(snippet: &str, lexical: &[TextSpan], semantic: Option<&TextSpan>) -> Vec<Segment> {
    let mut spans: Vec<(usize, usize, Mark)> = lexical
        .iter()
        .map(|s| (s.start, s.end, Mark::Lexical))
        .chain(semantic.map(|s| (s.start, s.end, Mark::Semantic)))
        .filter_map(|(start, end, mark)| {
            let (start, end) = (start as usize, end as usize);
            let valid = start < end
                && end <= snippet.len()
                && snippet.is_char_boundary(start)
                && snippet.is_char_boundary(end);
            valid.then_some((start, end, mark))
        })
        .collect();
    spans.sort_by_key(|s| (s.0, s.1));
    let mut out = Vec::new();
    let mut at = 0usize;
    for (start, end, mark) in spans {
        if start < at {
            continue;
        }
        if start > at {
            out.push(Segment {
                text: snippet[at..start].to_string(),
                mark: Mark::Plain,
            });
        }
        out.push(Segment {
            text: snippet[start..end].to_string(),
            mark,
        });
        at = end;
    }
    if at < snippet.len() {
        out.push(Segment {
            text: snippet[at..].to_string(),
            mark: Mark::Plain,
        });
    }
    out
}

/// The fallback notice shown when semantic or hybrid search ran lexically.
pub fn fallback_notice(lang: Lang, fallback: &SearchFallback) -> Option<String> {
    let mut notice = lang
        .t("Semantic search unavailable: no local embedding cache; lexical results shown")
        .to_string();
    if fallback.requires_source_text {
        notice.push_str(". ");
        notice.push_str(lang.t("Source text required; excerpts were not used"));
    }
    Some(notice)
}

#[cfg(test)]
mod tests;

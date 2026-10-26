//! Presentation facts derived from one board revision.
//!
//! The backend owns the stage, the summary column, the current assignee and
//! every relationship status. This module only groups and labels them for the
//! view; it never decides who is responsible or whether work is finished.

use super::text::{Lang, short_time};
use chrono::{DateTime, Utc};
use spm_contracts::{
    BoardCard, BoardSnapshot, BoardStage, CoverageState, PersonRef, ProgressState, RecordId,
    RelationState, Relationship, RelationshipBasis, RelationshipKind, RelationshipStatus,
    SourceSystem, StageBasis, StageOverlay, StatementKind,
};

/// Board columns. Doing is split where work waits for different people:
/// Develop (implementation, review) and Verify (build, regression, acceptance).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DisplayColumn {
    Unmapped,
    Todo,
    Develop,
    Verify,
    Done,
}

impl DisplayColumn {
    pub const ALL: [Self; 5] = [
        Self::Unmapped,
        Self::Todo,
        Self::Develop,
        Self::Verify,
        Self::Done,
    ];

    pub fn of(stage: BoardStage) -> Self {
        match stage {
            BoardStage::NotStarted => Self::Todo,
            BoardStage::Implementation | BoardStage::Review => Self::Develop,
            BoardStage::IntegrationBuild | BoardStage::Regression | BoardStage::Acceptance => {
                Self::Verify
            }
            BoardStage::Done => Self::Done,
            BoardStage::Unmapped => Self::Unmapped,
        }
    }

    pub fn label(self, lang: Lang) -> &'static str {
        lang.t(match self {
            Self::Unmapped => "Unmapped",
            Self::Todo => "To do",
            Self::Develop => "Develop",
            Self::Verify => "Verify",
            Self::Done => "Done",
        })
    }
}

pub fn stage_label(stage: BoardStage, lang: Lang) -> &'static str {
    lang.t(match stage {
        BoardStage::NotStarted => "Not started",
        BoardStage::Implementation => "Implementation",
        BoardStage::Review => "Review",
        BoardStage::IntegrationBuild => "Integration build",
        BoardStage::Regression => "Regression",
        BoardStage::Acceptance => "Acceptance",
        BoardStage::Done => "Done",
        BoardStage::Unmapped => "Unmapped",
    })
}

pub fn system_label(system: SourceSystem) -> &'static str {
    match system {
        SourceSystem::Jira => "Jira",
        SourceSystem::Meegle => "Meegle",
        SourceSystem::Gerrit => "Gerrit",
        SourceSystem::Jenkins => "Jenkins",
        SourceSystem::Lark => "Lark",
        SourceSystem::People => "People",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tone {
    Danger,
    Warning,
    Info,
}

/// Facts that ask for a person's attention, most severe first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Signal {
    Blocked,
    TestFailed,
    BuildFailed,
    Conflict,
    Unassigned,
    Unmapped,
    SourceNotRefreshed,
    WorkflowLag,
    NotInTracker,
}

impl Signal {
    pub fn tone(self) -> Tone {
        match self {
            Self::Blocked | Self::TestFailed | Self::BuildFailed => Tone::Danger,
            Self::Conflict | Self::Unassigned | Self::Unmapped => Tone::Warning,
            // A source outage is a board-level fact (header banner); on a card it is
            // context in the detail view, not a reason to act on that card.
            Self::SourceNotRefreshed | Self::WorkflowLag | Self::NotInTracker => Tone::Info,
        }
    }

    /// Segoe Fluent Icons glyph. The shape differs per meaning, so the signal
    /// still reads in high contrast where every tone is the text colour.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Blocked => "\u{E733}",
            Self::TestFailed | Self::BuildFailed => "\u{E783}",
            Self::Conflict | Self::Unmapped => "\u{E7BA}",
            Self::Unassigned => "\u{E77B}",
            Self::SourceNotRefreshed => "\u{E823}",
            Self::WorkflowLag => "\u{E81C}",
            Self::NotInTracker => "\u{E8F2}",
        }
    }

    pub fn label(self, lang: Lang) -> &'static str {
        lang.t(match self {
            Self::Blocked => "Blocked",
            Self::TestFailed => "Test failed",
            Self::BuildFailed => "Build failed",
            Self::Conflict => "Owner conflict",
            Self::Unassigned => "Unassigned",
            Self::Unmapped => "Unmapped node",
            Self::SourceNotRefreshed => "Source not refreshed",
            Self::WorkflowLag => "Code ahead of workflow",
            Self::NotInTracker => "Not in Meegle",
        })
    }

    /// Signals that put a card on the "Needs attention" list.
    pub fn needs_attention(self) -> bool {
        self.tone() != Tone::Info
    }
}

/// Sources whose last refresh failed or came back incomplete.
pub fn degraded_sources(board: &BoardSnapshot) -> Vec<SourceSystem> {
    board
        .sources
        .iter()
        .filter(|s| s.error.is_some() || s.coverage != CoverageState::Complete || !s.page_complete)
        .map(|s| s.source)
        .collect()
}

pub fn signals(card: &BoardCard, degraded: &[SourceSystem]) -> Vec<Signal> {
    let mut signals = Vec::new();
    let overlays = &card.stage.overlays;
    if overlays.contains(&StageOverlay::Blocked) {
        signals.push(Signal::Blocked);
    }
    let open = card.stage.stage != BoardStage::Done;
    if open && card.progress.verification == ProgressState::Failed {
        signals.push(Signal::TestFailed);
    }
    if open && card.progress.build == ProgressState::Failed {
        signals.push(Signal::BuildFailed);
    }
    if !card.conflicts.is_empty() {
        signals.push(Signal::Conflict);
    }
    if open && card.current_assignee.is_none() {
        signals.push(Signal::Unassigned);
    }
    if card.stage.stage == BoardStage::Unmapped {
        signals.push(Signal::Unmapped);
    }
    if card
        .observed_through
        .keys()
        .any(|system| degraded.contains(system))
    {
        signals.push(Signal::SourceNotRefreshed);
    }
    if overlays.contains(&StageOverlay::WorkflowLag)
        || card.stage.basis == StageBasis::ActivityEvidence
    {
        signals.push(Signal::WorkflowLag);
    }
    if card
        .assignee()
        .is_some_and(|a| a.basis == RelationshipBasis::ExplicitStatement)
    {
        signals.push(Signal::NotInTracker);
    }
    signals.sort();
    signals.dedup();
    signals
}

/// One-line progress facts kept apart from the workflow stage: code, build and
/// test are separate observations, and none of them finishes a card.
pub fn fact_line(card: &BoardCard, lang: Lang) -> String {
    let mut parts = vec![stage_label(card.stage.stage, lang).to_owned()];
    let code = &card.progress.code;
    if code.linked > 0 {
        let mut text = if code.merged == code.linked {
            lang.t("merged").to_owned()
        } else if code.open > 0 {
            lang.f("{} open", &[&code.open.to_string()])
        } else {
            lang.f(
                "{} of {} merged",
                &[&code.merged.to_string(), &code.linked.to_string()],
            )
        };
        if code.relation == RelationState::Candidate {
            text = format!("{text} ({})", lang.t("link not confirmed"));
        }
        parts.push(format!("{} {text}", lang.t("Code")));
    }
    match card.progress.build {
        ProgressState::Failed => parts.push(format!("{} {}", lang.t("Build"), lang.t("failed"))),
        ProgressState::InProgress => {
            parts.push(format!("{} {}", lang.t("Build"), lang.t("running")))
        }
        _ => {}
    }
    let verification = match card.progress.verification {
        ProgressState::Passed => Some("passed"),
        ProgressState::Failed => Some("failed"),
        ProgressState::InProgress => Some("running"),
        ProgressState::NotStarted
            if matches!(
                card.stage.stage,
                BoardStage::Regression | BoardStage::IntegrationBuild
            ) =>
        {
            Some("not started")
        }
        _ => None,
    };
    if let Some(state) = verification {
        parts.push(format!("{} {}", lang.t("Test"), lang.t(state)));
    }
    parts.join(" · ")
}

/// A short reason for the card's most severe signal, from backend text.
pub fn signal_text(card: &BoardCard, signal: Signal, lang: Lang) -> String {
    let detail = match signal {
        Signal::Blocked => card.blockers.first().map(|b| b.reason.clone()),
        Signal::Conflict => card.conflicts.first().map(|c| c.explanation.clone()),
        Signal::Unmapped => card.stage.raw_node.clone(),
        Signal::WorkflowLag | Signal::TestFailed | Signal::BuildFailed => {
            Some(card.stage.explanation.clone())
        }
        _ => None,
    };
    match detail {
        Some(detail) => format!("{}: {detail}", signal.label(lang)),
        None => signal.label(lang).to_owned(),
    }
}

pub fn lane_label(lane: &spm_contracts::Swimlane) -> String {
    match &lane.model {
        Some(model) => format!("{} / {model}", lane.project),
        None => lane.project.clone(),
    }
}

pub fn first_name(person: &PersonRef) -> &str {
    person.display_name.as_str()
}

/// Rank for the compact list: most severe signal first, then work that has
/// been in progress longest, then backend order.
pub fn attention_rank(card: &BoardCard, signals: &[Signal]) -> (u8, i64) {
    let severity = signals
        .iter()
        .filter(|s| s.needs_attention())
        .map(|s| *s as u8)
        .min()
        .unwrap_or(u8::MAX);
    let started = card.started_at.map_or(i64::MAX, |at| at.timestamp());
    (severity, started)
}

/// Filters chosen in the view. A card matches when it passes all of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub model: Option<String>,
    pub attention: bool,
    pub query: Option<String>,
}

impl Filter {
    pub fn is_active(&self) -> bool {
        self.model.is_some() || self.attention || self.query.is_some()
    }

    pub fn matches(&self, card: &BoardCard, signals: &[Signal]) -> bool {
        if self
            .model
            .as_ref()
            .is_some_and(|model| card.swimlane.model.as_ref() != Some(model))
        {
            return false;
        }
        if self.attention && !signals.iter().any(|s| s.needs_attention()) {
            return false;
        }
        if let Some(query) = &self.query {
            let query = query.to_lowercase();
            let mut fields = vec![card.title.to_lowercase()];
            fields.extend(card.business_number.iter().map(|n| n.to_lowercase()));
            fields.extend(
                card.relationships
                    .iter()
                    .map(|r| r.person.display_name.to_lowercase()),
            );
            fields.extend(
                card.next_action
                    .iter()
                    .filter_map(|n| n.owner.as_ref())
                    .map(|o| o.display_name.to_lowercase()),
            );
            if !fields.iter().any(|field| field.contains(&query)) {
                return false;
            }
        }
        true
    }
}

/// One line in the detail view's People section.
#[derive(Clone, Debug, PartialEq)]
pub struct PersonRow {
    pub role: &'static str,
    pub name: String,
    pub detail: String,
    pub emphasis: bool,
    pub relationship_ids: Vec<String>,
}

fn when(r: &Relationship, timezone: &str) -> String {
    r.source_time
        .map(|at| short_time(at, timezone))
        .unwrap_or_default()
}

fn basis_text(r: &Relationship, timezone: &str, lang: Lang) -> String {
    match (&r.statement, r.basis) {
        (Some(statement), _) => {
            let channel = if r.evidence.iter().any(|e| e.starts_with("ev:l-")) {
                "Lark"
            } else {
                "chat"
            };
            let mut text = lang.f(
                "{} named {} in {} · {}",
                &[
                    &statement.author.display_name,
                    &r.person.display_name,
                    channel,
                    &when(r, timezone),
                ],
            );
            if statement.kind == StatementKind::Correction {
                text.push_str(" · ");
                text.push_str(lang.t("correction"));
            }
            if statement.authority == spm_contracts::Authority::Unknown {
                text.push_str(" · ");
                text.push_str(lang.t("authority unknown"));
            }
            text
        }
        (None, RelationshipBasis::IssueField) => lang.f(
            "{} field · {}",
            &[system_label(r.subject.system), &when(r, timezone)],
        ),
        (None, RelationshipBasis::Derived) => lang.t("derived from code history").to_owned(),
        (None, _) => format!("{} · {}", system_label(r.subject.system), when(r, timezone)),
    }
}

/// People grouped by what each relationship means. Assignment, code
/// contribution, review and acceptance stay in separate rows; a derived contact
/// never fills the assignee slot.
pub fn people_rows(card: &BoardCard, timezone: &str, lang: Lang) -> Vec<PersonRow> {
    let mut rows = Vec::new();
    let current = card.current_assignee.as_deref();
    let assigned: Vec<&Relationship> = card.related(RelationshipKind::Assigned).collect();
    if let Some(r) = card.assignee() {
        rows.push(PersonRow {
            role: lang.t("Responsible now"),
            name: r.person.display_name.clone(),
            detail: basis_text(r, timezone, lang),
            emphasis: true,
            relationship_ids: vec![r.id.clone()],
        });
    } else if card.stage.stage != BoardStage::Done {
        rows.push(PersonRow {
            role: lang.t("Responsible now"),
            name: lang.t("Unassigned").to_owned(),
            detail: String::new(),
            emphasis: true,
            relationship_ids: Vec::new(),
        });
    }
    for r in assigned.iter().filter(|r| Some(r.id.as_str()) != current) {
        let role = match r.status {
            RelationshipStatus::Conflicting => lang.t("Conflicting"),
            RelationshipStatus::Current
                if card.assignee().is_some_and(|a| a.person.same_as(&r.person)) =>
            {
                lang.t("Earlier statement")
            }
            RelationshipStatus::Current => lang.t("Also assigned"),
            _ => lang.t("Earlier statement"),
        };
        rows.push(PersonRow {
            role,
            name: r.person.display_name.clone(),
            detail: basis_text(r, timezone, lang),
            emphasis: r.status == RelationshipStatus::Conflicting,
            relationship_ids: vec![r.id.clone()],
        });
    }
    let mut group = |role: &'static str,
                     kinds: &[RelationshipKind],
                     describe: &dyn Fn(&Relationship) -> String| {
        let mut merged: Vec<(PersonRef, Vec<String>, Vec<String>)> = Vec::new();
        for r in card
            .relationships
            .iter()
            .filter(|r| kinds.contains(&r.kind))
        {
            let text = describe(r);
            match merged.iter_mut().find(|(p, _, _)| p.same_as(&r.person)) {
                Some((_, details, ids)) => {
                    if !details.contains(&text) {
                        details.push(text);
                    }
                    ids.push(r.id.clone());
                }
                None => merged.push((r.person.clone(), vec![text], vec![r.id.clone()])),
            }
        }
        for (person, details, ids) in merged {
            rows.push(PersonRow {
                role,
                name: person.display_name.clone(),
                detail: details.join("; "),
                emphasis: false,
                relationship_ids: ids,
            });
        }
    };
    let subject = |r: &Relationship| -> String {
        let id = &r.subject.record_id;
        match r.subject.record_kind.as_str() {
            "patchset" => {
                let (change, ps) = id.split_once('/').unwrap_or((id, ""));
                format!("{change} PS{ps}")
            }
            _ => id.clone(),
        }
    };
    group(
        lang.t("Developer"),
        &[
            RelationshipKind::ChangeOwner,
            RelationshipKind::PatchsetAuthor,
            RelationshipKind::PatchsetUploader,
        ],
        &|r| {
            let what = match r.kind {
                RelationshipKind::ChangeOwner => "owns change {}",
                RelationshipKind::PatchsetAuthor => "wrote {}",
                _ => "uploaded {}",
            };
            lang.f(what, &[&subject(r)])
        },
    );
    group(
        lang.t("Reviewer"),
        &[RelationshipKind::ReviewerListed, RelationshipKind::Approval],
        &|r| match &r.approval {
            Some(a) => {
                let mut text = format!("{} {:+} on {}", a.label, a.value, subject(r));
                if a.carried_over != spm_contracts::CarriedOver::No {
                    text.push_str(&format!(" ({})", lang.t("copied")));
                }
                if a.self_approval {
                    text.push_str(&format!(" ({})", lang.t("self-review")));
                }
                text
            }
            None => lang.f("listed on change {}", &[&subject(r)]),
        },
    );
    group(lang.t("Submitter"), &[RelationshipKind::Submitter], &|r| {
        lang.f("merged change {} · {}", &[&subject(r), &when(r, timezone)])
    });
    group(lang.t("Verified by"), &[RelationshipKind::Verified], &|r| {
        format!("{} · {}", system_label(r.subject.system), when(r, timezone))
    });
    group(lang.t("Accepted by"), &[RelationshipKind::Accepted], &|r| {
        format!("{} · {}", system_label(r.subject.system), when(r, timezone))
    });
    group(
        lang.t("Code history"),
        &[RelationshipKind::HistoricalContributor],
        &|r| {
            format!(
                "{} · {}",
                lang.t("derived from code history"),
                when(r, timezone)
            )
        },
    );
    group(
        lang.t("Contact"),
        &[RelationshipKind::ImplementationContact],
        &|r| {
            lang.f(
                "{} · not an assignment",
                &[r.derivation.as_deref().unwrap_or("derived")],
            )
        },
    );
    rows
}

/// Columns that moved between two revisions, by card identity.
pub fn moves(
    before: &BoardSnapshot,
    after: &BoardSnapshot,
) -> Vec<(String, DisplayColumn, DisplayColumn)> {
    let mut moved = Vec::new();
    for card in &after.cards {
        if let Some(old) = before.cards.iter().find(|c| c.card_id == card.card_id) {
            let (from, to) = (
                DisplayColumn::of(old.stage.stage),
                DisplayColumn::of(card.stage.stage),
            );
            if from != to {
                moved.push((label_of(card), from, to));
            }
        }
    }
    moved
}

pub fn departures(before: &BoardSnapshot, after: &BoardSnapshot) -> Vec<String> {
    before
        .cards
        .iter()
        .filter(|c| !after.cards.iter().any(|n| n.card_id == c.card_id))
        .map(label_of)
        .collect()
}

pub fn label_of(card: &BoardCard) -> String {
    card.business_number
        .clone()
        .unwrap_or_else(|| card.source.record_id.clone())
}

pub fn find<'a>(board: &'a BoardSnapshot, id: &RecordId) -> Option<&'a BoardCard> {
    board.cards.iter().find(|c| &c.card_id == id)
}

pub fn workflow_text(card: &BoardCard, timezone: &str, lang: Lang) -> (String, String) {
    let stage = stage_label(card.stage.stage, lang);
    match &card.workflow {
        Some(w) => (
            format!("{} · {stage}", w.source_state),
            w.source_time
                .map(|at| {
                    format!(
                        "{} · {}",
                        system_label(card.source.system),
                        lang.f("since {}", &[&short_time(at, timezone)])
                    )
                })
                .unwrap_or_default(),
        ),
        None => (stage.to_owned(), String::new()),
    }
}

pub fn acceptance_text(card: &BoardCard, timezone: &str, lang: Lang) -> String {
    match card.related(RelationshipKind::Accepted).next() {
        Some(r) => format!(
            "{} · {}",
            lang.f("by {}", &[&r.person.display_name]),
            when(r, timezone)
        ),
        None if card.stage.stage == BoardStage::Done => lang.t("Done").to_owned(),
        None => lang.t("Not accepted").to_owned(),
    }
}

pub fn progress_word(state: ProgressState, lang: Lang) -> &'static str {
    lang.t(match state {
        ProgressState::NotStarted => "not started",
        ProgressState::InProgress => "running",
        ProgressState::Passed => "passed",
        ProgressState::Failed => "failed",
        ProgressState::Unknown => "unknown",
    })
}

/// Glyph shapes for progress states: filled check, cross, ring, sync, question.
pub fn progress_glyph(state: ProgressState) -> &'static str {
    match state {
        ProgressState::Passed => "\u{E73E}",
        ProgressState::Failed => "\u{E711}",
        ProgressState::NotStarted => "\u{EA3A}",
        ProgressState::InProgress => "\u{E895}",
        ProgressState::Unknown => "\u{E897}",
    }
}

pub fn code_text(card: &BoardCard, lang: Lang) -> String {
    let code = &card.progress.code;
    if code.linked == 0 {
        return lang.t("No linked change").to_owned();
    }
    let mut text = lang.f(
        "{} of {} merged",
        &[&code.merged.to_string(), &code.linked.to_string()],
    );
    if code.open > 0 {
        text.push_str(&format!(
            " · {}",
            lang.f("{} open", &[&code.open.to_string()])
        ));
    }
    if code.relation == RelationState::Candidate {
        text.push_str(&format!(" · {}", lang.t("link not confirmed")));
    }
    text
}

pub fn code_state(card: &BoardCard) -> ProgressState {
    let code = &card.progress.code;
    if code.linked == 0 {
        ProgressState::NotStarted
    } else if code.relation == RelationState::Candidate {
        // Merged code that may not belong to this requirement proves nothing yet.
        ProgressState::Unknown
    } else if code.merged == code.linked {
        ProgressState::Passed
    } else {
        ProgressState::InProgress
    }
}

pub fn freshest(card: &BoardCard) -> Option<DateTime<Utc>> {
    card.observed_through.values().max().copied()
}

/// What one relationship row means, in words, for the evidence timeline.
pub fn kind_label(kind: RelationshipKind, lang: Lang) -> &'static str {
    lang.t(match kind {
        RelationshipKind::ChangeOwner => "change owner",
        RelationshipKind::PatchsetAuthor => "patch set author",
        RelationshipKind::PatchsetUploader => "patch set uploader",
        RelationshipKind::ReviewerListed => "listed reviewer",
        RelationshipKind::Approval => "vote",
        RelationshipKind::Submitter => "submitter",
        RelationshipKind::Assigned => "assignment",
        RelationshipKind::Accepted => "acceptance",
        RelationshipKind::Verified => "verification",
        RelationshipKind::HistoricalContributor => "code history",
        RelationshipKind::ImplementationContact => "contact (not an assignment)",
    })
}

pub fn status_label(status: RelationshipStatus, lang: Lang) -> Option<&'static str> {
    match status {
        RelationshipStatus::Current => None,
        RelationshipStatus::Superseded => Some(lang.t("superseded")),
        RelationshipStatus::Retracted => Some(lang.t("retracted")),
        RelationshipStatus::Conflicting => Some(lang.t("conflicting")),
    }
}

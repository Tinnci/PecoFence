//! One geometry description feeds paint, hit testing and accessibility.

mod release_train;
mod workspace;

use super::{
    Config, Scope, Target, View,
    text::{self, Lang},
};
use crate::board::{
    palette::Palette,
    text::{Fitter, Measure},
};
use crate::scene_builder::{Builder, r};
use chrono::{DateTime, Utc};
use pecofence_plugin_api::paper;
use pecofence_plugin_api::{
    scene::{NodeStyle, SceneNode},
    *,
};
use release_train::{dashed_vertical, release_train, stroke_path};
use spm_contracts::*;
use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
};
use workspace::workspace;

pub struct Layout {
    pub nodes: Vec<SceneNode>,
    pub actions: Vec<(String, Target, RectDip)>,
    pub content_height: f32,
    pub field: RectDip,
    /// Row keys that an entry point can reveal (change cards, release-train rows), with their
    /// top edge in document coordinates (before scrolling).
    pub rows: Vec<(String, f32)>,
}
pub(super) fn stage_tone(state: CiStageState, p: &Palette) -> [f32; 4] {
    match state {
        CiStageState::Done => p.success,
        CiStageState::Running => p.accent_ink,
        CiStageState::Failed => p.danger,
        CiStageState::Warn => p.warning,
        CiStageState::Pending => p.secondary,
    }
}
/// Stage segment fill: pending is light paper, running is a tint, done and others use the tone.
pub(super) fn segment_fill(state: CiStageState, p: &Palette) -> [f32; 4] {
    match state {
        CiStageState::Pending => p.pending,
        CiStageState::Running => paper::mix(p.base, p.accent_ink, 0.22),
        other => stage_tone(other, p),
    }
}
pub(super) fn build_tone(state: CiBuildState, p: &Palette) -> [f32; 4] {
    match state {
        CiBuildState::Success => p.success,
        CiBuildState::Failure => p.danger,
        CiBuildState::Unstable => p.warning,
        CiBuildState::Running => p.accent_ink,
        _ => p.secondary,
    }
}
/// Glyph and word so that build state is never shown by colour alone.
pub(super) fn build_glyph(b: &CiBuild) -> &'static str {
    match (b.state, b.overdue) {
        (CiBuildState::Running | CiBuildState::Queued, _) => "●",
        (CiBuildState::Success, _) => "✓",
        (CiBuildState::Failure, _) => "×",
        (CiBuildState::Unstable, _) => "!",
        _ => "○",
    }
}
/// Headline status for a change, from the first stage that needs attention.
pub(super) fn headline(change: &CiChange, lang: Lang) -> (String, CiStageState) {
    let stages = &change.pipeline;
    if stages.iter().any(|s| s.state == CiStageState::Failed) {
        return (lang.t("Needs fix").into(), CiStageState::Failed);
    }
    if let Some(s) = stages.iter().find(|s| s.state == CiStageState::Warn) {
        let word = if s.kind == CiStageKind::Merge {
            lang.t("Needs rebase")
        } else {
            lang.t("Needs attention")
        };
        return (word.into(), CiStageState::Warn);
    }
    if let Some(s) = stages.iter().find(|s| s.state == CiStageState::Running) {
        return (lang.running_word(lang.stage(s.kind)), CiStageState::Running);
    }
    match stages.iter().find(|s| s.state != CiStageState::Done) {
        Some(s) => (lang.waiting_word(lang.stage(s.kind)), CiStageState::Pending),
        None if !stages.is_empty() => (lang.t("Shipped").into(), CiStageState::Done),
        None => ("—".into(), CiStageState::Pending),
    }
}
pub(super) fn next_role(role: CiJobRole) -> CiJobRole {
    match role {
        CiJobRole::Other => CiJobRole::Verify,
        CiJobRole::Verify => CiJobRole::Daily,
        CiJobRole::Daily => CiJobRole::Release,
        CiJobRole::Release => CiJobRole::Other,
    }
}
/// The first-run list: recent jobs and open-change branches as ticks, then the footer. Returns the
/// y after the footer, in the same coordinates as the other views.
pub(super) fn seeding_body(
    b: &mut Builder<'_, Target>,
    snapshot: &CiWatchSnapshot,
    seeds: &CiSuggestions,
    selected: &BTreeSet<usize>,
    lang: Lang,
    w: f32,
    mut y: f32,
) -> f32 {
    let p = *b.p;
    b.label(
        "seed:title",
        r(14.0, y, w - 28.0, 30.0),
        lang.t("Start with your recent work").into(),
        p.primary,
        22.0,
        FontRole::Display,
    );
    y += 34.0;
    let who = format!(
        "{} {} · {} {}",
        lang.t("Jenkins user"),
        snapshot.user.jenkins_user,
        lang.t("Gerrit account"),
        snapshot.user.gerrit_user,
    );
    b.label(
        "seed:who",
        r(14.0, y, w - 28.0, 18.0),
        who,
        p.secondary,
        12.0,
        FontRole::Text,
    );
    y += 18.0;
    b.label(
        "seed:hint",
        r(14.0, y, w - 28.0, 18.0),
        lang.t("Tick what to follow. You can change the list later.")
            .into(),
        p.secondary,
        12.0,
        FontRole::Text,
    );
    y += 26.0;
    for (heading, kind) in [
        (
            lang.t("Your recently triggered jobs"),
            CiSuggestionKind::Job,
        ),
        (
            lang.t("Branches of your open changes"),
            CiSuggestionKind::Branch,
        ),
    ] {
        b.section(&format!("seed:{kind:?}:heading"), y, w, heading, "");
        y += 26.0;
        let mut any = false;
        for (i, seed) in seeds
            .items
            .iter()
            .enumerate()
            .filter(|(_, s)| s.kind == kind)
        {
            any = true;
            let on = selected.contains(&i);
            let meta = match (kind, seed.count) {
                (CiSuggestionKind::Job, Some(n)) if lang.zh => format!("7 天 {n} 次"),
                (CiSuggestionKind::Job, Some(n)) => format!("{n} {}", lang.t("in 7 days")),
                (_, Some(n)) if lang.zh => format!("{n} {}", lang.t("of your changes")),
                (_, Some(n)) => format!("{n} {}", lang.t("of your changes")),
                _ => String::new(),
            };
            let mark = if on { "☑" } else { "☐" };
            b.node(
                &format!("seed:{i}"),
                r(14.0, y, w - 28.0, 34.0),
                "option",
                format!(
                    "{} {} {}",
                    seed.label,
                    if on {
                        lang.t("selected")
                    } else {
                        lang.t("not selected")
                    },
                    meta
                ),
                format!("{mark}  {}   {meta}", seed.label),
                FontRole::Display,
                14.0,
                if on { 700 } else { 400 },
                if on { p.accent_ink } else { p.primary },
                Some(p.card),
                Some(Target::Seed(i)),
            );
            y += 38.0;
        }
        if !any {
            b.label(
                "seed:none",
                r(14.0, y, w - 28.0, 22.0),
                lang.t("No suggestions yet. Add a job or branch manually.")
                    .into(),
                p.secondary,
                12.0,
                FontRole::Text,
            );
            y += 26.0;
        }
        y += 8.0;
    }
    let count = selected.len();
    let apply = format!("{} {count} {}", lang.t("Follow"), lang.t("items"));
    b.node(
        "seed:apply",
        r(14.0, y, (w - 150.0).max(120.0), 34.0),
        "button",
        apply.clone(),
        apply,
        FontRole::Text,
        13.0,
        700,
        p.accent_ink,
        Some(p.card),
        Some(Target::ApplySeeds),
    );
    b.node(
        "seed:manual",
        r(w - 128.0, y, 114.0, 34.0),
        "button",
        lang.t("Manual add…").into(),
        lang.t("Manual add…").into(),
        FontRole::Text,
        13.0,
        400,
        p.primary,
        Some(p.card),
        Some(Target::ManualAdd),
    );
    y + 44.0
}

/// Shared grid for the six stage columns inside a card.
pub(super) fn stage_columns(w: f32) -> (f32, f32) {
    let inner = (w - 40.0).max(60.0);
    let col = (inner - 15.0) / 6.0;
    (col, 3.0 + col)
}
#[allow(clippy::too_many_arguments)]
pub fn build(
    snapshot: Option<&CiWatchSnapshot>,
    suggestions: &CiSuggestions,
    view: View,
    query: &str,
    focus: Option<&str>,
    scroll: f32,
    config: &Config,
    scope: Scope,
    now: DateTime<Utc>,
    lang: Lang,
    viewport: RectDip,
    p: &Palette,
    measure: &dyn Measure,
    ids: &mut HashMap<String, u64>,
    next: &mut u64,
    seeding: Option<(&CiSuggestions, &BTreeSet<usize>)>,
) -> Layout {
    let cache = RefCell::new(HashMap::new());
    let fitter = Fitter::new(measure, 1.0, &cache);
    let mut b = Builder {
        rows: vec![],
        scroll,
        nodes: vec![SceneNode {
            id: 1,
            parent: None,
            rect: viewport,
            role: "pane".into(),
            name: lang.t("CI/CD").into(),
            text: String::new(),
            action: None,
            style: NodeStyle {
                font: FontRole::Text,
                size: 13.0,
                weight: 400,
                foreground: p.primary,
                fill: Some(p.base),
                stroke: None,
                radius: 0.0,
                text_inset: [0.0, 0.0],
                align: TextAlign::TopLeading,
                path: None,
            },
        }],
        actions: vec![],
        ids,
        next,
        p,
        fitter,
        focus,
        top: 0.0,
        bottom: viewport.h,
    };
    let w = viewport.w;
    if let (Some(s), Some((seeds, selected))) = (snapshot, seeding) {
        // First run: the watchlist is empty, so the fence offers what the user already works on.
        let y = seeding_body(&mut b, s, seeds, selected, lang, w, 14.0 - scroll);
        let content_height = y + scroll + 40.0;
        return Layout {
            nodes: b.nodes,
            actions: b.actions,
            content_height,
            field: r(14.0, 70.0 - scroll, (w - 100.0).max(1.0), 32.0),
            rows: b.rows,
        };
    }
    // Segmented pill: one track, the selected tab on the ground colour.
    b.node(
        "view:track",
        r(12.0, 3.0, 4.0 * 94.0 + 4.0, 30.0),
        "group",
        lang.t("Views").into(),
        String::new(),
        FontRole::Text,
        12.0,
        400,
        p.primary,
        Some(p.card),
        None,
    );
    for (i, (v, label)) in [
        (View::Pipeline, "Pipeline"),
        (View::Release, "Release"),
        (View::Workspace, "Workspace"),
        (View::Watchlist, "Watchlist"),
    ]
    .into_iter()
    .enumerate()
    {
        let text = lang.t(label);
        let on = view == v;
        b.node(
            &format!("view:{i}"),
            r(14.0 + i as f32 * 94.0, 5.0, 90.0, 26.0),
            "tab",
            format!("{text} {}", if on { lang.t("selected") } else { "" }),
            text.into(),
            FontRole::Text,
            12.0,
            if on { 700 } else { 400 },
            if on { p.accent_ink } else { p.primary },
            if on { Some(p.base) } else { None },
            Some(Target::View(v)),
        );
    }
    b.top = 40.0;
    b.bottom = (viewport.h - 42.0).max(40.0);
    let mut y = 40.0 - scroll;
    let mut field = r(14.0, 70.0 - scroll, (w - 100.0).max(1.0), 32.0);
    if let Some(s) = snapshot {
        match view {
            View::Pipeline => {
                let changes: Vec<(&str, &CiChange)> = s
                    .branches
                    .iter()
                    .flat_map(|branch| branch.changes.iter().map(move |c| (branch.id.as_str(), c)))
                    .collect();
                b.section(
                    "changes:heading",
                    y,
                    w,
                    lang.t("Changes"),
                    &changes.len().to_string(),
                );
                y += 26.0;
                let (col, step) = stage_columns(w);
                // The stage legend is printed once; each card carries only marks.
                for (i, kind) in CiStageKind::ALL.into_iter().enumerate() {
                    b.label(
                        &format!("legend:{i}"),
                        r(20.0 + i as f32 * step, y, col, 16.0),
                        lang.stage(kind).into(),
                        p.secondary,
                        11.0,
                        FontRole::Display,
                    );
                }
                y += 20.0;
                if changes.is_empty() {
                    b.label(
                        "changes:empty",
                        r(14.0, y, w - 28.0, 24.0),
                        lang.t("No changes observed").into(),
                        p.secondary,
                        12.0,
                        FontRole::Text,
                    );
                    y += 26.0;
                }
                for (branch_id, change) in changes {
                    let key = format!("change:{branch_id}:{}", change.number);
                    b.rows.push((key.clone(), y + scroll));
                    let (head, head_state) = headline(change, lang);
                    let name = format!(
                        "{} {}, {head}, {}",
                        change.number,
                        change.subject,
                        CiStageKind::ALL
                            .iter()
                            .zip(&change.pipeline)
                            .map(|(k, s)| format!(
                                "{} {} {}",
                                lang.stage(*k),
                                s.label,
                                lang.stage_state(s.state)
                            ))
                            .collect::<Vec<_>>()
                            .join("，")
                    );
                    b.node(
                        &key,
                        r(14.0, y, w - 28.0, 84.0),
                        "link",
                        name,
                        String::new(),
                        FontRole::Text,
                        13.0,
                        400,
                        p.primary,
                        Some(p.card),
                        Some(Target::Open(
                            change.url_origin.clone(),
                            change.path.clone(),
                            SourceSystem::Gerrit,
                        )),
                    );
                    b.label(
                        &format!("{key}:num"),
                        r(20.0, y + 4.0, 54.0, 20.0),
                        change.number.to_string(),
                        p.primary,
                        14.0,
                        FontRole::Display,
                    );
                    b.label(
                        &format!("{key}:subject"),
                        r(74.0, y + 4.0, w - 200.0, 20.0),
                        change.subject.clone(),
                        p.primary,
                        13.0,
                        FontRole::Text,
                    );
                    b.label(
                        &format!("{key}:head"),
                        r(w - 118.0, y + 4.0, 104.0, 20.0),
                        head,
                        stage_tone(head_state, p),
                        12.0,
                        FontRole::Text,
                    );
                    for (i, (kind, stage)) in CiStageKind::ALL
                        .into_iter()
                        .zip(&change.pipeline)
                        .enumerate()
                    {
                        let x = 20.0 + i as f32 * step;
                        b.node(
                            &format!("{key}:bar:{i}"),
                            r(x, y + 30.0, col, 6.0),
                            "decoration",
                            String::new(),
                            String::new(),
                            FontRole::Text,
                            11.0,
                            400,
                            p.primary,
                            Some(segment_fill(stage.state, p)),
                            None,
                        );
                        if stage.state == CiStageState::Running {
                            b.hatch(
                                &format!("{key}:bar:{i}"),
                                r(x, y + 30.0, col, 6.0),
                                p.accent_ink,
                            );
                        }
                        b.node(
                            &format!("{key}:stage:{i}"),
                            r(x, y + 39.0, col, 16.0),
                            "text",
                            format!(
                                "{} {}, {}",
                                lang.stage(kind),
                                stage.label,
                                lang.stage_state(stage.state)
                            ),
                            stage.label.clone(),
                            FontRole::Text,
                            11.0,
                            if stage.state == CiStageState::Done {
                                700
                            } else {
                                400
                            },
                            if stage.state == CiStageState::Pending {
                                p.secondary
                            } else {
                                stage_tone(stage.state, p)
                            },
                            None,
                            None,
                        );
                    }
                    if let Some(note) = &change.note {
                        let tone = change.note_state.map_or(p.secondary, |s| stage_tone(s, p));
                        b.label(
                            &format!("{key}:note"),
                            r(20.0, y + 60.0, w - 40.0, 18.0),
                            note.clone(),
                            tone,
                            12.0,
                            FontRole::Text,
                        );
                    }
                    y += 92.0;
                }
                y = release_train(&mut b, s, lang, &config.timezone, y, w, false);
            }
            View::Release => {
                y = release_train(&mut b, s, lang, &config.timezone, y, w, true);
            }
            View::Workspace => {
                y = workspace(&mut b, s, lang, config, scope, now, y, w);
            }
            View::Watchlist => {
                b.section("add:heading", y, w, lang.t("Add"), "");
                y += 26.0;
                field = r(14.0, y, w - 104.0, 32.0);
                b.node(
                    "add:field",
                    field,
                    "combobox",
                    format!("{}: {query}", lang.t("Job path or project:branch")),
                    if query.is_empty() {
                        lang.t("Job path or project:branch").into()
                    } else {
                        query.into()
                    },
                    FontRole::Text,
                    13.0,
                    400,
                    p.primary,
                    Some(p.card),
                    Some(Target::Find),
                );
                b.node(
                    "add:button",
                    r(w - 84.0, y, 70.0, 32.0),
                    "button",
                    lang.t("Add").into(),
                    lang.t("Add").into(),
                    FontRole::Text,
                    13.0,
                    700,
                    p.accent_ink,
                    Some(p.card),
                    Some(Target::Add),
                );
                y += 38.0;
                for (i, suggestion) in suggestions.items.iter().enumerate() {
                    let key = format!("suggest:{}", suggestion.value);
                    let kind = if suggestion.kind == CiSuggestionKind::Job {
                        "Jenkins"
                    } else {
                        "Gerrit"
                    };
                    b.node(
                        &key,
                        r(14.0, y, w - 28.0, 30.0),
                        "option",
                        format!(
                            "{kind} {} {}",
                            suggestion.label,
                            if suggestion.already_watched {
                                lang.t("Already watched")
                            } else {
                                ""
                            }
                        ),
                        format!(
                            "{kind}  {}{}",
                            suggestion.label,
                            if suggestion.already_watched {
                                format!(" · {}", lang.t("Already watched"))
                            } else {
                                String::new()
                            }
                        ),
                        FontRole::Display,
                        14.0,
                        if i == 0 { 700 } else { 400 },
                        p.primary,
                        Some(p.card),
                        Some(Target::Suggestion(i)),
                    );
                    y += 32.0;
                }
                if !query.is_empty() && suggestions.items.is_empty() {
                    b.label(
                        "suggest:empty",
                        r(14.0, y, w - 28.0, 22.0),
                        lang.t("No matching suggestions").into(),
                        p.secondary,
                        12.0,
                        FontRole::Text,
                    );
                    y += 24.0;
                }
                b.section("list:jobs", y, w, lang.t("Watched jobs"), "");
                y += 26.0;
                for job in &s.jobs {
                    let key = format!("job:{}", job.id);
                    let role_text = lang.role(job.role);
                    b.node(
                        &key,
                        r(14.0, y, w - 28.0, 92.0),
                        "group",
                        format!(
                            "{} · {} · {role_text} · {}",
                            job.job,
                            lang.health(job.health),
                            job.last
                                .as_ref()
                                .and_then(|b| b.number)
                                .map_or_else(|| "—".into(), |n| format!("#{n}"))
                        ),
                        String::new(),
                        FontRole::Text,
                        13.0,
                        400,
                        p.primary,
                        Some(p.card),
                        None,
                    );
                    b.node(
                        &format!("{key}:name"),
                        r(20.0, y + 4.0, w - 60.0, 22.0),
                        "text",
                        job.job.clone(),
                        job.job.clone(),
                        FontRole::Display,
                        14.0,
                        700,
                        p.primary,
                        None,
                        None,
                    );
                    b.node(
                        &format!("{key}:remove"),
                        r(w - 48.0, y + 4.0, 30.0, 30.0),
                        "button",
                        format!("{} {}", lang.t("Remove"), job.job),
                        "×".into(),
                        FontRole::Text,
                        14.0,
                        400,
                        p.secondary,
                        None,
                        Some(Target::Remove(job.id.clone())),
                    );
                    let role_next = next_role(job.role);
                    b.node(
                        &format!("{key}:role"),
                        r(20.0, y + 32.0, w - 52.0, 26.0),
                        "switch",
                        format!(
                            "{} {}: {role_text}, {} {}",
                            job.job,
                            lang.t("Role"),
                            lang.t("Next"),
                            lang.role(role_next)
                        ),
                        format!("{} {role_text}", lang.t("Role")),
                        FontRole::Text,
                        12.0,
                        700,
                        if job.role == CiJobRole::Other {
                            p.secondary
                        } else {
                            p.accent_ink
                        },
                        Some(p.base),
                        Some(Target::Role(job.id.clone(), role_next)),
                    );
                    let flags = [
                        (CiWatchFlag::OnlyMine, job.only_mine, "Only my triggers"),
                        (
                            CiWatchFlag::NotifyOnFailure,
                            job.notify_on_failure,
                            "Failure alert",
                        ),
                    ];
                    for (i, (flag, on, label)) in flags.into_iter().enumerate() {
                        let text = lang.t(label);
                        b.node(
                            &format!("{key}:toggle:{i}"),
                            r(20.0 + i as f32 * 132.0, y + 62.0, 126.0, 26.0),
                            "switch",
                            format!(
                                "{}, {text}: {}",
                                job.job,
                                lang.t(if on { "on" } else { "off" })
                            ),
                            format!("{} {text}", if on { "●" } else { "○" }),
                            FontRole::Text,
                            12.0,
                            if on { 700 } else { 400 },
                            if on { p.accent_ink } else { p.secondary },
                            Some(if on { p.base } else { p.card }),
                            Some(Target::Toggle(job.id.clone(), flag, !on)),
                        );
                    }
                    y += 100.0;
                }
                b.section("list:branches", y, w, lang.t("Gerrit branches"), "");
                y += 26.0;
                for branch in &s.branches {
                    let key = format!("branch:{}", branch.id);
                    b.node(
                        &key,
                        r(14.0, y, w - 28.0, 62.0),
                        "group",
                        format!("{} : {}", branch.project, branch.branch),
                        String::new(),
                        FontRole::Text,
                        13.0,
                        400,
                        p.primary,
                        Some(p.card),
                        None,
                    );
                    b.node(
                        &format!("{key}:name"),
                        r(20.0, y + 4.0, w - 60.0, 24.0),
                        "text",
                        format!("{} : {}", branch.project, branch.branch),
                        format!("{} : {}", branch.project, branch.branch),
                        FontRole::Display,
                        14.0,
                        700,
                        p.primary,
                        None,
                        None,
                    );
                    b.node(
                        &format!("{key}:remove"),
                        r(w - 48.0, y + 4.0, 30.0, 30.0),
                        "button",
                        format!(
                            "{} {} : {}",
                            lang.t("Remove"),
                            branch.project,
                            branch.branch
                        ),
                        "×".into(),
                        FontRole::Text,
                        14.0,
                        400,
                        p.secondary,
                        None,
                        Some(Target::Remove(branch.id.clone())),
                    );
                    let flags = [
                        (CiWatchFlag::OnlyMine, branch.only_mine, "Only mine"),
                        (
                            CiWatchFlag::IncludeMerged,
                            branch.include_merged,
                            "Include merged",
                        ),
                    ];
                    for (i, (flag, on, label)) in flags.into_iter().enumerate() {
                        let text = lang.t(label);
                        b.node(
                            &format!("{key}:toggle:{i}"),
                            r(20.0 + i as f32 * 132.0, y + 32.0, 126.0, 26.0),
                            "switch",
                            format!(
                                "{} : {}, {text}: {}",
                                branch.project,
                                branch.branch,
                                lang.t(if on { "on" } else { "off" })
                            ),
                            format!("{} {text}", if on { "●" } else { "○" }),
                            FontRole::Text,
                            12.0,
                            if on { 700 } else { 400 },
                            if on { p.accent_ink } else { p.secondary },
                            Some(if on { p.base } else { p.card }),
                            Some(Target::Toggle(branch.id.clone(), flag, !on)),
                        );
                    }
                    y += 70.0;
                }
            }
        }
    } else {
        b.label(
            "loading",
            r(14.0, y, w - 28.0, 30.0),
            lang.t("Loading CI…").into(),
            p.secondary,
            13.0,
            FontRole::Text,
        );
        y += 32.0;
    }
    let content_height = y + scroll - 40.0;
    b.top = 0.0;
    b.bottom = viewport.h;
    let footer = if view == View::Watchlist {
        lang.t("Local watchlist; Jenkins and Gerrit are read-only.")
            .into()
    } else {
        snapshot.map_or_else(
            || lang.t("Not observed").into(),
            |s| {
                s.sources
                    .iter()
                    .map(|source| {
                        format!(
                            "{:?} {} · {}",
                            source.source,
                            source.observed_at.map_or_else(
                                || lang.t("Not observed").into(),
                                |at| text::freshness(
                                    at,
                                    if config.demo {
                                        s.observed_through.unwrap_or(at)
                                    } else {
                                        chrono::Utc::now()
                                    },
                                    lang
                                )
                            ),
                            lang.health(source.health)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("   ")
            },
        )
    };
    b.node(
        "footer",
        r(8.0, viewport.h - 38.0, w - 16.0, 34.0),
        "status",
        footer.clone(),
        footer,
        FontRole::Text,
        11.0,
        400,
        p.secondary,
        Some(p.base),
        None,
    );
    Layout {
        nodes: b.nodes,
        actions: b.actions,
        content_height,
        field,
        rows: b.rows,
    }
}

/// Grouped digits for byte offsets: 1204880 becomes "1 204 880".
pub(super) fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

pub(super) fn job_glyph(state: CiBuildState) -> &'static str {
    match state {
        CiBuildState::Success => "✓",
        CiBuildState::Failure => "×",
        CiBuildState::Unstable => "!",
        CiBuildState::Running => "●",
        _ => "○",
    }
}

pub(super) fn cell_fill(state: CiBuildState, p: &Palette) -> [f32; 4] {
    match state {
        CiBuildState::Success => p.success,
        CiBuildState::Failure => p.danger,
        CiBuildState::Unstable => p.warning,
        CiBuildState::Running => p.accent_ink,
        _ => p.pending,
    }
}

/// Short human name for a build trigger.
pub(super) fn trigger_text(trigger: &CiTrigger, lang: Lang) -> String {
    match trigger {
        CiTrigger::Gerrit { change, patchset } => {
            format!(
                "{} {change} PS{patchset} · {}",
                lang.t("Change"),
                lang.t("Gerrit trigger")
            )
        }
        CiTrigger::Timer => lang.t("Timer trigger").into(),
        CiTrigger::User { id } => format!("{} · {id}", lang.t("Manual trigger")),
        CiTrigger::Upstream { job, number } => {
            format!("{} {job} #{number}", lang.t("Upstream trigger"))
        }
        _ => lang.t("Upstream trigger").into(),
    }
}

pub(super) fn tiles_rows(cols: usize) -> usize {
    4_usize.div_ceil(cols)
}

//! One geometry description feeds paint, hit testing and accessibility.
use super::{
    Config, Target, View,
    text::{self, Lang},
};
use crate::board::{
    palette::Palette,
    text::{Fitter, Measure},
};
use pecofence_plugin_api::{
    scene::{NodeStyle, SceneNode},
    *,
};
use spm_contracts::*;
use std::{cell::RefCell, collections::HashMap};

pub struct Layout {
    pub nodes: Vec<SceneNode>,
    pub actions: Vec<(String, Target, RectDip)>,
    pub content_height: f32,
    pub field: RectDip,
}
struct Builder<'a> {
    nodes: Vec<SceneNode>,
    actions: Vec<(String, Target, RectDip)>,
    ids: &'a mut HashMap<String, u64>,
    next: &'a mut u64,
    p: &'a Palette,
    fitter: Fitter<'a>,
    focus: Option<&'a str>,
    top: f32,
    bottom: f32,
}
impl Builder<'_> {
    #[allow(clippy::too_many_arguments)]
    fn node(
        &mut self,
        key: &str,
        rect: RectDip,
        role: &str,
        name: String,
        text: String,
        font: FontRole,
        size: f32,
        weight: u16,
        color: [f32; 4],
        fill: Option<[f32; 4]>,
        target: Option<Target>,
    ) {
        if rect.h <= 0.0 || rect.w <= 0.0 || rect.y < self.top || rect.y + rect.h > self.bottom {
            return;
        }
        let id = *self.ids.entry(key.into()).or_insert_with(|| {
            let n = *self.next;
            *self.next += 1;
            n
        });
        let action = target.map(|t| {
            self.actions.push((key.into(), t, rect));
            16 + self.actions.len() as u64 - 1
        });
        let fitted = self
            .fitter
            .with_font(font)
            .fit(&text, size, weight, (rect.w - 12.0).max(1.0), 1)
            .0;
        self.nodes.push(SceneNode {
            id,
            parent: Some(1),
            rect,
            role: role.into(),
            name,
            text: fitted,
            action,
            style: NodeStyle {
                font,
                size,
                weight,
                foreground: color,
                fill,
                stroke: if self.focus == Some(key) || self.p.hc && action.is_some() {
                    Some(StrokeStyle {
                        rgba: self.p.accent_ink,
                        width: 1.5,
                    })
                } else {
                    None
                },
                radius: 6.0,
                text_inset: [6.0, 0.0],
                align: TextAlign::MiddleLeading,
                path: None,
            },
        });
    }
    fn label(
        &mut self,
        key: &str,
        r: RectDip,
        text: String,
        color: [f32; 4],
        size: f32,
        font: FontRole,
    ) {
        self.node(
            key,
            r,
            "text",
            text.clone(),
            text,
            font,
            size,
            400,
            color,
            None,
            None,
        );
    }
    fn section(&mut self, key: &str, y: f32, w: f32, title: &str, meta: &str) {
        self.node(
            key,
            RectDip {
                x: 14.0,
                y,
                w: w - 28.0,
                h: 24.0,
            },
            "heading",
            format!("{title} {meta}"),
            format!("{title}   {meta}"),
            FontRole::Display,
            13.0,
            700,
            self.p.secondary,
            None,
            None,
        );
    }
}
fn stage_tone(state: CiStageState, p: &Palette) -> [f32; 4] {
    match state {
        CiStageState::Done => p.success,
        CiStageState::Running => p.accent_ink,
        CiStageState::Failed => p.danger,
        CiStageState::Warn => p.warning,
        CiStageState::Pending => p.secondary,
    }
}
fn build_tone(state: CiBuildState, p: &Palette) -> [f32; 4] {
    match state {
        CiBuildState::Success => p.success,
        CiBuildState::Failure => p.danger,
        CiBuildState::Unstable => p.warning,
        CiBuildState::Running => p.accent_ink,
        _ => p.secondary,
    }
}
/// Glyph and word so that build state is never shown by colour alone.
fn build_glyph(b: &CiBuild) -> &'static str {
    match (b.state, b.overdue) {
        (CiBuildState::Running | CiBuildState::Queued, _) => "●",
        (CiBuildState::Success, _) => "✓",
        (CiBuildState::Failure, _) => "×",
        (CiBuildState::Unstable, _) => "!",
        _ => "○",
    }
}
fn r(x: f32, y: f32, w: f32, h: f32) -> RectDip {
    RectDip {
        x,
        y,
        w: w.max(0.0),
        h: h.max(0.0),
    }
}
/// Headline status for a change, from the first stage that needs attention.
fn headline(change: &CiChange, lang: Lang) -> (String, CiStageState) {
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
fn next_role(role: CiJobRole) -> CiJobRole {
    match role {
        CiJobRole::Other => CiJobRole::Verify,
        CiJobRole::Verify => CiJobRole::Daily,
        CiJobRole::Daily => CiJobRole::Release,
        CiJobRole::Release => CiJobRole::Other,
    }
}
/// Shared grid for the six stage columns inside a card.
fn stage_columns(w: f32) -> (f32, f32) {
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
    lang: Lang,
    viewport: RectDip,
    p: &Palette,
    measure: &dyn Measure,
    ids: &mut HashMap<String, u64>,
    next: &mut u64,
) -> Layout {
    let cache = RefCell::new(HashMap::new());
    let fitter = Fitter::new(measure, 1.0, &cache);
    let mut b = Builder {
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
    for (i, (v, label)) in [
        (View::Pipeline, "Pipeline"),
        (View::Release, "Release"),
        (View::Watchlist, "Watchlist"),
    ]
    .into_iter()
    .enumerate()
    {
        let text = lang.t(label);
        b.node(
            &format!("view:{i}"),
            r(14.0 + i as f32 * 94.0, 4.0, 90.0, 28.0),
            "tab",
            format!("{text} {}", if view == v { lang.t("selected") } else { "" }),
            text.into(),
            FontRole::Text,
            12.0,
            if view == v { 700 } else { 400 },
            if view == v { p.accent_ink } else { p.primary },
            Some(if view == v { p.base } else { p.card }),
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
                            Some(stage_tone(stage.state, p)),
                            None,
                        );
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
    }
}

/// The release train: one row or card per daily or release job's current, scheduled and
/// last finished build. `detailed` shows the job's pending merges, median and recent results.
fn release_train(
    b: &mut Builder<'_>,
    s: &CiWatchSnapshot,
    lang: Lang,
    timezone: &str,
    mut y: f32,
    w: f32,
    detailed: bool,
) -> f32 {
    b.section("train:heading", y, w, lang.t("Release train"), "");
    y += 26.0;
    if s.release_train.is_empty() {
        b.label(
            "train:empty",
            r(14.0, y, w - 28.0, 40.0),
            lang.t("Release train empty").into(),
            b.p.secondary,
            12.0,
            FontRole::Text,
        );
        return y + 44.0;
    }
    for entry in &s.release_train {
        let meta = {
            let mut parts = vec![];
            if let Some(n) = entry.pending_merges {
                parts.push(lang.pending(n, entry.pending_mine.unwrap_or(0)));
            }
            if let Some(ratio) = entry.recent {
                parts.push(lang.recent(ratio));
            }
            if let Some(ms) = entry.median_ms {
                parts.push(lang.median(ms));
            }
            parts.join(" · ")
        };
        let rows: Vec<(String, Option<&CiBuild>, String, String, CiBuildState)> = {
            let mut rows = vec![];
            if let Some(build) = &entry.current {
                rows.push((
                    "●".into(),
                    Some(build),
                    lang.state(build.state).into(),
                    text::eta(build, lang, timezone),
                    CiBuildState::Running,
                ));
            }
            if let Some(at) = entry.scheduled_at {
                let time = crate::board::text::short_time(at, timezone)
                    .split_once(' ')
                    .map_or_else(|| "?".into(), |(_, t)| t.to_owned());
                rows.push((
                    "○".into(),
                    None,
                    format!("{} {time}", lang.t("Scheduled")),
                    String::new(),
                    CiBuildState::Queued,
                ));
            }
            if let Some(build) = &entry.last_finished {
                rows.push((
                    build_glyph(build).into(),
                    Some(build),
                    lang.state(build.state).into(),
                    lang.minutes(build.elapsed_ms),
                    build.state,
                ));
            }
            rows
        };
        if detailed {
            let card_h = 36.0 + rows.len() as f32 * 38.0;
            b.node(
                &format!("train:{}", entry.job),
                r(14.0, y, w - 28.0, card_h),
                "group",
                format!("{} · {meta}", entry.job),
                String::new(),
                FontRole::Text,
                13.0,
                400,
                b.p.primary,
                Some(b.p.card),
                None,
            );
            b.label(
                &format!("train:{}:meta", entry.job),
                r(20.0, y + 4.0, w - 40.0, 22.0),
                entry.job.clone(),
                b.p.primary,
                14.0,
                FontRole::Display,
            );
            b.label(
                &format!("train:{}:sub", entry.job),
                r(20.0, y + 22.0, w - 40.0, 16.0),
                meta.clone(),
                b.p.secondary,
                11.0,
                FontRole::Text,
            );
            y += 36.0;
        }
        for (i, (glyph, build, word, detail, state)) in rows.into_iter().enumerate() {
            let key = format!("train:{}:{i}", entry.job);
            let tone = build_tone(state, b.p);
            let target = build.map(|build| {
                Target::Open(
                    build.url_origin.clone(),
                    build.path.clone(),
                    SourceSystem::Jenkins,
                )
            });
            let row_h = 34.0;
            let (x, row_w) = if detailed {
                (20.0, w - 40.0)
            } else {
                (14.0, w - 28.0)
            };
            b.node(
                &key,
                r(x, y, row_w, row_h),
                if target.is_some() { "link" } else { "group" },
                {
                    let number = build.and_then(|b| b.number).map(|n| format!("#{n}"));
                    let head = [Some(entry.job.clone()), number, Some(word.clone())]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if detail.is_empty() {
                        head
                    } else {
                        format!("{head}, {detail}")
                    }
                },
                String::new(),
                FontRole::Text,
                13.0,
                400,
                b.p.primary,
                Some(b.p.card),
                target,
            );
            b.label(
                &format!("{key}:glyph"),
                r(x + 6.0, y + 6.0, 18.0, 22.0),
                glyph,
                tone,
                13.0,
                FontRole::Display,
            );
            let name = if detailed {
                build
                    .and_then(|b| b.number)
                    .map_or_else(|| entry.job.clone(), |n| format!("{} #{n}", entry.job))
            } else {
                entry.job.clone()
            };
            b.label(
                &format!("{key}:name"),
                r(x + 28.0, y + 3.0, row_w - 150.0, 18.0),
                name,
                b.p.primary,
                13.0,
                FontRole::Display,
            );
            b.label(
                &format!("{key}:detail"),
                r(x + 28.0, y + 18.0, row_w - 150.0, 14.0),
                detail,
                b.p.secondary,
                11.0,
                FontRole::Text,
            );
            b.label(
                &format!("{key}:word"),
                r(x + row_w - 116.0, y + 8.0, 110.0, 18.0),
                word,
                tone,
                12.0,
                FontRole::Text,
            );
            y += row_h + 4.0;
        }
        y += 6.0;
    }
    y
}

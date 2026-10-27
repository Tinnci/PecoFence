//! One geometry description feeds paint, hit testing and accessibility.
use super::{
    Config, Scope, Target, View,
    text::{self, Lang},
};
use crate::board::{
    palette::Palette,
    text::{Fitter, Measure},
};
use chrono::{DateTime, Utc};
use pecofence_plugin_api::paper;
use pecofence_plugin_api::{
    scene::{NodeStyle, SceneNode},
    *,
};
use spm_contracts::*;
use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
};

pub struct Layout {
    pub nodes: Vec<SceneNode>,
    pub actions: Vec<(String, Target, RectDip)>,
    pub content_height: f32,
    pub field: RectDip,
    /// Row keys that an entry point can reveal (change cards, release-train rows), with their
    /// top edge in document coordinates (before scrolling).
    pub rows: Vec<(String, f32)>,
}
struct Builder<'a> {
    nodes: Vec<SceneNode>,
    actions: Vec<(String, Target, RectDip)>,
    rows: Vec<(String, f32)>,
    /// The scroll offset this pass was laid out with, so rows can be recorded in document space.
    scroll: f32,
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
    /// Diagonal hatch inside `rect`, drawn as stroked paths so running segments differ from done ones
    /// by pattern as well as colour. Lines stay inside the rectangle.
    fn hatch(&mut self, key: &str, rect: RectDip, color: [f32; 4]) {
        if rect.h <= 0.0 || rect.w <= 0.0 || rect.y < self.top || rect.y + rect.h > self.bottom {
            return;
        }
        let right = rect.x + rect.w;
        let mut k = 0u32;
        while rect.x + k as f32 * 4.0 + rect.h <= right {
            let x = rect.x + k as f32 * 4.0;
            let id = *self
                .ids
                .entry(format!("{key}:hatch:{k}"))
                .or_insert_with(|| {
                    let n = *self.next;
                    *self.next += 1;
                    n
                });
            self.nodes.push(SceneNode {
                id,
                parent: Some(1),
                rect,
                role: "decoration".into(),
                name: String::new(),
                text: String::new(),
                action: None,
                style: NodeStyle {
                    font: FontRole::Text,
                    size: 11.0,
                    weight: 400,
                    foreground: color,
                    fill: None,
                    stroke: Some(StrokeStyle {
                        rgba: color,
                        width: 1.2,
                    }),
                    radius: 0.0,
                    text_inset: [0.0, 0.0],
                    align: TextAlign::TopLeading,
                    path: Some(Path {
                        points: vec![
                            PointDip {
                                x,
                                y: rect.y + rect.h,
                            },
                            PointDip {
                                x: x + rect.h,
                                y: rect.y,
                            },
                        ],
                        closed: false,
                    }),
                },
            });
            k += 1;
        }
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
/// Stage segment fill: pending is light paper, running is a tint, done and others use the tone.
fn segment_fill(state: CiStageState, p: &Palette) -> [f32; 4] {
    match state {
        CiStageState::Pending => p.pending,
        CiStageState::Running => paper::mix(p.base, p.accent_ink, 0.22),
        other => stage_tone(other, p),
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
/// The first-run list: recent jobs and open-change branches as ticks, then the footer. Returns the
/// y after the footer, in the same coordinates as the other views.
fn seeding_body(
    b: &mut Builder<'_>,
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
            b.rows.push((key.clone(), y + b.scroll));
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

/// Stroked polyline or closed polygon, drawn in `rect`'s coordinates; never hit-tested.
/// A zero-width or zero-height box is widened to one dip so the renderer keeps the path.
fn stroke_path(
    b: &mut Builder<'_>,
    key: &str,
    rect: RectDip,
    points: Vec<(f32, f32)>,
    closed: bool,
    color: [f32; 4],
    width: f32,
) {
    let rect = RectDip {
        w: rect.w.max(1.0),
        h: rect.h.max(1.0),
        ..rect
    };
    let id = *b.ids.entry(format!("{key}:path")).or_insert_with(|| {
        let n = *b.next;
        *b.next += 1;
        n
    });
    b.nodes.push(SceneNode {
        id,
        parent: Some(1),
        rect,
        role: "decoration".into(),
        name: String::new(),
        text: String::new(),
        action: None,
        style: NodeStyle {
            font: FontRole::Text,
            size: 11.0,
            weight: 400,
            foreground: color,
            fill: None,
            stroke: Some(StrokeStyle { rgba: color, width }),
            radius: 0.0,
            text_inset: [0.0, 0.0],
            align: TextAlign::TopLeading,
            path: Some(Path {
                points: points.into_iter().map(|(x, y)| PointDip { x, y }).collect(),
                closed,
            }),
        },
    });
}

/// Dashes: short vertical strokes every 6 dip, for code freeze and release lines.
fn dashed_vertical(b: &mut Builder<'_>, key: &str, x: f32, y0: f32, y1: f32, color: [f32; 4]) {
    let mut y = y0;
    let mut k = 0;
    while y < y1 {
        let end = (y + 4.0).min(y1);
        stroke_path(
            b,
            &format!("{key}:dash:{k}"),
            r(x, y, 0.0, end - y),
            vec![(x, y), (x, end)],
            false,
            color,
            1.5,
        );
        y += 6.0;
        k += 1;
    }
}

/// Grouped digits for byte offsets: 1204880 becomes "1 204 880".
fn grouped(n: u64) -> String {
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

fn job_glyph(state: CiBuildState) -> &'static str {
    match state {
        CiBuildState::Success => "✓",
        CiBuildState::Failure => "×",
        CiBuildState::Unstable => "!",
        CiBuildState::Running => "●",
        _ => "○",
    }
}

fn cell_fill(state: CiBuildState, p: &Palette) -> [f32; 4] {
    match state {
        CiBuildState::Success => p.success,
        CiBuildState::Failure => p.danger,
        CiBuildState::Unstable => p.warning,
        CiBuildState::Running => p.accent_ink,
        _ => p.pending,
    }
}

/// Short human name for a build trigger.
fn trigger_text(trigger: &CiTrigger, lang: Lang) -> String {
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

/// The workspace: scope switch, KPIs, stage columns, release lanes, job health and triage.
#[allow(clippy::too_many_arguments)]
fn workspace(
    b: &mut Builder<'_>,
    s: &CiWatchSnapshot,
    lang: Lang,
    config: &Config,
    scope: Scope,
    now: DateTime<Utc>,
    mut y: f32,
    w: f32,
) -> f32 {
    let p = b.p;
    let timezone = config.timezone.as_str();
    // Scope switch: a segmented pill like the view switcher.
    let on = |mine: bool| (scope == Scope::Mine) == mine;
    b.node(
        "scope:track",
        r(14.0, y, 124.0, 28.0),
        "group",
        lang.t("Scope").into(),
        String::new(),
        FontRole::Text,
        12.0,
        400,
        p.primary,
        Some(p.card),
        None,
    );
    for (i, (mine, label)) in [(true, lang.t("Mine")), (false, lang.t("Team"))]
        .into_iter()
        .enumerate()
    {
        let selected = on(mine);
        b.node(
            if mine { "scope:mine" } else { "scope:team" },
            r(16.0 + i as f32 * 60.0, y + 2.0, 58.0, 24.0),
            "tab",
            format!("{label} {}", if selected { lang.t("selected") } else { "" }),
            label.into(),
            FontRole::Text,
            12.0,
            if selected { 700 } else { 400 },
            if selected { p.accent_ink } else { p.primary },
            if selected { Some(p.base) } else { None },
            Some(Target::Scope(if mine { Scope::Mine } else { Scope::Team })),
        );
    }
    y += 40.0;

    // KPIs: four tiles; unavailable values show a dash and an accessible word.
    let k = &s.kpis;
    let unavailable = lang.t("Unavailable");
    let median = |ms: Option<u64>, label: &str| match ms {
        Some(ms) => (lang.age(ms), label.to_owned(), p.primary),
        None => ("—".into(), format!("{label}, {unavailable}"), p.secondary),
    };
    let (sub_val, sub_name, sub_tone) = median(
        k.submit_to_merge_median_ms,
        lang.t("Submit to merge, median"),
    );
    let (rel_val, rel_name, rel_tone) = median(
        k.merge_to_release_median_ms,
        lang.t("Merge to release, median"),
    );
    let verify = match k.verify_pass_7d {
        Some(r) if r.total > 0 => (
            format!("{}/{} · {}%", r.passed, r.total, r.passed * 100 / r.total),
            lang.t("Verify pass, 7 days").to_owned(),
            p.warning,
        ),
        _ => (
            "—".into(),
            format!("{}, {unavailable}", lang.t("Verify pass, 7 days")),
            p.secondary,
        ),
    };
    let failing = (
        format!("{} / {}", k.watched_failing, k.watched_total),
        lang.t("Watched jobs failing").to_owned(),
        if k.watched_failing > 0 {
            p.danger
        } else {
            p.primary
        },
    );
    let tiles = [
        (sub_val, sub_name, sub_tone),
        (rel_val, rel_name, rel_tone),
        verify,
        failing,
    ];
    let cols = if w >= 640.0 { 4 } else { 2 };
    let gap = 8.0;
    let tile_w = ((w - 28.0) - gap * (cols as f32 - 1.0)) / cols as f32;
    for (i, (value, name, tone)) in tiles.into_iter().enumerate() {
        let x = 14.0 + (i % cols) as f32 * (tile_w + gap);
        let ty = y + (i / cols) as f32 * 62.0;
        b.node(
            &format!("kpi:{i}"),
            r(x, ty, tile_w, 56.0),
            "group",
            format!("{name}: {value}"),
            String::new(),
            FontRole::Text,
            12.0,
            400,
            p.primary,
            Some(p.card),
            None,
        );
        b.label(
            &format!("kpi:{i}:value"),
            r(x + 8.0, ty + 4.0, tile_w - 16.0, 26.0),
            value,
            tone,
            20.0,
            FontRole::Display,
        );
        b.label(
            &format!("kpi:{i}:name"),
            r(x + 8.0, ty + 32.0, tile_w - 16.0, 18.0),
            name,
            p.secondary,
            11.0,
            FontRole::Text,
        );
    }
    y += (tiles_rows(cols)) as f32 * 62.0 + 8.0;

    // Stage columns: each change sits in the first stage not yet done; all done means released.
    let mine_name = s.user.gerrit_user.as_str();
    let mut columns: Vec<Vec<(&str, &CiChange, usize)>> = vec![vec![]; CiStageKind::ALL.len()];
    for branch in &s.branches {
        for c in &branch.changes {
            if c.pipeline.is_empty() {
                continue;
            }
            let mine = !mine_name.is_empty() && c.owner == mine_name;
            if scope == Scope::Mine && !mine {
                continue;
            }
            let column = c
                .pipeline
                .iter()
                .position(|st| st.state != CiStageState::Done)
                .unwrap_or(CiStageKind::ALL.len() - 1);
            columns[column].push((branch.id.as_str(), c, column));
        }
    }
    b.section("stages:heading", y, w, lang.t("Stage"), "");
    y += 26.0;
    let wide = w >= 880.0;
    let col_w = if wide {
        (w - 28.0 - 5.0 * 6.0) / 6.0
    } else {
        w - 28.0
    };
    let top = y;
    let mut bottom = y;
    for (ci, kind) in CiStageKind::ALL.into_iter().enumerate() {
        let x = 14.0 + if wide { ci as f32 * (col_w + 6.0) } else { 0.0 };
        let mut cy = if wide { top } else { bottom };
        let count = columns[ci].len();
        b.label(
            &format!("stages:{ci}:name"),
            r(x, cy, col_w, 18.0),
            format!("{} {count}", lang.stage(kind)),
            p.primary,
            12.0,
            FontRole::Display,
        );
        cy += 22.0;
        for (branch_id, change, _) in &columns[ci] {
            let key = format!("ws:{branch_id}:{}", change.number);
            let mine = !mine_name.is_empty() && change.owner == mine_name;
            let (head, head_state) = headline(change, lang);
            let stage = change.pipeline.get(ci).or(change
                .pipeline
                .iter()
                .find(|st| st.state != CiStageState::Done));
            let age = stage
                .and_then(|st| st.entered_at)
                .map(|at| lang.age(((now - at).num_milliseconds().max(0)) as u64))
                .unwrap_or_else(|| "—".into());
            let card_h = 70.0;
            let name = format!(
                "{} {}，{}，{head}，{} {age}{}",
                change.number,
                change.subject,
                lang.stage(kind),
                lang.t("Age in stage"),
                if mine {
                    format!("，{}", lang.t("Your change"))
                } else {
                    String::new()
                }
            );
            b.node(
                &key,
                r(x, cy, col_w, card_h),
                "link",
                name,
                String::new(),
                FontRole::Text,
                12.0,
                400,
                p.primary,
                if mine { Some(p.card) } else { None },
                Some(Target::Open(
                    change.url_origin.clone(),
                    change.path.clone(),
                    SourceSystem::Gerrit,
                )),
            );
            if !mine {
                stroke_path(
                    b,
                    &key,
                    r(x, cy, col_w, card_h),
                    vec![
                        (x, cy),
                        (x + col_w, cy),
                        (x + col_w, cy + card_h),
                        (x, cy + card_h),
                    ],
                    true,
                    p.rule,
                    1.0,
                );
            }
            if matches!(head_state, CiStageState::Failed | CiStageState::Warn) {
                b.node(
                    &format!("{key}:bar"),
                    r(x, cy + 4.0, 3.0, card_h - 8.0),
                    "decoration",
                    String::new(),
                    String::new(),
                    FontRole::Text,
                    11.0,
                    400,
                    p.primary,
                    Some(stage_tone(head_state, p)),
                    None,
                );
            }
            b.label(
                &format!("{key}:num"),
                r(x + 8.0, cy + 3.0, col_w - 16.0, 18.0),
                change.number.to_string(),
                p.primary,
                13.0,
                FontRole::Display,
            );
            b.label(
                &format!("{key}:subject"),
                r(x + 8.0, cy + 21.0, col_w - 16.0, 16.0),
                change.subject.clone(),
                p.primary,
                11.0,
                FontRole::Text,
            );
            b.label(
                &format!("{key}:state"),
                r(x + 8.0, cy + 38.0, col_w - 16.0, 16.0),
                head,
                stage_tone(head_state, p),
                11.0,
                FontRole::Text,
            );
            b.label(
                &format!("{key}:age"),
                r(x + 8.0, cy + 54.0, col_w - 16.0, 14.0),
                format!("{} {age}", lang.t("Age in stage")),
                p.secondary,
                10.0,
                FontRole::Text,
            );
            cy += card_h + 6.0;
        }
        if columns[ci].is_empty() {
            b.label(
                &format!("stages:{ci}:empty"),
                r(x, cy, col_w, 18.0),
                "—".into(),
                p.secondary,
                12.0,
                FontRole::Text,
            );
            cy += 20.0;
        }
        // Side by side, the next section starts below the tallest column.
        bottom = if wide { bottom.max(cy) } else { cy };
    }
    y = bottom.max(top + 20.0) + 8.0;

    // Release lanes on a date axis, from the earliest observed build to the latest milestone or now.
    let lanes: Vec<&WatchedJob> = s
        .jobs
        .iter()
        .filter(|j| matches!(j.role, CiJobRole::Daily | CiJobRole::Release))
        .collect();
    b.section("lanes:heading", y, w, lang.t("Release lanes"), "");
    y += 26.0;
    let starts: Vec<i64> = lanes
        .iter()
        .flat_map(|j| {
            j.history
                .iter()
                .filter_map(|h| h.started_at.map(|t| t.timestamp()))
        })
        .chain(s.milestones.iter().map(|m| m.at.timestamp()))
        .chain(std::iter::once(now.timestamp()))
        .collect();
    let lo = starts.iter().copied().min().unwrap_or(now.timestamp()) - 86_400;
    let hi = starts.iter().copied().max().unwrap_or(now.timestamp()) + 86_400;
    let span = (hi - lo).max(1) as f32;
    let label_w = 150.0;
    let ax0 = 14.0 + label_w;
    let ax1 = w - 14.0 - 8.0;
    let at_x = |t: i64| ax0 + (t - lo) as f32 / span * (ax1 - ax0);
    // Axis ticks.
    for k in 0..=4 {
        let t = lo + (hi - lo) * k / 4;
        let text =
            crate::board::text::short_time(DateTime::from_timestamp(t, 0).unwrap_or(now), timezone)
                .split_once(' ')
                .map_or_else(|| "?".into(), |(d, _)| d.to_owned());
        b.label(
            &format!("lanes:tick:{k}"),
            r(at_x(t) - 24.0, y, 48.0, 14.0),
            text,
            p.secondary,
            10.0,
            FontRole::Text,
        );
    }
    y += 18.0;
    let lanes_top = y;
    let lane_h = 40.0;
    for (li, job) in lanes.iter().enumerate() {
        let ly = lanes_top + li as f32 * lane_h;
        let meta = job.median_ms.map(|ms| lang.median(ms)).unwrap_or_default();
        b.label(
            &format!("lane:{li}:name"),
            r(14.0, ly + 2.0, label_w - 6.0, 16.0),
            job.job.clone(),
            p.primary,
            12.0,
            FontRole::Display,
        );
        b.label(
            &format!("lane:{li}:meta"),
            r(14.0, ly + 19.0, label_w - 6.0, 14.0),
            meta,
            p.secondary,
            10.0,
            FontRole::Text,
        );
        for (bi, mark) in job.history.iter().enumerate() {
            let Some(at) = mark.started_at else { continue };
            let x = at_x(at.timestamp());
            let key = format!("lane:{li}:{bi}");
            let glyph = job_glyph(mark.state);
            let tone = build_tone(mark.state, p);
            b.node(
                &key,
                r(x - 10.0, ly + 8.0, 20.0, 20.0),
                "link",
                format!("{} #{} {}", job.job, mark.number, lang.state(mark.state)),
                glyph.into(),
                FontRole::Display,
                12.0,
                700,
                tone,
                Some(p.base),
                if mark.path.is_empty() {
                    None
                } else {
                    Some(Target::Open(
                        mark.url_origin.clone(),
                        mark.path.clone(),
                        SourceSystem::Jenkins,
                    ))
                },
            );
            let cx = x;
            let cy = ly + 18.0;
            let ring: Vec<(f32, f32)> = (0..16)
                .map(|i| {
                    let a = i as f32 / 16.0 * std::f32::consts::TAU;
                    (cx + 10.0 * a.cos(), cy + 10.0 * a.sin())
                })
                .collect();
            stroke_path(
                b,
                &key,
                r(cx - 10.0, cy - 10.0, 20.0, 20.0),
                ring,
                true,
                tone,
                1.5,
            );
        }
        if let Some(entry) = s.release_train.iter().find(|e| e.job == job.job) {
            if let Some(at) = entry.scheduled_at {
                let x = at_x(at.timestamp());
                b.label(
                    &format!("lane:{li}:scheduled"),
                    r(x - 24.0, ly + 28.0, 48.0, 12.0),
                    lang.t("Scheduled build").into(),
                    p.secondary,
                    9.0,
                    FontRole::Text,
                );
            }
        }
    }
    let lanes_bottom = lanes_top + lanes.len() as f32 * lane_h;
    // Today: solid. Milestones: dashed.
    let today_x = at_x(now.timestamp());
    stroke_path(
        b,
        "lanes:today",
        r(today_x, lanes_top, 0.0, lanes_bottom - lanes_top),
        vec![(today_x, lanes_top), (today_x, lanes_bottom)],
        false,
        p.accent_ink,
        1.5,
    );
    b.label(
        "lanes:today:label",
        r(today_x - 30.0, lanes_bottom + 2.0, 60.0, 14.0),
        lang.t("Today").into(),
        p.accent_ink,
        10.0,
        FontRole::Text,
    );
    for (mi, milestone) in s.milestones.iter().enumerate() {
        let x = at_x(milestone.at.timestamp());
        dashed_vertical(
            b,
            &format!("lanes:milestone:{mi}"),
            x,
            lanes_top,
            lanes_bottom,
            p.secondary,
        );
        let name = match milestone.kind {
            CiMilestoneKind::CodeFreeze => lang.t("Code freeze"),
            CiMilestoneKind::Release => lang.t("Release milestone"),
        };
        let date = crate::board::text::short_time(milestone.at, timezone)
            .split_once(' ')
            .map_or_else(|| "?".into(), |(d, _)| d.to_owned());
        b.label(
            &format!("lanes:milestone:{mi}:label"),
            r(x - 40.0, lanes_bottom + 2.0, 80.0, 14.0),
            format!("{name} {date}"),
            p.secondary,
            10.0,
            FontRole::Text,
        );
    }
    y = lanes_bottom + 22.0;

    // Job health: the last 20 builds per watched job, with a text summary.
    b.section(
        "health:heading",
        y,
        w,
        lang.t("Job health, last 20 builds"),
        "",
    );
    y += 26.0;
    for (ji, job) in s.jobs.iter().enumerate() {
        let finished: Vec<&BuildResultMark> = job
            .history
            .iter()
            .filter(|h| !matches!(h.state, CiBuildState::Running | CiBuildState::Queued))
            .collect();
        let passed = finished
            .iter()
            .filter(|h| h.state == CiBuildState::Success)
            .count();
        let streak = finished
            .iter()
            .rev()
            .take_while(|h| h.state == CiBuildState::Failure)
            .count();
        let (summary, bad) = if streak >= 2 {
            (
                format!("× {} {streak} {}", lang.t("Streak"), lang.t("Builds")),
                true,
            )
        } else if finished
            .last()
            .is_some_and(|h| h.state == CiBuildState::Failure)
        {
            (format!("× {}", lang.t("Last finished failed")), true)
        } else {
            let median = job
                .median_ms
                .map(|ms| format!(" · {}", lang.median(ms)))
                .unwrap_or_default();
            (format!("{passed}/{}{median}", finished.len()), false)
        };
        let words: Vec<&str> = job.history.iter().map(|h| lang.state(h.state)).collect();
        let ry = y + ji as f32 * 30.0;
        b.node(
            &format!("health:{ji}"),
            r(14.0, ry, w - 28.0, 26.0),
            "group",
            format!(
                "{} {} {}：{}，{summary}",
                job.job,
                lang.t("Recent"),
                job.history.len(),
                words.join("，")
            ),
            String::new(),
            FontRole::Text,
            12.0,
            400,
            p.primary,
            None,
            None,
        );
        b.label(
            &format!("health:{ji}:name"),
            r(14.0, ry + 4.0, label_w - 6.0, 18.0),
            job.job.clone(),
            p.primary,
            12.0,
            FontRole::Display,
        );
        let cell = ((ax1 - ax0) / 20.0 - 2.0).clamp(4.0, 14.0);
        for (ci, mark) in job.history.iter().enumerate() {
            b.node(
                &format!("health:{ji}:cell:{ci}"),
                r(ax0 + ci as f32 * (cell + 2.0), ry + 6.0, cell, 14.0),
                "decoration",
                String::new(),
                String::new(),
                FontRole::Text,
                11.0,
                400,
                p.primary,
                Some(cell_fill(mark.state, p)),
                None,
            );
        }
        b.label(
            &format!("health:{ji}:summary"),
            r(w - 124.0, ry + 4.0, 110.0, 18.0),
            summary,
            if bad { p.danger } else { p.secondary },
            11.0,
            FontRole::Text,
        );
    }
    y += s.jobs.len() as f32 * 30.0 + 10.0;

    // Triage for the selected failed build.
    if let Some(t) = &s.triage {
        let build = &t.build;
        let title = format!(
            "{} {} {} #{}",
            job_glyph(build.state),
            lang.t("Failure"),
            build.job,
            build.number.unwrap_or(0)
        );
        let panel_top = y;
        // The panel ground goes in before its contents so the text draws on top of it.
        let ground_at = b.nodes.len();
        let mut py = y + 6.0;
        b.label(
            "triage:title",
            r(14.0 + 10.0, py, w - 48.0, 22.0),
            title.clone(),
            p.danger,
            15.0,
            FontRole::Display,
        );
        py += 26.0;
        let meta = format!(
            "{} · {} {} · {}",
            trigger_text(&build.trigger, lang),
            lang.t("Elapsed"),
            lang.minutes(build.elapsed_ms),
            build.node.clone().unwrap_or_default()
        );
        b.label(
            "triage:meta",
            r(24.0, py, w - 48.0, 16.0),
            meta,
            p.secondary,
            11.0,
            FontRole::Text,
        );
        py += 22.0;
        if t.steps.is_empty() {
            b.label(
                "triage:steps:note",
                r(24.0, py, w - 48.0, 16.0),
                lang.t("Steps not in build record").into(),
                p.secondary,
                11.0,
                FontRole::Text,
            );
            py += 20.0;
        } else {
            for (si, step) in t.steps.iter().enumerate() {
                let glyph = match step.state {
                    CiStepState::Done => "✓",
                    CiStepState::Failed => "×",
                    CiStepState::Pending => "○",
                };
                let tone = match step.state {
                    CiStepState::Done => p.success,
                    CiStepState::Failed => p.danger,
                    CiStepState::Pending => p.secondary,
                };
                let time = step
                    .elapsed_ms
                    .map(|ms| lang.minutes(ms))
                    .unwrap_or_else(|| "—".into());
                b.label(
                    &format!("triage:step:{si}"),
                    r(24.0, py, w - 48.0, 16.0),
                    format!("{glyph} {}   {time}", step.name),
                    tone,
                    12.0,
                    FontRole::Text,
                );
                py += 18.0;
            }
        }
        b.label(
            "triage:log:heading",
            r(24.0, py, w - 48.0, 16.0),
            lang.t("Log first error").into(),
            p.primary,
            12.0,
            FontRole::Display,
        );
        py += 18.0;
        match &t.log {
            Some(log) => {
                for (li, line) in log.text.lines().take(12).enumerate() {
                    b.label(
                        &format!("triage:log:{li}"),
                        r(24.0, py, w - 48.0, 14.0),
                        line.to_owned(),
                        p.primary,
                        11.0,
                        FontRole::Text,
                    );
                    py += 14.0;
                }
                let caption = format!(
                    "{} {}–{} / {} · {}",
                    lang.t("Log window bytes"),
                    grouped(log.start),
                    grouped(log.end),
                    grouped(log.total),
                    lang.t("Log window note")
                );
                b.label(
                    "triage:log:window",
                    r(24.0, py, w - 48.0, 14.0),
                    caption,
                    p.secondary,
                    10.0,
                    FontRole::Text,
                );
                py += 18.0;
            }
            None => {
                b.label(
                    "triage:log:none",
                    r(24.0, py, w - 48.0, 16.0),
                    lang.t("Log window not read").into(),
                    p.secondary,
                    11.0,
                    FontRole::Text,
                );
                py += 20.0;
            }
        }
        b.label(
            "triage:compare:heading",
            r(24.0, py, w - 48.0, 16.0),
            lang.t("Compared with last success").into(),
            p.primary,
            12.0,
            FontRole::Display,
        );
        py += 18.0;
        let change_for = |id: &str| -> String {
            s.branches
                .iter()
                .flat_map(|br| br.changes.iter())
                .find(|c| c.revision == id)
                .map(|c| format!("{} PS{}", c.number, c.patchset))
                .unwrap_or_else(|| id.chars().take(8).collect())
        };
        match &t.last_success {
            Some(success) => {
                let added: Vec<String> = build
                    .commit_ids
                    .iter()
                    .filter(|id| !success.commit_ids.contains(id))
                    .map(|id| change_for(id))
                    .collect();
                let removed: Vec<String> = success
                    .commit_ids
                    .iter()
                    .filter(|id| !build.commit_ids.contains(id))
                    .map(|id| change_for(id))
                    .collect();
                let diff = if added.is_empty() && removed.is_empty() {
                    lang.t("Same changes").to_owned()
                } else {
                    let mut parts = vec![];
                    if !added.is_empty() {
                        parts.push(format!("{} {}", lang.t("Only more"), added.join("，")));
                    }
                    if !removed.is_empty() {
                        parts.push(format!("{} {}", lang.t("Only fewer"), removed.join("，")));
                    }
                    parts.join("；")
                };
                b.label(
                    "triage:compare",
                    r(24.0, py, w - 48.0, 16.0),
                    format!(
                        "#{} ✓ → #{} ×：{diff}",
                        success.number,
                        build.number.unwrap_or(0)
                    ),
                    p.primary,
                    12.0,
                    FontRole::Text,
                );
                py += 18.0;
            }
            None => {
                b.label(
                    "triage:compare",
                    r(24.0, py, w - 48.0, 16.0),
                    lang.t("No last success").into(),
                    p.secondary,
                    11.0,
                    FontRole::Text,
                );
                py += 18.0;
            }
        }
        b.label(
            "triage:compare:scope",
            r(24.0, py, w - 48.0, 16.0),
            lang.t("Compare scope note").into(),
            p.secondary,
            10.0,
            FontRole::Text,
        );
        py += 22.0;
        // Actions only open things.
        let mut actions: Vec<(String, Target)> = vec![(
            lang.t("Open full log").into(),
            Target::Open(
                build.url_origin.clone(),
                build.path.clone(),
                SourceSystem::Jenkins,
            ),
        )];
        if let CiTrigger::Gerrit { change, .. } = build.trigger {
            if let Some(c) = s
                .branches
                .iter()
                .flat_map(|br| br.changes.iter())
                .find(|c| c.number == change)
            {
                actions.push((
                    format!("{} {change}", lang.t("Open in Gerrit")),
                    Target::Open(c.url_origin.clone(), c.path.clone(), SourceSystem::Gerrit),
                ));
            }
        }
        if let Some(origin) = config
            .navigation_origins
            .keys()
            .find(|k| k.starts_with("lark"))
        {
            actions.push((
                lang.t("Ask in Lark").into(),
                Target::Open(origin.clone(), "/".into(), SourceSystem::Lark),
            ));
        }
        for (ai, (label, target)) in actions.into_iter().enumerate() {
            b.node(
                &format!("triage:action:{ai}"),
                r(24.0, py, w - 48.0, 28.0),
                "button",
                label.clone(),
                label,
                FontRole::Text,
                12.0,
                700,
                p.accent_ink,
                Some(p.card),
                Some(target),
            );
            py += 32.0;
        }
        b.label(
            "triage:readonly",
            r(24.0, py, w - 48.0, 16.0),
            lang.t("Read-only note").into(),
            p.secondary,
            10.0,
            FontRole::Text,
        );
        py += 20.0;
        let before = b.nodes.len();
        b.node(
            "triage:panel",
            r(14.0, panel_top, w - 28.0, py - panel_top + 4.0),
            "group",
            title,
            String::new(),
            FontRole::Text,
            12.0,
            400,
            p.primary,
            Some(p.card),
            None,
        );
        if b.nodes.len() > before {
            let panel = b.nodes.pop().expect("panel node was just pushed");
            b.nodes.insert(ground_at, panel);
        }
        y = py + 8.0;
    }
    y
}

fn tiles_rows(cols: usize) -> usize {
    4_usize.div_ceil(cols)
}

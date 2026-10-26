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
fn tone(state: CiBuildState, p: &Palette) -> [f32; 4] {
    match state {
        CiBuildState::Success => p.success,
        CiBuildState::Failure => p.danger,
        CiBuildState::Unstable => p.warning,
        CiBuildState::Running => p.accent_ink,
        _ => p.secondary,
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
            name: lang.t("CI watch").into(),
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
    for (i, (v, label)) in [(View::Builds, "Builds"), (View::Watchlist, "Watchlist")]
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
        if view == View::Builds {
            b.section(
                "mine:heading",
                y,
                w,
                lang.t("My builds"),
                &format!("{} · {}", s.user.jenkins_user, s.my_builds.len()),
            );
            y += 26.0;
            if s.my_builds.is_empty() {
                b.label(
                    "mine:empty",
                    r(14.0, y, w - 28.0, 24.0),
                    lang.t("No builds observed").into(),
                    p.secondary,
                    12.0,
                    FontRole::Text,
                );
                y += 26.0;
            }
            for build in &s.my_builds {
                let key = format!("mine:{}:{:?}", build.job, build.number);
                let state = lang.state(build.state);
                let eta = text::eta(build, lang, &config.timezone);
                let title = format!(
                    "{} {}",
                    build.job,
                    build
                        .number
                        .map_or_else(|| state.into(), |n| format!("#{n}"))
                );
                b.node(
                    &key,
                    r(14.0, y, w - 28.0, 64.0),
                    "link",
                    format!("{title}, {state}, {eta}"),
                    String::new(),
                    FontRole::Text,
                    13.0,
                    400,
                    p.primary,
                    Some(p.card),
                    Some(Target::Open(
                        build.url_origin.clone(),
                        build.path.clone(),
                        SourceSystem::Jenkins,
                    )),
                );
                b.node(
                    &format!("{key}:job"),
                    r(20.0, y + 3.0, w - 40.0, 22.0),
                    "text",
                    title.clone(),
                    title,
                    FontRole::Display,
                    15.0,
                    700,
                    p.primary,
                    None,
                    None,
                );
                let pct = build
                    .estimated_ms
                    .filter(|e| *e > 0)
                    .map(|e| (build.elapsed_ms as f64 / e as f64).clamp(0.0, 1.0) as f32)
                    .unwrap_or(0.0);
                let bar = r(26.0, y + 29.0, w - 52.0, 5.0);
                b.node(
                    &format!("{key}:bar"),
                    bar,
                    "progressbar",
                    format!(
                        "{} · {} / {} · {}",
                        build.job,
                        lang.minutes(build.elapsed_ms),
                        build
                            .estimated_ms
                            .map_or_else(|| "?".into(), |ms| lang.minutes(ms)),
                        state
                    ),
                    String::new(),
                    FontRole::Text,
                    12.0,
                    400,
                    p.primary,
                    Some(p.rule),
                    None,
                );
                if build.overdue {
                    let mut x = bar.x;
                    let mut i = 0;
                    while x < bar.x + bar.w {
                        b.node(
                            &format!("{key}:stripe:{i}"),
                            r(x, bar.y, 4.0_f32.min(bar.x + bar.w - x), bar.h),
                            "decoration",
                            String::new(),
                            String::new(),
                            FontRole::Text,
                            12.0,
                            400,
                            p.warning,
                            Some(p.warning),
                            None,
                        );
                        x += 7.0;
                        i += 1;
                    }
                } else if pct > 0.0 {
                    b.node(
                        &format!("{key}:fill"),
                        r(bar.x, bar.y, bar.w * pct, bar.h),
                        "decoration",
                        String::new(),
                        String::new(),
                        FontRole::Text,
                        12.0,
                        400,
                        p.accent,
                        Some(p.accent),
                        None,
                    );
                }
                let meta = build.queue_reason.clone().unwrap_or_else(|| {
                    format!(
                        "{} · {} / {} · {}",
                        eta,
                        lang.minutes(build.elapsed_ms),
                        build
                            .estimated_ms
                            .map_or_else(|| "?".into(), |ms| lang.minutes(ms)),
                        build.node.as_deref().unwrap_or("—")
                    )
                });
                b.label(
                    &format!("{key}:state"),
                    r(20.0, y + 36.0, w - 40.0, 18.0),
                    format!("{state} · {meta}"),
                    if build.overdue {
                        p.warning
                    } else {
                        tone(build.state, p)
                    },
                    12.0,
                    FontRole::Text,
                );
                y += 70.0;
            }
            b.section(
                "jobs:heading",
                y,
                w,
                lang.t("Watched jobs"),
                &s.jobs.len().to_string(),
            );
            y += 26.0;
            if s.jobs.is_empty() {
                b.label(
                    "jobs:empty",
                    r(14.0, y, w - 28.0, 24.0),
                    lang.t("No watched jobs").into(),
                    p.secondary,
                    12.0,
                    FontRole::Text,
                );
                y += 26.0;
            }
            for job in &s.jobs {
                let key = format!("job:{}", job.id);
                let state = job.last.as_ref().map(|b| b.state);
                let summary = job
                    .history
                    .iter()
                    .map(|h| format!("#{} {}", h.number, lang.state(h.state)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let target = job.last.as_ref().map(|b| {
                    Target::Open(b.url_origin.clone(), b.path.clone(), SourceSystem::Jenkins)
                });
                b.node(
                    &key,
                    r(14.0, y, w - 28.0, 40.0),
                    "link",
                    format!("{} · {} · {summary}", job.job, lang.health(job.health)),
                    String::new(),
                    FontRole::Text,
                    13.0,
                    400,
                    p.primary,
                    Some(p.card),
                    target,
                );
                let history_w = 92.0_f32.min((w - 28.0) * 0.24);
                let name_w = (w - 28.0 - history_w - 76.0).max(32.0);
                b.node(
                    &format!("{key}:name"),
                    r(20.0, y + 1.0, name_w, 21.0),
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
                let meta = if job.health != WatchHealth::Ok {
                    lang.health(job.health).into()
                } else {
                    job.last.as_ref().map_or_else(
                        || lang.t("Not observed").into(),
                        |b| {
                            format!(
                                "#{} · {} · {}",
                                b.number.unwrap_or(0),
                                lang.state(b.state),
                                lang.minutes(b.elapsed_ms)
                            )
                        },
                    )
                };
                b.label(
                    &format!("{key}:meta"),
                    r(20.0, y + 22.0, name_w, 16.0),
                    meta,
                    if job.health == WatchHealth::Ok {
                        p.secondary
                    } else {
                        p.warning
                    },
                    11.0,
                    FontRole::Text,
                );
                let hx = w - 14.0 - 72.0 - history_w;
                b.node(
                    &format!("{key}:history"),
                    r(hx, y + 9.0, history_w, 22.0),
                    "img",
                    summary,
                    String::new(),
                    FontRole::Text,
                    12.0,
                    400,
                    p.primary,
                    None,
                    None,
                );
                for (i, h) in job.history.iter().enumerate() {
                    let height = if h.state == CiBuildState::Failure {
                        18.0
                    } else if h.state == CiBuildState::Running {
                        14.0
                    } else {
                        10.0
                    };
                    b.node(
                        &format!("{key}:h:{i}"),
                        r(
                            hx + i as f32 * history_w / 10.0,
                            y + 30.0 - height,
                            (history_w / 10.0 - 2.0).max(1.0),
                            height,
                        ),
                        "img",
                        format!("#{} {}", h.number, lang.state(h.state)),
                        String::new(),
                        FontRole::Text,
                        12.0,
                        400,
                        tone(h.state, p),
                        Some(tone(h.state, p)),
                        None,
                    );
                }
                b.label(
                    &format!("{key}:result"),
                    r(w - 84.0, y + 8.0, 70.0, 24.0),
                    state.map_or_else(|| "—".into(), |s| lang.state(s).into()),
                    state.map_or(p.secondary, |s| tone(s, p)),
                    11.0,
                    FontRole::Text,
                );
                y += 44.0;
            }
            b.section("branches:heading", y, w, lang.t("Gerrit branches"), "");
            y += 26.0;
            if s.branches.is_empty() {
                b.label(
                    "branches:empty",
                    r(14.0, y, w - 28.0, 24.0),
                    lang.t("No watched branches").into(),
                    p.secondary,
                    12.0,
                    FontRole::Text,
                );
                y += 26.0;
            }
            for branch in &s.branches {
                for change in &branch.changes {
                    let key = format!("change:{}:{}", branch.id, change.number);
                    let status = if change.mergeable == Some(false) {
                        "Conflict"
                    } else if change.status == CiChangeStatus::Merged {
                        "Merged"
                    } else if change.status == CiChangeStatus::Abandoned {
                        "Abandoned"
                    } else if change.labels.get("Verified").is_some_and(|v| *v < 0)
                        || change
                            .ci
                            .as_ref()
                            .is_some_and(|b| b.state == CiBuildState::Failure)
                    {
                        "Needs repair"
                    } else if change.labels.get("Verified").is_some_and(|v| *v > 0)
                        && change.labels.get("Code-Review").is_some_and(|v| *v >= 2)
                    {
                        "Ready to merge"
                    } else {
                        "Awaiting verification"
                    };
                    let title = format!("{}  {}", change.number, change.subject);
                    b.node(
                        &key,
                        r(14.0, y, w - 28.0, 46.0),
                        "link",
                        format!(
                            "{title} · {} PS{} · {}",
                            branch.branch,
                            change.patchset,
                            lang.t(status)
                        ),
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
                        &format!("{key}:title"),
                        r(20.0, y, w - 40.0, 22.0),
                        title,
                        p.primary,
                        13.0,
                        FontRole::Text,
                    );
                    let ci = change.ci.as_ref().map_or_else(
                        || lang.t("CI unknown").into(),
                        |b| format!("{} #{}", lang.state(b.state), b.number.unwrap_or(0)),
                    );
                    b.label(
                        &format!("{key}:ci"),
                        r(20.0, y + 23.0, (w - 40.0) * 0.57, 22.0),
                        format!("{} · PS{} · {ci}", branch.branch, change.patchset),
                        change.ci.as_ref().map_or(p.secondary, |b| tone(b.state, p)),
                        11.0,
                        FontRole::Text,
                    );
                    let mut x = w - 14.0 - 150.0;
                    for (label, value) in &change.labels {
                        let short = if label == "Verified" { "V" } else { "CR" };
                        let text = format!("{short}{value:+}");
                        b.node(
                            &format!("{key}:pill:{label}"),
                            r(x, y + 25.0, 38.0, 18.0),
                            "text",
                            format!("{label} {value:+}"),
                            text,
                            FontRole::Display,
                            11.0,
                            700,
                            if *value < 0 {
                                p.danger
                            } else if *value > 0 {
                                p.success
                            } else {
                                p.secondary
                            },
                            Some(p.base),
                            None,
                        );
                        x += 40.0;
                    }
                    b.label(
                        &format!("{key}:status"),
                        r(w - 84.0, y + 25.0, 70.0, 18.0),
                        lang.t(status).into(),
                        if status == "Needs repair" {
                            p.danger
                        } else if status == "Ready to merge" {
                            p.success
                        } else {
                            p.warning
                        },
                        11.0,
                        FontRole::Text,
                    );
                    y += 48.0;
                }
                if branch.truncated {
                    b.label(
                        &format!("branch:{}:truncated", branch.id),
                        r(14.0, y, w - 28.0, 20.0),
                        format!("{} · {}", branch.branch, lang.t("Partial results")),
                        p.warning,
                        12.0,
                        FontRole::Text,
                    );
                    y += 22.0;
                }
            }
        } else {
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
            for (kind, title) in [(false, "Watched jobs"), (true, "Gerrit branches")] {
                b.section(
                    if kind { "list:branches" } else { "list:jobs" },
                    y,
                    w,
                    lang.t(title),
                    "",
                );
                y += 26.0;
                let entries: Vec<_> = if kind {
                    s.branches
                        .iter()
                        .map(|e| {
                            (
                                e.id.clone(),
                                format!("{} : {}", e.project, e.branch),
                                e.only_mine,
                                e.include_merged,
                                String::new(),
                            )
                        })
                        .collect()
                } else {
                    s.jobs
                        .iter()
                        .map(|e| {
                            (
                                e.id.clone(),
                                e.job.clone(),
                                e.only_mine,
                                e.notify_on_failure,
                                lang.health(e.health).into(),
                            )
                        })
                        .collect()
                };
                for (id, name, mine, second, health) in entries {
                    let key = format!("watch:{id}");
                    b.node(
                        &key,
                        r(14.0, y, w - 28.0, 62.0),
                        "group",
                        format!("{name} {health}"),
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
                        r(20.0, y, w - 82.0, 24.0),
                        "text",
                        name.clone(),
                        format!("{name} {health}"),
                        FontRole::Display,
                        14.0,
                        700,
                        p.primary,
                        None,
                        None,
                    );
                    for (i, (flag, on, label)) in [
                        (
                            CiWatchFlag::OnlyMine,
                            mine,
                            if kind {
                                "Only mine"
                            } else {
                                "Only my triggers"
                            },
                        ),
                        (
                            if kind {
                                CiWatchFlag::IncludeMerged
                            } else {
                                CiWatchFlag::NotifyOnFailure
                            },
                            second,
                            if kind {
                                "Include merged"
                            } else {
                                "Failure alert"
                            },
                        ),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let text = lang.t(label);
                        b.node(
                            &format!("{key}:toggle:{i}"),
                            r(20.0 + i as f32 * 132.0, y + 30.0, 126.0, 26.0),
                            "switch",
                            format!("{name}, {text}: {}", lang.t(if on { "on" } else { "off" })),
                            format!("{} {text}", if on { "●" } else { "○" }),
                            FontRole::Text,
                            12.0,
                            if on { 700 } else { 400 },
                            if on { p.accent_ink } else { p.secondary },
                            Some(if on { p.base } else { p.card }),
                            Some(Target::Toggle(id.clone(), flag, !on)),
                        );
                    }
                    b.node(
                        &format!("{key}:remove"),
                        r(w - 48.0, y + 16.0, 30.0, 30.0),
                        "button",
                        format!("{} {name}", lang.t("Remove")),
                        "×".into(),
                        FontRole::Text,
                        14.0,
                        400,
                        p.secondary,
                        None,
                        Some(Target::Remove(id)),
                    );
                    y += 68.0;
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
                                |at| crate::board::text::short_time(at, &config.timezone)
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

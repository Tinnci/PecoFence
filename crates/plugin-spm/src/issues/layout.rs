//! Geometry of the issue panel. Narrow: the 我的问题 inbox, or one issue. Wide: the 问题
//! workspace with list, detail and people. The gate header stays put; the panes scroll.
use super::{
    Mark, PROPOSAL_ACTIONS, ProposalAction,
    panel::{State, Target, View, WORKSPACE_WIDTH},
    text::Lang,
};
use crate::board::{
    palette::Palette,
    text::{Fitter, Measure},
};
use crate::scene_builder::{Builder, r};
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
}

type B<'a> = Builder<'a, Target>;

const LIST_W: f32 = 330.0;
const SIDE_W: f32 = 290.0;

fn kind_tone(kind: IssueKind, p: &Palette) -> [f32; 4] {
    match kind {
        IssueKind::Ar => p.accent_ink,
        IssueKind::Dts => p.danger,
        IssueKind::Risk => p.warning,
        _ => p.secondary,
    }
}

/// The word that says why an item is in the list. Always a word, with its tone.
fn flag(item: &IssueItem, lang: Lang, p: &Palette) -> (String, [f32; 4]) {
    if let Some(s) = item.severity {
        let tone = match s {
            DefectSeverity::Fatal | DefectSeverity::Major => p.danger,
            DefectSeverity::Normal => p.warning,
            DefectSeverity::Hint => p.secondary,
        };
        return (lang.severity(s).into(), tone);
    }
    if item.conflict.is_some() {
        return (format!("⇄ {}", lang.t("Source conflict flag")), p.warning);
    }
    if item.overdue {
        return (format!("! {}", lang.t("Overdue")), p.danger);
    }
    if item
        .queues
        .contains(&IssueQueue::AwaitingAssignmentConfirmation)
    {
        return (format!("? {}", lang.t("Owner unconfirmed")), p.warning);
    }
    if let Some(gate) = item.blocks_gate {
        return (
            format!("⊘ {} {}", lang.t("Blocks gate"), super::gate_name(gate)),
            p.danger,
        );
    }
    (item.status.clone(), p.secondary)
}

fn meta(item: &IssueItem, lang: Lang) -> String {
    let mut parts = vec![];
    match &item.conflict {
        Some(c) => parts.push(format!(
            "{} {} · {} {}",
            source_name(c.first.source),
            c.first.value,
            source_name(c.second.source),
            c.second.value
        )),
        None => {
            if let Some(owner) = &item.owner {
                parts.push(owner.clone());
            }
            parts.push(item.status.clone());
        }
    }
    if let Some(due) = item.due_at {
        parts.push(format!("{} {}", lang.t("Due"), due.format("%m-%d")));
    }
    parts.join(" · ")
}

fn source_name(source: SourceSystem) -> &'static str {
    match source {
        SourceSystem::Lark => "Lark",
        SourceSystem::Meegle => "Meegle",
        SourceSystem::Gerrit => "Gerrit",
        SourceSystem::Jenkins => "Jenkins",
        _ => "SPM",
    }
}

/// Greedy line breaking with the measured face; the last kept line ends in an ellipsis.
fn wrap(fitter: &Fitter<'_>, text: &str, size: f32, width: f32, max_lines: usize) -> Vec<String> {
    let mut lines = vec![];
    let mut line = String::new();
    for ch in text.chars() {
        if ch == '\n' {
            lines.push(std::mem::take(&mut line));
            continue;
        }
        line.push(ch);
        if fitter.width(&line, size, 400) > width {
            line.pop();
            lines.push(std::mem::replace(&mut line, ch.to_string()));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            last.pop();
            last.push('…');
        }
    }
    lines
}

fn text_lines(b: &mut B<'_>, key: &str, x: f32, y: f32, w: f32, text: &str, lines: usize) -> f32 {
    let p = *b.p;
    let wrapped = wrap(
        &b.fitter.with_font(FontRole::PaperBody),
        text,
        13.0,
        w - 12.0,
        lines,
    );
    for (i, line) in wrapped.iter().enumerate() {
        b.label(
            &format!("{key}:{i}"),
            r(x, y + i as f32 * 19.0, w, 19.0),
            line.clone(),
            p.primary,
            13.0,
            FontRole::PaperBody,
        );
    }
    wrapped.len() as f32 * 19.0
}

fn heading(b: &mut B<'_>, key: &str, x: f32, y: f32, w: f32, title: &str, meta: &str) {
    let p = *b.p;
    b.node(
        key,
        r(x, y, w, 22.0),
        "heading",
        format!("{title} {meta}"),
        if meta.is_empty() {
            title.into()
        } else {
            format!("{title}   {meta}")
        },
        FontRole::Display,
        13.0,
        700,
        p.secondary,
        None,
        None,
    );
}

/// A pill-shaped control. Selected pills sit on the ground colour with accent ink.
#[allow(clippy::too_many_arguments)]
fn pill(b: &mut B<'_>, key: &str, x: f32, y: f32, text: &str, on: bool, target: Target) -> f32 {
    let p = *b.p;
    let w = b.fitter.width(text, 12.0, 700) + 26.0;
    b.node(
        key,
        r(x, y, w, 26.0),
        "button",
        if on {
            format!("{text}, selected")
        } else {
            text.into()
        },
        text.into(),
        FontRole::Text,
        12.0,
        if on { 700 } else { 400 },
        if on { p.chip_on_text } else { p.primary },
        Some(if on { p.chip_on } else { p.card }),
        Some(target),
    );
    w
}

#[allow(clippy::too_many_arguments)]
pub fn build(
    state: &State,
    focus: Option<&str>,
    scroll: f32,
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
        rows: vec![],
        scroll,
        nodes: vec![SceneNode {
            id: 1,
            parent: None,
            rect: viewport,
            role: "pane".into(),
            name: lang.t("Issues").into(),
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
    let wide = w >= WORKSPACE_WIDTH;
    let field = header(&mut b, state, lang, w, wide);
    let top = if wide { 112.0 } else { 108.0 };
    b.top = top;
    let y = top - scroll;
    let bottom = match (state.view, wide) {
        (View::Search, true) => {
            let main = search(&mut b, state, lang, 14.0, y, w - SIDE_W - 28.0);
            let side = suggestions(&mut b, state, lang, w - SIDE_W, y, SIDE_W - 14.0);
            main.max(side)
        }
        (View::Search, false) => {
            let main = search(&mut b, state, lang, 14.0, y, w - 28.0);
            suggestions(&mut b, state, lang, 14.0, main + 12.0, w - 28.0)
        }
        (View::Issues, true) => {
            let list = issue_list(&mut b, state, lang, 0.0, y, LIST_W);
            let main = detail(
                &mut b,
                state,
                lang,
                LIST_W + 8.0,
                y,
                w - LIST_W - SIDE_W - 16.0,
            );
            let side = people(&mut b, state, lang, w - SIDE_W, y, SIDE_W - 14.0);
            list.max(main).max(side)
        }
        (View::Issues, false) if state.narrow_detail && state.detail.is_some() => {
            let p = *b.p;
            b.node(
                "back",
                r(14.0, y, 140.0, 28.0),
                "button",
                lang.t("Back to my issues").into(),
                format!("‹ {}", lang.t("My issues")),
                FontRole::Text,
                13.0,
                700,
                p.accent_ink,
                None,
                Some(Target::Back),
            );
            let main = detail(&mut b, state, lang, 6.0, y + 34.0, w - 12.0);
            people(&mut b, state, lang, 14.0, main + 8.0, w - 28.0)
        }
        (View::Issues, false) => {
            let list = inbox(&mut b, state, lang, 14.0, y, w - 28.0);
            what_if(&mut b, state, lang, 14.0, list + 12.0, w - 28.0)
        }
    };
    Layout {
        nodes: b.nodes,
        actions: b.actions,
        content_height: bottom + scroll + 24.0,
        field,
    }
}

/// Search field, IPD phase strip (wide) and the gate readiness. Returns the field rectangle.
fn header(b: &mut B<'_>, state: &State, lang: Lang, w: f32, wide: bool) -> RectDip {
    let p = *b.p;
    let field_w = if wide { 360.0 } else { w - 28.0 };
    let field = r(14.0, 6.0, field_w, 30.0);
    b.node(
        "find",
        field,
        "searchbox",
        lang.t("Search issues, messages or people").into(),
        if state.query.is_empty() {
            format!("⌕  {}", lang.t("Search issues, messages or people"))
        } else {
            format!("⌕  {}", state.query)
        },
        FontRole::Text,
        13.0,
        400,
        if state.query.is_empty() {
            p.secondary
        } else {
            p.primary
        },
        Some(p.card),
        Some(Target::Find),
    );
    let Some(readiness) = &state.readiness else {
        b.label(
            "loading",
            r(14.0, 44.0, w - 28.0, 24.0),
            lang.t("Loading issues…").into(),
            p.secondary,
            12.0,
            FontRole::Text,
        );
        return field;
    };
    let view = super::readiness_view(lang, readiness);
    if wide {
        // The phase strip: the current gate is filled and named "current", never colour alone.
        let cells = super::phase_strip(lang, &readiness.track);
        let room = w - 14.0 - (14.0 + field_w + 16.0);
        let cell_w = (room / cells.len().max(1) as f32 - 4.0).clamp(40.0, 104.0);
        let mut x = w - 14.0 - cells.len() as f32 * (cell_w + 4.0);
        for (i, c) in cells.iter().enumerate() {
            b.node(
                &format!("phase:{i}"),
                r(x, 6.0, cell_w, 30.0),
                "listitem",
                format!("{} {} {}", c.phase, c.gate, c.state),
                format!("{} {}", c.phase, c.gate),
                FontRole::Text,
                12.0,
                if c.current { 700 } else { 400 },
                if c.current { p.chip_on_text } else { p.primary },
                Some(if c.current { p.accent } else { p.card }),
                None,
            );
            x += cell_w + 4.0;
        }
    }
    // Readiness: gate and date, DI against its threshold, each criterion's word, the verdict.
    let y = 44.0;
    let h = if wide { 58.0 } else { 56.0 };
    b.node(
        "gate",
        r(14.0, y, w - 28.0, h),
        "group",
        format!("{} {} {}", view.gate, view.di_text, view.verdict),
        String::new(),
        FontRole::Text,
        12.0,
        400,
        p.primary,
        Some(p.card),
        None,
    );
    let due = readiness
        .next_gate_due
        .map(|d| d.format("%m-%d").to_string())
        .unwrap_or_default();
    b.label(
        "gate:name",
        r(24.0, y + 6.0, 170.0, 18.0),
        lang.t("Next gate").into(),
        p.secondary,
        11.0,
        FontRole::Display,
    );
    b.node(
        "gate:title",
        r(24.0, y + 24.0, 170.0, 26.0),
        "text",
        format!("{} {due}", view.gate),
        format!("{} · {due}", view.gate),
        FontRole::Display,
        18.0,
        700,
        p.primary,
        None,
        None,
    );
    let di_tone = if view.di > view.threshold {
        p.danger
    } else {
        p.success
    };
    b.node(
        "gate:di",
        r(200.0, y + 8.0, 130.0, 28.0),
        "text",
        view.di_text.clone(),
        view.di_text.clone(),
        FontRole::Display,
        20.0,
        700,
        di_tone,
        None,
        None,
    );
    // The DI bar: value fill and a threshold tick, both fractions of the larger number.
    let bar_x = 206.0;
    let bar_w = 118.0;
    b.node(
        "gate:bar",
        r(bar_x, y + 40.0, bar_w, 6.0),
        "decoration",
        String::new(),
        String::new(),
        FontRole::Text,
        11.0,
        400,
        p.primary,
        Some(p.rule),
        None,
    );
    b.node(
        "gate:fill",
        r(bar_x, y + 40.0, bar_w * view.bar_value as f32, 6.0),
        "decoration",
        String::new(),
        String::new(),
        FontRole::Text,
        11.0,
        400,
        p.primary,
        Some(di_tone),
        None,
    );
    b.node(
        "gate:tick",
        r(
            bar_x + bar_w * view.bar_threshold as f32 - 1.0,
            y + 36.0,
            2.0,
            14.0,
        ),
        "decoration",
        String::new(),
        String::new(),
        FontRole::Text,
        11.0,
        400,
        p.primary,
        Some(p.primary),
        None,
    );
    let verdict_w = 110.0;
    b.node(
        "gate:verdict",
        r(w - 24.0 - verdict_w, y + 16.0, verdict_w, 26.0),
        "status",
        view.verdict.into(),
        format!(
            "{} {}",
            if readiness.ready { "✓" } else { "×" },
            view.verdict
        ),
        FontRole::Text,
        13.0,
        700,
        if readiness.ready { p.success } else { p.danger },
        None,
        None,
    );
    if wide {
        // Failing criteria first; each shows value, threshold and a pass word.
        let mut x = 344.0;
        let mut criteria: Vec<_> = view.criteria.iter().collect();
        criteria.sort_by_key(|c| c.pass);
        for (i, c) in criteria.iter().enumerate() {
            let text = format!("{} {}", if c.pass { "✓" } else { "×" }, c.label);
            let cw = b.fitter.width(&text, 12.0, 400) + 16.0;
            if x + cw > w - 24.0 - verdict_w {
                break;
            }
            b.node(
                &format!("criterion:{i}"),
                r(x, y + 17.0, cw, 24.0),
                "text",
                format!("{} {} / {} {}", c.label, c.value, c.threshold, c.pass_word),
                text,
                FontRole::Text,
                12.0,
                if c.pass { 400 } else { 700 },
                if c.pass { p.secondary } else { p.danger },
                None,
                None,
            );
            x += cw + 4.0;
        }
    }
    field
}

fn visible_items(state: &State) -> Vec<&IssueItem> {
    let Some(list) = &state.list else {
        return vec![];
    };
    list.items
        .iter()
        .filter(|i| state.queue.is_none_or(|q| i.queues.contains(&q)))
        .collect()
}

fn queue_chips(b: &mut B<'_>, state: &State, lang: Lang, x0: f32, mut y: f32, w: f32) -> f32 {
    let (Some(list), Some(readiness)) = (&state.list, &state.readiness) else {
        return y;
    };
    let mut x = x0;
    for (i, chip) in super::queue_chips(lang, &list.queue_counts, readiness.next_gate)
        .iter()
        .enumerate()
    {
        let text = format!("{} {}", chip.label, chip.count);
        let cw = b.fitter.width(&text, 12.0, 700) + 26.0;
        if x + cw > x0 + w {
            x = x0;
            y += 30.0;
        }
        pill(
            b,
            &format!("queue:{i}"),
            x,
            y,
            &text,
            state.queue == Some(chip.queue),
            Target::Queue(Some(chip.queue)),
        );
        x += cw + 6.0;
    }
    y + 34.0
}

/// One list row: type tag, id and flag word, the title, then owner, status and due date.
fn item_row(b: &mut B<'_>, item: &IssueItem, selected: bool, lang: Lang, x: f32, y: f32, w: f32) {
    let p = *b.p;
    let key = format!("item:{}", item.id);
    let (flag_text, flag_tone) = flag(item, lang, &p);
    b.node(
        &key,
        r(x, y, w, 60.0),
        "listitem",
        format!(
            "{} {} {}, {flag_text}, {}",
            lang.kind(item.kind),
            item.id,
            item.title,
            meta(item, lang)
        ),
        String::new(),
        FontRole::Text,
        13.0,
        400,
        p.primary,
        Some(if selected { p.selected } else { p.card }),
        Some(Target::Select(item.id.clone())),
    );
    let tone = kind_tone(item.kind, &p);
    b.node(
        &format!("{key}:kind"),
        r(x + 8.0, y + 6.0, 40.0, 18.0),
        "text",
        lang.kind(item.kind).into(),
        lang.kind(item.kind).into(),
        FontRole::Display,
        11.0,
        700,
        tone,
        None,
        None,
    );
    b.label(
        &format!("{key}:id"),
        r(x + 44.0, y + 4.0, 80.0, 22.0),
        item.id.clone(),
        p.primary,
        15.0,
        FontRole::Display,
    );
    let fw = (b.fitter.width(&flag_text, 12.0, 700) + 14.0).min(w * 0.5);
    b.node(
        &format!("{key}:flag"),
        r(x + w - fw - 4.0, y + 5.0, fw, 20.0),
        "text",
        flag_text.clone(),
        flag_text,
        FontRole::Text,
        12.0,
        700,
        flag_tone,
        None,
        None,
    );
    b.label(
        &format!("{key}:title"),
        r(x + 2.0, y + 24.0, w - 4.0, 18.0),
        item.title.clone(),
        p.primary,
        13.0,
        FontRole::PaperBody,
    );
    b.label(
        &format!("{key}:meta"),
        r(x + 2.0, y + 41.0, w - 4.0, 16.0),
        meta(item, lang),
        p.secondary,
        11.0,
        FontRole::Text,
    );
}

fn issue_list(b: &mut B<'_>, state: &State, lang: Lang, x: f32, y: f32, w: f32) -> f32 {
    let mut y = queue_chips(b, state, lang, x + 14.0, y + 8.0, w - 20.0);
    for item in visible_items(state) {
        item_row(
            b,
            item,
            state.selected.as_deref() == Some(item.id.as_str()),
            lang,
            x + 10.0,
            y,
            w - 16.0,
        );
        y += 64.0;
    }
    y
}

/// The narrow inbox: one group per queue, each item in the first group it belongs to.
fn inbox(b: &mut B<'_>, state: &State, lang: Lang, x: f32, mut y: f32, w: f32) -> f32 {
    let (Some(list), Some(readiness)) = (&state.list, &state.readiness) else {
        return y;
    };
    let mut shown = BTreeSet::new();
    for chip in super::queue_chips(lang, &list.queue_counts, readiness.next_gate) {
        let items: Vec<&IssueItem> = list
            .items
            .iter()
            .filter(|i| i.queues.contains(&chip.queue) && !shown.contains(&i.id))
            .collect();
        if items.is_empty() {
            continue;
        }
        heading(
            b,
            &format!("group:{:?}", chip.queue),
            x,
            y + 4.0,
            w,
            &chip.label,
            &items.len().to_string(),
        );
        y += 28.0;
        for item in items {
            shown.insert(item.id.clone());
            item_row(b, item, false, lang, x, y, w);
            y += 64.0;
        }
    }
    if shown.is_empty() {
        b.label(
            "inbox:empty",
            r(x, y, w, 24.0),
            lang.t("Nothing needs you right now").into(),
            b.p.secondary,
            13.0,
            FontRole::Text,
        );
        y += 28.0;
    }
    y
}

fn detail(b: &mut B<'_>, state: &State, lang: Lang, x: f32, y: f32, w: f32) -> f32 {
    let p = *b.p;
    let Some(d) = &state.detail else {
        b.label(
            "detail:empty",
            r(x + 8.0, y + 10.0, w - 16.0, 24.0),
            lang.t(if state.selected.is_some() {
                "Loading issues…"
            } else {
                "Select an issue"
            })
            .into(),
            p.secondary,
            13.0,
            FontRole::Text,
        );
        return y + 40.0;
    };
    let item = &d.item;
    let mut y = y + 8.0;
    let idw = b
        .fitter
        .with_font(FontRole::Display)
        .width(&item.id, 24.0, 700)
        + 16.0;
    b.node(
        "detail:id",
        r(x + 8.0, y, idw, 34.0),
        "heading",
        format!("{} {}", item.id, item.title),
        item.id.clone(),
        FontRole::Display,
        24.0,
        700,
        p.primary,
        None,
        None,
    );
    let title = wrap(
        &b.fitter.with_font(FontRole::Serif),
        &item.title,
        18.0,
        w - idw - 32.0,
        2,
    );
    for (i, line) in title.iter().enumerate() {
        b.label(
            &format!("detail:title:{i}"),
            r(
                x + idw + 8.0,
                y + 5.0 + i as f32 * 24.0,
                w - idw - 16.0,
                24.0,
            ),
            line.clone(),
            p.primary,
            18.0,
            FontRole::Serif,
        );
    }
    y += 10.0 + 24.0 * title.len().max(1) as f32 + 6.0;
    // Facts as chips: type and trace, status, severity, owner, the gate this blocks, conflicts.
    let mut chips = vec![(lang.kind(item.kind).to_string(), kind_tone(item.kind, &p))];
    if let Some(trace) = &item.trace
        && !trace.parents.is_empty()
    {
        chips.push((
            format!("{} {}", lang.t("Trace"), trace.parents.join(" / ")),
            p.secondary,
        ));
    }
    chips.push((item.status.clone(), p.primary));
    if let Some(s) = item.severity {
        chips.push((
            format!("{} {}", lang.t("Severity"), lang.severity(s)),
            p.danger,
        ));
    }
    if let Some(due) = item.due_at {
        chips.push((
            format!("{} {}", lang.t("Due"), due.format("%m-%d")),
            p.secondary,
        ));
    }
    if item.blocks_gate.is_some() || d.gate_impact.di_contribution > 0.0 {
        chips.push((
            format!(
                "{} {} · DI +{}",
                lang.t("Affects"),
                super::gate_name(d.gate_impact.next_gate),
                super::number(d.gate_impact.di_contribution)
            ),
            p.danger,
        ));
    }
    if let Some(c) = &item.conflict {
        chips.push((
            format!(
                "⇄ {} {} · {} {}",
                source_name(c.first.source),
                c.first.value,
                source_name(c.second.source),
                c.second.value
            ),
            p.warning,
        ));
    } else if item.owner.is_none() {
        chips.push((format!("? {}", lang.t("Owner unconfirmed")), p.warning));
    }
    let mut cx = x + 8.0;
    for (i, (text, tone)) in chips.iter().enumerate() {
        let cw = b.fitter.width(text, 12.0, 700) + 18.0;
        if cx + cw > x + w {
            cx = x + 8.0;
            y += 28.0;
        }
        b.node(
            &format!("detail:chip:{i}"),
            r(cx, y, cw, 24.0),
            "text",
            text.clone(),
            text.clone(),
            FontRole::Text,
            12.0,
            700,
            *tone,
            Some(p.card),
            None,
        );
        cx += cw + 6.0;
    }
    y += 34.0;
    if let Some(strip) = super::suggestion_strip(lang, d) {
        b.node(
            "detail:suggest",
            r(x + 8.0, y, w - 16.0, 30.0),
            "link",
            strip.clone(),
            format!("✦ {strip} ›"),
            FontRole::Text,
            12.0,
            700,
            p.accent_ink,
            Some(p.chip),
            Some(Target::View(View::Search)),
        );
        y += 38.0;
    }
    if let Some(notice) = &state.notice {
        y += 2.0 + text_lines(b, "detail:notice", x + 8.0, y, w - 16.0, notice, 2);
    }
    for (i, proposal) in d.proposals.iter().enumerate() {
        y = proposal_card(b, proposal, i, lang, x + 8.0, y, w - 16.0) + 8.0;
    }
    if d.proposals_rejected > 0 || d.proposals_truncated {
        let mut note = vec![];
        if d.proposals_rejected > 0 {
            note.push(if lang.zh {
                format!("已否认 {} 条分配", d.proposals_rejected)
            } else {
                format!("{} assignments rejected", d.proposals_rejected)
            });
        }
        if d.proposals_truncated {
            note.push(lang.t("Some assignments were not read").into());
        }
        b.label(
            "detail:proposal-note",
            r(x + 8.0, y, w - 16.0, 18.0),
            note.join(" · "),
            p.secondary,
            12.0,
            FontRole::Text,
        );
        y += 24.0;
    }
    heading(
        b,
        "messages",
        x + 8.0,
        y,
        w - 16.0,
        lang.t("Lark discussion"),
        &d.messages.len().to_string(),
    );
    y += 28.0;
    for (i, m) in d.messages.iter().enumerate() {
        y = message(b, d, m, i, lang, x + 8.0, y, w - 16.0) + 6.0;
    }
    y
}

fn proposal_card(
    b: &mut B<'_>,
    proposal: &AssignmentProposal,
    i: usize,
    lang: Lang,
    x: f32,
    y: f32,
    w: f32,
) -> f32 {
    let p = *b.p;
    let key = format!("proposal:{i}");
    let pending = proposal.status == ProposalStatus::Proposed;
    let h = if pending { 112.0 } else { 80.0 };
    b.node(
        &key,
        r(x, y, w, h),
        "group",
        format!(
            "{} {} {}",
            lang.t("Assignment from Lark"),
            proposal.assignee,
            lang.proposal_status(proposal.status)
        ),
        String::new(),
        FontRole::Text,
        12.0,
        400,
        p.primary,
        Some(p.chip),
        None,
    );
    b.node(
        &format!("{key}:edge"),
        r(x, y, 3.0, h),
        "decoration",
        String::new(),
        String::new(),
        FontRole::Text,
        11.0,
        400,
        p.primary,
        Some(p.accent),
        None,
    );
    b.node(
        &format!("{key}:head"),
        r(x + 10.0, y + 6.0, w - 20.0, 18.0),
        "heading",
        String::new(),
        format!(
            "{} · {}",
            lang.t("Assignment from Lark"),
            lang.proposal_status(proposal.status)
        ),
        FontRole::Display,
        12.0,
        700,
        p.accent_ink,
        None,
        None,
    );
    let who = match &proposal.statement_author {
        Some(author) => format!(
            "{} {} {} · {}",
            proposal.assignee,
            lang.t("assigned by"),
            author,
            lang.basis(proposal.basis)
        ),
        None => format!("{} · {}", proposal.assignee, lang.basis(proposal.basis)),
    };
    b.label(
        &format!("{key}:who"),
        r(x + 10.0, y + 26.0, w - 20.0, 22.0),
        who,
        p.primary,
        14.0,
        FontRole::PaperBody,
    );
    let standing = format!(
        "{} {} · {} {} · {} {}",
        lang.t("Authority"),
        lang.standing(proposal.authority),
        lang.t("Acceptance"),
        lang.standing(proposal.acceptance),
        lang.t("Evidence"),
        proposal.evidence.len()
    );
    b.label(
        &format!("{key}:standing"),
        r(x + 10.0, y + 48.0, w - 20.0, 18.0),
        standing,
        p.secondary,
        12.0,
        FontRole::Text,
    );
    if pending {
        let mut bx = x + 10.0;
        for action in PROPOSAL_ACTIONS {
            let text = action.label(lang);
            let bw = b.fitter.width(text, 12.0, 700) + 24.0;
            let primary = action == ProposalAction::ConfirmMeegle;
            b.node(
                &format!("{key}:{action:?}"),
                r(bx, y + 74.0, bw, 28.0),
                "button",
                text.into(),
                text.into(),
                FontRole::Text,
                12.0,
                if primary { 700 } else { 400 },
                if primary { p.chip_on_text } else { p.primary },
                Some(if primary { p.accent } else { p.base }),
                Some(Target::Proposal(proposal.id.clone(), action)),
            );
            bx += bw + 6.0;
        }
    } else if let Some(at) = proposal.reviewed_at {
        b.label(
            &format!("{key}:reviewed"),
            r(x + 10.0, y + 58.0, w - 20.0, 18.0),
            format!(
                "✓ {} {}",
                lang.proposal_status(proposal.status),
                at.format("%m-%d %H:%M")
            ),
            p.success,
            12.0,
            FontRole::Text,
        );
    }
    y + h
}

#[allow(clippy::too_many_arguments)]
fn message(
    b: &mut B<'_>,
    d: &IssueDetail,
    m: &IssueMessage,
    i: usize,
    lang: Lang,
    x: f32,
    y: f32,
    w: f32,
) -> f32 {
    let p = *b.p;
    let key = format!("message:{i}");
    let indent = if m.reply_to.is_some() { 24.0 } else { 0.0 };
    let (x, w) = (x + indent, w - indent);
    let evidence = d
        .proposals
        .iter()
        .flat_map(|pr| pr.evidence.iter())
        .any(|e| e.record_id == m.record_id);
    let reasons = m
        .reasons
        .iter()
        .map(|r| lang.reason(*r))
        .chain(m.precision.map(|pr| lang.precision(pr)))
        .collect::<Vec<_>>()
        .join(" · ");
    let lines = m
        .excerpt
        .as_deref()
        .map(|e| {
            wrap(
                &b.fitter.with_font(FontRole::PaperBody),
                e,
                13.0,
                w - 24.0,
                4,
            )
        })
        .unwrap_or_default();
    let h = 30.0 + lines.len() as f32 * 19.0 + if m.missing_context { 20.0 } else { 6.0 };
    b.node(
        &key,
        r(x, y, w, h),
        "article",
        format!(
            "{} {}, {}{}",
            m.author,
            m.occurred_at.format("%m-%d %H:%M"),
            reasons,
            m.excerpt
                .as_deref()
                .map(|e| format!(", {e}"))
                .unwrap_or_default()
        ),
        String::new(),
        FontRole::Text,
        12.0,
        400,
        p.primary,
        Some(if evidence { p.card } else { p.base }),
        None,
    );
    let aw = b.fitter.width(&m.author, 13.0, 700) + 14.0;
    b.node(
        &format!("{key}:author"),
        r(x + 6.0, y + 4.0, aw, 22.0),
        "text",
        m.author.clone(),
        m.author.clone(),
        FontRole::Text,
        13.0,
        700,
        p.primary,
        None,
        None,
    );
    b.label(
        &format!("{key}:at"),
        r(x + 6.0 + aw, y + 4.0, 90.0, 22.0),
        m.occurred_at.format("%m-%d %H:%M").to_string(),
        p.secondary,
        12.0,
        FontRole::Text,
    );
    let tag = if evidence {
        format!("{} · {reasons}", lang.t("Evidence"))
    } else {
        reasons
    };
    let tw = (b.fitter.width(&tag, 11.0, 700) + 14.0).min(w * 0.55);
    b.node(
        &format!("{key}:why"),
        r(x + w - tw - 4.0, y + 5.0, tw, 20.0),
        "text",
        tag.clone(),
        tag,
        FontRole::Text,
        11.0,
        700,
        if evidence { p.accent_ink } else { p.secondary },
        None,
        None,
    );
    for (j, line) in lines.iter().enumerate() {
        b.label(
            &format!("{key}:text:{j}"),
            r(x + 6.0, y + 28.0 + j as f32 * 19.0, w - 12.0, 19.0),
            line.clone(),
            p.primary,
            13.0,
            FontRole::PaperBody,
        );
    }
    if m.missing_context {
        b.label(
            &format!("{key}:missing"),
            r(x + 6.0, y + h - 20.0, w - 12.0, 18.0),
            lang.t("Replies to a message that was not stored").into(),
            p.warning,
            11.0,
            FontRole::Text,
        );
    }
    y + h
}

fn people(b: &mut B<'_>, state: &State, lang: Lang, x: f32, y: f32, w: f32) -> f32 {
    let p = *b.p;
    let Some(d) = &state.detail else {
        return what_if(b, state, lang, x, y + 8.0, w);
    };
    let mut y = y + 8.0;
    heading(b, "people", x, y, w, lang.t("PDT and related people"), "");
    y += 26.0;
    for (i, person) in d.people.iter().enumerate() {
        let relations = person
            .relations
            .iter()
            .map(|r| lang.relation(*r))
            .collect::<Vec<_>>()
            .join(" · ");
        let handles = person
            .handles
            .iter()
            .map(|h| format!("{} {}", source_name(h.source), h.value))
            .collect::<Vec<_>>()
            .join("  ");
        b.node(
            &format!("person:{i}"),
            r(x, y, w, 62.0),
            "listitem",
            format!(
                "{} {} {relations} {handles}",
                person.display_name,
                person.title.clone().unwrap_or_default()
            ),
            String::new(),
            FontRole::Text,
            12.0,
            400,
            p.primary,
            Some(p.card),
            None,
        );
        b.label(
            &format!("person:{i}:name"),
            r(x + 4.0, y + 3.0, w * 0.45, 20.0),
            person.display_name.clone(),
            p.primary,
            14.0,
            FontRole::Text,
        );
        b.node(
            &format!("person:{i}:rel"),
            r(x + w * 0.42, y + 4.0, w * 0.58 - 4.0, 18.0),
            "text",
            relations.clone(),
            relations,
            FontRole::Text,
            11.0,
            700,
            p.accent_ink,
            None,
            None,
        );
        b.label(
            &format!("person:{i}:title"),
            r(x + 4.0, y + 23.0, w - 8.0, 16.0),
            person.title.clone().unwrap_or_default(),
            p.secondary,
            11.0,
            FontRole::Text,
        );
        b.label(
            &format!("person:{i}:handles"),
            r(x + 4.0, y + 40.0, w - 8.0, 16.0),
            handles,
            p.secondary,
            11.0,
            FontRole::Text,
        );
        y += 66.0;
    }
    if !d.who_to_ask.is_empty() {
        y += 6.0;
        heading(b, "who", x, y, w, lang.t("Who else to ask"), "");
        y += 26.0;
        for (i, who) in d.who_to_ask.iter().enumerate() {
            let reasons = who
                .reasons
                .iter()
                .map(|r| lang.reason_kind(*r))
                .collect::<Vec<_>>()
                .join(" · ");
            let score = format!("{:.2}", who.score);
            b.node(
                &format!("who:{i}"),
                r(x, y, w, 46.0),
                "listitem",
                format!(
                    "{} {reasons}, {} {score}",
                    who.person.display_name,
                    lang.t("rank score, not a confidence")
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
                &format!("who:{i}:name"),
                r(x + 4.0, y + 2.0, w - 60.0, 20.0),
                who.person.display_name.clone(),
                p.primary,
                13.0,
                FontRole::Text,
            );
            b.label(
                &format!("who:{i}:score"),
                r(x + w - 54.0, y + 2.0, 52.0, 20.0),
                score,
                p.accent_ink,
                14.0,
                FontRole::Display,
            );
            b.label(
                &format!("who:{i}:why"),
                r(x + 4.0, y + 22.0, w - 8.0, 18.0),
                reasons,
                p.secondary,
                11.0,
                FontRole::Text,
            );
            y += 50.0;
        }
    }
    what_if(b, state, lang, x, y + 10.0, w)
}

/// The path to the gate, as numbered steps with the DI left after each.
fn what_if(b: &mut B<'_>, state: &State, lang: Lang, x: f32, mut y: f32, w: f32) -> f32 {
    let p = *b.p;
    let Some(plan) = &state.what_if else {
        return y;
    };
    let view = super::what_if_view(lang, plan);
    heading(
        b,
        "whatif",
        x,
        y,
        w,
        &format!(
            "{} {}",
            lang.t("Shortest path to"),
            super::gate_name(plan.gate)
        ),
        &format!("{} · {}", view.start, view.threshold),
    );
    y += 26.0;
    for line in &view.lines {
        let k = format!("whatif:{}", line.index);
        b.node(
            &k,
            r(x, y, w, 40.0),
            "listitem",
            format!(
                "{} {} {} {} {} {}",
                line.index, line.item, line.severity, line.reduction, line.di_after, line.effort
            ),
            String::new(),
            FontRole::Text,
            12.0,
            400,
            p.primary,
            Some(p.card),
            None,
        );
        b.label(
            &format!("{k}:item"),
            r(x + 4.0, y + 2.0, w * 0.6, 20.0),
            format!("{}. {} {}", line.index, line.item, line.severity),
            p.primary,
            13.0,
            FontRole::Text,
        );
        b.label(
            &format!("{k}:effort"),
            r(x + 4.0, y + 20.0, w * 0.6, 18.0),
            line.effort.clone(),
            p.secondary,
            11.0,
            FontRole::Text,
        );
        let after = format!("{} → {}", line.reduction, line.di_after);
        let dw = b
            .fitter
            .with_font(FontRole::Display)
            .width(&after, 15.0, 700)
            + 14.0;
        b.node(
            &format!("{k}:after"),
            r(x + w - dw - 2.0, y + 8.0, dw, 24.0),
            "text",
            after.clone(),
            after,
            FontRole::Display,
            15.0,
            700,
            p.success,
            None,
            None,
        );
        y += 44.0;
    }
    let mut summary = view.summary.to_string();
    if let Some(note) = &view.unknown_effort_note {
        summary.push_str(" · ");
        summary.push_str(note);
    }
    b.label(
        "whatif:summary",
        r(x, y, w, 20.0),
        format!(
            "{} {summary}",
            if plan.reaches_threshold { "✓" } else { "×" }
        ),
        if plan.reaches_threshold {
            p.success
        } else {
            p.danger
        },
        12.0,
        FontRole::Text,
    );
    y + 24.0
}

fn search(b: &mut B<'_>, state: &State, lang: Lang, x: f32, y: f32, w: f32) -> f32 {
    let p = *b.p;
    let mut y = y + 8.0;
    let mut cx = x;
    for (i, mode) in [
        SearchMode::Lexical,
        SearchMode::Semantic,
        SearchMode::Hybrid,
    ]
    .into_iter()
    .enumerate()
    {
        cx += pill(
            b,
            &format!("mode:{i}"),
            cx,
            y,
            lang.search_mode(mode),
            state.mode == mode,
            Target::Mode(mode),
        ) + 4.0;
    }
    cx += 14.0;
    for (i, scope) in [
        SearchScope::All,
        SearchScope::Issues,
        SearchScope::Messages,
        SearchScope::People,
        SearchScope::Changes,
    ]
    .into_iter()
    .enumerate()
    {
        let label = lang.search_scope(scope);
        let pw = b.fitter.width(label, 12.0, 700) + 26.0;
        if cx + pw > x + w {
            cx = x;
            y += 30.0;
        }
        cx += pill(
            b,
            &format!("scope:{i}"),
            cx,
            y,
            label,
            state.scope == scope,
            Target::Scope(scope),
        ) + 4.0;
    }
    y += 34.0;
    let Some(found) = &state.search else {
        b.label(
            "search:hint",
            r(x, y, w, 22.0),
            lang.t("Type a question; words and meaning both count")
                .into(),
            p.secondary,
            12.0,
            FontRole::Text,
        );
        return y + 26.0;
    };
    if let Some(notice) = found
        .fallback
        .as_ref()
        .and_then(|f| super::fallback_notice(lang, f))
    {
        y += 4.0 + text_lines(b, "search:fallback", x, y, w, &notice, 2);
    }
    if found.results.is_empty() {
        b.label(
            "search:none",
            r(x, y, w, 22.0),
            lang.t("No results").into(),
            p.secondary,
            12.0,
            FontRole::Text,
        );
        y += 26.0;
    }
    for (i, result) in found.results.iter().enumerate() {
        y = search_row(b, result, i, lang, x, y, w) + 6.0;
    }
    b.label(
        "search:rule",
        r(x, y + 4.0, w, 34.0),
        lang.t("Ranking fuses word and meaning ranks; similar means related, not the same")
            .into(),
        p.secondary,
        11.0,
        FontRole::Text,
    );
    y + 40.0
}

fn search_row(
    b: &mut B<'_>,
    result: &SearchResult,
    i: usize,
    lang: Lang,
    x: f32,
    y: f32,
    w: f32,
) -> f32 {
    let p = *b.p;
    let key = format!("result:{i}");
    let opens = match result.kind {
        SearchKind::Issue => Some(result.id.clone()),
        _ => result.linked_issue.clone(),
    };
    let channels = result
        .channels
        .iter()
        .map(|c| lang.channel(*c))
        .collect::<Vec<_>>()
        .join(" + ");
    let why = result
        .why
        .iter()
        .map(|w| lang.search_why(*w))
        .collect::<Vec<_>>()
        .join(" · ");
    let similarity = result
        .semantic
        .as_ref()
        .map(|s| super::similarity_text(lang, s.similarity));
    b.node(
        &key,
        r(x, y, w, 70.0),
        if opens.is_some() { "link" } else { "listitem" },
        format!(
            "{} {} {}, {}, {why}, {channels}{}",
            lang.search_kind(result.kind),
            result.id,
            result.title,
            result.snippet,
            similarity
                .as_deref()
                .map(|s| format!(", {s}"))
                .unwrap_or_default()
        ),
        String::new(),
        FontRole::Text,
        12.0,
        400,
        p.primary,
        Some(p.card),
        opens.map(Target::Select),
    );
    b.node(
        &format!("{key}:kind"),
        r(x + 4.0, y + 4.0, 70.0, 20.0),
        "text",
        lang.search_kind(result.kind).into(),
        lang.search_kind(result.kind).into(),
        FontRole::Display,
        11.0,
        700,
        p.secondary,
        None,
        None,
    );
    b.label(
        &format!("{key}:title"),
        r(x + 70.0, y + 3.0, w - 230.0, 22.0),
        format!("{}  {}", result.id, result.title),
        p.primary,
        13.0,
        FontRole::Text,
    );
    b.node(
        &format!("{key}:channels"),
        r(x + w - 156.0, y + 4.0, 152.0, 20.0),
        "text",
        channels.clone(),
        channels,
        FontRole::Text,
        11.0,
        700,
        p.accent_ink,
        None,
        None,
    );
    // The snippet: word hits underlined solid, the meaning-matched segment dotted.
    let segments = super::highlight(
        &result.snippet,
        result
            .lexical
            .as_ref()
            .map(|l| l.spans.as_slice())
            .unwrap_or(&[]),
        result.semantic.as_ref().and_then(|s| s.span.as_ref()),
    );
    let body = b.fitter.with_font(FontRole::PaperBody);
    let mut sx = x + 10.0;
    let limit = x + w - 10.0;
    for (j, seg) in segments.iter().enumerate() {
        let sw = body.width(&seg.text, 13.0, 400);
        if sx + sw > limit {
            b.label(
                &format!("{key}:more"),
                r(sx - 6.0, y + 26.0, 24.0, 20.0),
                "…".into(),
                p.secondary,
                13.0,
                FontRole::PaperBody,
            );
            break;
        }
        b.label(
            &format!("{key}:seg:{j}"),
            r(sx - 6.0, y + 26.0, sw + 12.0, 20.0),
            seg.text.clone(),
            p.primary,
            13.0,
            FontRole::PaperBody,
        );
        match seg.mark {
            Mark::Plain => {}
            Mark::Lexical => b.node(
                &format!("{key}:lex:{j}"),
                r(sx, y + 45.0, sw, 2.0),
                "decoration",
                String::new(),
                String::new(),
                FontRole::Text,
                11.0,
                400,
                p.primary,
                Some(p.accent),
                None,
            ),
            Mark::Semantic => {
                let mut dx = 0.0;
                let mut k = 0;
                while dx + 2.0 <= sw {
                    b.node(
                        &format!("{key}:sem:{j}:{k}"),
                        r(sx + dx, y + 45.0, 2.0, 2.0),
                        "decoration",
                        String::new(),
                        String::new(),
                        FontRole::Text,
                        11.0,
                        400,
                        p.primary,
                        Some(p.accent_ink),
                        None,
                    );
                    dx += 4.0;
                    k += 1;
                }
            }
        }
        sx += sw;
    }
    let foot = match &similarity {
        Some(s) => format!("{why} · {s}"),
        None => why,
    };
    b.label(
        &format!("{key}:why"),
        r(x + 4.0, y + 48.0, w - 8.0, 18.0),
        foot,
        p.secondary,
        11.0,
        FontRole::Text,
    );
    y + 70.0
}

/// Suggestions for the open issue. Judging records a local decision; it never auto-applies.
fn suggestions(b: &mut B<'_>, state: &State, lang: Lang, x: f32, y: f32, w: f32) -> f32 {
    let p = *b.p;
    let Some(d) = &state.detail else {
        return what_if(b, state, lang, x, y + 8.0, w);
    };
    let mut y = y + 8.0;
    heading(
        b,
        "suggest",
        x,
        y,
        w,
        &format!("{} {}", lang.t("Suggestions for"), d.item.id),
        &super::dismissed_note(lang, d.suggestions_dismissed).unwrap_or_default(),
    );
    y += 28.0;
    for (i, s) in d.suggestions.iter().enumerate() {
        let key = format!("suggestion:{i}");
        let origin = match (s.similarity, s.origin) {
            (Some(sim), _) => format!("{} {:.2}", lang.t("similarity"), sim),
            (None, SuggestionOrigin::Rule) => lang.t("IPD rule").into(),
            (None, SuggestionOrigin::PeopleWho) => "people who".into(),
            (None, _) => lang.t("Same module and time").into(),
        };
        let judged = s.object_key.is_some();
        let h = if judged { 92.0 } else { 62.0 };
        b.node(
            &key,
            r(x, y, w, h),
            "group",
            format!("{} {} {origin}", lang.suggestion_kind(s.kind), s.title),
            String::new(),
            FontRole::Text,
            12.0,
            400,
            p.primary,
            Some(p.card),
            None,
        );
        b.node(
            &format!("{key}:kind"),
            r(x + 4.0, y + 4.0, w * 0.55, 18.0),
            "text",
            lang.suggestion_kind(s.kind).into(),
            lang.suggestion_kind(s.kind).into(),
            FontRole::Display,
            11.0,
            700,
            p.accent_ink,
            None,
            None,
        );
        b.label(
            &format!("{key}:origin"),
            r(x + w * 0.55, y + 4.0, w * 0.45 - 4.0, 18.0),
            origin,
            p.secondary,
            11.0,
            FontRole::Text,
        );
        let lines = text_lines(
            b,
            &format!("{key}:title"),
            x + 2.0,
            y + 24.0,
            w - 4.0,
            &s.title,
            2,
        );
        if judged {
            let mut bx = x + 6.0;
            for (verdict, label) in [
                (SuggestionVerdict::Accept, lang.t("Accept")),
                (SuggestionVerdict::Dismiss, lang.t("Dismiss")),
            ] {
                let bw = b.fitter.width(label, 12.0, 700) + 24.0;
                b.node(
                    &format!("{key}:{verdict:?}"),
                    r(bx, y + 28.0 + lines, bw, 26.0),
                    "button",
                    format!("{label} {}", s.title),
                    label.into(),
                    FontRole::Text,
                    12.0,
                    if verdict == SuggestionVerdict::Accept {
                        700
                    } else {
                        400
                    },
                    if verdict == SuggestionVerdict::Accept {
                        p.chip_on_text
                    } else {
                        p.primary
                    },
                    Some(if verdict == SuggestionVerdict::Accept {
                        p.accent
                    } else {
                        p.base
                    }),
                    Some(Target::Suggestion(i, verdict)),
                );
                bx += bw + 6.0;
            }
        }
        y += h.max(30.0 + lines + if judged { 34.0 } else { 4.0 }) + 6.0;
    }
    b.label(
        "suggest:rule",
        r(x, y, w, 34.0),
        lang.t("Suggestions are proposals: nothing is linked or changed until you accept")
            .into(),
        p.secondary,
        11.0,
        FontRole::Text,
    );
    what_if(b, state, lang, x, y + 44.0, w)
}

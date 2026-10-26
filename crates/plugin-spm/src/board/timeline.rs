//! Delivery presentation only: all stage boundaries, due dates and dependencies
//! come from the snapshot. Date arithmetic here maps them into view coordinates.
use super::super::text::project_day;
use super::*;
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use spm_contracts::{BoardStage, SourceSystem};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Zoom {
    #[default]
    Week,
    Month,
    Quarter,
}

#[derive(Clone, Copy, Debug)]
pub struct Range {
    pub from: DateTime<Utc>,
    pub days: i64,
}

impl Range {
    pub fn for_zoom(today: DateTime<Utc>, zoom: Zoom, timezone: &str) -> Self {
        let offset = super::super::text::offset(timezone)
            .unwrap_or_else(|| chrono::FixedOffset::east_opt(0).unwrap());
        let day = today.with_timezone(&offset).date_naive();
        let from = match zoom {
            Zoom::Week => {
                day - Duration::days(i64::from(day.weekday().num_days_from_monday()) + 14)
            }
            _ => NaiveDate::from_ymd_opt(day.year(), day.month(), 1).unwrap_or(day),
        };
        let days = match zoom {
            Zoom::Week => 35,
            Zoom::Month => 61,
            Zoom::Quarter => 92,
        };
        Self {
            from: offset
                .from_local_datetime(&from.and_hms_opt(0, 0, 0).unwrap())
                .single()
                .unwrap()
                .with_timezone(&Utc),
            days,
        }
    }
    pub fn x(self, at: DateTime<Utc>, track: RectDip) -> f32 {
        let fraction = (at - self.from).num_seconds() as f32 / (self.days * 86400) as f32;
        track.x + fraction.clamp(0.0, 1.0) * track.w
    }
    fn contains(self, at: DateTime<Utc>) -> bool {
        at >= self.from && at <= self.from + Duration::days(self.days)
    }
}

fn date(at: DateTime<Utc>, timezone: &str) -> String {
    short_time(at, timezone).chars().take(5).collect()
}

fn label(b: &mut Builder, key: &str, r: RectDip, text: &str, size: f32, color: [f32; 4]) {
    let full_name = text.to_owned();
    let (text, _) = b.fit.fit(text, size, 400, r.w, 1);
    let start = b.out.nodes.len();
    b.text(
        key.into(),
        r,
        &text,
        size,
        400,
        color,
        TextAlign::MiddleLeading,
    );
    if let Some(node) = b.out.nodes.get_mut(start) {
        node.name = full_name;
    }
}

fn number(b: &mut Builder, key: &str, r: RectDip, text: &str) {
    let text = text.strip_prefix("ORION-").unwrap_or(text);
    let (text, _) = b
        .fit
        .with_font(FontRole::Condensed)
        .fit(text, 18.0, 700, r.w, 1);
    b.push(
        key.into(),
        r,
        "heading",
        text.clone(),
        text,
        None,
        style(
            18.0,
            700,
            FontRole::Condensed,
            b.pal.primary,
            None,
            0.0,
            TextAlign::MiddleLeading,
        ),
    );
}

fn dashed(b: &mut Builder, key: &str, r: RectDip, ink: [f32; 4], vertical: bool) {
    let length = if vertical { r.h } else { r.w };
    for i in 0..(length / 8.0).ceil() as usize {
        let d = i as f32 * 8.0;
        let part = (length - d).min(4.0);
        b.fill(
            format!("{key}:{i}"),
            if vertical {
                rect(r.x, r.y + d, r.w, part)
            } else {
                rect(r.x + d, r.y, part, r.h)
            },
            ink,
            0.0,
        );
    }
}

fn dashed_box(b: &mut Builder, key: &str, r: RectDip, ink: [f32; 4]) {
    dashed(
        b,
        &format!("{key}:top"),
        rect(r.x, r.y, r.w, 1.0),
        ink,
        false,
    );
    dashed(
        b,
        &format!("{key}:bottom"),
        rect(r.x, r.y + r.h - 1.0, r.w, 1.0),
        ink,
        false,
    );
    dashed(
        b,
        &format!("{key}:left"),
        rect(r.x, r.y, 1.0, r.h),
        ink,
        true,
    );
    dashed(
        b,
        &format!("{key}:right"),
        rect(r.x + r.w - 1.0, r.y, 1.0, r.h),
        ink,
        true,
    );
}

fn span(b: &mut Builder, key: &str, r: RectDip, stage: BoardStage, name: String) {
    if r.w <= 0.0 {
        return;
    }
    b.fill(key.into(), r, b.pal.stage(stage), 2.0);
    if stage == BoardStage::NotStarted || b.pal.hc {
        dashed_box(b, &format!("{key}:outline"), r, b.pal.primary);
    }
    b.push(
        format!("{key}:name"),
        r,
        "img",
        name,
        String::new(),
        None,
        style(
            12.0,
            400,
            BODY,
            b.pal.primary,
            None,
            0.0,
            TextAlign::TopLeading,
        ),
    );
}

fn blocked(b: &mut Builder, key: &str, r: RectDip) {
    if r.w <= 0.0 {
        return;
    }
    // Small staircase diagonals form hatching without a renderer-specific path.
    for i in 0..(r.w / 7.0).ceil() as usize {
        for y in 0..r.h as usize {
            let x = i as f32 * 7.0 + y as f32 * 0.5;
            if x + 1.0 <= r.w {
                b.fill(
                    format!("{key}:hatch:{i}:{y}"),
                    rect(r.x + x, r.y + y as f32, 1.5, 1.0),
                    b.pal.danger,
                    0.0,
                );
            }
        }
    }
}

pub(super) fn workspace(b: &mut Builder, input: &Input, viewport: RectDip) {
    let board = input.board.unwrap();
    if input.evidence_open && input.detail_open {
        b.out.mode = Mode::Detail;
        b.button(
            "timeline:back",
            PAD,
            4.0,
            28.0,
            None,
            b.lang.t("Back to timeline"),
            false,
            Target::CloseDetail,
            200.0,
        );
        detail(
            b,
            board,
            input,
            rect(PAD, 42.0, viewport.w - 2.0 * PAD, viewport.h - 50.0),
        );
        return;
    }
    if viewport.w < COMPACT_MAX_W || viewport.h < COMPACT_MAX_H {
        compact(b, input, viewport);
        return;
    }
    b.out.mode = Mode::Board;
    let mut x = PAD;
    for (zoom, text) in [
        (Zoom::Week, "Week"),
        (Zoom::Month, "Month"),
        (Zoom::Quarter, "Quarter"),
    ] {
        x += b.button(
            &format!("zoom:{zoom:?}"),
            x,
            2.0,
            26.0,
            None,
            b.lang.t(text),
            input.zoom == zoom,
            Target::Zoom(zoom),
            120.0,
        ) + 4.0;
    }
    label(
        b,
        "timeline:fresh",
        rect(300.0, 2.0, viewport.w - 316.0, 26.0),
        &format!(
            "{} · {}",
            b.lang.f(
                "Data as of {}",
                &[&short_time(board.computed_at, &board.project_timezone)]
            ),
            board.project_timezone
        ),
        12.0,
        b.pal.secondary,
    );
    let range = Range::for_zoom(input.now, input.zoom, &board.project_timezone);
    let chart_x = PAD + 260.0;
    let track = rect(chart_x, 82.0, viewport.w - chart_x - PAD, 1.0);
    let drawer_h = if input.detail_open && input.selected.is_some() {
        if viewport.w < 732.0 { 212.0 } else { 182.0 }
    } else {
        0.0
    };
    let area = rect(
        PAD,
        124.0,
        viewport.w - 2.0 * PAD,
        viewport.h - 132.0 - drawer_h,
    );
    b.out.board = Some(area);
    let cards = visible_cards(board, input.filter);
    let mut y = area.y - input.scroll;
    b.clip = Some(area);
    // Guides are clipped to the scroll region; the axis remains fixed.
    for day in 0..range.days {
        let at = range.from + Duration::days(day);
        let gx = range.x(at, track);
        if input.zoom == Zoom::Week
            && project_day(at, &board.project_timezone)
                .weekday()
                .num_days_from_monday()
                >= 5
        {
            b.fill(
                format!("weekend:{day}"),
                rect(gx, area.y, track.w / range.days as f32, area.h),
                b.pal.weekend(),
                0.0,
            );
        }
        if project_day(at, &board.project_timezone).weekday() == chrono::Weekday::Mon {
            b.fill(
                format!("guide:{day}"),
                rect(gx, area.y, 1.0, area.h),
                b.pal.rule,
                0.0,
            );
        }
    }
    for (li, lane) in board.lanes.iter().enumerate() {
        let items: Vec<_> = cards
            .iter()
            .filter(|c| c.card.swimlane == lane.swimlane)
            .collect();
        if items.is_empty() {
            continue;
        }
        label(
            b,
            &format!("tl:lane:{li}"),
            rect(PAD, y, 250.0, 26.0),
            &lane_name(&lane.swimlane),
            14.0,
            b.pal.accent_ink,
        );
        b.hit(
            format!("tl:lane:{li}:hit"),
            rect(PAD, y, 250.0, 26.0),
            "button",
            lane_name(&lane.swimlane),
            Target::ToggleLane(lane.swimlane.clone()),
        );
        y += 26.0;
        if input.collapsed.contains(&lane.swimlane) {
            continue;
        }
        for info in items
            .iter()
            .filter(|c| c.card.stage.stage != BoardStage::Done)
        {
            row(
                b,
                input,
                info.card,
                li,
                y,
                track,
                range,
                area.y - input.scroll,
            );
            y += 46.0;
        }
        let done: Vec<_> = items
            .iter()
            .filter(|c| c.card.stage.stage == BoardStage::Done)
            .collect();
        label(
            b,
            &format!("tl:done:{li}"),
            rect(PAD, y, 250.0, 24.0),
            &format!("✓ {} {}", b.lang.t("Done"), done.len()),
            12.0,
            b.pal.secondary,
        );
        // Each completed item gets its own named miniature in the lane's collapsed strip.
        for info in done {
            let c = info.card;
            if let (Some(start), Some(end)) = (
                c.started_at,
                c.stage_history
                    .iter()
                    .find(|s| s.stage == BoardStage::Done)
                    .map(|s| s.from),
            ) {
                let l = range.x(start, track);
                let r = range.x(end, track);
                span(
                    b,
                    &format!("done:{}", c.card_id.as_str()),
                    rect(l, y + 4.0, r - l, 14.0),
                    BoardStage::Done,
                    format!(
                        "{} ✓ {}",
                        c.business_number.as_deref().unwrap_or(""),
                        b.lang.t("Done")
                    ),
                );
            }
        }
        y += 24.0;
    }
    b.out.board_content = (y + input.scroll - area.y).max(0.0);
    for m in &board.milestones {
        if range.contains(m.at) {
            dashed(
                b,
                &format!("milestone:{}", m.id),
                rect(range.x(m.at, track), area.y, 1.0, area.h),
                b.pal.accent_ink,
                true,
            );
        }
    }
    let today_x = range.x(input.now, track);
    b.fill(
        "today:line".into(),
        rect(today_x, area.y, 2.0, area.h),
        b.pal.accent,
        0.0,
    );
    b.clip = None;
    axis(b, input, board, track, range);
    if input.detail_open
        && let Some(card) = input.selected.and_then(|id| model::find(board, id))
    {
        drawer(
            b,
            input,
            card,
            rect(PAD, viewport.h - drawer_h, viewport.w - 2.0 * PAD, drawer_h),
        );
    }
    tooltip(b, input, viewport);
}

fn axis(b: &mut Builder, input: &Input, board: &BoardSnapshot, track: RectDip, range: Range) {
    let tz = &input.board.unwrap().project_timezone;
    let date = |at| date(at, tz);

    for day in 0..range.days {
        let at = range.from + Duration::days(day);
        let tick = if input.zoom == Zoom::Quarter {
            project_day(at, tz).day() == 1
        } else {
            project_day(at, tz).weekday() == chrono::Weekday::Mon
        };
        if tick {
            let text = if input.zoom == Zoom::Quarter {
                project_day(at, tz).format("%Y-%m").to_string()
            } else {
                format!("W{} {}", project_day(at, tz).iso_week().week(), date(at))
            };
            let x = range.x(at, track);
            label(
                b,
                &format!("axis:{day}"),
                rect(
                    x + 3.0,
                    100.0,
                    (track.x + track.w - x - 3.0).min(110.0),
                    22.0,
                ),
                &text,
                12.0,
                b.pal.secondary,
            );
        }
    }
    label(
        b,
        "today:tag",
        rect(range.x(input.now, track) + 4.0, 30.0, 110.0, 22.0),
        &format!("{} {}", b.lang.t("Today"), date(input.now)),
        12.0,
        b.pal.accent_ink,
    );
    for (i, m) in board.milestones.iter().enumerate() {
        if range.contains(m.at) {
            let x = range.x(m.at, track);
            let text = format!("◆ {} {}", m.label, date(m.at));
            label(
                b,
                &format!("milestone:{}:flag", m.id),
                rect(
                    x.min(track.x + track.w - 160.0),
                    50.0 + (i % 2) as f32 * 22.0,
                    160.0,
                    22.0,
                ),
                &text,
                12.0,
                b.pal.accent_ink,
            );
        }
    }
    label(
        b,
        "tl:legend",
        rect(PAD, 36.0, 260.0, 40.0),
        &format!(
            "○ {} · ▰ {} · ✓ {}",
            b.lang.t("Not started"),
            b.lang.t("Develop"),
            b.lang.t("Done")
        ),
        11.0,
        b.pal.secondary,
    );
}

fn row(
    b: &mut Builder,
    input: &Input,
    c: &BoardCard,
    lane: usize,
    y: f32,
    track: RectDip,
    range: Range,
    origin: f32,
) {
    let tz = &input.board.unwrap().project_timezone;
    let date = |at| date(at, tz);

    let key = format!("tl:{}", c.card_id.as_str());
    let selected = input.selected == Some(&c.card_id);
    if selected {
        b.outline(
            &format!("{key}:selected"),
            rect(PAD, y, track.x + track.w - PAD, 44.0),
            b.pal.accent,
            1.5,
            3.0,
            b.pal.selected,
        );
    }
    b.hit(
        format!("{key}:hit"),
        rect(PAD, y, track.x + track.w - PAD, 44.0),
        "listitem",
        format!(
            "{} {} {}",
            c.business_number.as_deref().unwrap_or(""),
            c.title,
            model::stage_label(c.stage.stage, b.lang)
        ),
        Target::Select(c.card_id.clone()),
    );
    b.out.nav.push(NavCard {
        id: c.card_id.clone(),
        column: 0,
        lane,
        y: y - origin,
        h: 46.0,
    });
    number(
        b,
        &format!("{key}:number"),
        rect(PAD + 4.0, y, 62.0, 25.0),
        c.business_number
            .as_deref()
            .unwrap_or(c.card_id.as_str())
            .strip_prefix("ORION-")
            .unwrap_or(c.business_number.as_deref().unwrap_or("")),
    );
    label(
        b,
        &format!("{key}:title"),
        rect(PAD + 72.0, y, 178.0, 25.0),
        &c.title,
        12.0,
        b.pal.primary,
    );
    let who = c
        .assignee()
        .map_or(b.lang.t("Unassigned"), |a| a.person.display_name.as_str());
    label(
        b,
        &format!("{key}:who"),
        rect(PAD + 4.0, y + 24.0, 246.0, 20.0),
        who,
        12.0,
        b.pal.secondary,
    );
    for (i, s) in c.stage_history.iter().enumerate() {
        let end = s.to.unwrap_or(input.now).min(input.now);
        if end <= s.from {
            continue;
        }
        let l = range.x(s.from, track);
        let r = range.x(end, track);
        span(
            b,
            &format!("{key}:span:{i}"),
            rect(l, y + 5.0, r - l, 16.0),
            s.stage,
            format!(
                "{} {} – {}",
                model::stage_label(s.stage, b.lang),
                date(s.from),
                date(end)
            ),
        );
    }
    if let Some(schedule) = &c.schedule {
        let l = range.x(input.now, track);
        let r = range.x(schedule.due, track);
        if r > l {
            dashed_box(
                b,
                &format!("{key}:plan"),
                rect(l, y + 5.0, r - l, 16.0),
                b.pal.secondary,
            );
        }
        b.fill(
            format!("{key}:due"),
            rect(r, y + 1.0, 2.0, 24.0),
            b.pal.secondary,
            0.0,
        );
        if let Some(base) = schedule.baseline_due.filter(|base| *base != schedule.due) {
            dashed(
                b,
                &format!("{key}:baseline"),
                rect(range.x(base, track), y + 1.0, 1.0, 24.0),
                b.pal.secondary,
                true,
            );
        }
        for (i, blocker) in c.blockers.iter().enumerate() {
            if let Some(since) = blocker.since {
                let l = range.x(since, track);
                let r = range.x(input.now, track);
                blocked(
                    b,
                    &format!("{key}:blocked:{i}"),
                    rect(l, y + 5.0, r - l, 16.0),
                );
            }
            if let Some(expected) = blocker.expected_at {
                let l = range.x(schedule.due, track);
                let r = range.x(expected, track);
                dashed(
                    b,
                    &format!("{key}:dependency:{i}"),
                    rect(l.min(r), y + 15.0, (r - l).abs(), 2.0),
                    b.pal.danger,
                    false,
                );
                label(
                    b,
                    &format!("{key}:dependency:{i}:label"),
                    rect(r.min(track.x + track.w - 160.0), y + 24.0, 160.0, 20.0),
                    &format!("⊘ {} {}", b.lang.t("Expected"), date(expected)),
                    12.0,
                    b.pal.danger,
                );
            }
        }
    }
    if let Some(s) = &c.schedule {
        let mut name = format!(
            "{}: {} {}",
            c.business_number.as_deref().unwrap_or(""),
            b.lang.t("Due"),
            date(s.due)
        );
        if let Some(base) = s.baseline_due.filter(|d| *d != s.due) {
            name.push_str(&format!("; {} {}", b.lang.t("Original due"), date(base)));
        }
        for blocker in &c.blockers {
            name.push_str(&format!("; ⊘ {}", blocker.reason));
            if let Some(at) = blocker.expected_at {
                name.push_str(&format!("; {} {}", b.lang.t("Expected"), date(at)));
            }
        }
        b.push(
            format!("{key}:schedule:name"),
            rect(track.x, y + 1.0, track.w, 25.0),
            "img",
            name,
            String::new(),
            None,
            style(
                12.0,
                400,
                BODY,
                b.pal.primary,
                None,
                0.0,
                TextAlign::TopLeading,
            ),
        );
    }
    let status = if !c.blockers.is_empty() {
        format!("⊘ {}", b.lang.t("Blocked"))
    } else {
        format!(
            "{} {}",
            if c.stage.stage == BoardStage::NotStarted {
                "○"
            } else {
                "▰"
            },
            model::stage_label(c.stage.stage, b.lang)
        )
    };
    label(
        b,
        &format!("{key}:stage"),
        rect(track.x, y + 24.0, 155.0, 20.0),
        &status,
        11.0,
        b.pal.secondary,
    );
    for (i, m) in c.event_marks.iter().enumerate() {
        if !range.contains(m.at) {
            continue;
        }
        let x = range.x(m.at, track);
        let glyph = match m.system {
            SourceSystem::Gerrit => "●",
            SourceSystem::Meegle => "■",
            SourceSystem::Jenkins => "▲",
            SourceSystem::Lark => "◆",
            SourceSystem::Jira => "◇",
            SourceSystem::People => "○",
        };
        b.fill(
            format!("{key}:event:{i}:bg"),
            rect(x - 7.0, y + 5.0, 14.0, 16.0),
            b.pal.base,
            8.0,
        );
        b.text(
            format!("{key}:event:{i}:glyph"),
            rect(x - 7.0, y + 4.0, 14.0, 18.0),
            glyph,
            11.0,
            600,
            b.pal.primary,
            TextAlign::Center,
        );
        b.hit(
            format!("{key}:event:{i}"),
            rect(x - 9.0, y + 2.0, 18.0, 24.0),
            "button",
            format!(
                "{} {} {}",
                date(m.at),
                model::system_label(m.system),
                b.lang.t("Open evidence")
            ),
            Target::Event(c.card_id.clone(), m.evidence_id.clone()),
        );
    }
}

fn drawer(b: &mut Builder, input: &Input, c: &BoardCard, r: RectDip) {
    let tz = &input.board.unwrap().project_timezone;
    let date = |at| date(at, tz);

    b.out.detail = Some(r);
    b.fill("drawer:bg".into(), r, b.pal.card, 0.0);
    number(
        b,
        "drawer:number",
        rect(r.x + 10.0, r.y + 4.0, r.w - 60.0, 26.0),
        c.business_number.as_deref().unwrap_or(""),
    );
    label(
        b,
        "drawer:title",
        rect(r.x + 10.0, r.y + 32.0, r.w - 20.0, 24.0),
        &c.title,
        14.0,
        b.pal.primary,
    );
    let mut parts = Vec::new();
    for column in [
        DisplayColumn::Todo,
        DisplayColumn::Develop,
        DisplayColumn::Verify,
        DisplayColumn::Done,
    ] {
        let days: i64 = c
            .stage_history
            .iter()
            .filter(|s| DisplayColumn::of(s.stage) == column)
            .map(|s| {
                (s.to.unwrap_or(input.now).min(input.now) - s.from)
                    .num_days()
                    .max(0)
            })
            .sum();
        parts.push(format!(
            "{} {} {}",
            column.label(b.lang),
            days,
            b.lang.t("days")
        ));
    }
    let blocked_days = c
        .blockers
        .iter()
        .filter_map(|v| v.since)
        .min()
        .map_or(0, |since| (input.now - since).num_days().max(0));
    parts.push(format!(
        "{} {} {}",
        b.lang.t("Blocked"),
        blocked_days,
        b.lang.t("days")
    ));
    if let Some(s) = &c.schedule {
        parts.push(format!(
            "{} {} {} · {}",
            b.lang.t("Days to due"),
            (project_day(s.due, tz) - project_day(input.now, tz)).num_days(),
            b.lang.t("days"),
            date(s.due)
        ));
    }
    let narrow = r.w < 700.0;
    if narrow {
        for (i, part) in parts.iter().enumerate() {
            label(
                b,
                &format!("drawer:stats:{i}"),
                rect(
                    r.x + 10.0 + (i % 3) as f32 * (r.w - 20.0) / 3.0,
                    r.y + 60.0 + (i / 3) as f32 * 22.0,
                    (r.w - 20.0) / 3.0 - 6.0,
                    22.0,
                ),
                part,
                11.0,
                b.pal.primary,
            );
        }
    } else {
        label(
            b,
            "drawer:stats",
            rect(r.x + 10.0, r.y + 60.0, r.w - 20.0, 24.0),
            &parts.join(" · "),
            12.0,
            b.pal.primary,
        );
    }
    let extra = if narrow { 22.0 } else { 0.0 };
    let next = c
        .next_action
        .as_ref()
        .map_or(b.lang.t("No next action"), |n| n.text.as_str());
    label(
        b,
        "drawer:next",
        rect(r.x + 10.0, r.y + 86.0 + extra, r.w - 20.0, 24.0),
        &format!("{}: {}", b.lang.t("Next action"), next),
        12.0,
        b.pal.accent_ink,
    );
    let mut notes = c
        .blockers
        .iter()
        .map(|v| v.reason.as_str())
        .chain(c.conflicts.iter().map(|v| v.explanation.as_str()))
        .collect::<Vec<_>>()
        .join(" · ");
    if let Some(s) = &c.schedule
        && let Some(baseline) = s.baseline_due.filter(|d| *d != s.due)
    {
        if !notes.is_empty() {
            notes.push_str(" · ");
        }
        notes.push_str(&format!(
            "{} {} → {}",
            b.lang.t("Original due"),
            date(baseline),
            date(s.due)
        ));
    }
    let note = if notes.is_empty() {
        c.stage.explanation.as_str()
    } else {
        notes.as_str()
    };
    label(
        b,
        "drawer:notes",
        rect(r.x + 10.0, r.y + 110.0 + extra, r.w - 20.0, 24.0),
        note,
        12.0,
        b.pal.secondary,
    );
    let max_button = ((r.w - 28.0) / 2.0).min(180.0);
    let w = b.button(
        "drawer:evidence",
        r.x + 10.0,
        r.y + 142.0 + extra,
        28.0,
        None,
        b.lang.t("Open evidence"),
        true,
        Target::OpenEvidence,
        max_button,
    );
    b.button(
        "drawer:board",
        r.x + w + 18.0,
        r.y + 142.0 + extra,
        28.0,
        None,
        b.lang.t("Show on board"),
        false,
        Target::ShowOnBoard,
        max_button,
    );
    b.button(
        "drawer:close",
        r.x + r.w - 76.0,
        r.y + 4.0,
        28.0,
        None,
        b.lang.t("Close"),
        false,
        Target::CloseDetail,
        76.0,
    );
}

fn tooltip(b: &mut Builder, input: &Input, viewport: RectDip) {
    let Some(id) = input.preview_event else {
        return;
    };
    let summary = match input.preview_evidence {
        EvidenceView::Loaded(e) => e
            .items
            .iter()
            .find(|e| e.id == id)
            .map_or(b.lang.t("Evidence unavailable"), |e| e.summary.as_str()),
        EvidenceView::Failed(e) => e,
        _ => b.lang.t("Loading evidence…"),
    };
    let w = (viewport.w - 32.0).min(480.0);
    let y = if input.detail_open {
        viewport.h - 252.0
    } else {
        viewport.h - 72.0
    };
    b.fill(
        "event:tooltip:bg".into(),
        rect(PAD, y, w, 64.0),
        b.pal.card,
        3.0,
    );
    let (text, lines) = b.fit.fit(summary, 12.0, 400, w - 20.0, 2);
    b.text(
        "event:tooltip".into(),
        rect(PAD + 10.0, y + 8.0, w - 20.0, lines as f32 * b.line_h(12.0)),
        &text,
        12.0,
        400,
        b.pal.primary,
        TextAlign::TopLeading,
    );
}

/// Fourteen calendar dates including today, ordered by the supplied due date.
fn compact(b: &mut Builder, input: &Input, viewport: RectDip) {
    let tz = &input.board.unwrap().project_timezone;
    let date = |at| date(at, tz);

    let board = input.board.unwrap();
    b.out.mode = Mode::Compact;
    let offset =
        super::super::text::offset(tz).unwrap_or_else(|| chrono::FixedOffset::east_opt(0).unwrap());
    let today = input.now.with_timezone(&offset).date_naive();
    let from = offset
        .from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .unwrap()
        .with_timezone(&Utc);
    let range = Range { from, days: 13 };
    let w = viewport.w - 2.0 * PAD;
    let mut my = 2.0;
    for m in &board.milestones {
        let left = (m.at.date_naive() - today).num_days();
        label(
            b,
            &format!("compact:milestone:{}", m.id),
            rect(PAD, my, w, 22.0),
            &format!(
                "◆ {} {} · {} {}",
                m.label,
                date(m.at),
                left,
                b.lang.t("days")
            ),
            12.0,
            b.pal.accent_ink,
        );
        my += 24.0;
    }
    if input.detail_open
        && let Some(c) = input.selected.and_then(|id| model::find(board, id))
    {
        let area = rect(PAD, my + 4.0, w, (viewport.h - my - 8.0).max(0.0));
        b.clip = Some(area);
        drawer(
            b,
            input,
            c,
            rect(PAD, my + 4.0 - input.detail_scroll, w, 220.0),
        );
        b.clip = None;
        b.out.detail = Some(area);
        b.out.detail_content = 340.0;
        tooltip(b, input, viewport);
        return;
    }
    label(
        b,
        "compact:range",
        rect(PAD, my, w, 22.0),
        &format!(
            "{} {} – {} · {}",
            b.lang.t("Today"),
            date(from),
            date(from + Duration::days(13)),
            b.lang.t("14 days")
        ),
        12.0,
        b.pal.secondary,
    );
    my += 26.0;
    let area = rect(PAD, my, w, viewport.h - my - 8.0);
    b.out.board = Some(area);
    let mut cards = visible_cards(board, input.filter);
    cards.retain(|c| c.card.stage.stage != BoardStage::Done);
    cards.sort_by_key(|c| {
        (
            c.card
                .schedule
                .as_ref()
                .map_or(i64::MAX, |s| s.due.timestamp()),
            c.card.card_id.as_str(),
        )
    });
    let mut y = area.y - input.scroll;
    let mut group = None;
    b.clip = Some(area);
    for info in cards {
        let c = info.card;
        let days = c
            .schedule
            .as_ref()
            .map(|s| (project_day(s.due, tz) - today).num_days());
        let g = match days {
            Some(d) if d <= 1 => 0,
            Some(d) if d <= 6 - i64::from(today.weekday().num_days_from_monday()) => 1,
            Some(_) => 2,
            None => 3,
        };
        if group != Some(g) {
            let text = match g {
                0 => "Overdue / tomorrow",
                1 => "This week",
                2 => "Later",
                _ => "No schedule",
            };
            label(
                b,
                &format!("compact:group:{g}"),
                rect(PAD, y, w, 24.0),
                b.lang.t(text),
                12.0,
                if g == 0 {
                    b.pal.danger
                } else {
                    b.pal.secondary
                },
            );
            y += 26.0;
            group = Some(g);
        }
        let key = format!("compact:tl:{}", c.card_id.as_str());
        if input.selected == Some(&c.card_id) {
            b.outline(
                &format!("{key}:selected"),
                rect(PAD, y, w, 88.0),
                b.pal.accent,
                1.5,
                3.0,
                b.pal.selected,
            );
        }
        b.hit(
            format!("{key}:hit"),
            rect(PAD, y, w, 88.0),
            "listitem",
            format!(
                "{} {} {}",
                c.business_number.as_deref().unwrap_or(""),
                c.title,
                model::stage_label(c.stage.stage, b.lang)
            ),
            Target::Select(c.card_id.clone()),
        );
        b.out.nav.push(NavCard {
            id: c.card_id.clone(),
            column: 0,
            lane: 0,
            y: y - area.y - input.scroll,
            h: 90.0,
        });
        number(
            b,
            &format!("{key}:number"),
            rect(PAD + 4.0, y, 90.0, 24.0),
            c.business_number.as_deref().unwrap_or(""),
        );
        label(
            b,
            &format!("{key}:title"),
            rect(PAD + 98.0, y, w - 158.0, 24.0),
            &c.title,
            12.0,
            b.pal.primary,
        );
        let left = days.map_or_else(|| "—".into(), |d| format!("{d} {}", b.lang.t("days")));
        label(
            b,
            &format!("{key}:left"),
            rect(PAD + w - 60.0, y, 60.0, 24.0),
            &left,
            12.0,
            if days.is_some_and(|d| d <= 1) {
                b.pal.danger
            } else {
                b.pal.primary
            },
        );
        let who = c
            .assignee()
            .map_or(b.lang.t("Unassigned"), |a| a.person.display_name.as_str());
        let mut status = format!("▰ {}", model::stage_label(c.stage.stage, b.lang));
        if let Some(signal) = info.signals.iter().find(|s| s.needs_attention()) {
            status = format!(
                "{} {}",
                if *signal == Signal::Blocked {
                    "⊘"
                } else {
                    "!"
                },
                signal.label(b.lang)
            );
        }
        if let Some(expected) = c.blockers.iter().find_map(|v| v.expected_at) {
            status.push_str(&format!(" · {} {}", b.lang.t("Expected"), date(expected)));
        } else if let Some(schedule) = &c.schedule
            && let Some(base) = schedule.baseline_due.filter(|d| *d != schedule.due)
        {
            status.push_str(&format!(
                " · {} {} → {}",
                b.lang.t("Original due"),
                date(base),
                date(schedule.due)
            ));
        }
        label(
            b,
            &format!("{key}:meta"),
            rect(PAD + 4.0, y + 25.0, w - 8.0, 23.0),
            &format!("{status} · {who}"),
            12.0,
            b.pal.secondary,
        );
        let track = rect(PAD + 8.0, y + 58.0, w - 24.0, 12.0);
        b.fill(format!("{key}:track"), track, b.pal.skeleton, 2.0);
        if let Some(s) = &c.schedule {
            let x = range.x(s.due, track);
            dashed_box(
                b,
                &format!("{key}:plan"),
                rect(track.x, track.y, x - track.x, 12.0),
                b.pal.secondary,
            );
            b.fill(
                format!("{key}:due"),
                rect(x, track.y - 3.0, 2.0, 18.0),
                b.pal.primary,
                0.0,
            );
            if let Some(base) = s.baseline_due.filter(|base| *base != s.due) {
                dashed(
                    b,
                    &format!("{key}:baseline"),
                    rect(range.x(base, track), track.y - 3.0, 1.0, 18.0),
                    b.pal.secondary,
                    true,
                );
            }
            if !c.blockers.is_empty() {
                blocked(
                    b,
                    &key,
                    rect(track.x, track.y, (x - track.x).max(8.0), 12.0),
                );
            }
        }
        span(
            b,
            &format!("{key}:current"),
            rect(track.x, track.y, 8.0, 12.0),
            c.stage.stage,
            model::stage_label(c.stage.stage, b.lang).into(),
        );
        y += 92.0;
    }
    b.out.board_content = y + input.scroll - area.y;
    b.clip = None;
}

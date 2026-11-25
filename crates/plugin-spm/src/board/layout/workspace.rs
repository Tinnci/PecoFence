use super::*;

pub(super) fn workspace(b: &mut Builder, input: &Input, viewport: RectDip) {
    let lang = b.lang;
    let pal = *b.pal;
    let Some(board) = input.board else {
        loading(b, input, viewport);
        return;
    };
    let dock = viewport.w >= DETAIL_DOCK_MIN;
    let detail_w = (viewport.w * 0.34).clamp(380.0, 460.0);
    let has_detail = input.detail_open
        && input
            .selected
            .is_some_and(|id| model::find(board, id).is_some());
    let board_w = if has_detail && dock {
        viewport.w - detail_w - GAP
    } else {
        viewport.w
    };
    if has_detail && !dock {
        b.out.mode = Mode::Detail;
        // The title, refresh and copy commands live in the host's title row (PanelChrome).
        let top = TOP;
        b.button(
            "back",
            PAD,
            top,
            32.0,
            Some("\u{E72B}"),
            lang.t("Back to board"),
            false,
            Target::CloseDetail,
            240.0,
        );
        let area = rect(
            PAD,
            top + 44.0,
            viewport.w - 2.0 * PAD,
            viewport.h - top - 44.0 - 36.0,
        );
        detail(b, board, input, area);
        status_line(
            b,
            input,
            rect(PAD, viewport.h - 30.0, viewport.w - 2.0 * PAD, 24.0),
        );
        return;
    }
    let fy = TOP;
    filters(b, board, input, PAD, fy, board_w - 2.0 * PAD);
    let fresh_y = fy + 36.0;
    freshness(b, board, input, PAD, fresh_y, board_w - 2.0 * PAD);
    let cards = visible_cards(board, input.filter);
    let columns: Vec<DisplayColumn> = DisplayColumn::ALL
        .iter()
        .copied()
        .filter(|c| {
            *c != DisplayColumn::Unmapped
                || cards.iter().any(|i| i.column == DisplayColumn::Unmapped)
        })
        .collect();
    // A tab that no longer exists (for example Unmapped after a refresh) falls back.
    let tab = if columns.contains(&input.tab) {
        input.tab
    } else if columns.contains(&DisplayColumn::Develop) {
        DisplayColumn::Develop
    } else {
        columns[0]
    };
    let avail = board_w - 2.0 * PAD;
    let n = columns.len() as f32;
    // Rails: Unmapped (rare, needs a policy fix rather than space) and Done (finished
    // work) shrink to a count before any working column gets narrower than a card.
    let roomy = n * 232.0 + (n - 1.0) * GAP <= avail;
    let unmapped_rail = !roomy && columns.contains(&DisplayColumn::Unmapped);
    let fits = |done_rail: bool| {
        let rails = f32::from(u8::from(unmapped_rail) + u8::from(done_rail));
        (n - rails) * CARD_MIN + rails * RAIL + (n - 1.0) * GAP <= avail
    };
    let done_rail = !roomy && !(input.done_open && fits(false)) && fits(true);
    let tabs = !fits(done_rail);
    let is_rail = |c: DisplayColumn| {
        (done_rail && c == DisplayColumn::Done) || (unmapped_rail && c == DisplayColumn::Unmapped)
    };
    let head_y = fresh_y + 30.0;
    let board_top = head_y + 36.0;
    let board_bottom = viewport.h - 36.0;
    let region = rect(0.0, board_top, board_w, (board_bottom - board_top).max(0.0));
    b.out.board = Some(region);
    // Column geometry.
    let mut xs = Vec::new();
    if tabs {
        b.out.mode = Mode::Tabs;
        b.out.tabs = columns.clone();
        let mut x = PAD;
        for column in &columns {
            let count = cards.iter().filter(|i| i.column == *column).count();
            let label = format!(
                "{} {}",
                column.label(lang),
                count_text(count, board.totals_complete)
            );
            let w = b.button(
                &format!("tab:{column:?}"),
                x,
                head_y,
                30.0,
                None,
                &label,
                *column == tab,
                Target::Tab(*column),
                200.0,
            );
            x += w + 6.0;
        }
        xs.push((tab, PAD, avail));
    } else {
        let rails = f32::from(u8::from(unmapped_rail) + u8::from(done_rail));
        let cw = (avail - rails * RAIL - (n - 1.0) * GAP) / (n - rails);
        let mut x = PAD;
        for column in &columns {
            let w = if is_rail(*column) { RAIL } else { cw };
            xs.push((*column, x, w));
            let count = cards.iter().filter(|i| i.column == *column).count();
            let unmapped = *column == DisplayColumn::Unmapped;
            let color = if unmapped { pal.warning } else { pal.primary };
            let label = column.label(lang);
            let mut lx = x + 4.0;
            if unmapped && !is_rail(*column) {
                b.glyph(
                    format!("colhead:{column:?}:glyph"),
                    rect(lx, head_y, 16.0, 28.0),
                    "\u{E7BA}",
                    12.0,
                    pal.warning,
                );
                lx += 20.0;
            }
            if is_rail(*column) {
                let text = format!(
                    "{} {}",
                    if unmapped { "\u{26A0}" } else { "\u{2713}" },
                    count_text(count, board.totals_complete)
                );
                b.text(
                    format!("colhead:{column:?}"),
                    rect(x, head_y, w, 28.0),
                    &text,
                    13.0,
                    700,
                    color,
                    TextAlign::Center,
                );
            } else {
                let lw = b.fit.width(label, 13.0, 700);
                b.text(
                    format!("colhead:{column:?}"),
                    rect(lx, head_y, lw + 2.0, 28.0),
                    label,
                    13.0,
                    700,
                    color,
                    TextAlign::MiddleLeading,
                );
                b.text(
                    format!("colhead:{column:?}:n"),
                    rect(lx + lw + 8.0, head_y, 60.0, 28.0),
                    &count_text(count, board.totals_complete),
                    13.0,
                    400,
                    pal.secondary,
                    TextAlign::MiddleLeading,
                );
            }
            x += w + GAP;
        }
    }
    b.fill(
        "rule:head".into(),
        rect(PAD, board_top - 6.0, avail, 1.0),
        pal.rule,
        0.0,
    );
    // Lanes.
    b.clip = Some(region);
    let mut y = board_top + 6.0 - input.scroll;
    let lanes: Vec<&Swimlane> = board
        .lanes
        .iter()
        .map(|l| &l.swimlane)
        .filter(|l| {
            input
                .filter
                .model
                .as_ref()
                .is_none_or(|m| l.model.as_ref() == Some(m))
        })
        .collect();
    let content_start = y;
    let column_index = |c: DisplayColumn| columns.iter().position(|x| *x == c).unwrap_or(0);
    if cards.is_empty() {
        let message = if input.filter.is_active() {
            lang.t("No cards match the filters.")
        } else {
            lang.t("No requirements in this delivery scope.")
        };
        b.text(
            "empty".into(),
            rect(PAD, y + 24.0, avail, 22.0),
            message,
            TITLE,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
        if input.filter.is_active() {
            b.button(
                "empty:clear",
                PAD,
                y + 56.0,
                32.0,
                None,
                lang.t("Clear filters"),
                false,
                Target::ClearFilters,
                200.0,
            );
        }
        y += 100.0;
    }
    for (lane_index, lane) in lanes.iter().enumerate() {
        let lane_cards: Vec<&CardInfo> =
            cards.iter().filter(|i| &i.card.swimlane == *lane).collect();
        if lane_cards.is_empty() && input.filter.is_active() {
            continue;
        }
        let collapsed = input.collapsed.contains(lane);
        let lkey = format!("lane:{}:{}", lane.project, lane_name(lane));
        let blocked = lane_cards
            .iter()
            .filter(|i| i.signals.contains(&Signal::Blocked))
            .count();
        let attention = lane_cards
            .iter()
            .filter(|i| i.signals.iter().any(|s| s.needs_attention()))
            .count();
        b.glyph(
            format!("{lkey}:chev"),
            rect(PAD, y, 18.0, 28.0),
            if collapsed { "\u{E76C}" } else { "\u{E70D}" },
            11.0,
            pal.secondary,
        );
        let name = lane_name(lane);
        let nw = b.fit.width(&name, 14.0, 700);
        b.text(
            format!("{lkey}:name"),
            rect(PAD + 22.0, y, nw + 2.0, 28.0),
            &name,
            14.0,
            700,
            pal.primary,
            TextAlign::MiddleLeading,
        );
        let mut summary = format!("{} {}", lane_cards.len(), lang.t("cards"));
        if blocked > 0 {
            summary.push_str(&format!(" · {blocked} {}", lang.t("blocked")));
        }
        if attention > 0 {
            summary.push_str(&format!(
                " · {attention} {}",
                lang.t("Needs attention").to_lowercase()
            ));
        }
        b.text(
            format!("{lkey}:summary"),
            rect(PAD + 30.0 + nw, y, 360.0, 28.0),
            &summary,
            META,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
        b.hit(
            format!("{lkey}:hit"),
            rect(PAD, y, nw + 40.0 + b.fit.width(&summary, META, 400), 28.0),
            "button",
            format!("{name}, {summary}"),
            Target::ToggleLane((*lane).clone()),
        );
        y += 32.0;
        if collapsed {
            y += 8.0;
            continue;
        }
        // Column cells; the lane is as tall as its tallest column.
        let mut tallest: f32 = 0.0;
        let mut placed = Vec::new();
        for (column, x, w) in &xs {
            let mut cy = 8.0;
            let in_cell: Vec<&&CardInfo> =
                lane_cards.iter().filter(|i| i.column == *column).collect();
            if is_rail(*column) {
                cy += 28.0;
            } else {
                for info in &in_cell {
                    let h = card_height(b, info, *w - 12.0);
                    placed.push((*info, *column, *x + 6.0, cy, *w - 12.0, h));
                    cy += h + CARD_GAP;
                }
                if in_cell.is_empty() {
                    cy += 28.0;
                }
            }
            tallest = tallest.max(cy);
        }
        let lane_h = tallest + 4.0;
        for (column, x, w) in &xs {
            let ckey = format!("{lkey}:cell:{column:?}");
            b.fill(
                format!("{ckey}:bg"),
                rect(*x, y, *w, lane_h),
                pal.column,
                10.0,
            );
            let count = lane_cards.iter().filter(|i| i.column == *column).count();
            if is_rail(*column) {
                if count == 0 {
                    b.text(
                        format!("{ckey}:none"),
                        rect(*x, y + 10.0, *w, 20.0),
                        "—",
                        META,
                        400,
                        pal.secondary,
                        TextAlign::Center,
                    );
                    continue;
                }
                let tone = if *column == DisplayColumn::Unmapped {
                    pal.warning
                } else {
                    pal.secondary
                };
                b.text(
                    format!("{ckey}:n"),
                    rect(*x, y + 8.0, *w, 22.0),
                    &count.to_string(),
                    14.0,
                    700,
                    tone,
                    TextAlign::Center,
                );
                let label = format!("{} {count}", column.label(lang));
                // Done expands; an unmapped card opens directly, since it needs a decision.
                let target = match lane_cards.iter().find(|i| i.column == *column) {
                    Some(info) if *column == DisplayColumn::Unmapped => {
                        Target::Select(info.card.card_id.clone())
                    }
                    _ => Target::ToggleDone,
                };
                b.hit(
                    format!("{ckey}:hit"),
                    rect(*x, y, *w, lane_h),
                    "button",
                    label,
                    target,
                );
            } else if count == 0 {
                b.text(
                    format!("{ckey}:none"),
                    rect(*x, y + 10.0, *w, 20.0),
                    "—",
                    META,
                    400,
                    pal.secondary,
                    TextAlign::Center,
                );
            }
        }
        for (info, column, x, cy, w, _) in placed {
            let selected = input.selected == Some(&info.card.card_id);
            let h = card(b, info, x, y + cy, w, selected);
            b.out.nav.push(NavCard {
                id: info.card.card_id.clone(),
                column: column_index(column),
                lane: lane_index,
                y: y + cy - content_start,
                h,
            });
        }
        y += lane_h + 16.0;
    }
    b.out.board_content = y - content_start;
    b.clip = None;
    if b.out.board_content > region.h {
        scrollbar(
            b,
            "board",
            region,
            input.scroll,
            b.out.board_content,
            board_w - 6.0,
        );
    }
    status_line(
        b,
        input,
        rect(PAD, viewport.h - 30.0, board_w - 2.0 * PAD, 24.0),
    );
    if has_detail {
        let x = board_w + GAP;
        let area = rect(x, 12.0, viewport.w - x - 12.0, viewport.h - 24.0);
        // On paper the pane is the same sheet, divided by a rule; high contrast keeps a panel.
        if pal.hc {
            b.fill("detail:pane".into(), area, pal.column, 12.0);
        } else {
            b.fill(
                "detail:rule".into(),
                rect(x - GAP / 2.0, 12.0, 1.0, viewport.h - 24.0),
                pal.rule,
                0.0,
            );
        }
        detail(
            b,
            board,
            input,
            rect(area.x + 4.0, area.y + 4.0, area.w - 8.0, area.h - 8.0),
        );
    } else if !b.out.nav.is_empty() && dock && input.selected.is_none() && board_w > 1300.0 {
        let _ = lang.t("Select a card to see its evidence.");
    }
}

pub(super) fn filters(
    b: &mut Builder,
    board: &BoardSnapshot,
    input: &Input,
    x0: f32,
    y: f32,
    w: f32,
) {
    let lang = b.lang;
    let filter = input.filter;
    let models: Vec<String> = board
        .lanes
        .iter()
        .filter_map(|l| l.swimlane.model.clone())
        .collect();
    let degraded = model::degraded_sources(board);
    let attention = board
        .cards
        .iter()
        .filter(|c| {
            model::signals(c, &degraded)
                .iter()
                .any(|s| s.needs_attention())
        })
        .count();
    let attention_label = format!("{} {attention}", lang.t("Needs attention"));
    let query_label = filter.query.as_ref().map(|q| format!("“{q}”"));
    // Measure: when the model chips do not fit, one chip cycles through them.
    let chip_w = |b: &Builder, s: &str| b.fit.width(s, META, 600) + 24.0;
    let tail = chip_w(b, &attention_label)
        + 40.0
        + 8.0
        + query_label
            .as_ref()
            .map_or(chip_w(b, lang.t("Find")) + 22.0, |q| chip_w(b, q) + 60.0);
    let all = lang.t("All models");
    let models_w: f32 = chip_w(b, all) + models.iter().map(|m| chip_w(b, m) + 6.0).sum::<f32>();
    let mut x = x0;
    if models_w + tail <= w {
        x += b.button(
            "model:all",
            x,
            y,
            28.0,
            None,
            all,
            filter.model.is_none(),
            Target::Model(None),
            200.0,
        ) + 6.0;
        for m in &models {
            let on = filter.model.as_ref() == Some(m);
            x += b.button(
                &format!("model:{m}"),
                x,
                y,
                28.0,
                None,
                m,
                on,
                Target::Model(Some(m.clone())),
                200.0,
            ) + 6.0;
        }
    } else {
        let label = filter.model.clone().unwrap_or_else(|| all.to_owned());
        x += b.button(
            "model:cycle",
            x,
            y,
            28.0,
            Some("\u{E71C}"),
            &label,
            filter.model.is_some(),
            Target::CycleModel,
            220.0,
        ) + 6.0;
    }
    x += 10.0;
    x += b.button(
        "attention",
        x,
        y,
        28.0,
        Some("\u{E7BA}"),
        &attention_label,
        filter.attention,
        Target::Attention,
        240.0,
    ) + 6.0;
    match &query_label {
        Some(q) => {
            x += b.button(
                "find",
                x,
                y,
                28.0,
                Some("\u{E721}"),
                q,
                true,
                Target::Find,
                240.0,
            ) + 4.0;
            b.button(
                "find:clear",
                x,
                y,
                28.0,
                Some("\u{E711}"),
                "",
                false,
                Target::ClearQuery,
                28.0,
            );
        }
        None => {
            b.button(
                "find",
                x,
                y,
                28.0,
                Some("\u{E721}"),
                lang.t("Find"),
                false,
                Target::Find,
                160.0,
            );
        }
    }
}

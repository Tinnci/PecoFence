use super::*;

pub(super) fn compact(b: &mut Builder, input: &Input, viewport: RectDip) {
    let lang = b.lang;
    let pal = *b.pal;
    let Some(board) = input.board else {
        b.out.mode = Mode::Compact;
        loading(b, input, viewport);
        return;
    };
    let tz = board.project_timezone.as_str();
    let cards = visible_cards(board, input.filter);
    if input.detail_open
        && let Some(card) = input
            .selected
            .and_then(|id| cards.iter().find(|i| &i.card.card_id == id))
    {
        b.out.mode = Mode::CompactDetail;
        b.button(
            "back",
            12.0,
            10.0,
            28.0,
            Some("\u{E72B}"),
            lang.t("Back to board"),
            false,
            Target::CloseDetail,
            200.0,
        );
        let x = 16.0;
        let w = viewport.w - 32.0;
        let mut y = 48.0;
        b.text(
            "cd:number".into(),
            rect(x, y, w, 26.0),
            &model::label_of(card.card),
            18.0,
            700,
            pal.primary,
            TextAlign::MiddleLeading,
        );
        y += 28.0;
        let lh = b.line_h(14.0);
        let (title, lines) = b.fit.fit(&card.card.title, 14.0, 600, w, 2);
        b.text(
            "cd:title".into(),
            rect(x, y, w, lines as f32 * lh),
            &title,
            14.0,
            600,
            pal.primary,
            TextAlign::TopLeading,
        );
        y += lines as f32 * lh + 8.0;
        let region = rect(0.0, y, viewport.w, viewport.h - y - 8.0);
        b.clip = Some(region);
        let (workflow, since) = model::workflow_text(card.card, tz, lang);
        y = row(
            b,
            "cd:wf",
            lang.t("Workflow"),
            None,
            &workflow,
            &since,
            x,
            y,
            w,
            true,
        );
        let who = card.card.assignee().map_or_else(
            || lang.t("Unassigned").to_owned(),
            |a| a.person.display_name.clone(),
        );
        y = row(
            b,
            "cd:who",
            lang.t("Responsible now"),
            None,
            &who,
            "",
            x,
            y,
            w,
            false,
        );
        y = row(
            b,
            "cd:code",
            lang.t("Code"),
            None,
            &model::code_text(card.card, lang),
            "",
            x,
            y,
            w,
            false,
        );
        if let Some(signal) = card.signals.first() {
            y = row(
                b,
                "cd:signal",
                signal.label(lang),
                Some((signal.glyph(), pal.tone(signal.tone()))),
                &model::signal_text(card.card, *signal, lang),
                "",
                x,
                y,
                w,
                false,
            );
        }
        if let Some(action) = &card.card.next_action {
            let owner = action.owner.as_ref().map_or_else(
                || lang.t("Unassigned").to_owned(),
                |o| o.display_name.clone(),
            );
            row(
                b,
                "cd:next",
                &owner,
                Some(("\u{E72A}", pal.accent_ink)),
                &action.text,
                "",
                x,
                y,
                w,
                true,
            );
        }
        b.clip = None;
        return;
    }
    b.out.mode = Mode::Compact;
    let x = 12.0;
    let w = viewport.w - 24.0;
    // Title, refresh and source status are in the host's title row (PanelChrome).
    let counts: Vec<String> = DisplayColumn::ALL
        .iter()
        .filter_map(|c| {
            let n = cards.iter().filter(|i| i.column == *c).count();
            (n > 0).then(|| format!("{} {}", c.label(lang), count_text(n, board.totals_complete)))
        })
        .collect();
    let (fitted, _) = b.fit.fit(&counts.join(" · "), META, 400, w - 8.0, 1);
    b.text(
        "c:counts".into(),
        rect(x + 4.0, 8.0, w - 8.0, 18.0),
        &fitted,
        META,
        400,
        pal.secondary,
        TextAlign::MiddleLeading,
    );
    // Ranked list: what needs a person first.
    let mut ranked: Vec<&CardInfo> = cards
        .iter()
        .filter(|i| i.column != DisplayColumn::Done)
        .collect();
    ranked.sort_by_key(|i| model::attention_rank(i.card, &i.signals));
    let row_h = 52.0;
    let top = 32.0;
    let room = ((viewport.h - top - 28.0) / (row_h + 4.0)).floor().max(1.0) as usize;
    let shown = ranked.len().min(room);
    for (i, info) in ranked.iter().take(shown).enumerate() {
        let y = top + i as f32 * (row_h + 4.0);
        let key = format!("c:row:{}", info.card.card_id.0);
        let r = rect(x, y, w, row_h);
        let selected = input.selected == Some(&info.card.card_id);
        if selected {
            b.outline(&key, r, pal.accent, 1.5, 8.0, pal.selected);
        } else {
            b.fill(format!("{key}:bg"), r, pal.card, 8.0);
        }
        let lead = info.signals.iter().copied().find(|s| s.needs_attention());
        let glyph_color = lead.map_or(pal.secondary, |s| pal.tone(s.tone()));
        if let Some(signal) = lead {
            b.glyph(
                format!("{key}:glyph"),
                rect(x + 12.0, y + 6.0, 18.0, 20.0),
                signal.glyph(),
                12.0,
                glyph_color,
            );
        }
        let number = model::label_of(info.card);
        let who = info.card.assignee().map_or_else(
            || lang.t("Unassigned").to_owned(),
            |a| a.person.display_name.clone(),
        );
        let (who_fit, _) = b.fit.fit(&who, META, 400, w * 0.35, 1);
        let who_w = b.fit.width(&who_fit, META, 400) + 4.0;
        let nx = x + 34.0;
        let nw = b.fit.width(&number, NUMBER, 700) + 2.0;
        b.text(
            format!("{key}:number"),
            rect(nx, y + 6.0, nw, 20.0),
            &number,
            NUMBER,
            700,
            pal.primary,
            TextAlign::MiddleLeading,
        );
        let title_w = (x + w - 12.0 - who_w - 8.0 - (nx + nw + 8.0)).max(0.0);
        let (title, _) = b.fit.fit(&info.card.title, 13.0, 400, title_w, 1);
        b.text(
            format!("{key}:title"),
            rect(nx + nw + 8.0, y + 6.0, title_w, 20.0),
            &title,
            13.0,
            400,
            pal.primary,
            TextAlign::MiddleLeading,
        );
        b.text(
            format!("{key}:who"),
            rect(x + w - 12.0 - who_w, y + 6.0, who_w, 20.0),
            &who_fit,
            META,
            400,
            if info.card.assignee().is_some() {
                pal.secondary
            } else {
                pal.warning
            },
            TextAlign::MiddleLeading,
        );
        let second = match (lead, &info.card.next_action) {
            (Some(signal), _) if signal.tone() == Tone::Danger => {
                model::signal_text(info.card, signal, lang)
            }
            (_, Some(action)) => format!(
                "→ {}: {}",
                action
                    .owner
                    .as_ref()
                    .map_or("—", |o| o.display_name.as_str()),
                action.text
            ),
            (Some(signal), None) => model::signal_text(info.card, signal, lang),
            (None, None) => model::fact_line(info.card, lang),
        };
        let (fitted, _) = b.fit.fit(&second, META, 400, w - 46.0, 1);
        b.text(
            format!("{key}:second"),
            rect(nx, y + 27.0, w - 46.0, 18.0),
            &fitted,
            META,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
        b.hit(
            format!("{key}:hit"),
            r,
            "listitem",
            format!("{number}, {}, {who}, {second}", info.card.title),
            Target::Select(info.card.card_id.clone()),
        );
        b.out.nav.push(NavCard {
            id: info.card.card_id.clone(),
            column: 0,
            lane: 0,
            y,
            h: row_h,
        });
    }
    let hidden = ranked.len() - shown;
    if let Some((text, tone)) = input.status {
        let color = tone.map_or(pal.secondary, |t| pal.tone(t));
        let (fitted, _) = b.fit.fit(text, META, 400, w, 1);
        b.text(
            "status".into(),
            rect(x + 4.0, viewport.h - 24.0, w - 8.0, 20.0),
            &fitted,
            META,
            400,
            color,
            TextAlign::MiddleLeading,
        );
    } else if hidden > 0 {
        b.text(
            "c:more".into(),
            rect(x + 4.0, viewport.h - 24.0, w - 8.0, 20.0),
            &lang.f("{} more", &[&hidden.to_string()]),
            META,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
    }
}

pub(super) fn capsule(b: &mut Builder, input: &Input, viewport: RectDip) {
    let lang = b.lang;
    let pal = *b.pal;
    // The CI/CD segment sits after the summary and only while something runs or has failed.
    let ci = crate::ci_watch::entry::Summary::from_snapshot(input.ci);
    let ci_label = ci.label(lang.zh);
    let ci_w = if ci.visible() {
        (b.fit.width(&ci_label, META, 600).ceil() + 26.0).min(viewport.w * 0.45)
    } else {
        0.0
    };
    let Some(board) = input.board else {
        b.text(
            "cap:loading".into(),
            rect(12.0, 0.0, viewport.w - 24.0, viewport.h),
            lang.t("Loading the board…"),
            META,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
        return;
    };
    let cards = visible_cards(board, &Filter::default());
    let open: Vec<String> = [
        DisplayColumn::Todo,
        DisplayColumn::Develop,
        DisplayColumn::Verify,
        DisplayColumn::Done,
    ]
    .iter()
    .map(|c| {
        format!(
            "{} {}",
            c.label(lang),
            cards.iter().filter(|i| i.column == *c).count()
        )
    })
    .collect();
    let blocked = cards
        .iter()
        .filter(|i| i.signals.contains(&Signal::Blocked))
        .count();
    let degraded = !model::degraded_sources(board).is_empty();
    let mut x = 12.0;
    let name = board.project_id.0.clone();
    let nw = b.fit.width(&name, 13.0, 700);
    b.text(
        "cap:name".into(),
        rect(x, 0.0, nw + 2.0, viewport.h),
        &name,
        13.0,
        700,
        pal.primary,
        TextAlign::MiddleLeading,
    );
    x += nw + 12.0;
    let line = open.join(" · ");
    let right = 60.0 + ci_w;
    let (fitted, _) = b.fit.fit(&line, META, 400, viewport.w - x - right, 1);
    let lw = b.fit.width(&fitted, META, 400) + 2.0;
    b.text(
        "cap:counts".into(),
        rect(x, 0.0, lw, viewport.h),
        &fitted,
        META,
        400,
        pal.secondary,
        TextAlign::MiddleLeading,
    );
    x += lw + 10.0;
    if blocked > 0 {
        b.glyph(
            "cap:blocked:glyph".into(),
            rect(x, 0.0, 16.0, viewport.h),
            Signal::Blocked.glyph(),
            12.0,
            pal.danger,
        );
        b.text(
            "cap:blocked".into(),
            rect(x + 18.0, 0.0, 24.0, viewport.h),
            &blocked.to_string(),
            META,
            700,
            pal.danger,
            TextAlign::MiddleLeading,
        );
    }
    if degraded {
        b.glyph(
            "cap:stale".into(),
            rect(viewport.w - 28.0, 0.0, 16.0, viewport.h),
            "\u{E823}",
            12.0,
            pal.warning,
        );
    }
    if ci.visible() {
        let x_ci = viewport.w - 36.0 - ci_w;
        b.button(
            "cap:ci",
            x_ci,
            4.0,
            (viewport.h - 8.0).max(24.0),
            None,
            &ci_label,
            true,
            Target::OpenCi,
            ci_w,
        );
        // The visible text is short; the accessible name says the whole status.
        let name = ci.accessible_name(lang.zh);
        if let Some(node) = b.out.nodes.iter_mut().rev().find(|n| n.key == "cap:ci:hit") {
            node.name = name;
        }
    }
}

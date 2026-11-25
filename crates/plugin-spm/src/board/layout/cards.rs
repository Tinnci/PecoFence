use super::*;

pub(super) fn card(
    b: &mut Builder,
    info: &CardInfo,
    x: f32,
    y: f32,
    w: f32,
    selected: bool,
) -> f32 {
    let pal = *b.pal;
    let lang = b.lang;
    let card = info.card;
    let key = format!("card:{}", card.card_id.0);
    let inner_x = x + 12.0;
    let inner_w = w - 24.0;
    let number = model::label_of(card);
    let h = card_height(b, info, w);
    let r = rect(x, y, w, h);
    if selected {
        b.outline(&format!("{key}:sel"), r, pal.accent, 1.5, 8.0, pal.selected);
    } else if pal.hc {
        b.outline(&format!("{key}:hc"), r, pal.primary, 1.0, 8.0, pal.card);
    } else {
        b.fill(format!("{key}:bg"), r, pal.card, 8.0);
    }
    let meta_lh = b.line_h(META).max(18.0);
    let title_lh = b.line_h(TITLE);

    // Line 1: requirement number; the most severe signal on the right, as glyph and word.
    let lead = info.signals.iter().copied().find(|s| {
        (s.tone() != Tone::Info || *s == Signal::WorkflowLag) && *s != Signal::Unassigned
    });
    let mut flag_w = 0.0;
    if let Some(signal) = lead {
        let color = pal.tone(signal.tone());
        let (label, _) = b
            .fit
            .fit(signal.label(lang), META, 600, inner_w * 0.55 - 16.0, 1);
        flag_w = b.fit.width(&label, META, 600).ceil() + 16.0;
        let fx = inner_x + inner_w - flag_w;
        b.glyph(
            format!("{key}:flag:glyph"),
            rect(fx, y + 10.0, 14.0, 18.0),
            signal.glyph(),
            11.0,
            color,
        );
        b.text(
            format!("{key}:flag"),
            rect(fx + 16.0, y + 10.0, flag_w - 16.0, 18.0),
            &label,
            META,
            600,
            color,
            TextAlign::MiddleLeading,
        );
    }
    let (number_fit, _) = b.fit.fit(&number, NUMBER, 600, inner_w - flag_w - 8.0, 1);
    b.text(
        format!("{key}:number"),
        rect(inner_x, y + 10.0, inner_w - flag_w - 8.0, 18.0),
        &number_fit,
        NUMBER,
        600,
        pal.primary,
        TextAlign::MiddleLeading,
    );
    let mut cy = y + 10.0 + 18.0 + 4.0;

    let (title, title_lines) = b.fit.fit(&card.title, TITLE, 400, inner_w, 2);
    b.text(
        format!("{key}:title"),
        rect(inner_x, cy, inner_w, title_lines as f32 * title_lh),
        &title,
        TITLE,
        400,
        pal.primary,
        TextAlign::TopLeading,
    );
    cy += title_lines as f32 * title_lh + 6.0;

    // Line 3: who holds it now. An empty slot is a warning, with its own glyph.
    let assignee = card.assignee().map(|a| a.person.display_name.clone());
    let done = card.stage.stage == spm_contracts::BoardStage::Done;
    let (who, who_color) = match &assignee {
        Some(name) => (name.clone(), pal.secondary),
        None if !done => (lang.t("Unassigned").to_owned(), pal.warning),
        None => (String::new(), pal.secondary),
    };
    if !who.is_empty() {
        let d = 20.0;
        let ay = cy + (meta_lh - d) / 2.0;
        if assignee.is_some() {
            b.fill(
                format!("{key}:avatar"),
                rect(inner_x, ay, d, d),
                pal.chip_avatar(),
                d / 2.0,
            );
            let initial: String = who.chars().next().into_iter().collect();
            b.text(
                format!("{key}:avatar:t"),
                rect(inner_x, ay, d, d),
                &initial,
                11.0,
                600,
                pal.primary,
                TextAlign::Center,
            );
        } else {
            b.glyph(
                format!("{key}:avatar:g"),
                rect(inner_x, ay, d, d),
                Signal::Unassigned.glyph(),
                12.0,
                who_color,
            );
        }
        let (fitted, _) = b.fit.fit(&who, META, 400, inner_w - d - 6.0, 1);
        b.text(
            format!("{key}:who"),
            rect(inner_x + d + 6.0, cy, inner_w - d - 6.0, meta_lh),
            &fitted,
            META,
            400,
            who_color,
            TextAlign::MiddleLeading,
        );
        cy += meta_lh + 2.0;
    }

    // Line 4: implementation progress, separate from the workflow column.
    let code = &card.progress.code;
    let code_state = if code.linked > 0 && code.merged == code.linked {
        ProgressState::Passed
    } else if code.linked > 0 {
        ProgressState::InProgress
    } else {
        ProgressState::NotStarted
    };
    for (i, state) in [code_state, card.progress.build, card.progress.verification]
        .into_iter()
        .enumerate()
    {
        let color = match state {
            ProgressState::Passed => pal.success,
            ProgressState::Failed => pal.danger,
            ProgressState::InProgress => pal.accent,
            _ => pal.rule,
        };
        b.fill(
            format!("{key}:seg:{i}"),
            rect(
                inner_x + i as f32 * 16.0,
                cy + meta_lh / 2.0 - 2.0,
                14.0,
                4.0,
            ),
            color,
            2.0,
        );
    }
    let facts = model::fact_line(card, lang);
    let (fitted, _) = b.fit.fit(&facts, META, 400, inner_w - 54.0, 1);
    b.text(
        format!("{key}:facts"),
        rect(inner_x + 54.0, cy, inner_w - 54.0, meta_lh),
        &fitted,
        META,
        400,
        pal.secondary,
        TextAlign::MiddleLeading,
    );
    cy += meta_lh;

    let action = next_action_text(card);
    if let Some(action) = &action {
        cy += 2.0;
        let (fitted, _) = b.fit.fit(action, META, 400, inner_w, 1);
        b.text(
            format!("{key}:next"),
            rect(inner_x, cy, inner_w, meta_lh),
            &fitted,
            META,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
    }

    let mut name = format!("{number}, {}, {}", card.title, info.column.label(lang));
    if let Some(a) = &assignee {
        name.push_str(&format!(", {a}"));
    }
    for s in &info.signals {
        name.push_str(&format!(", {}", s.label(lang)));
    }
    if let Some(a) = &action {
        name.push_str(&format!(", {a}"));
    }
    b.hit(
        format!("{key}:hit"),
        r,
        "listitem",
        name,
        Target::Select(card.card_id.clone()),
    );
    h
}

fn next_action_text(card: &BoardCard) -> Option<String> {
    card.next_action.as_ref().map(|a| match &a.owner {
        Some(owner) => format!("→ {}: {}", model::first_name(owner), a.text),
        None => format!("→ {}", a.text),
    })
}

pub(super) fn card_height(b: &Builder, info: &CardInfo, w: f32) -> f32 {
    let card = info.card;
    let (_, lines) = b.fit.fit(&card.title, TITLE, 400, w - 24.0, 2);
    let meta_lh = b.line_h(META).max(18.0);
    let who = card.assignee().is_some() || card.stage.stage != spm_contracts::BoardStage::Done;
    10.0 + 18.0
        + 4.0
        + lines as f32 * b.line_h(TITLE)
        + 6.0
        + if who { meta_lh + 2.0 } else { 0.0 }
        + meta_lh
        + if card.next_action.is_some() {
            2.0 + meta_lh
        } else {
            0.0
        }
        + 10.0
}

// ---------------------------------------------------------------------------
// Workspace

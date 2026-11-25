use super::*;

pub(super) fn detail(b: &mut Builder, board: &BoardSnapshot, input: &Input, area: RectDip) {
    let lang = b.lang;
    let pal = *b.pal;
    let Some(card) = input.selected.and_then(|id| model::find(board, id)) else {
        return;
    };
    let tz = board.project_timezone.as_str();
    let degraded = model::degraded_sources(board);
    let signals = model::signals(card, &degraded);
    let x = area.x + 16.0;
    let w = area.w - 32.0;
    b.out.detail = Some(area);
    let close_w = 32.0;
    // Fixed header: requirement number, title, identity and source actions.
    let number = model::label_of(card);
    b.text(
        "d:number".into(),
        rect(x, area.y + 12.0, w - close_w - 8.0, 30.0),
        &number,
        22.0,
        700,
        pal.primary,
        TextAlign::MiddleLeading,
    );
    if b.out.mode != Mode::Detail {
        b.button(
            "d:close",
            x + w - close_w,
            area.y + 12.0,
            close_w,
            Some("\u{E711}"),
            "",
            false,
            Target::CloseDetail,
            close_w,
        );
    }
    let serif = b.fit.with_font(FontRole::Serif);
    let (title, lines) = serif.fit(&card.title, 20.0, 600, w, 3);
    let title_lh = serif.line_height(20.0).ceil();
    let mut y = area.y + 46.0;
    b.serif(
        "d:title".into(),
        rect(x, y, w, lines as f32 * title_lh),
        &title,
        20.0,
        pal.primary,
    );
    y += lines as f32 * title_lh + 6.0;
    let identity = format!(
        "{} · {} {} {} · {}",
        model::lane_label(&card.swimlane),
        model::system_label(card.source.system),
        card.source.record_kind,
        card.source.record_id,
        lang.f("rev {}", &[&board.revision.0.to_string()])
    );
    let (identity_fit, _) = b.fit.fit(&identity, META, 400, w, 1);
    b.text(
        "d:identity".into(),
        rect(x, y, w, 18.0),
        &identity_fit,
        META,
        400,
        pal.secondary,
        TextAlign::MiddleLeading,
    );
    y += 26.0;
    let system = model::system_label(card.source.system);
    let open_label = lang.f("Open in {}", &[system]);
    let used = b.button(
        "d:open",
        x,
        y,
        30.0,
        Some("\u{E8A7}"),
        &open_label,
        false,
        Target::OpenSource(card.source.clone()),
        220.0,
    );
    b.button(
        "d:copy",
        x + used + 8.0,
        y,
        30.0,
        Some("\u{E8C8}"),
        lang.t("Copy ID"),
        false,
        Target::Copy(card.card_id.0.clone()),
        180.0,
    );
    y += 40.0;
    let region = rect(area.x, y, area.w, area.y + area.h - y);
    b.clip = Some(region);
    let start = y;
    y -= input.detail_scroll;
    // Status: the workflow, then code, build, test and acceptance as separate facts.
    y = section(b, "d:status", lang.t("Status"), x, y, w);
    let (workflow, since) = model::workflow_text(card, tz, lang);
    y = row(
        b,
        "d:wf",
        lang.t("Workflow"),
        None,
        &workflow,
        &since,
        x,
        y,
        w,
        true,
    );
    let code_state = model::code_state(card);
    y = row(
        b,
        "d:code",
        lang.t("Code"),
        Some((model::progress_glyph(code_state), pal.progress(code_state))),
        &model::code_text(card, lang),
        "",
        x,
        y,
        w,
        false,
    );
    let build = card.progress.build;
    y = row(
        b,
        "d:build",
        lang.t("Build"),
        Some((model::progress_glyph(build), pal.progress(build))),
        model::progress_word(build, lang),
        "",
        x,
        y,
        w,
        false,
    );
    let test = card.progress.verification;
    y = row(
        b,
        "d:test",
        lang.t("Test"),
        Some((model::progress_glyph(test), pal.progress(test))),
        model::progress_word(test, lang),
        "",
        x,
        y,
        w,
        false,
    );
    let accepted = card
        .related(spm_contracts::RelationshipKind::Accepted)
        .next()
        .is_some()
        || card.stage.stage == spm_contracts::BoardStage::Done;
    let accept_state = if accepted {
        ProgressState::Passed
    } else {
        ProgressState::NotStarted
    };
    y = row(
        b,
        "d:accept",
        lang.t("Acceptance"),
        Some((
            model::progress_glyph(accept_state),
            pal.progress(accept_state),
        )),
        &model::acceptance_text(card, tz, lang),
        &card.stage.explanation,
        x,
        y,
        w,
        false,
    );
    // Signals with their reasons.
    let shown: Vec<Signal> = signals.to_vec();
    if !shown.is_empty() {
        y = section(b, "d:signals", lang.t("Signals"), x, y, w);
        for (i, signal) in shown.iter().enumerate() {
            let detail = match signal {
                Signal::Blocked => card
                    .blockers
                    .iter()
                    .map(|blk| {
                        let mut text = blk.reason.clone();
                        if let Some(contact) = &blk.contact {
                            text.push_str(&format!(
                                " · {}: {}",
                                lang.t("Contact"),
                                contact.display_name
                            ));
                        }
                        if let Some(since) = blk.since {
                            text.push_str(&format!(
                                " · {}",
                                lang.f("since {}", &[&short_time(since, tz)])
                            ));
                        }
                        text
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                Signal::Conflict => card
                    .conflicts
                    .iter()
                    .map(|c| c.explanation.clone())
                    .collect::<Vec<_>>()
                    .join("\n"),
                Signal::SourceNotRefreshed => degraded
                    .iter()
                    .filter(|s| card.observed_through.contains_key(s))
                    .map(|s| {
                        let at = card.observed_through[s];
                        format!(
                            "{} · {}",
                            lang.f("{} did not refresh", &[model::system_label(*s)]),
                            lang.f("observed {}", &[&short_time(at, tz)])
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                Signal::NotInTracker => card
                    .assignee()
                    .map(|a| basis_note(a, tz, lang))
                    .unwrap_or_default(),
                Signal::Unmapped
                | Signal::WorkflowLag
                | Signal::TestFailed
                | Signal::BuildFailed => card.stage.explanation.clone(),
                Signal::Unassigned => card
                    .next_action
                    .as_ref()
                    .map(|n| n.text.clone())
                    .unwrap_or_default(),
            };
            y = row(
                b,
                &format!("d:signal:{i}"),
                signal.label(lang),
                Some((signal.glyph(), pal.tone(signal.tone()))),
                &detail,
                "",
                x,
                y,
                w,
                false,
            );
        }
    }
    if let Some(action) = &card.next_action {
        y = section(b, "d:next", lang.t("Next action"), x, y, w);
        let owner = action.owner.as_ref().map_or_else(
            || lang.t("Unassigned").to_owned(),
            |o| o.display_name.clone(),
        );
        y = row(
            b,
            "d:next:row",
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
    y = section(b, "d:people", lang.t("People"), x, y, w);
    for (i, person) in model::people_rows(card, tz, lang).iter().enumerate() {
        y = row(
            b,
            &format!("d:person:{i}"),
            person.role,
            None,
            &person.name,
            &person.detail,
            x,
            y,
            w,
            person.emphasis,
        );
    }
    // Evidence timeline, loaded on selection.
    let count = match input.evidence {
        EvidenceView::Loaded(e) => e.items.len() as u32,
        _ => card.evidence_count,
    };
    y = section(
        b,
        "d:evidence",
        &format!("{} ({count})", lang.t("Evidence")),
        x,
        y,
        w,
    );
    match input.evidence {
        EvidenceView::None | EvidenceView::Loading => {
            b.text(
                "d:ev:loading".into(),
                rect(x, y, w, 20.0),
                lang.t("Loading evidence…"),
                META,
                400,
                pal.secondary,
                TextAlign::MiddleLeading,
            );
            y += 24.0;
            for i in 0..3 {
                b.fill(
                    format!("d:ev:skel:{i}"),
                    rect(x, y, w * (0.9 - i as f32 * 0.15), 12.0),
                    pal.skeleton,
                    3.0,
                );
                y += 22.0;
            }
        }
        EvidenceView::Failed(error) => {
            let mh = b.line_h(META);
            b.glyph(
                "d:ev:failed:g".into(),
                rect(x, y, 16.0, 20.0),
                "\u{E783}",
                12.0,
                pal.danger,
            );
            b.text(
                "d:ev:failed".into(),
                rect(x + 20.0, y, w - 20.0, 20.0),
                lang.t("Evidence did not load."),
                13.0,
                700,
                pal.danger,
                TextAlign::MiddleLeading,
            );
            let (reason, lines) = b.fit.fit(error, META, 400, w - 20.0, 2);
            b.text(
                "d:ev:reason".into(),
                rect(x + 20.0, y + 22.0, w - 20.0, lines as f32 * mh),
                &reason,
                META,
                400,
                pal.secondary,
                TextAlign::TopLeading,
            );
            y += 28.0 + lines as f32 * mh;
            b.button(
                "d:ev:retry",
                x,
                y,
                28.0,
                Some("\u{E72C}"),
                lang.t("Try again"),
                false,
                Target::RetryEvidence,
                200.0,
            );
            y += 36.0;
        }
        EvidenceView::Loaded(evidence) => {
            y = timeline(
                b,
                card,
                &evidence.items,
                input.ci,
                input.selected_evidence,
                tz,
                x,
                y,
                w,
            );
        }
    }
    b.clip = None;
    b.out.detail_content = y + input.detail_scroll - start + 12.0;
    if b.out.detail_content > region.h {
        scrollbar(
            b,
            "detail",
            region,
            input.detail_scroll,
            b.out.detail_content,
            area.x + area.w - 6.0,
        );
    }
}

fn basis_note(r: &spm_contracts::Relationship, tz: &str, lang: Lang) -> String {
    match &r.statement {
        Some(s) => lang.f(
            "{} named {} in {} · {}",
            &[
                &s.author.display_name,
                &r.person.display_name,
                "Lark",
                &r.source_time.map(|t| short_time(t, tz)).unwrap_or_default(),
            ],
        ),
        None => String::new(),
    }
}

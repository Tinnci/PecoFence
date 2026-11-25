use super::*;

/// The workspace: scope switch, KPIs, stage columns, release lanes, job health and triage.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace(
    b: &mut Builder<'_, Target>,
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

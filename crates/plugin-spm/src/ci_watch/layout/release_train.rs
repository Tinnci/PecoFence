use super::*;

pub(super) fn release_train(
    b: &mut Builder<'_, Target>,
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
pub(super) fn stroke_path(
    b: &mut Builder<'_, Target>,
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
pub(super) fn dashed_vertical(
    b: &mut Builder<'_, Target>,
    key: &str,
    x: f32,
    y0: f32,
    y1: f32,
    color: [f32; 4],
) {
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

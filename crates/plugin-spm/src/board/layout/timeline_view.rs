use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn timeline(
    b: &mut Builder,
    card: &BoardCard,
    items: &[EvidenceItem],
    ci: &spm_contracts::CiWatchSnapshot,
    selected: Option<&str>,
    tz: &str,
    x: f32,
    mut y: f32,
    w: f32,
) -> f32 {
    let lang = b.lang;
    let pal = *b.pal;
    let mh = b.line_h(META);
    let lh = b.line_h(13.0);
    let current = card.current_assignee.as_deref();
    let time_w = 84.0;
    let rail_x = x + time_w + 4.0;
    let tx = rail_x + 16.0;
    let tw = x + w - tx;
    for (i, item) in items.iter().enumerate() {
        let key = format!("d:ev:{}", item.id);
        let at = item.source_time.unwrap_or(item.observed_at);
        let on = selected == Some(item.id.as_str());
        let supports_current =
            current.is_some_and(|c| item.relationship_ids.iter().any(|r| r == c));
        let system = model::system_label(item.source.system);
        let (summary, lines) = b
            .fit
            .fit(&item.summary, 13.0, 400, tw, if on { 6 } else { 3 });
        let mut h = mh + lines as f32 * lh + 8.0;
        let mut kinds: Vec<String> = item
            .relationship_ids
            .iter()
            .filter_map(|id| card.relationship(id))
            .map(|r| {
                let mut kind = format!(
                    "{} {}",
                    model::kind_label(r.kind, lang),
                    r.person.display_name
                );
                if let Some(status) = model::status_label(r.status, lang) {
                    kind.push_str(&format!(" ({status})"));
                }
                if Some(r.id.as_str()) == current {
                    kind = format!("{} ← {}", kind, lang.t("Responsible now"));
                }
                kind
            })
            .collect();
        kinds.dedup();
        let roles = kinds.join(" · ");
        if !roles.is_empty() {
            h += mh;
        }
        if on {
            h += mh + 40.0;
        }
        let r = rect(x - 8.0, y - 4.0, w + 16.0, h);
        if on {
            b.fill(format!("{key}:sel"), r, pal.chip, 8.0);
        }
        // Rail: a dot per item, filled for the item that set the current assignee.
        if i + 1 < items.len() {
            b.fill(
                format!("{key}:rail"),
                rect(rail_x + 5.0, y + 10.0, 1.0, h),
                pal.rule,
                0.0,
            );
        }
        let dot = if supports_current {
            pal.accent_ink
        } else {
            pal.secondary
        };
        b.glyph(
            format!("{key}:dot"),
            rect(rail_x - 2.0, y, 16.0, mh),
            if supports_current {
                "\u{EA3B}"
            } else {
                "\u{EA3A}"
            },
            9.0,
            dot,
        );
        b.text(
            format!("{key}:time"),
            rect(x, y, time_w, mh),
            &short_time(at, tz),
            META,
            400,
            pal.secondary,
            TextAlign::MiddleLeading,
        );
        // The source name is a link to the record itself, one click from the timeline.
        let link_w = (b.fit.width(system, META, 700).ceil() + 18.0).min(tw);
        b.text(
            format!("{key}:system"),
            rect(tx, y, link_w - 16.0, mh),
            system,
            META,
            700,
            pal.accent_ink,
            TextAlign::MiddleLeading,
        );
        b.glyph(
            format!("{key}:system:go"),
            rect(tx + link_w - 15.0, y, 14.0, mh),
            "\u{E8A7}",
            10.0,
            pal.accent_ink,
        );
        let mut cy = y + mh;
        b.text(
            format!("{key}:summary"),
            rect(tx, cy, tw, lines as f32 * lh),
            &summary,
            13.0,
            400,
            pal.primary,
            TextAlign::TopLeading,
        );
        cy += lines as f32 * lh;
        if !roles.is_empty() {
            let (fitted, _) = b.fit.fit(&roles, META, 400, tw, 1);
            b.text(
                format!("{key}:roles"),
                rect(tx, cy, tw, mh),
                &fitted,
                META,
                400,
                if supports_current {
                    pal.accent_ink
                } else {
                    pal.secondary
                },
                TextAlign::MiddleLeading,
            );
            cy += mh;
        }
        b.hit(
            format!("{key}:hit"),
            r,
            "listitem",
            format!("{} {system}: {}", short_time(at, tz), item.summary),
            Target::SelectEvidence(item.id.clone()),
        );
        b.hit(
            format!("{key}:link"),
            rect(tx - 2.0, y, link_w + 4.0, mh),
            "link",
            lang.f("Open in {}", &[system]),
            Target::OpenSource(item.source.clone()),
        );
        // Gerrit and Jenkins rows can be followed in the CI/CD fence. The label says whether the
        // source is already followed, in words as well as the chip style.
        if matches!(
            item.source.system,
            spm_contracts::SourceSystem::Gerrit | spm_contracts::SourceSystem::Jenkins
        ) {
            let watched = crate::ci_watch::entry::source_watched(ci, &item.source);
            let label = lang.t(if watched {
                "Already followed ✓"
            } else {
                "Track in CI/CD"
            });
            let bw = b.fit.width(label, META, 600).ceil() + 26.0;
            b.button(
                &format!("{key}:track"),
                (x + w - bw).max(tx + link_w + 8.0),
                y,
                mh,
                None,
                label,
                watched,
                Target::TrackCi(item.source.clone()),
                bw,
            );
        }
        if on {
            let detail = format!(
                "{} · {} · {}",
                item.field_locator.as_deref().unwrap_or("—"),
                lang.f("observed {}", &[&short_time(item.observed_at, tz)]),
                item.id
            );
            let (fitted, _) = b.fit.fit(&detail, META, 400, tw, 1);
            b.text(
                format!("{key}:locator"),
                rect(tx, cy, tw, mh),
                &fitted,
                META,
                400,
                pal.secondary,
                TextAlign::MiddleLeading,
            );
            cy += mh + 6.0;
            let used = b.button(
                &format!("{key}:open"),
                tx,
                cy,
                28.0,
                Some("\u{E8A7}"),
                lang.t("Open source"),
                false,
                Target::OpenSource(item.source.clone()),
                200.0,
            );
            let reference = format!(
                "{}:{}/{}/{}/{}",
                format!("{:?}", item.source.system).to_lowercase(),
                item.source.tenant,
                item.source.project,
                item.source.record_kind,
                item.source.record_id
            );
            b.button(
                &format!("{key}:copy"),
                tx + used + 8.0,
                cy,
                28.0,
                Some("\u{E8C8}"),
                lang.t("Copy reference"),
                false,
                Target::Copy(reference),
                220.0,
            );
        }
        y += h + 4.0;
    }
    y
}

// ---------------------------------------------------------------------------
// Compact and capsule

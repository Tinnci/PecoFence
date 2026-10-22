//! Organizing suggestions computed from a saved workspace. Pure and read-only: nothing here
//! changes the configuration. The fence menu applies one on a click; `pecofence-cli suggest`
//! prints them for a person or an agent to act on.
//!
//! Every suggestion is explained by counts from the workspace itself (manual placements, open
//! counts, file types and times) and has to clear a threshold, so a sparse or mixed desktop
//! produces no suggestions rather than guesses. See `docs/SUGGESTIONS.md`.

use crate::model::{AssignedBy, Config, ContentId, ContentSpec, Item, ItemId, ItemRef, SortMode};
use crate::order::{extension, file_name, frequent_score, type_rank};
use crate::rules::{
    CLEANUP_IDLE_DAYS, Cond, Decision, ItemFacts, Rule, Target, Template, is_temporary,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

/// Consistent manual placements of one extension before a rule is proposed.
pub const MIN_RULE_EXAMPLES: usize = 3;
/// Share of an extension's manual placements that must agree on one fence.
pub const MIN_RULE_SHARE: f32 = 0.8;
/// Fences smaller than this keep whatever order they have.
pub const MIN_SORT_ITEMS: usize = 6;
/// Idle installers/archives before the cleanup fence is proposed.
pub const MIN_CLEANUP_ITEMS: usize = 3;
/// A placement must beat the runner-up fence by this much, or it is not suggested.
const PLACEMENT_MARGIN: f32 = 0.1;
const DAY: i64 = 86_400;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum Action {
    /// Change how a fence orders its items.
    SetSort { fence: ContentId, sort: SortMode },
    /// A rule that files every `.ext` item into `target` (now and when new ones arrive).
    AddRule { ext: String, target: ContentId },
    /// Move one inbox item into a fence that already holds items like it.
    Move {
        item: ItemId,
        from: ContentId,
        to: ContentId,
    },
    /// Add the "待清理" template fence for idle installers and archives.
    AddCleanup,
}

impl Action {
    /// Whether this action changes or fills `fence`.
    pub fn concerns(&self, fence: ContentId) -> bool {
        match self {
            Action::SetSort { fence: f, .. } => *f == fence,
            Action::AddRule { target, .. } => *target == fence,
            Action::Move { from, to, .. } => *from == fence || *to == fence,
            Action::AddCleanup => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// Stable while the workspace is unchanged: `sort:<fence>:<mode>`, `rule:<ext>:<fence>`,
    /// `move:<item>:<fence>` or `cleanup`.
    pub id: String,
    pub action: Action,
    /// 0–1; only ordering is meaningful, not the absolute value.
    pub confidence: f32,
    /// One line naming the change (English; the fence menu builds its own localized label).
    pub summary: String,
    /// The counts that justify it.
    pub reason: String,
    /// Items that would move or reorder.
    pub affects: usize,
    /// Up to three item names, for recognition.
    pub examples: Vec<String>,
}

/// One file-collection fence of the layout.
struct Fence<'a> {
    id: ContentId,
    title: &'a str,
    inbox: bool,
    sort: SortMode,
    members: Vec<(&'a ItemRef, &'a Item)>,
}

fn fences(config: &Config, layout: usize) -> Vec<Fence<'_>> {
    let Some(layout) = config.layouts.get(layout) else {
        return Vec::new();
    };
    layout
        .contents
        .iter()
        .filter_map(|content| {
            let ContentSpec::FileCollection { inbox, items } = &content.content else {
                return None;
            };
            let members = items
                .iter()
                .filter_map(|r| config.items.get(&r.item_id).map(|it| (r, it)))
                .filter(|(_, it)| it.orphaned_since.is_none())
                .collect();
            Some(Fence {
                id: content.id,
                title: &content.title,
                inbox: *inbox,
                sort: content.view.sort,
                members,
            })
        })
        .collect()
}

fn short(id: impl std::fmt::Display) -> String {
    id.to_string().chars().take(8).collect()
}

/// Items a suggestion may move: real files, not shell namespace entries or downloads in flight.
fn movable(item: &Item) -> bool {
    !item.is_namespace() && !is_temporary(file_name(item))
}

fn examples<'a>(items: impl Iterator<Item = &'a Item>) -> Vec<String> {
    items.take(3).map(|it| it.display_name.clone()).collect()
}

fn idle_days(item: &Item, now: i64) -> Option<i64> {
    let last = item.mtime.max(item.last_opened.unwrap_or(0));
    (last > 0).then(|| (now - last).max(0) / DAY)
}

/// The rule an `AddRule` action stands for.
pub fn rule_for(ext: &str, target: ContentId, fence_title: &str) -> Rule {
    Rule::new(
        &format!(".{ext} → {fence_title}"),
        Target::Collection(target),
        vec![Cond::Ext(vec![format!(".{ext}")])],
    )
}

/// All suggestions for `layout`, most confident first. `now` is Unix seconds.
pub fn suggest(config: &Config, layout: usize, now: i64) -> Vec<Suggestion> {
    let fences = fences(config, layout);
    let mut out = Vec::new();
    let ruled = learned_rules(config, &fences, &mut out);
    placements(&fences, &ruled, &mut out);
    sorts(&fences, now, &mut out);
    cleanup(config, &fences, now, &mut out);
    out.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// "You keep dragging .pdf files into Work": extensions the user files by hand, consistently,
/// into one fence that no rule fills yet. Returns the extensions it proposed rules for.
fn learned_rules(config: &Config, fences: &[Fence], out: &mut Vec<Suggestion>) -> Vec<String> {
    // ext → fence → manual placements. Placements into the inbox count against a rule
    // (the user wanted those left unsorted) but never become a target.
    let mut placed: BTreeMap<String, HashMap<ContentId, usize>> = BTreeMap::new();
    for fence in fences {
        for (r, it) in &fence.members {
            let ext = extension(it);
            // Shortcuts are too varied to file by extension alone (games, tools, sites).
            if r.assigned_by != AssignedBy::User
                || !movable(it)
                || ext.is_empty()
                || matches!(ext.as_str(), "lnk" | "url" | "website")
            {
                continue;
            }
            *placed.entry(ext).or_default().entry(fence.id).or_default() += 1;
        }
    }
    let mut proposed = Vec::new();
    for (ext, by_fence) in placed {
        let total: usize = by_fence.values().sum();
        let Some((&target, &count)) = by_fence
            .iter()
            .filter(|(id, _)| fences.iter().any(|f| f.id == **id && !f.inbox))
            .max_by_key(|(id, n)| (**n, std::cmp::Reverse(**id)))
        else {
            continue;
        };
        let share = count as f32 / total as f32;
        if count < MIN_RULE_EXAMPLES || share < MIN_RULE_SHARE {
            continue;
        }
        let facts = ItemFacts {
            file_name: format!("example.{ext}"),
            ..ItemFacts::default()
        };
        let current = config.rules.evaluate(&facts);
        if matches!(current, Decision::Route { target: Target::Collection(t), .. } if t == target) {
            continue;
        }
        let Some(fence) = fences.iter().find(|f| f.id == target) else {
            continue;
        };
        let overrides = match current {
            Decision::Route { rule, .. } => config
                .rules
                .list
                .iter()
                .find(|r| r.id == rule)
                .map(|r| format!("; it would take precedence over rule “{}”", r.name)),
            _ => None,
        };
        // Items of this kind still waiting elsewhere (not placed by hand).
        let waiting: Vec<&Item> = fences
            .iter()
            .filter(|f| f.id != target)
            .flat_map(|f| f.members.iter())
            .filter(|(r, it)| {
                r.assigned_by != AssignedBy::User && movable(it) && extension(it) == ext
            })
            .map(|(_, it)| *it)
            .collect();
        let strength = ((count - MIN_RULE_EXAMPLES) as f32 / 3.0).min(1.0);
        out.push(Suggestion {
            id: format!("rule:{ext}:{}", short(target)),
            action: Action::AddRule {
                ext: ext.clone(),
                target,
            },
            confidence: share * (0.6 + 0.4 * strength),
            summary: format!("File .{ext} items into “{}” automatically", fence.title),
            reason: format!(
                "{count} of {total} .{ext} items were placed there by hand{}",
                overrides.unwrap_or_default()
            ),
            affects: waiting.len(),
            examples: examples(waiting.into_iter()),
        });
        proposed.push(ext);
    }
    proposed
}

/// Inbox items that look like the contents of exactly one fence.
fn placements(fences: &[Fence], ruled: &[String], out: &mut Vec<Suggestion>) {
    let targets: Vec<&Fence> = fences
        .iter()
        .filter(|f| !f.inbox && f.members.len() >= 2)
        .collect();
    for inbox in fences.iter().filter(|f| f.inbox) {
        for (_, it) in &inbox.members {
            let ext = extension(it);
            if !movable(it) || ruled.contains(&ext) {
                continue;
            }
            let rank = type_rank(it);
            let mut scored: Vec<(f32, &Fence, &'static str)> = targets
                .iter()
                .filter_map(|f| {
                    let size = f.members.len() as f32;
                    let same_ext = f
                        .members
                        .iter()
                        .filter(|(_, m)| !ext.is_empty() && extension(m) == ext)
                        .count();
                    let same_kind = f
                        .members
                        .iter()
                        .filter(|(_, m)| type_rank(m) == rank)
                        .count();
                    let ext_share = same_ext as f32 / size;
                    let kind_share = same_kind as f32 / size;
                    if same_ext >= 2 && ext_share >= 0.5 {
                        Some((0.4 + 0.4 * ext_share, *f, "extension"))
                    } else if rank < 9 && same_kind >= 3 && kind_share >= 0.6 {
                        Some((0.3 + 0.3 * kind_share, *f, "kind"))
                    } else {
                        None
                    }
                })
                .collect();
            scored.sort_by(|a, b| b.0.total_cmp(&a.0));
            let Some(&(confidence, fence, basis)) = scored.first() else {
                continue;
            };
            // Two fences that fit about equally (Games and Tools both full of shortcuts):
            // the user has to decide.
            if scored
                .get(1)
                .is_some_and(|(next, _, _)| confidence - next < PLACEMENT_MARGIN)
            {
                continue;
            }
            let reason = if basis == "extension" {
                format!("“{}” mostly holds .{ext} files", fence.title)
            } else {
                format!("“{}” mostly holds items of the same kind", fence.title)
            };
            out.push(Suggestion {
                id: format!("move:{}:{}", short(it.id), short(fence.id)),
                action: Action::Move {
                    item: it.id,
                    from: inbox.id,
                    to: fence.id,
                },
                confidence,
                summary: format!("Move “{}” to “{}”", it.display_name, fence.title),
                reason,
                affects: 1,
                examples: vec![it.display_name.clone()],
            });
        }
    }
}

/// A better order for fences still in name or unarranged manual order. Fences sorted another
/// way, or arranged by hand, were the user's choice and are left alone.
fn sorts(fences: &[Fence], now: i64, out: &mut Vec<Suggestion>) {
    for fence in fences {
        let n = fence.members.len();
        let arranged = fence.members.iter().any(|(r, _)| r.manual_index.is_some());
        let default_order = fence.sort == SortMode::Name || fence.sort == SortMode::Manual;
        if n < MIN_SORT_ITEMS || !default_order || (fence.sort == SortMode::Manual && arranged) {
            continue;
        }
        let items: Vec<&Item> = fence.members.iter().map(|(_, it)| *it).collect();
        let mut candidates: Vec<(f32, SortMode, String)> = Vec::new();

        // Frequent use: a few items take most of the recent opens.
        let mut scores: Vec<f64> = items.iter().map(|it| frequent_score(it, now)).collect();
        scores.sort_by(|a, b| b.total_cmp(a));
        let total: f64 = scores.iter().sum();
        let opens: u32 = items.iter().map(|it| it.open_count).sum();
        let opened = items.iter().filter(|it| it.open_count > 0).count();
        let recent = items
            .iter()
            .filter_map(|it| it.last_opened)
            .any(|at| now - at <= 30 * DAY);
        if opens >= 10 && opened >= 3 && recent && total > 0.0 {
            let top = n.div_ceil(5);
            let concentration = (scores[..top].iter().sum::<f64>() / total) as f32;
            if concentration >= 0.5 {
                candidates.push((
                    (0.5 + 0.4 * concentration).min(0.9),
                    SortMode::OpenCount,
                    format!(
                        "{top} of {n} items take {:.0}% of recent opens",
                        concentration * 100.0
                    ),
                ));
            }
        }

        // Type: several real groups, none dominating.
        let mut groups: HashMap<u8, usize> = HashMap::new();
        for it in &items {
            *groups.entry(type_rank(it)).or_default() += 1;
        }
        let real = groups.values().filter(|&&c| c >= 2).count();
        let largest = groups.values().copied().max().unwrap_or(0) as f32 / n as f32;
        if real >= 3 && largest <= 0.7 {
            candidates.push((
                (0.5 + 0.05 * real as f32).min(0.75),
                SortMode::Type,
                format!("{real} kinds of items with two or more each"),
            ));
        }

        // Date: mostly files, several edited this week.
        let files = items
            .iter()
            .filter(|it| (3..=7).contains(&type_rank(it)))
            .count();
        let this_week = items
            .iter()
            .filter(|it| it.mtime > 0 && now - it.mtime <= 7 * DAY)
            .count();
        if files as f32 / n as f32 >= 0.6 && this_week >= 2 {
            candidates.push((
                0.55,
                SortMode::Date,
                format!("{this_week} of {n} items changed in the last 7 days"),
            ));
        }

        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        if let Some((confidence, sort, reason)) = candidates.into_iter().next() {
            let view = crate::model::FenceView {
                sort,
                ..Default::default()
            };
            let mut ordered: Vec<(Option<&ItemRef>, &Item)> = fence
                .members
                .iter()
                .map(|(r, it)| (Some(*r), *it))
                .collect();
            crate::order::order_items(&mut ordered, &view, now);
            out.push(Suggestion {
                id: format!("sort:{}:{}", short(fence.id), sort_key(sort)),
                action: Action::SetSort {
                    fence: fence.id,
                    sort,
                },
                confidence,
                summary: format!("Sort “{}” by {}", fence.title, sort_label(sort)),
                reason,
                affects: n,
                examples: examples(ordered.iter().map(|(_, it)| *it)),
            });
        }
    }
}

/// Installers and archives nobody has touched for a month, when no cleanup fence exists.
fn cleanup(config: &Config, fences: &[Fence], now: i64, out: &mut Vec<Suggestion>) {
    let key = Template::Cleanup.key();
    if config
        .rules
        .list
        .iter()
        .any(|r| r.template.as_deref() == Some(key))
    {
        return;
    }
    let idle: Vec<&Item> = fences
        .iter()
        .flat_map(|f| f.members.iter())
        .filter(|(r, it)| {
            r.assigned_by != AssignedBy::User
                && movable(it)
                && matches!(type_rank(it), 7 | 8)
                && idle_days(it, now).is_some_and(|d| d >= i64::from(CLEANUP_IDLE_DAYS))
        })
        .map(|(_, it)| *it)
        .collect();
    if idle.len() < MIN_CLEANUP_ITEMS {
        return;
    }
    out.push(Suggestion {
        id: "cleanup".into(),
        action: Action::AddCleanup,
        confidence: (0.5 + 0.05 * (idle.len() - MIN_CLEANUP_ITEMS) as f32).min(0.8),
        summary: "Gather idle installers and archives into a cleanup fence".into(),
        reason: format!(
            "{} installers or archives untouched for {CLEANUP_IDLE_DAYS}+ days (nothing is deleted)",
            idle.len()
        ),
        affects: idle.len(),
        examples: examples(idle.into_iter()),
    });
}

/// The CLI spelling of a sort mode.
pub fn sort_key(sort: SortMode) -> &'static str {
    match sort {
        SortMode::Manual => "manual",
        SortMode::Name => "name",
        SortMode::Type => "type",
        SortMode::Date => "date",
        SortMode::Size => "size",
        SortMode::OpenCount => "frequent",
    }
}

pub fn parse_sort(key: &str) -> Option<SortMode> {
    [
        SortMode::Manual,
        SortMode::Name,
        SortMode::Type,
        SortMode::Date,
        SortMode::Size,
        SortMode::OpenCount,
    ]
    .into_iter()
    .find(|s| sort_key(*s) == key)
}

fn sort_label(sort: SortMode) -> &'static str {
    match sort {
        SortMode::Manual => "manual order",
        SortMode::Name => "name",
        SortMode::Type => "type",
        SortMode::Date => "date modified",
        SortMode::Size => "size",
        SortMode::OpenCount => "frequent use",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentInstance, Layout};
    use crate::order::tests::item;

    const NOW: i64 = 1_000 * DAY;

    struct Desk {
        config: Config,
    }

    impl Desk {
        fn new() -> Self {
            let mut config = Config::default();
            config.layouts.push(Layout {
                fingerprint: Vec::new(),
                containers: Vec::new(),
                contents: Vec::new(),
            });
            Self { config }
        }

        fn fence(&mut self, title: &str, inbox: bool) -> ContentId {
            let fence = ContentInstance::collection(title, inbox);
            let id = fence.id;
            self.config.layouts[0].contents.push(fence);
            id
        }

        fn add(&mut self, fence: ContentId, it: Item, by: AssignedBy) -> ItemId {
            let id = it.id;
            self.config.items.insert(id, it);
            let content = self.config.layouts[0]
                .content_mut(fence)
                .expect("fence exists");
            if let ContentSpec::FileCollection { items, .. } = &mut content.content {
                items.push(ItemRef {
                    item_id: id,
                    manual_index: None,
                    assigned_by: by,
                });
            }
            id
        }

        fn file(&mut self, fence: ContentId, name: &str, by: AssignedBy) -> ItemId {
            self.add(
                fence,
                item(&format!(r"c:\users\a\desktop\{name}"), false),
                by,
            )
        }

        fn suggest(&self) -> Vec<Suggestion> {
            suggest(&self.config, 0, NOW)
        }
    }

    #[test]
    fn empty_and_missing_layouts_suggest_nothing() {
        assert!(suggest(&Config::default(), 0, NOW).is_empty());
        let mut desk = Desk::new();
        desk.fence("Desktop", true);
        assert!(desk.suggest().is_empty());
        assert!(suggest(&desk.config, 7, NOW).is_empty());
    }

    #[test]
    fn consistent_manual_placements_become_a_rule_that_also_files_waiting_items() {
        let mut desk = Desk::new();
        let inbox = desk.fence("Desktop", true);
        let work = desk.fence("Work", false);
        for name in ["a.pdf", "b.pdf", "c.pdf"] {
            desk.file(work, name, AssignedBy::User);
        }
        desk.file(inbox, "d.pdf", AssignedBy::Default);
        let all = desk.suggest();
        let rule = all
            .iter()
            .find(|s| matches!(s.action, Action::AddRule { .. }))
            .expect("rule suggested");
        assert_eq!(
            rule.action,
            Action::AddRule {
                ext: "pdf".into(),
                target: work
            }
        );
        assert_eq!(rule.affects, 1);
        assert_eq!(rule.examples, ["d"]);
        // The rule covers d.pdf, so no separate move is proposed for it.
        assert!(!all.iter().any(|s| matches!(s.action, Action::Move { .. })));
    }

    #[test]
    fn too_few_or_inconsistent_placements_propose_no_rule() {
        let mut desk = Desk::new();
        desk.fence("Desktop", true);
        let work = desk.fence("Work", false);
        let home = desk.fence("Home", false);
        desk.file(work, "a.pdf", AssignedBy::User);
        desk.file(work, "b.pdf", AssignedBy::User);
        assert!(desk.suggest().iter().all(|s| !s.id.starts_with("rule:")));
        desk.file(work, "c.pdf", AssignedBy::User);
        desk.file(home, "d.pdf", AssignedBy::User);
        // 3 of 4 is 75%, under the 80% agreement threshold.
        assert!(desk.suggest().iter().all(|s| !s.id.starts_with("rule:")));
    }

    #[test]
    fn no_rule_when_an_existing_rule_already_fills_that_fence_or_for_shortcuts() {
        let mut desk = Desk::new();
        desk.fence("Desktop", true);
        let work = desk.fence("Work", false);
        let apps = desk.fence("Apps", false);
        for name in ["a.pdf", "b.pdf", "c.pdf"] {
            desk.file(work, name, AssignedBy::User);
        }
        for name in ["a.lnk", "b.lnk", "c.lnk", "d.lnk"] {
            desk.file(apps, name, AssignedBy::User);
        }
        desk.config.rules.list.push(rule_for("pdf", work, "Work"));
        assert!(desk.suggest().iter().all(|s| !s.id.starts_with("rule:")));
    }

    #[test]
    fn inbox_items_move_only_to_a_clearly_better_fence() {
        let mut desk = Desk::new();
        let inbox = desk.fence("Desktop", true);
        let photos = desk.fence("Photos", false);
        for name in ["1.jpg", "2.jpg", "3.jpg"] {
            desk.file(photos, name, AssignedBy::Rule(uuid::Uuid::nil()));
        }
        let new = desk.file(inbox, "4.jpg", AssignedBy::Default);
        desk.file(inbox, "~$draft.docx", AssignedBy::Default);
        let moves: Vec<_> = desk
            .suggest()
            .into_iter()
            .filter(|s| matches!(s.action, Action::Move { .. }))
            .collect();
        assert_eq!(moves.len(), 1, "temporary files are never moved");
        assert_eq!(
            moves[0].action,
            Action::Move {
                item: new,
                from: inbox,
                to: photos
            }
        );

        // A second fence that fits just as well makes it ambiguous.
        let more = desk.fence("More photos", false);
        for name in ["5.jpg", "6.jpg", "7.jpg"] {
            desk.file(more, name, AssignedBy::Default);
        }
        assert!(desk.suggest().iter().all(|s| !s.id.starts_with("move:")));
    }

    #[test]
    fn frequent_order_is_suggested_when_a_few_items_take_most_recent_opens() {
        let mut desk = Desk::new();
        let apps = desk.fence("Apps", false);
        for (i, opens) in [40u32, 25, 1, 1, 0, 0, 0, 0, 0, 0].into_iter().enumerate() {
            let id = desk.file(apps, &format!("app{i}.lnk"), AssignedBy::Default);
            let it = desk.config.items.get_mut(&id).unwrap();
            it.open_count = opens;
            it.last_opened = (opens > 0).then_some(NOW - DAY);
        }
        let sort = desk
            .suggest()
            .into_iter()
            .find(|s| s.id.starts_with("sort:"))
            .expect("sort suggested");
        assert_eq!(
            sort.action,
            Action::SetSort {
                fence: apps,
                sort: SortMode::OpenCount
            }
        );
        assert_eq!(sort.examples, ["app0", "app1", "app2"]);

        // An order the user picked themselves is left alone.
        desk.config.layouts[0].contents[0].view.sort = SortMode::Size;
        assert!(desk.suggest().iter().all(|s| !s.id.starts_with("sort:")));
    }

    #[test]
    fn hand_arranged_fences_keep_their_order() {
        let mut desk = Desk::new();
        let mixed = desk.fence("Mixed", false);
        for name in ["a.pdf", "b.pdf", "c.png", "d.png", "e.mp3", "f.mp3"] {
            desk.file(mixed, name, AssignedBy::Default);
        }
        assert!(
            desk.suggest()
                .iter()
                .any(|s| s.id.ends_with(":type") && s.action.concerns(mixed))
        );
        if let ContentSpec::FileCollection { items, .. } =
            &mut desk.config.layouts[0].contents[0].content
        {
            items[0].manual_index = Some(0);
        }
        assert!(desk.suggest().iter().all(|s| !s.id.starts_with("sort:")));
    }

    #[test]
    fn idle_installers_propose_cleanup_once() {
        let mut desk = Desk::new();
        let inbox = desk.fence("Desktop", true);
        for name in ["setup.exe", "tool.msi", "old.zip"] {
            let id = desk.file(inbox, name, AssignedBy::Default);
            desk.config.items.get_mut(&id).unwrap().mtime = NOW - 60 * DAY;
        }
        assert!(desk.suggest().iter().any(|s| s.id == "cleanup"));
        let mut rule = Template::Cleanup.rule(inbox);
        rule.template = Some(Template::Cleanup.key().into());
        desk.config.rules.list.push(rule);
        assert!(desk.suggest().iter().all(|s| s.id != "cleanup"));
    }

    #[test]
    fn sort_keys_round_trip() {
        for key in ["manual", "name", "type", "date", "size", "frequent"] {
            assert_eq!(parse_sort(key).map(sort_key), Some(key));
        }
        assert_eq!(parse_sort("opened"), None);
    }
}

//! Display order of a fence's items. Shared by the fence windows and `pecofence-cli order`, so
//! a preview is exactly what the fence will show.

use crate::model::{FenceView, Item, ItemRef, SortMode};
use crate::rules::{TypeCategory, category_of};
use std::cmp::Ordering;

/// Opens lose half their weight every this many days in "按使用频率" order.
pub const FREQUENT_HALF_LIFE_DAYS: f64 = 30.0;

/// Weight of opens recorded before `last_opened` existed: as if three half-lives old.
const UNDATED_OPEN_WEIGHT: f64 = 0.125;

/// Case-insensitive natural ordering (digits compared numerically), like Explorer.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let mut na = 0u64;
                while let Some(c) = ai.peek().copied().filter(|c| c.is_ascii_digit()) {
                    na = na.saturating_mul(10).saturating_add(c as u64 - '0' as u64);
                    ai.next();
                }
                let mut nb = 0u64;
                while let Some(c) = bi.peek().copied().filter(|c| c.is_ascii_digit()) {
                    nb = nb.saturating_mul(10).saturating_add(c as u64 - '0' as u64);
                    bi.next();
                }
                if na != nb {
                    return na.cmp(&nb);
                }
            }
            (Some(ca), Some(cb)) => {
                if ca != cb {
                    return ca.cmp(&cb);
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// File name with extension: the last segment of a path key, else the display name
/// (shell namespace items have no path).
pub fn file_name(item: &Item) -> &str {
    item.key
        .as_path()
        .and_then(|p| p.rsplit(['\\', '/']).next())
        .filter(|name| !name.is_empty())
        .unwrap_or(&item.display_name)
}

/// Lower-case extension without the dot; empty for folders and extensionless files.
pub fn extension(item: &Item) -> String {
    if item.is_folder {
        return String::new();
    }
    match file_name(item).rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_lowercase(),
        _ => String::new(),
    }
}

/// Group rank in type order: folders, programs, web shortcuts, documents, images, music,
/// video, archives, installers, then everything else.
pub fn type_rank(item: &Item) -> u8 {
    match category_of(file_name(item), item.is_folder) {
        Some(TypeCategory::Folders) => 0,
        Some(TypeCategory::Programs) => 1,
        Some(TypeCategory::Shortcuts) => 2,
        Some(TypeCategory::Documents) => 3,
        Some(TypeCategory::Images) => 4,
        Some(TypeCategory::Music) => 5,
        Some(TypeCategory::Video) => 6,
        Some(TypeCategory::Archives) => 7,
        Some(TypeCategory::Installers) => 8,
        None => 9,
    }
}

/// Opens weighted by how recent the last one was, so an app used daily this month outranks
/// one opened often last year.
pub fn frequent_score(item: &Item, now: i64) -> f64 {
    if item.open_count == 0 {
        return 0.0;
    }
    let weight = match item.last_opened {
        Some(at) => {
            let days = (now - at).max(0) as f64 / 86_400.0;
            0.5f64.powf(days / FREQUENT_HALF_LIFE_DAYS)
        }
        None => UNDATED_OPEN_WEIGHT,
    };
    f64::from(item.open_count) * weight
}

/// Sorts `items` into display order for `view` at time `now` (Unix seconds). `ItemRef`s carry
/// the manual positions; portal items have none.
pub fn order_items(items: &mut [(Option<&ItemRef>, &Item)], view: &FenceView, now: i64) {
    match view.sort {
        SortMode::Manual => items.sort_by_key(|(r, it)| {
            (
                r.and_then(|r| r.manual_index).unwrap_or(u32::MAX),
                it.display_name.to_lowercase(),
            )
        }),
        SortMode::Name => items.sort_by(|a, b| natural_cmp(&a.1.display_name, &b.1.display_name)),
        SortMode::Type => items.sort_by(|a, b| {
            (type_rank(a.1), extension(a.1))
                .cmp(&(type_rank(b.1), extension(b.1)))
                .then_with(|| natural_cmp(&a.1.display_name, &b.1.display_name))
        }),
        SortMode::Date => items.sort_by_key(|(_, it)| std::cmp::Reverse(it.mtime)),
        SortMode::Size => items.sort_by(|a, b| {
            // Folders first (Explorer), then largest first, ties by name.
            (!a.1.is_folder, std::cmp::Reverse(a.1.size))
                .cmp(&(!b.1.is_folder, std::cmp::Reverse(b.1.size)))
                .then_with(|| natural_cmp(&a.1.display_name, &b.1.display_name))
        }),
        SortMode::OpenCount => items.sort_by(|a, b| {
            frequent_score(b.1, now)
                .total_cmp(&frequent_score(a.1, now))
                .then_with(|| b.1.open_count.cmp(&a.1.open_count))
                .then_with(|| natural_cmp(&a.1.display_name, &b.1.display_name))
        }),
    }
    if view.sort != SortMode::Manual {
        // Explorer keeps the Recycle Bin and friends ahead of files whatever the order.
        items.sort_by_key(|(_, it)| !it.is_namespace());
    }
    if view.reverse {
        items.reverse();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{IconKey, ItemKey, Origin};

    pub(crate) fn item(path: &str, is_folder: bool) -> Item {
        let name = path.rsplit('\\').next().unwrap_or(path);
        let display = match name.rsplit_once('.') {
            Some((stem, _)) if !is_folder && !stem.is_empty() => stem.to_string(),
            _ => name.to_string(),
        };
        Item {
            id: uuid::Uuid::new_v4(),
            key: ItemKey::from_path(path),
            origin: Origin::UserDesktop,
            display_name: display,
            file_id: None,
            mtime: 0,
            is_folder,
            attrs: 0,
            icon_key: IconKey::ByContent {
                path: path.to_lowercase(),
                mtime: 0,
            },
            orphaned_since: None,
            size: 0,
            open_count: 0,
            last_opened: None,
        }
    }

    fn names(items: &[(Option<&ItemRef>, &Item)]) -> Vec<String> {
        items
            .iter()
            .map(|(_, it)| it.display_name.clone())
            .collect()
    }

    #[test]
    fn type_order_groups_by_kind_not_by_extension_spelling() {
        let all = [
            item(r"c:\d\photo.png", false),
            item(r"c:\d\Projects", true),
            item(r"c:\d\Report 10.docx", false),
            item(r"c:\d\Report 9.pdf", false),
            item(r"c:\d\Chrome.lnk", false),
            item(r"c:\d\setup.exe", false),
            item(r"c:\d\notes.xyz", false),
        ];
        let mut items: Vec<_> = all.iter().map(|it| (None, it)).collect();
        let view = FenceView {
            sort: SortMode::Type,
            ..FenceView::default()
        };
        order_items(&mut items, &view, 0);
        // Documents stay together (docx before pdf by extension); the installer goes after
        // the files, not among the programs.
        assert_eq!(
            names(&items),
            [
                "Projects",
                "Chrome",
                "Report 10",
                "Report 9",
                "photo",
                "setup",
                "notes"
            ]
        );
    }

    #[test]
    fn frequent_order_lets_recent_use_outrank_old_heavy_use() {
        let now = 400 * 86_400;
        let mut old = item(r"c:\d\Old.lnk", false);
        old.open_count = 200;
        old.last_opened = Some(now - 120 * 86_400);
        let mut fresh = item(r"c:\d\Fresh.lnk", false);
        fresh.open_count = 20;
        fresh.last_opened = Some(now - 86_400);
        let mut never = item(r"c:\d\Never.lnk", false);
        never.open_count = 0;
        let all = [never, old, fresh];
        let mut items: Vec<_> = all.iter().map(|it| (None, it)).collect();
        let view = FenceView {
            sort: SortMode::OpenCount,
            ..FenceView::default()
        };
        order_items(&mut items, &view, now);
        assert_eq!(names(&items), ["Fresh", "Old", "Never"]);
    }

    #[test]
    fn undated_opens_still_count_but_weigh_less() {
        let mut legacy = item(r"c:\d\Legacy.lnk", false);
        legacy.open_count = 8;
        assert_eq!(frequent_score(&legacy, 1_000_000), 1.0);
    }

    #[test]
    fn names_compare_numbers_by_value() {
        assert_eq!(natural_cmp("File 9", "file 10"), Ordering::Less);
        assert_eq!(natural_cmp("a", "A"), Ordering::Equal);
    }
}

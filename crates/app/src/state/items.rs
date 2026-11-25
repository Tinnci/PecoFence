use super::*;

impl AppState {
    // ---- desktop items ---------------------------------------------------------------------

    /// Rules only run automatically while "新项目出现在桌面时自动归类" is on; otherwise new
    /// items go to the inbox until the user applies the rules by hand.
    fn route_decision(&self, entry: &DesktopEntry) -> Decision {
        // The Recycle Bin and friends are not files: they always live in the inbox ("桌面").
        if entry.origin == EntryOrigin::Namespace {
            return Decision::Default(Target::Inbox);
        }
        if self.config.rules.keep_updated {
            self.config.rules.evaluate(&self.facts_for(entry))
        } else {
            Decision::Default(Target::Inbox)
        }
    }

    /// `id` if it is a fence that can hold routed desktop items. A folder portal never shows
    /// `fence.items` (its content comes from the folder), so routing into one would make the
    /// item vanish from every fence.
    pub fn routable_fence(&self, id: FenceId) -> Option<FenceId> {
        let content = self.config.layouts.get(self.layout)?.content(id)?;
        matches!(content.content, ContentSpec::FileCollection { .. }).then_some(id)
    }

    pub(super) fn target_fence(&self, decision: Decision) -> Option<(FenceId, AssignedBy)> {
        let inbox = self.inbox_id();
        match decision {
            Decision::Skip => None,
            Decision::Route { target, rule } => {
                let fence = match target {
                    Target::Inbox => inbox?,
                    Target::Collection(id) if self.routable_fence(id).is_some() => id,
                    Target::Collection(_) => inbox?,
                };
                Some((fence, AssignedBy::Rule(rule)))
            }
            Decision::Default(target) => {
                let fence = match target {
                    Target::Inbox => inbox?,
                    Target::Collection(id) if self.routable_fence(id).is_some() => id,
                    Target::Collection(_) => inbox?,
                };
                Some((fence, AssignedBy::Default))
            }
        }
    }

    fn facts_for(&self, entry: &DesktopEntry) -> pecofence_core::ItemFacts {
        let last_opened = self
            .catalog
            .get(&ItemKey::from_path(&entry.path.to_string_lossy()))
            .and_then(|id| self.config.items.get(id))
            .and_then(|it| it.last_opened);
        Self::facts_at(entry, last_opened, pecofence_core::now_unix())
    }

    /// Whole days between `now` and the later of last write and last open; `None` when the
    /// file reports no time at all.
    fn idle_days(mtime: i64, last_opened: Option<i64>, now: i64) -> Option<u32> {
        let last = mtime.max(last_opened.unwrap_or(0));
        (last > 0).then(|| ((now - last).max(0) / 86_400) as u32)
    }

    fn facts_at(
        entry: &DesktopEntry,
        last_opened: Option<i64>,
        now: i64,
    ) -> pecofence_core::ItemFacts {
        let lower = entry.file_name.to_lowercase();
        let (shortcut_target, shortcut_target_ext) = if lower.ends_with(".lnk") {
            let t = shell::shortcut_target(&entry.path);
            let ext = t.as_deref().map(pecofence_core::rules::ext_of);
            (t, ext)
        } else if lower.ends_with(".url") {
            (shell::url_shortcut_target(&entry.path), Some(".url".into()))
        } else {
            (None, None)
        };
        pecofence_core::ItemFacts {
            file_name: entry.file_name.clone(),
            is_folder: entry.is_folder,
            size_bytes: entry.size,
            shortcut_target,
            shortcut_target_ext,
            created_minutes_local: pecofence_platform::fileinfo::local_time_parts(entry.created)
                .map(|(m, _)| m),
            created_weekday: pecofence_platform::fileinfo::local_time_parts(entry.created)
                .map(|(_, d)| d),
            origin: match entry.origin {
                EntryOrigin::UserDesktop => Origin::UserDesktop,
                EntryOrigin::PublicDesktop => Origin::PublicDesktop,
                EntryOrigin::Namespace => Origin::Namespace,
            },
            is_hidden: false,
            is_system: false,
            idle_days: Self::idle_days(entry.mtime, last_opened, now),
        }
    }

    /// Reconciles the item table with the current desktop contents. New items are routed by the
    /// rule set; missing items are marked orphaned (membership kept) and garbage-collected
    /// after seven days.
    pub fn sync_desktop(&mut self, entries: &[DesktopEntry]) -> SyncReport {
        let mut report = SyncReport::default();
        let now = pecofence_core::now_unix();
        let mut seen: HashSet<ItemId> = HashSet::new();

        for entry in entries {
            let key = ItemKey::from_path(&entry.path.to_string_lossy());
            let (icon_key, _) =
                crate::icons::icon_key_for(&entry.path, entry.is_folder, entry.mtime);
            if let Some(&id) = self.catalog.get(&key) {
                seen.insert(id);
                if let Some(item) = self.config.items.get_mut(&id) {
                    let mut changed = false;
                    if item.orphaned_since.is_some() {
                        item.orphaned_since = None;
                        changed = true;
                    }
                    if item.mtime != entry.mtime || item.icon_key != icon_key {
                        item.mtime = entry.mtime;
                        item.icon_key = icon_key;
                        changed = true;
                    }
                    if item.size != entry.size {
                        item.size = entry.size;
                        changed = true;
                    }
                    if changed {
                        report.updated += 1;
                        self.mark_dirty();
                    }
                }
                // An item that exists but is in no fence (e.g. its fence was deleted) → route.
                if self.config.content_of_item(self.layout, id).is_none() {
                    let decision = self.route_decision(entry);
                    if let Some((fence, by)) = self.target_fence(decision) {
                        self.config.assign(self.layout, id, fence, by);
                        self.mark_dirty();
                    }
                }
                continue;
            }
            // New item.
            let display_name = shell::display_name(&entry.path).unwrap_or_else(|| {
                let stem = Path::new(&entry.file_name)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| entry.file_name.clone());
                if entry.is_folder {
                    entry.file_name.clone()
                } else {
                    stem
                }
            });
            let item = Item {
                id: uuid::Uuid::new_v4(),
                key: key.clone(),
                origin: match entry.origin {
                    EntryOrigin::UserDesktop => Origin::UserDesktop,
                    EntryOrigin::PublicDesktop => Origin::PublicDesktop,
                    EntryOrigin::Namespace => Origin::Namespace,
                },
                display_name,
                file_id: None,
                mtime: entry.mtime,
                is_folder: entry.is_folder,
                attrs: entry.attributes,
                icon_key,
                orphaned_since: None,
                size: entry.size,
                open_count: 0,
                last_opened: None,
            };
            let id = item.id;
            self.config.items.insert(id, item);
            self.catalog.insert(key, id);
            seen.insert(id);
            let decision = self.route_decision(entry);
            if let Some((fence, by)) = self.target_fence(decision) {
                self.config.assign(self.layout, id, fence, by);
            }
            report.added += 1;
            self.mark_dirty();
        }

        // Orphans.
        let mut to_remove = Vec::new();
        for (id, item) in self.config.items.iter_mut() {
            if seen.contains(id) {
                continue;
            }
            match item.orphaned_since {
                None => {
                    item.orphaned_since = Some(now);
                    report.removed += 1;
                    self.document.change();
                }
                Some(since) if now - since > ORPHAN_GC_SECS => to_remove.push(*id),
                Some(_) => {}
            }
        }
        for id in to_remove {
            if let Some(item) = self.config.items.remove(&id) {
                self.catalog.remove(&item.key);
            }
            for layout in &mut self.config.layouts {
                for content in &mut layout.contents {
                    if let ContentSpec::FileCollection { items, .. } = &mut content.content {
                        items.retain(|r| r.item_id != id);
                    }
                }
            }
            self.mark_dirty();
        }
        report
    }

    /// Live workspace entries, including the current contents of folder portals.
    /// The same file mirrored by several fences is counted once.
    pub fn workspace_item_count(&self) -> usize {
        self.config
            .items
            .values()
            .chain(self.portal_items.values())
            .filter(|item| item.orphaned_since.is_none())
            .map(|item| &item.key)
            .collect::<HashSet<_>>()
            .len()
    }

    /// Items of `fence` in display order (orphans hidden).
    pub fn items_of(&self, fence: &Fence) -> Vec<&Item> {
        let mut items: Vec<(Option<&ItemRef>, &Item)> = if fence.kind == FenceKind::FolderPortal {
            self.portal_members
                .get(&fence.id)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|i| self.portal_items.get(i).map(|it| (None, it)))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            fence
                .items
                .iter()
                .filter_map(|r| self.config.items.get(&r.item_id).map(|it| (Some(r), it)))
                .filter(|(_, it)| it.orphaned_since.is_none())
                .collect()
        };
        pecofence_core::order::order_items(&mut items, &fence.view, pecofence_core::now_unix());
        items.into_iter().map(|(_, it)| it).collect()
    }

    /// Counts a launch (desktop items only; portal items are rebuilt from the folder).
    pub fn note_opened(&mut self, id: ItemId) {
        if let Some(it) = self.config.items.get_mut(&id) {
            it.open_count = it.open_count.saturating_add(1);
            it.last_opened = Some(pecofence_core::now_unix());
            self.mark_dirty();
        }
    }

    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.config
            .items
            .get(&id)
            .or_else(|| self.portal_items.get(&id))
    }

    pub fn item_by_path(&self, path: &Path) -> Option<ItemId> {
        self.catalog
            .get(&ItemKey::from_path(&path.to_string_lossy()))
            .copied()
    }

    /// Re-keys an item after a rename so it stays in its fence. Returns false if unknown.
    pub fn rename_item(&mut self, old: &Path, new: &Path) -> bool {
        let old_key = ItemKey::from_path(&old.to_string_lossy());
        let Some(id) = self.catalog.remove(&old_key) else {
            return false;
        };
        let new_key = ItemKey::from_path(&new.to_string_lossy());
        let Some(item) = self.config.items.get_mut(&id) else {
            return false;
        };
        let is_folder = std::fs::metadata(new)
            .map(|m| m.is_dir())
            .unwrap_or(item.is_folder);
        item.key = new_key.clone();
        item.is_folder = is_folder;
        item.display_name = shell::display_name(new).unwrap_or_else(|| {
            new.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        });
        let (icon_key, _) = crate::icons::icon_key_for(new, is_folder, item.mtime);
        item.icon_key = icon_key;
        item.orphaned_since = None;
        // A stale item already keyed by the new path (e.g. deleted then re-created under the old
        // name) would otherwise leave two items with one key; drop it first.
        if let Some(&other) = self.catalog.get(&new_key)
            && other != id
        {
            self.config.items.remove(&other);
            for layout in &mut self.config.layouts {
                for f in &mut layout.contents {
                    if let ContentSpec::FileCollection { items, .. } = &mut f.content {
                        items.retain(|r| r.item_id != other);
                    }
                }
            }
        }
        self.catalog.insert(new_key, id);
        self.mark_dirty();
        true
    }

    pub fn move_items(&mut self, items: &[ItemId], to: FenceId) -> usize {
        // Portal fences show a folder, not desktop membership: nothing can be moved into them,
        // and their (runtime) items cannot be moved out.
        if self.routable_fence(to).is_none() {
            return 0;
        }
        let mut n = 0;
        for &id in items {
            if !self.config.items.contains_key(&id) {
                continue;
            }
            if self
                .config
                .assign(self.layout, id, to, AssignedBy::User)
                .is_some()
            {
                n += 1;
            }
        }
        if n > 0 {
            self.mark_dirty();
        }
        n
    }

    /// Manual arrangement: places `items` (keeping their relative order) before display index
    /// `index` of `fence`. Only for 手动-sorted virtual fences (portals are sort-only; a sorted
    /// fence snaps back, like Explorer with auto-arrange on). Returns true when the order changed.
    pub fn reorder_items(&mut self, fence: FenceId, items: &[ItemId], index: usize) -> bool {
        if self.routable_fence(fence).is_none() {
            return false;
        }
        let Some(f) = self.fence(fence) else {
            return false;
        };
        if f.view.sort != SortMode::Manual {
            return false;
        }
        let reverse = f.view.reverse;
        let mut order: Vec<ItemId> = self.items_of(&f).iter().map(|it| it.id).collect();
        let before_order = order.clone();
        let moved: Vec<ItemId> = order
            .iter()
            .copied()
            .filter(|id| items.contains(id))
            .collect();
        if moved.is_empty() {
            return false;
        }
        let before = order[..index.min(order.len())]
            .iter()
            .filter(|id| items.contains(id))
            .count();
        order.retain(|id| !items.contains(id));
        let at = index.saturating_sub(before).min(order.len());
        order.splice(at..at, moved);
        if order == before_order {
            return false;
        }
        if reverse {
            // manual_index is stored in forward order; the view reverses it on display.
            order.reverse();
        }
        let Some(fm) = self.content_mut(fence) else {
            return false;
        };
        let ContentSpec::FileCollection { items, .. } = &mut fm.content else {
            return false;
        };
        for (i, id) in order.iter().enumerate() {
            if let Some(r) = items.iter_mut().find(|r| r.item_id == *id) {
                r.manual_index = Some(i as u32);
            }
        }
        self.mark_dirty();
        true
    }

    /// Re-runs the rules over every item (user assignments are respected).
    pub fn apply_rules_all(&mut self, entries: &[DesktopEntry]) -> usize {
        self.apply_rules_filtered(entries, false)
    }

    /// The hourly sweep for clock-dependent rules: only an item whose winning rule has an
    /// idle-days condition moves, so a later-added type rule still waits for "立即应用".
    pub fn apply_idle_rules(&mut self, entries: &[DesktopEntry]) -> usize {
        self.apply_rules_filtered(entries, true)
    }

    fn apply_rules_filtered(&mut self, entries: &[DesktopEntry], idle_only: bool) -> usize {
        let mut moved = 0;
        for entry in entries {
            if entry.origin == EntryOrigin::Namespace {
                continue;
            }
            let key = ItemKey::from_path(&entry.path.to_string_lossy());
            let Some(&id) = self.catalog.get(&key) else {
                continue;
            };
            let snapshots = self.fences();
            let current = snapshots
                .iter()
                .find(|f| f.items.iter().any(|r| r.item_id == id));
            if let Some(f) = current
                && f.items
                    .iter()
                    .any(|r| r.item_id == id && r.assigned_by == AssignedBy::User)
            {
                continue;
            }
            let decision = self.config.rules.evaluate(&self.facts_for(entry));
            if idle_only {
                let by_idle_rule = match decision {
                    Decision::Route { rule, .. } => self
                        .config
                        .rules
                        .list
                        .iter()
                        .any(|r| r.id == rule && Self::has_idle_cond(r)),
                    _ => false,
                };
                if !by_idle_rule {
                    continue;
                }
            }
            if let Some((fence, by)) = self.target_fence(decision)
                && self.config.assign(self.layout, id, fence, by).is_some()
            {
                moved += 1;
            }
        }
        if moved > 0 {
            self.mark_dirty();
        }
        moved
    }

    /// Re-files `ids` by the rules as if they had just arrived on the desktop (the user dragged
    /// them out of a fence onto the desktop: an explicit "let go", so an earlier manual
    /// placement no longer pins them). Unmatched items stay where they are (the inbox).
    /// Returns the fences whose membership changed.
    pub fn apply_rules_to(&mut self, ids: &[ItemId], entries: &[DesktopEntry]) -> Vec<FenceId> {
        let mut touched = Vec::new();
        for entry in entries {
            if entry.origin == EntryOrigin::Namespace {
                continue;
            }
            let key = ItemKey::from_path(&entry.path.to_string_lossy());
            let Some(&id) = self.catalog.get(&key) else {
                continue;
            };
            if !ids.contains(&id) {
                continue;
            }
            let from = self
                .fences()
                .iter()
                .find(|f| f.items.iter().any(|r| r.item_id == id))
                .map(|f| f.id);
            let decision = self.config.rules.evaluate(&self.facts_for(entry));
            if let Some((fence, by)) = self.target_fence(decision)
                && Some(fence) != from
                && self.config.assign(self.layout, id, fence, by).is_some()
            {
                touched.extend(from);
                touched.push(fence);
            }
        }
        if !touched.is_empty() {
            self.mark_dirty();
            touched.sort_unstable();
            touched.dedup();
        }
        touched
    }

    pub fn add_rule(&mut self, rule: pecofence_core::rules::Rule) -> bool {
        let Target::Collection(target) = rule.target else {
            return false;
        };
        let duplicate = self
            .config
            .rules
            .list
            .iter()
            .any(|r| r.target == rule.target && r.all_of == rule.all_of);
        if !self.save_allowed || duplicate || self.routable_fence(target).is_none() {
            return false;
        }
        self.insert_rule(rule);
        true
    }

    pub(super) fn insert_rule(&mut self, rule: pecofence_core::rules::Rule) {
        // Idle-days rules must stay ahead of the type-only ones ("待清理" before "安装包"),
        // otherwise a plain type rule claims every installer first and the idle rule never fires.
        let at = if Self::has_idle_cond(&rule) {
            0
        } else {
            self.config
                .rules
                .list
                .iter()
                .take_while(|r| Self::has_idle_cond(r))
                .count()
        };
        self.config.rules.list.insert(at, rule);
        self.mark_dirty();
    }

    fn has_idle_cond(rule: &pecofence_core::rules::Rule) -> bool {
        rule.all_of
            .iter()
            .any(|c| matches!(c, Cond::IdleDays { .. }))
    }
}

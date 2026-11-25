use super::*;

impl AppState {
    pub fn is_portal_item(&self, id: ItemId) -> bool {
        self.portal_items.contains_key(&id)
    }

    /// The folder a portal fence currently shows (its root, or the subfolder navigated into).
    pub fn portal_path(&self, id: FenceId) -> Option<PathBuf> {
        let root = self.portal_root(id)?;
        Some(self.portal_cwd.get(&id).cloned().unwrap_or(root))
    }

    /// The folder a portal was created for.
    pub fn portal_root(&self, id: FenceId) -> Option<PathBuf> {
        let content = self.config.layouts.get(self.layout)?.content(id)?;
        match &content.content {
            ContentSpec::FolderPortal { root, .. } => Some(PathBuf::from(root)),
            _ => None,
        }
    }

    /// True when the portal shows a subfolder of its root (an "up" is possible).
    pub fn portal_navigated(&self, id: FenceId) -> bool {
        self.portal_cwd.contains_key(&id)
    }

    /// Navigates a portal into `dir` (must be inside its root). Returns false if refused.
    pub fn portal_enter(&mut self, id: FenceId, dir: &Path) -> bool {
        self.reconcile_portal_sources();
        let Some(root) = self.portal_root(id) else {
            return false;
        };
        // Lexical navigation policy, not a junction/symlink security sandbox.
        // Split explicitly: Path::components normalizes interior "." away.
        if dir
            .to_string_lossy()
            .split(['/', '\\'])
            .any(|part| part == "." || part == "..")
        {
            return false;
        }
        let root_key = ItemKey::from_path(&root.to_string_lossy());
        let dir_key = ItemKey::from_path(&dir.to_string_lossy());
        let (Some(rk), Some(dk)) = (root_key.as_path(), dir_key.as_path()) else {
            return false;
        };
        if dk == rk {
            self.portal_cwd.remove(&id);
        } else if dk.starts_with(&dir_prefix(rk)) {
            // Preserve casing from the committed read, not another UI-thread directory scan.
            let actual = self
                .portals
                .snapshot(id)
                .and_then(|s| {
                    s.entries
                        .iter()
                        .find(|entry| ItemKey::from_path(&entry.path.to_string_lossy()) == dir_key)
                })
                .map(|entry| entry.path.clone())
                .unwrap_or_else(|| dir.to_path_buf());
            self.portal_cwd.insert(id, actual);
        } else {
            return false;
        }
        self.request_portal_read(id);
        true
    }

    /// One folder up (never above the root). Returns false when already at the root.
    pub fn portal_up(&mut self, id: FenceId) -> bool {
        self.reconcile_portal_sources();
        let Some(cur) = self.portal_cwd.get(&id).cloned() else {
            return false;
        };
        match cur.parent() {
            Some(parent) => self.portal_enter(id, parent),
            None => {
                self.portal_cwd.remove(&id);
                self.request_portal_read(id);
                true
            }
        }
    }

    /// Back to the root folder.
    pub fn portal_home(&mut self, id: FenceId) -> bool {
        self.reconcile_portal_sources();
        if self.portal_cwd.remove(&id).is_some() {
            self.request_portal_read(id);
            true
        } else {
            false
        }
    }

    /// The portal (if any) whose current folder contains this runtime item.
    pub fn portal_of_item(&self, item: ItemId) -> Option<FenceId> {
        self.portal_members
            .iter()
            .find(|(_, ids)| ids.contains(&item))
            .map(|(f, _)| *f)
    }

    pub fn set_portal_navigate(&mut self, id: FenceId, on: bool) {
        if let Some(f) = self.content_mut(id)
            && let ContentSpec::FolderPortal { navigate, .. } = &mut f.content
            && *navigate != on
        {
            *navigate = on;
            self.mark_dirty();
        }
    }

    pub fn set_hide_title_icon(&mut self, id: FenceId, on: bool) {
        if let Some(f) = self.content_mut(id)
            && let ContentSpec::FolderPortal {
                hide_title_icon, ..
            } = &mut f.content
            && *hide_title_icon != on
        {
            *hide_title_icon = on;
            self.mark_dirty();
        }
    }

    /// Title shown for a fence: a navigated portal shows the current folder's name.
    pub fn display_title(&self, f: &Fence) -> String {
        let title = match self.portal_cwd.get(&f.id) {
            Some(cwd) => cwd
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| f.title.clone()),
            None => f.title.clone(),
        };
        match self.portals.snapshot(f.id).map(|s| &s.health) {
            Some(PortalHealth::Loading) => {
                format!("{title} · {}", pecofence_core::i18n::text("正在读取文件夹"))
            }
            Some(PortalHealth::Stale(_)) => format!(
                "{title} · {}",
                pecofence_core::i18n::text("文件夹不可用，内容可能已过期")
            ),
            _ => title,
        }
    }

    /// Creates a folder-portal fence (plan §12 "文件夹门户", pulled into the MVP on request): a
    /// fence whose items are the folder's entries. Files are never moved.
    pub fn new_portal_fence(&mut self, folder: &Path, rect: RECT) -> Option<FenceId> {
        let title = folder
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| folder.to_string_lossy().to_string());
        let id = self.new_fence(&title, rect)?;
        if let Some(f) = self.content_mut(id) {
            f.content = ContentSpec::FolderPortal {
                root: folder.to_string_lossy().to_string(),
                recursive: false,
                filter: None,
                navigate: true,
                hide_title_icon: false,
            };
            f.view.sort = SortMode::Name;
        }
        self.reconcile_portal_sources();
        Some(id)
    }

    /// Forgets only this portal's projection. Item ids include the fence identity.
    pub(super) fn evict_portal_members(&mut self, id: FenceId) {
        let old_ids = self.portal_members.remove(&id).unwrap_or_default();
        for i in &old_ids {
            self.portal_items.remove(i);
        }
    }

    pub(super) fn reset_portal_runtime(&mut self) {
        self.portals.reset();
        self.portal_sources.clear();
        self.portal_items.clear();
        self.portal_members.clear();
        self.portal_cwd.clear();
        self.portal_layout = self.layout;
    }

    /// Reconcile activations before issuing requests or accepting results. No filesystem IO.
    pub(crate) fn reconcile_portal_sources(&mut self) -> Vec<FenceId> {
        if self.portal_layout != self.layout {
            self.reset_portal_runtime();
        }
        let sources: HashMap<_, _> = self
            .fences()
            .iter()
            .filter(|f| self.portal_root(f.id).is_some())
            .map(|f| (f.id, f.source.clone()))
            .collect();
        let gone: Vec<_> = self
            .portal_sources
            .keys()
            .filter(|id| !sources.contains_key(id))
            .copied()
            .collect();
        for id in gone {
            self.portals.remove(id);
            self.evict_portal_members(id);
            self.portal_cwd.remove(&id);
        }
        let mut changed = Vec::new();
        for (id, source) in &sources {
            if self.portal_sources.get(id) != Some(source) {
                self.portals.remove(*id);
                self.evict_portal_members(*id);
                self.portal_cwd.remove(id);
                if let Some(path) = self.portal_path(*id) {
                    self.portals.request(*id, path);
                    changed.push(*id);
                }
            }
        }
        self.portal_sources = sources;
        changed
    }

    pub(crate) fn request_portal_read(&mut self, id: FenceId) {
        self.reconcile_portal_sources();
        if let Some(path) = self.portal_path(id) {
            if self
                .portals
                .snapshot(id)
                .is_some_and(|s| s.request.path != path)
            {
                self.evict_portal_members(id);
            }
            self.portals.request(id, path);
        }
    }

    pub(crate) fn next_portal_read(&mut self) -> Option<PortalRead> {
        self.reconcile_portal_sources();
        self.portals.next_read()
    }

    /// Atomic snapshot commit followed by legacy Item conversion at the native boundary.
    /// Neither runtime reads nor health changes mark durable configuration dirty.
    pub(crate) fn accept_portal_result(&mut self, result: PortalResult) -> Option<FenceId> {
        self.reconcile_portal_sources();
        let complete = matches!(result.outcome, PortalOutcome::Complete(_));
        let id = self.portals.complete(result)?;
        if complete {
            let items: Vec<_> = self
                .portals
                .snapshot(id)?
                .entries
                .iter()
                .map(|entry| {
                    let path_str = entry.path.to_string_lossy();
                    let (icon_key, _) =
                        crate::icons::icon_key_for(&entry.path, entry.is_folder, entry.mtime);
                    Item {
                        id: portal_item_id(id, &path_str),
                        key: ItemKey::from_path(&path_str),
                        origin: Origin::Namespace,
                        display_name: entry.display_name.clone(),
                        file_id: None,
                        mtime: entry.mtime,
                        is_folder: entry.is_folder,
                        attrs: entry.attributes,
                        icon_key,
                        orphaned_since: None,
                        size: entry.size,
                        open_count: 0,
                        last_opened: None,
                    }
                })
                .collect();
            self.evict_portal_members(id);
            self.portal_members
                .insert(id, items.iter().map(|it| it.id).collect());
            self.portal_items
                .extend(items.into_iter().map(|it| (it.id, it)));
        }
        Some(id)
    }
}

/// Stable per-portal item identity: shared paths never overwrite another source's snapshot.
pub(crate) fn portal_item_id(fence: FenceId, path: &str) -> ItemId {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let key = path.to_lowercase().replace('/', "\\");
    let mut h1 = DefaultHasher::new();
    (fence, &key).hash(&mut h1);
    let mut h2 = DefaultHasher::new();
    (&key, 0x5eedu16).hash(&mut h2);
    uuid::Uuid::from_u128(((h1.finish() as u128) << 64) | h2.finish() as u128)
}

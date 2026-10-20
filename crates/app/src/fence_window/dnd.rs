//! OLE drop target: internal-drag thread-local, drop-spot resolution and highlight feedback, drag dwell, in-place drag image, right-button drop menu, effect / transfer mapping, `FenceDropHandler`.

use super::*;

pub(super) fn drag_image_fits(w_dip: f32, h_dip: f32) -> bool {
    w_dip <= DRAG_IMAGE_MAX_DIP && h_dip <= DRAG_IMAGE_MAX_DIP
}

/// The items an OLE drag started from one of our fences carries (same-thread modal loop, so a
/// thread-local beside the shell data object is enough — no private clipboard format).
#[derive(Clone, Debug)]
pub(super) struct InternalDrag {
    pub(super) from: ContentId,
    pub(super) items: Vec<ItemId>,
    pub(super) paths: Vec<PathBuf>,
}

thread_local! {
    pub(super) static INTERNAL_DRAG: RefCell<Option<InternalDrag>> = const { RefCell::new(None) };
}

/// What a drag would hit at a screen point inside this window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct DropSpot {
    /// A tab header (targets that tab).
    pub(super) tab: Option<usize>,
    /// A folder item (files go into the folder).
    pub(super) folder: Option<usize>,
    /// A program, or a shortcut to one: dropped files open with it (Explorer's semantics).
    pub(super) launch: Option<usize>,
    /// Insertion index for reordering an internal drag inside its own 手动 fence.
    pub(super) insert: Option<usize>,
    /// An internal drag over its own fence — its own tab pill, or a content area that cannot
    /// reorder (sorted / portal): no-op, "not allowed" cursor, no highlight.
    pub(super) self_drop: bool,
}

/// Identity of a resolved drop across data-object callbacks and modal menus.
/// An insertion slot also depends on the entire stable order, not just its index.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DropCommitIdentity {
    active: ContentId,
    tab: Option<ContentId>,
    folder: Option<(ItemId, PathBuf)>,
    launch: Option<(ItemId, PathBuf)>,
    insertion: Option<(usize, Vec<ItemId>)>,
    self_drop: bool,
}

/// Report an accepted effect only together with its queued action, after any
/// modal/native callback has finished and the original target is still current.
fn commit_drop(
    pending: Option<Command>,
    effect: DropEffect,
    expected: Option<&DropCommitIdentity>,
    current: impl FnOnce() -> Option<DropCommitIdentity>,
    queue: &CommandQueue,
) -> DropEffect {
    let Some(command) = pending else {
        return DropEffect::None;
    };
    if effect == DropEffect::None || expected.is_none() || current().as_ref() != expected {
        return DropEffect::None;
    }
    queue.push(command);
    effect
}

/// What the data object being dragged over us carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DropKind {
    None,
    /// `CF_HDROP`: files from Explorer, another program, or one of our own fences.
    Files,
    /// A browser link (`UniformResourceLocatorW`): becomes an Internet Shortcut.
    Url,
}

/// The effect to report for a file drag: Explorer's modifier table (Ctrl copy, Shift move, Alt
/// or Ctrl+Shift link) clipped to what the source allows, so a COPY|LINK-only source (a zip
/// folder, a browser's download shelf) gets Copy for a plain drag instead of a Move it never
/// offered.
pub(super) fn effect_for(key_state: u32, allowed: u32) -> DropEffect {
    dragdrop::resolve(dragdrop::modifier_effect(key_state), allowed)
}

/// The effect for a drop into `folder`: the modifiers decide, except over the Recycle Bin
/// item, which only ever moves (recycles), as Explorer's own bin does.
pub(super) fn folder_effect(folder: Option<&Path>, key_state: u32, allowed: u32) -> DropEffect {
    if folder.is_some_and(pecofence_platform::shell::is_recycle_bin_path) {
        dragdrop::resolve(DropEffect::Move, allowed)
    } else {
        effect_for(key_state, allowed)
    }
}

/// Extensions the shell's executable drop handler starts with the dropped files as arguments.
const LAUNCH_EXTENSIONS: [&str; 4] = ["exe", "com", "bat", "cmd"];

/// A file-system path (not a shell namespace or URL target) to a program.
fn is_program_path(path: &str) -> bool {
    if path.starts_with("::") || path.contains("://") {
        return false;
    }
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| LAUNCH_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Whether files dropped on an item open with it, as on the Windows desktop: a program, or a
/// `.lnk` whose stored target is one. A shortcut to a folder never qualifies (Explorer would
/// move the files into its target) and neither does an Internet Shortcut. The target is not
/// checked for existence: the shell's link tracking may still resolve it, and if not the shell
/// declines the drop. `shortcut_target` reads the raw stored target (environment variables
/// unexpanded, which leaves the extension intact).
pub(super) fn opens_dropped_files(
    path: &Path,
    is_folder: bool,
    shortcut_target: impl FnOnce() -> Option<String>,
) -> bool {
    if is_folder || pecofence_platform::shell::is_namespace_path(path) {
        return false;
    }
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
    {
        return shortcut_target().is_some_and(|t| is_program_path(&t));
    }
    is_program_path(&path.to_string_lossy())
}

thread_local! {
    /// `opens_dropped_files` by (path, mtime): DragOver runs at mouse rate and a `.lnk` has to
    /// be loaded to know its target. A rewritten shortcut gets a new mtime.
    static LAUNCH_TARGETS: RefCell<HashMap<(PathBuf, i64), bool>> = RefCell::new(HashMap::new());
}

fn cached_opens_dropped_files(it: &ItemView) -> bool {
    const LIMIT: usize = 512;
    let key = (it.path.clone(), it.mtime);
    if let Some(hit) = LAUNCH_TARGETS.with(|c| c.borrow().get(&key).copied()) {
        return hit;
    }
    let value = opens_dropped_files(&it.path, it.is_folder, || {
        pecofence_platform::shell::shortcut_target(&it.path)
    });
    LAUNCH_TARGETS.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= LIMIT {
            c.clear();
        }
        c.insert(key, value);
    });
    value
}

/// The effect to report over a program target: it opens the files and never takes them, so
/// Copy (or Link, when that is all the source offers) — never a Move the source would follow
/// by deleting its originals.
pub(super) fn launch_effect(allowed: u32) -> DropEffect {
    dragdrop::resolve(DropEffect::Copy, allowed & !DropEffect::Move.to_raw())
}

/// The drag-image badge over a program target: "用 <程序> 打开" (Explorer's "Open with").
pub(super) fn launch_description(
    effect: DropEffect,
    name: String,
) -> (DropImage, &'static str, String) {
    match effect {
        DropEffect::None => (DropImage::Invalid, "", String::new()),
        DropEffect::Link => (
            DropImage::Link,
            pecofence_core::i18n::text("用 %1 打开"),
            name,
        ),
        _ => (
            DropImage::Copy,
            pecofence_core::i18n::text("用 %1 打开"),
            name,
        ),
    }
}

pub(super) fn transfer_mode(effect: DropEffect) -> Option<TransferMode> {
    match effect {
        DropEffect::None => None,
        DropEffect::Copy => Some(TransferMode::Copy),
        DropEffect::Move => Some(TransferMode::Move),
        DropEffect::Link => Some(TransferMode::Link),
    }
}

/// The effects a right-button drag can pick from, in Explorer's menu order, limited to what
/// the source allows and this target performs (`candidates`).
pub(super) fn right_drag_choices(candidates: &[DropEffect], allowed: u32) -> Vec<DropEffect> {
    [DropEffect::Move, DropEffect::Copy, DropEffect::Link]
        .into_iter()
        .filter(|e| candidates.contains(e) && e.to_raw() & allowed != 0)
        .collect()
}

pub(super) const CMD_RDRAG_MOVE: u32 = 1;
pub(super) const CMD_RDRAG_COPY: u32 = 2;
pub(super) const CMD_RDRAG_LINK: u32 = 3;
pub(super) const CMD_RDRAG_CANCEL: u32 = 4;

/// Explorer's right-button-drag menu at the drop point (移动到当前位置 / 复制到当前位置 /
/// 在当前位置创建快捷方式 / 取消; the effect the modifiers would have produced is the bold
/// default). Runs a menu loop **with no borrow held**; returns the chosen effect, `None` for
/// 取消 / Esc / a click elsewhere.
pub(super) fn right_drag_menu(
    owner: HWND,
    pt: DragPoint,
    default: DropEffect,
    choices: &[DropEffect],
) -> DropEffect {
    if choices.is_empty() {
        return DropEffect::None;
    }
    let menu = PopupMenu::new();
    for e in choices {
        match e {
            DropEffect::Move => menu.item(
                CMD_RDRAG_MOVE,
                pecofence_core::i18n::text("移动到当前位置(&M)"),
                false,
                false,
            ),
            DropEffect::Copy => menu.item(
                CMD_RDRAG_COPY,
                pecofence_core::i18n::text("复制到当前位置(&C)"),
                false,
                false,
            ),
            DropEffect::Link => menu.item(
                CMD_RDRAG_LINK,
                pecofence_core::i18n::text("在当前位置创建快捷方式(&S)"),
                false,
                false,
            ),
            DropEffect::None => &menu,
        };
    }
    menu.separator();
    menu.item(
        CMD_RDRAG_CANCEL,
        pecofence_core::i18n::text("取消"),
        false,
        false,
    );
    let default = if choices.contains(&default) {
        default
    } else {
        choices[0]
    };
    match default {
        DropEffect::Move => menu.default_item(CMD_RDRAG_MOVE),
        DropEffect::Copy => menu.default_item(CMD_RDRAG_COPY),
        DropEffect::Link => menu.default_item(CMD_RDRAG_LINK),
        DropEffect::None => &menu,
    };
    match menu.show_classic_context(owner, pt.x, pt.y) {
        CMD_RDRAG_MOVE => DropEffect::Move,
        CMD_RDRAG_COPY => DropEffect::Copy,
        CMD_RDRAG_LINK => DropEffect::Link,
        _ => DropEffect::None,
    }
}

pub(super) const CMD_RDRAG_OPEN: u32 = 5;

/// The right-button-drag menu over a program: 用 <程序> 打开 (bold default) / 取消. Runs a menu
/// loop **with no borrow held**; true when the user chose to open.
pub(super) fn open_with_menu(owner: HWND, pt: DragPoint, name: &str) -> bool {
    let menu = PopupMenu::new();
    menu.item(
        CMD_RDRAG_OPEN,
        &pecofence_core::i18n::format("用 {0} 打开(&O)", &[name.to_string()]),
        false,
        false,
    );
    menu.separator();
    menu.item(
        CMD_RDRAG_CANCEL,
        pecofence_core::i18n::text("取消"),
        false,
        false,
    );
    menu.default_item(CMD_RDRAG_OPEN);
    menu.show_classic_context(owner, pt.x, pt.y) == CMD_RDRAG_OPEN
}

/// Drops paths that already live in `folder`, are `folder` itself or contain it (a folder can
/// neither be moved into itself nor into one of its own subfolders). Namespace items (the
/// Recycle Bin, ...) are not files and are dropped as well.
pub fn filter_folder_paths(paths: Vec<PathBuf>, folder: &Path) -> Vec<PathBuf> {
    let folder_key = ItemKey::from_path(&folder.to_string_lossy());
    let Some(fk) = folder_key.as_path().map(str::to_string) else {
        return paths;
    };
    paths
        .into_iter()
        .filter(|p| !pecofence_platform::shell::is_namespace_path(p))
        .filter(|p| {
            let key = ItemKey::from_path(&p.to_string_lossy());
            let Some(k) = key.as_path() else {
                return true;
            };
            if k == fk || fk.starts_with(&format!("{k}\\")) {
                return false;
            }
            p.parent().is_none_or(|parent| {
                ItemKey::from_path(&parent.to_string_lossy()).as_path() != Some(fk.as_str())
            })
        })
        .collect()
}

/// The drag-image badge for a target, as Explorer's folder views describe themselves
/// (`DROPDESCRIPTION`): "移动到 <栅栏>", "复制到 <文件夹>", "在 <标签> 中创建链接". `None`
/// (not allowed) clears the description so the shell shows its default "no" badge.
pub(super) fn drop_description(
    effect: DropEffect,
    insert: String,
) -> (DropImage, &'static str, String) {
    match effect {
        DropEffect::Move => (
            DropImage::Move,
            pecofence_core::i18n::text("移动到 %1"),
            insert,
        ),
        DropEffect::Copy => (
            DropImage::Copy,
            pecofence_core::i18n::text("复制到 %1"),
            insert,
        ),
        DropEffect::Link => (
            DropImage::Link,
            pecofence_core::i18n::text("在 %1 中创建链接"),
            insert,
        ),
        DropEffect::None => (DropImage::Invalid, "", String::new()),
    }
}

/// What `feedback` resolved for one DragEnter / DragOver.
pub(super) struct Feedback {
    pub(super) effect: DropEffect,
    /// Badge for the drag image; None when the view was unavailable.
    pub(super) description: Option<(DropImage, &'static str, String)>,
    pub(super) hwnd: HWND,
}

/// This window's OLE drop target: files from Explorer / other programs and our own fences (the
/// latter via `INTERNAL_DRAG`), plus browser links. Highlights the exact target while hovering
/// — a tab pill, a folder item, the insertion caret of a 手动 fence, or the whole content — and
/// keeps the shell's drag image (with its "移动到 …" badge) informed through
/// `IDropTargetHelper`, as Explorer's folder views do.
pub(super) struct FenceDropHandler {
    pub(super) queue: CommandQueue,
    pub(super) view: Rc<RefCell<Option<FenceViewState>>>,
    pub(super) kind: DropKind,
    pub(super) behavior: Rc<Behavior>,
    /// The shell's drag-image helper (None when the shell refused to create it).
    pub(super) helper: Option<dragdrop::DropTargetHelper>,
    /// The data object of the drag in progress (DragEnter … DragLeave / Drop); the drop
    /// description is written onto it.
    pub(super) data: Option<IDataObject>,
    /// Last description written, so an unchanged badge costs no `SetData` per DragOver.
    pub(super) last_description: Option<(DropImage, &'static str, String)>,
    /// The drag is held with the right button (seen in DragEnter / DragOver, where the button
    /// is still down): the drop ends in Explorer's Move / Copy / Shortcut / Cancel menu.
    pub(super) right_button: bool,
    /// DragOver cadence of the hover in progress (diagnostics, logged at DragLeave / Drop).
    pub(super) over_stats: OverStats,
}

/// How smoothly the shell's drag image was fed while a drag hovered this window: OLE calls
/// DragOver at mouse-message rate and the image only moves when we answer, so a long gap or
/// a slow handler is what the user sees as the dragged icon stuttering.
#[derive(Default, Clone, Copy)]
pub(super) struct OverStats {
    pub(super) calls: u32,
    pub(super) last: Option<Instant>,
    pub(super) max_gap: Duration,
    pub(super) max_handler: Duration,
}

impl OverStats {
    fn begin(&mut self) -> Instant {
        let now = Instant::now();
        if let Some(last) = self.last {
            self.max_gap = self.max_gap.max(now.duration_since(last));
        }
        self.last = Some(now);
        self.calls += 1;
        now
    }

    fn end(&mut self, started: Instant) {
        self.max_handler = self.max_handler.max(started.elapsed());
    }

    /// Logs the hover's cadence (info when it stuttered) and resets.
    fn report(&mut self, what: &str) {
        let ms = |d: Duration| (d.as_secs_f32() * 1000.0 * 10.0).round() / 10.0;
        if self.calls > 0 {
            if self.max_gap > Duration::from_millis(100)
                || self.max_handler > Duration::from_millis(7)
            {
                tracing::info!(
                    what,
                    calls = self.calls,
                    max_gap_ms = ms(self.max_gap),
                    max_handler_ms = ms(self.max_handler),
                    "drag hover stuttered"
                );
            } else {
                tracing::debug!(
                    what,
                    calls = self.calls,
                    max_gap_ms = ms(self.max_gap),
                    max_handler_ms = ms(self.max_handler),
                    "drag hover cadence"
                );
            }
        }
        *self = Self::default();
    }
}

impl FenceDropHandler {
    pub(super) fn internal() -> Option<InternalDrag> {
        INTERNAL_DRAG.with(|d| d.borrow().clone())
    }

    /// The target a drop at `pt` would hit now, for re-checking it after callbacks that can
    /// pump messages; `None` when the window is hidden, retired or busy.
    fn current_identity(&self, pt: DragPoint, files: bool) -> Option<DropCommitIdentity> {
        let guard = self.view.try_borrow().ok()?;
        let v = guard.as_ref()?;
        (v.plugin_panel.is_none() && v.drop_input_available(pt)).then(|| {
            v.drop_commit_identity(v.drop_spot_with(pt.x, pt.y, Self::internal().as_ref(), files))
        })
    }

    /// Resolves the target under `pt`, updates the highlights and returns the effect to report
    /// (plus the badge text for it). `allowed` is the source's effect mask.
    pub(super) fn feedback(&self, pt: Option<DragPoint>, key_state: u32, allowed: u32) -> Feedback {
        let none = Feedback {
            effect: DropEffect::None,
            description: None,
            hwnd: HWND(std::ptr::null_mut()),
        };
        let Ok(mut guard) = self.view.try_borrow_mut() else {
            return none;
        };
        let Some(v) = guard.as_mut() else {
            return none;
        };
        if v.plugin_panel.is_some() || pt.is_some_and(|pt| !v.drop_input_available(pt)) {
            v.drag_scroll = None;
            v.apply_drop_feedback(None);
            return none;
        }
        let Some(pt) = pt else {
            v.drag_scroll = None;
            if v.snap_scroll() {
                let _ = v.redraw_content();
            }
            v.apply_drop_feedback(None);
            if v.peeking {
                // Unrolled by the drag dwell: roll back once the drag has ended outside (the
                // close timer re-arms itself while the button is still down).
                window::set_timer(v.hwnd, TIMER_PEEK_CLOSE, PEEK_CLOSE_MS);
            }
            return Feedback {
                hwnd: v.hwnd,
                ..none
            };
        };
        let internal = Self::internal();
        v.drop_launch = self.kind == DropKind::Files;
        let spot = v.drop_spot_at(pt.x, pt.y, internal.as_ref());
        v.apply_drop_feedback(Some(spot));
        v.update_drag_dwell(self.behavior.hover_peek.get());
        v.update_drag_scroll(pt);
        if let Some(it) = spot.launch.and_then(|i| v.items.get(i)) {
            let effect = launch_effect(allowed);
            return Feedback {
                effect,
                description: Some(launch_description(effect, it.name.clone())),
                hwnd: v.hwnd,
            };
        }
        let effect = match self.kind {
            DropKind::None => DropEffect::None,
            // A browser usually offers COPY|LINK: Link when it may, else the best it allows.
            DropKind::Url => dragdrop::resolve(DropEffect::Link, allowed),
            DropKind::Files => {
                if spot.self_drop {
                    // Same-folder drag: Explorer shows "not allowed" too.
                    DropEffect::None
                } else if internal.is_some() && spot.folder.is_none() {
                    // Fence membership has no copy / link; only a folder target honours the
                    // modifiers (our own source offers every effect, so no clipping needed).
                    DropEffect::Move
                } else {
                    let folder = spot
                        .folder
                        .and_then(|i| v.items.get(i))
                        .map(|it| it.path.as_path());
                    folder_effect(folder, key_state, allowed)
                }
            }
        };
        // What the badge names: the tab pill, the folder, or the fence being shown.
        let insert = if let Some(t) = spot.tab.and_then(|i| v.tabs.get(i)) {
            t.title.clone()
        } else if let Some(it) = spot.folder.and_then(|i| v.items.get(i)) {
            it.name.clone()
        } else {
            v.tabs
                .iter()
                .find(|t| t.id == v.active)
                .map(|t| t.title.clone())
                .unwrap_or_else(|| v.title.clone())
        };
        Feedback {
            effect,
            description: Some(drop_description(effect, insert)),
            hwnd: v.hwnd,
        }
    }

    /// Writes the badge onto the data object when it changed (no borrow held: the source may
    /// live on this thread).
    pub(super) fn describe(&mut self, description: Option<(DropImage, &'static str, String)>) {
        let (Some(desc), Some(data)) = (description, self.data.as_ref()) else {
            return;
        };
        if self.last_description.as_ref() == Some(&desc) {
            return;
        }
        dragdrop::set_drop_description(data, desc.0, desc.1, &desc.2);
        self.last_description = Some(desc);
    }

    /// The drag is over (left or dropped): the badge is cleared so the source's next target
    /// starts from the default image, and the data object is released.
    pub(super) fn forget_data(&mut self, clear_description: bool) {
        if let Some(data) = self.data.take()
            && clear_description
            && self.last_description.is_some()
        {
            dragdrop::set_drop_description(&data, DropImage::Invalid, "", "");
        }
        self.last_description = None;
    }
}

impl DropHandler for FenceDropHandler {
    fn drag_enter(
        &mut self,
        data: &IDataObject,
        key_state: u32,
        pt: DragPoint,
        allowed: u32,
    ) -> DropEffect {
        self.kind = if self
            .view
            .borrow()
            .as_ref()
            .is_some_and(|v| v.plugin_panel.is_some())
        {
            DropKind::None
        } else if dragdrop::has_hdrop(data) {
            DropKind::Files
        } else if dragdrop::has_inet_url(data) {
            DropKind::Url
        } else {
            DropKind::None
        };
        self.data = Some(data.clone());
        self.last_description = None;
        self.right_button = key_state & dragdrop::MK_RBUTTON != 0;
        let fb = if self.kind == DropKind::None {
            // Nothing we take: no highlight, but the helper still tracks the image over us.
            let hwnd = self
                .view
                .try_borrow()
                .ok()
                .and_then(|g| g.as_ref().map(|v| v.hwnd))
                .unwrap_or(HWND(std::ptr::null_mut()));
            Feedback {
                effect: DropEffect::None,
                description: None,
                hwnd,
            }
        } else {
            self.feedback(Some(pt), key_state, allowed)
        };
        self.describe(fb.description);
        if let Some(h) = &self.helper
            && !fb.hwnd.0.is_null()
        {
            h.drag_enter(fb.hwnd, data, pt, fb.effect);
        }
        fb.effect
    }

    fn drag_over(&mut self, key_state: u32, pt: DragPoint, allowed: u32) -> DropEffect {
        let started = self.over_stats.begin();
        if self.kind == DropKind::None {
            if let Some(h) = &self.helper {
                h.drag_over(pt, DropEffect::None);
            }
            self.over_stats.end(started);
            return DropEffect::None;
        }
        self.right_button = key_state & dragdrop::MK_RBUTTON != 0;
        let fb = self.feedback(Some(pt), key_state, allowed);
        self.describe(fb.description);
        if let Some(h) = &self.helper {
            h.drag_over(pt, fb.effect);
        }
        self.over_stats.end(started);
        fb.effect
    }

    fn drag_leave(&mut self) {
        self.over_stats.report("leave");
        self.feedback(None, 0, 0);
        self.forget_data(true);
        if let Some(h) = &self.helper {
            h.drag_leave();
        }
    }

    fn drop(
        &mut self,
        data: &IDataObject,
        key_state: u32,
        pt: DragPoint,
        allowed: u32,
    ) -> DropEffect {
        self.over_stats.report("drop");
        let drop_started = Instant::now();
        let kind = std::mem::replace(&mut self.kind, DropKind::None);
        let files = kind == DropKind::Files;
        let internal = Self::internal();
        // Resolve the target and clear the highlights under one borrow.
        let resolved = self.view.try_borrow_mut().ok().and_then(|mut g| {
            g.as_mut()
                .filter(|v| v.drop_input_available(pt) && v.plugin_panel.is_none())
                .map(|v| {
                    let spot = v.drop_spot_with(pt.x, pt.y, internal.as_ref(), files);
                    let tab_fence = spot.tab.and_then(|i| v.tabs.get(i)).map(|t| t.id);
                    let folder = spot
                        .folder
                        .and_then(|i| v.items.get(i))
                        .map(|it| it.path.clone());
                    let launch = spot
                        .launch
                        .and_then(|i| v.items.get(i))
                        .map(|it| (it.id, it.path.clone(), it.name.clone()));
                    v.drag_scroll = None;
                    if v.snap_scroll() {
                        let _ = v.redraw_content();
                    }
                    v.apply_drop_feedback(None);
                    (
                        spot,
                        v.active,
                        tab_fence,
                        folder,
                        v.hwnd,
                        v.drop_commit_identity(spot),
                        launch,
                    )
                })
        });
        self.forget_data(false);
        // A right-button drag ends in Explorer's Move / Copy / Shortcut / Cancel menu instead
        // of the silent default. The menu loop runs with no borrow held, after the helper has
        // taken the drag image down (`image_dropped`).
        let right = std::mem::take(&mut self.right_button) || key_state & dragdrop::MK_RBUTTON != 0;
        let mut image_dropped = false;
        let mut drop_image = |effect: DropEffect| {
            if let Some(h) = &self.helper
                && !image_dropped
            {
                image_dropped = true;
                h.drop(data, pt, effect);
            }
        };
        let expected = resolved.as_ref().map(|value| value.5.clone());
        if let Some((_, _, _, _, hwnd, _, Some((item, target, name)))) = resolved.clone() {
            // Files dropped on a program open with it; they are never imported instead (the
            // gesture asked for "open", not for a move into this fence).
            let mut effect = launch_effect(allowed);
            // Starting the program can take a moment: the drag image goes down first.
            drop_image(effect);
            if effect != DropEffect::None && right && !open_with_menu(hwnd, pt, &name) {
                effect = DropEffect::None;
            }
            // The menu and the data object can pump messages: open only for the same target.
            if effect != DropEffect::None && self.current_identity(pt, files) != expected {
                effect = DropEffect::None;
            }
            if effect != DropEffect::None {
                effect = dragdrop::delegate_drop(&target, data, key_state, pt, allowed)
                    .unwrap_or_else(|e| {
                        tracing::warn!(program = %target.display(), error = %e, "drop on program failed");
                        DropEffect::None
                    });
                self.queue.push(Command::LaunchDrop {
                    item,
                    name,
                    opened: effect != DropEffect::None,
                });
            }
            tracing::info!(
                ?effect,
                ms = drop_started.elapsed().as_secs_f32() * 1000.0,
                "drop on program"
            );
            return effect;
        }
        let mut pending = None;
        let effect = 'resolve: {
            let Some((spot, active, tab_fence, folder, hwnd, _, _)) = resolved else {
                break 'resolve DropEffect::None;
            };
            let to = tab_fence.unwrap_or(active);
            match kind {
                DropKind::None => DropEffect::None,
                DropKind::Url => {
                    let Some(url) = dragdrop::inet_url(data) else {
                        break 'resolve DropEffect::None;
                    };
                    let mut effect = dragdrop::resolve(DropEffect::Link, allowed);
                    if effect == DropEffect::None {
                        break 'resolve DropEffect::None;
                    }
                    if right {
                        drop_image(effect);
                        effect = right_drag_menu(hwnd, pt, effect, &[effect]);
                        if effect == DropEffect::None {
                            break 'resolve DropEffect::None;
                        }
                    }
                    let name = dragdrop::file_group_descriptor_name(data);
                    pending = Some(Command::ExternalUrlDrop { url, name, to });
                    effect
                }
                DropKind::Files if internal.is_some() => {
                    let Some(d) = internal else {
                        break 'resolve DropEffect::None;
                    };
                    if spot.self_drop {
                        break 'resolve DropEffect::None;
                    }
                    if let Some(folder) = folder {
                        // Only a folder target honours the modifiers (our own source offers
                        // every effect, so `allowed` never clips them).
                        let paths = filter_folder_paths(d.paths, &folder);
                        let effect = folder_effect(Some(&folder), key_state, allowed);
                        let (Some(mode), false) = (transfer_mode(effect), paths.is_empty()) else {
                            break 'resolve DropEffect::None;
                        };
                        pending = Some(Command::DropIntoFolder {
                            paths,
                            folder,
                            fence: active,
                            mode,
                        });
                        break 'resolve effect;
                    }
                    if let Some(index) = spot.insert {
                        pending = Some(Command::ReorderItems {
                            fence: active,
                            items: d.items,
                            index,
                        });
                        break 'resolve DropEffect::Move;
                    }
                    if to == d.from {
                        break 'resolve DropEffect::None;
                    }
                    pending = Some(Command::MoveItems { items: d.items, to });
                    DropEffect::Move
                }
                DropKind::Files => {
                    let paths = dragdrop::hdrop_paths(data);
                    if paths.is_empty() {
                        break 'resolve DropEffect::None;
                    }
                    let mut effect = folder_effect(folder.as_deref(), key_state, allowed);
                    if effect == DropEffect::None {
                        // The source offers nothing we can do with files (LINK-only, say).
                        break 'resolve DropEffect::None;
                    }
                    let folder_paths = folder
                        .as_ref()
                        .map(|f| filter_folder_paths(paths.clone(), f));
                    if folder_paths.as_ref().is_some_and(|p| p.is_empty()) {
                        // Already in that folder / the folder itself: Explorer does nothing.
                        break 'resolve DropEffect::None;
                    }
                    if right {
                        drop_image(effect);
                        let candidates: &[DropEffect] = if folder
                            .as_deref()
                            .is_some_and(pecofence_platform::shell::is_recycle_bin_path)
                        {
                            &[DropEffect::Move]
                        } else {
                            &[DropEffect::Move, DropEffect::Copy, DropEffect::Link]
                        };
                        let choices = right_drag_choices(candidates, allowed);
                        effect = right_drag_menu(hwnd, pt, effect, &choices);
                    }
                    let Some(mode) = transfer_mode(effect) else {
                        break 'resolve DropEffect::None;
                    };
                    match (folder, folder_paths) {
                        (Some(folder), Some(paths)) => {
                            pending = Some(Command::DropIntoFolder {
                                paths,
                                folder,
                                fence: active,
                                mode,
                            })
                        }
                        _ => pending = Some(Command::ExternalDrop { paths, to, mode }),
                    }
                    effect
                }
            }
        };
        // IDataObject access and right-drag menus can pump native messages. Do
        // not send an action to a hidden/replaced target or report Move to the
        // source unless the action was actually accepted for that same target.
        let effect = commit_drop(
            pending,
            effect,
            expected.as_ref(),
            || self.current_identity(pt, files),
            &self.queue,
        );
        drop_image(effect);
        let spent = drop_started.elapsed();
        if spent > Duration::from_millis(8) {
            tracing::info!(
                ms = spent.as_secs_f32() * 1000.0,
                ?effect,
                "slow Drop handler"
            );
        }
        effect
    }
}

impl FenceViewState {
    fn drop_input_available(&self, pt: DragPoint) -> bool {
        if !self.input_lifecycle().window() {
            return false;
        }
        let r = window::window_rect(self.hwnd);
        pt.x >= r.left
            && pt.x < r.right
            && pt.y >= r.top
            && pt.y < r.bottom
            && (self.input_lifecycle().body() || pt.y - r.top < self.title_h_px())
    }

    fn drop_commit_identity(&self, spot: DropSpot) -> DropCommitIdentity {
        DropCommitIdentity {
            active: self.active,
            tab: spot.tab.and_then(|i| self.tabs.get(i)).map(|tab| tab.id),
            folder: spot
                .folder
                .and_then(|i| self.items.get(i))
                .map(|item| (item.id, item.path.clone())),
            launch: spot
                .launch
                .and_then(|i| self.items.get(i))
                .map(|item| (item.id, item.path.clone())),
            insertion: spot
                .insert
                .map(|index| (index, self.items.iter().map(|item| item.id).collect())),
            self_drop: spot.self_drop,
        }
    }

    /// Insertion index for reordering at a client-pixel point: only in a 手动-sorted virtual
    /// fence and only inside the item area.
    pub(super) fn insertion_at(&self, x_px: i32, y_px: i32) -> Option<usize> {
        if self.rolled_up || self.sort != SortMode::Manual || self.is_portal {
            return None;
        }
        let title_h = self.title_h_px();
        let (cw, ch) = self.content_size_px();
        if x_px < 0 || x_px >= cw || y_px < title_h || y_px >= title_h + ch {
            return None;
        }
        let scale = self.scale();
        let layout = self.layout(cw as f32 / scale);
        let x = x_px as f32 / scale;
        let y = (y_px - title_h) as f32 / scale - layout.fixed_top();
        if y < 0.0 {
            return None;
        }
        Some(layout.insertion_index(x, y + self.scroll_y, self.items.len()))
    }

    /// The insertion caret in visual content DIPs (`header_h` offsets rows below the header):
    /// at the live slot, or at the last slot while the caret fades out.
    pub(super) fn insert_caret_dip(&self, header_h: f32) -> Option<Rect> {
        let i = self.drop_insert.or(self.caret_shown)?;
        let scale = self.scale();
        let (cw, _) = self.content_size_px();
        let layout = self.layout(cw as f32 / scale);
        let c = layout.insertion_caret(i, self.items.len());
        Some(Rect::from_xywh(
            c.x,
            c.y - self.scroll_y + header_h,
            c.w,
            c.h,
        ))
    }

    /// A folder item under a client-pixel point that is not one of `exclude` (the items being
    /// dragged): Explorer moves dropped files into it. The Recycle Bin item counts too (a drop
    /// on it recycles).
    pub(super) fn folder_drop_target(
        &self,
        x_px: i32,
        y_px: i32,
        exclude: &[ItemId],
    ) -> Option<usize> {
        let i = self.hit_item(x_px, y_px)?;
        let it = self.items.get(i)?;
        let target = it.is_folder || pecofence_platform::shell::is_recycle_bin_path(&it.path);
        (target && !exclude.contains(&it.id)).then_some(i)
    }

    /// A program or a shortcut to one under a client-pixel point that is not one of `exclude`:
    /// files dropped on it open with it.
    pub(super) fn launch_drop_target(
        &self,
        x_px: i32,
        y_px: i32,
        exclude: &[ItemId],
    ) -> Option<usize> {
        let i = self.hit_item(x_px, y_px)?;
        let it = self.items.get(i)?;
        (!exclude.contains(&it.id) && cached_opens_dropped_files(it)).then_some(i)
    }

    /// Resolves what a drag at screen point (sx, sy) targets in this window.
    pub(super) fn drop_spot_at(
        &self,
        sx: i32,
        sy: i32,
        internal: Option<&InternalDrag>,
    ) -> DropSpot {
        self.drop_spot_with(sx, sy, internal, self.drop_launch)
    }

    /// `drop_spot_at` with programs as targets or not (`launch`: the drag carries files).
    pub(super) fn drop_spot_with(
        &self,
        sx: i32,
        sy: i32,
        internal: Option<&InternalDrag>,
        launch: bool,
    ) -> DropSpot {
        let r = window::window_rect(self.hwnd);
        let (cx, cy) = (sx - r.left, sy - r.top);
        let mut spot = DropSpot::default();
        if self.tabs.len() > 1 {
            spot.tab = self.tab_at(cx, cy);
            // Items dragged onto the pill of the fence they already belong to: Explorer shows
            // "not allowed" for a same-folder drop, and the drop would be a no-op anyway.
            if let (Some(i), Some(d)) = (spot.tab, internal)
                && self.tabs.get(i).map(|t| t.id) == Some(d.from)
            {
                spot.tab = None;
                spot.self_drop = true;
                return spot;
            }
        }
        if spot.tab.is_some() {
            return spot;
        }
        let exclude: &[ItemId] = internal.map(|d| d.items.as_slice()).unwrap_or(&[]);
        spot.folder = self.folder_drop_target(cx, cy, exclude);
        if spot.folder.is_some() {
            return spot;
        }
        // Items dragged within their own fence keep reordering over a program icon (a
        // 程序 fence is all shortcuts); from anywhere else the program opens them.
        if launch && internal.is_none_or(|d| d.from != self.active) {
            spot.launch = self.launch_drop_target(cx, cy, exclude);
            if spot.launch.is_some() {
                return spot;
            }
        }
        if let Some(d) = internal
            && d.from == self.active
        {
            match self.insertion_at(cx, cy) {
                Some(i) => spot.insert = Some(i),
                None => spot.self_drop = true,
            }
        }
        spot
    }

    /// Shows the highlights for a drag target (`None` = drag left / dropped).
    pub(super) fn apply_drop_feedback(&mut self, spot: Option<DropSpot>) {
        let (content_changed, chrome_changed) = self.set_drop_feedback(spot);
        if content_changed {
            let _ = self.redraw();
        } else if chrome_changed {
            let _ = self.redraw_chrome_only();
        }
    }

    /// Updates the drop highlight state without drawing; returns (content changed, chrome
    /// changed) for the caller's own redraw.
    pub(super) fn set_drop_feedback(&mut self, spot: Option<DropSpot>) -> (bool, bool) {
        let (tab, folder, insert, wash) = match spot {
            Some(s) => (
                s.tab,
                s.folder.or(s.launch),
                s.insert,
                s.tab.is_none()
                    && s.folder.is_none()
                    && s.launch.is_none()
                    && s.insert.is_none()
                    && !s.self_drop,
            ),
            None => {
                self.drop_launch = false;
                (None, None, None, false)
            }
        };
        let now = Instant::now();
        let chrome_changed = self.drop_tab != tab;
        let content_changed =
            self.drop_item != folder || self.drop_insert != insert || self.drop_hover != wash;
        self.drop_tab = tab;
        self.drop_item = folder;
        self.drop_insert = insert;
        if self.drop_hover != wash {
            // Fluent fade in (83 ms) / direct exit (167 ms) for the fence-wide wash + ring.
            self.drop_hover = wash;
            let (to, dur) = if wash {
                (1.0, motion::FASTER)
            } else {
                (0.0, motion::FAST)
            };
            self.drop_wash
                .retarget(to, self.anim_dur(dur), Curve::Linear, now);
        }
        // The caret's position snaps between slots (a Win32 insert mark jumps too); only its
        // alpha fades, and it keeps its last slot while fading out.
        if insert.is_some() {
            self.caret_shown = insert;
        }
        let caret_to = insert.is_some() as u8 as f32;
        if (self.caret_alpha.target() - caret_to).abs() > f32::EPSILON {
            self.caret_alpha
                .retarget(caret_to, self.anim_dur(motion::FASTER), Curve::Linear, now);
        }
        // Own selection greys out while a foreign drag hovers anything in the fence (Explorer's
        // view is inactive then): 83 ms both ways, like the wash / caret / tab fades, so the
        // one remaining drop-feedback element no longer pops. `tick` repaints the content while
        // it runs (a tab-head hover alone changes no content flag).
        let foreign =
            !self.ole_drag && (wash || folder.is_some() || insert.is_some() || tab.is_some());
        let inactive_to = foreign as u8 as f32;
        if (self.drop_inactive.target() - inactive_to).abs() > f32::EPSILON {
            self.drop_inactive.retarget(
                inactive_to,
                self.anim_dur(motion::FASTER),
                Curve::Linear,
                now,
            );
            if !self.selected.is_empty() {
                self.content_busy = true;
                self.frames.request();
            }
        }
        if spot.is_none() {
            self.end_drag_dwell();
        }
        (content_changed, chrome_changed)
    }

    /// Stops the drag dwell timer (drag left, dropped, or ended).
    pub(super) fn end_drag_dwell(&mut self) {
        if self.drag_peek_armed {
            self.drag_peek_armed = false;
            window::kill_timer(self.hwnd, TIMER_DRAG_PEEK);
        }
    }

    /// Drag dwell bookkeeping for a drag hovering this window (called from DragEnter / DragOver
    /// after the highlights were updated): arms the rolled-fence unroll once. A drag resting on
    /// a tab head deliberately does NOT switch tabs (Explorer's own strip does not either, and
    /// a drop onto the head already lands in that tab's page); a switch mid-drag would also run
    /// the content cross-fade and clear the selection while the OLE loop is active.
    pub(super) fn update_drag_dwell(&mut self, hover_peek: bool) {
        if self.rolled_up
            && !self.peeking
            && self.roll_anim.is_none()
            && hover_peek
            && !self.drag_peek_armed
        {
            // A rolled fence is one title-bar target: any position within it keeps the dwell.
            self.drag_peek_armed = true;
            window::set_timer(self.hwnd, TIMER_DRAG_PEEK, Tooltip::hover_delay_ms());
        }
    }

    /// Renders the selected items exactly as they are on screen into a bitmap for the shell's
    /// drag image (`DI_GETDRAGIMAGE`), with the cursor's offset into it, so the translucent copy
    /// lifts off from the icons' own places instead of popping in at the cursor (Explorer's
    /// DefView answers this way for a small selection footprint). None = let the shell use its
    /// default image — the thumbnail stack with the item-count badge — which Explorer also
    /// falls back to for more than `DRAG_IMAGE_MAX_ITEMS` items or a footprint wider / taller
    /// than `DRAG_IMAGE_MAX_DIP`, so the image never blankets the drop target.
    pub(super) fn render_drag_image(&self) -> Option<DragImage> {
        if self.rolled_up || self.selected.is_empty() {
            return None;
        }
        let scale = self.scale();
        let (cw, ch) = self.content_size_px();
        let (width_dip, height_dip) = (cw as f32 / scale, ch as f32 / scale);
        let layout = self.layout(width_dip);
        let fixed = layout.fixed_top();
        let mut idx: Vec<usize> = self
            .selected
            .iter()
            .copied()
            .filter(|i| *i < self.items.len())
            .collect();
        idx.sort_unstable();
        // Cells in visual content DIPs, clipped to the viewport like a ListView's client area.
        let scene = self.committed_file_scene()?;
        let cells: Vec<(usize, CellRect)> = idx
            .iter()
            .filter_map(|&i| {
                let node = scene.node(self.items[i].id)?;
                let bounds = node.geometry.bounds;
                (node.alpha > 0.0).then_some((
                    i,
                    CellRect {
                        x: bounds.x,
                        y: bounds.y,
                        w: bounds.w,
                        h: bounds.h,
                    },
                ))
            })
            .filter(|(_, r)| r.y + r.h > fixed && r.y < height_dip)
            .collect();
        if cells.len() > DRAG_IMAGE_MAX_ITEMS {
            return None;
        }
        let first = cells.first()?.1;
        let (mut l, mut t, mut r, mut b) = (first.x, first.y, first.x + first.w, first.y + first.h);
        for (_, c) in &cells {
            l = l.min(c.x);
            t = t.min(c.y);
            r = r.max(c.x + c.w);
            b = b.max(c.y + c.h);
        }
        let (l, t, r, b) = (
            l.max(0.0),
            t.max(fixed),
            r.min(width_dip),
            b.min(height_dip),
        );
        let (bw, bh) = (r - l, b - t);
        if bw <= 0.0 || bh <= 0.0 || !drag_image_fits(bw, bh) {
            return None;
        }
        let (w_px, h_px) = ((bw * scale).ceil() as u32, (bh * scale).ceil() as u32);
        if w_px == 0 || h_px == 0 || w_px > 4096 || h_px > 4096 {
            return None;
        }
        // The image is anchored at the press point, not the live cursor: DI_GETDRAGIMAGE
        // arrives after the pointer has moved past the drag threshold (and mouse messages
        // coalesce), so on a 22 DIP row the cursor is often already outside the footprint;
        // the press is always inside the grabbed cell. The shell clamps the offset into the
        // bitmap, so a press outside the footprint (a selection that scrolled away from the
        // grabbed item) would make the image jump to the cursor: fall back to the shell's.
        let win = window::window_rect(self.hwnd);
        let (px, py) = self.ole_drag_press;
        let (press_x, press_y) = (win.left + px, win.top + py);
        let origin_x = win.left + (l * scale).round() as i32;
        let origin_y = win.top + self.title_h_px() + (t * scale).round() as i32;
        if press_x < origin_x
            || press_x >= origin_x + w_px as i32
            || press_y < origin_y
            || press_y >= origin_y + h_px as i32
        {
            return None;
        }
        let rt = self.stack.create_render_target(w_px, h_px).ok()?;
        let theme = self.theme;
        let chrome = self.chrome.clone();
        // Fresh uploads for the one-off target; the shared cache belongs to the surfaces.
        let mut bitmaps = BitmapCache::new();
        let transform = Matrix3x2 {
            m11: scale,
            m12: 0.0,
            m21: 0.0,
            m22: scale,
            m31: 0.0,
            m32: 0.0,
        };
        let icon_px = (self.icon_dip() * scale).round() as u32;
        let keys: Vec<String> = {
            let icons = self.icons.borrow();
            cells
                .iter()
                .map(|(i, _)| icons.draw_key(&self.items[*i].icon_key, icon_px))
                .collect()
        };
        let drawn = match &layout {
            ItemLayout::Grid(grid) => {
                let items: Vec<ItemCell<'_>> = cells
                    .iter()
                    .zip(keys.iter())
                    .map(|((i, c), key)| {
                        let item = &self.items[*i];
                        ItemCell {
                            x: c.x - l,
                            y: c.y - t,
                            w: c.w,
                            h: c.h,
                            icon_key: key,
                            icon: item.icon.as_deref(),
                            label: item.label.as_deref().unwrap_or(&item.name),
                            hover: 0.0,
                            selection: 0.0,
                            pressed: false,
                            focused: false,
                            dimmed: false,
                            failed: item.icon_failed,
                            drop_target: 0.0,
                            alpha: 1.0,
                            icon_alpha: 1.0,
                            full_label: None,
                        }
                    })
                    .collect();
                let content = ContentDraw {
                    items: &items,
                    group_headers: &[],
                    icon_size: self.icon_size as f32,
                    label_lines: self.label_lines,
                    line_h: grid.metrics.line_h,
                    icon_top: grid.metrics.icon_top,
                    label_gap: grid.metrics.label_gap,
                    scrollbar: None,
                    empty_text: None,
                    drop_highlight: 0.0,
                    marquee: None,
                    insert_caret: None,
                    insert_caret_alpha: 0.0,
                    selection_inactive: 0.0,
                };
                rt.draw(|session| {
                    session.set_transform(&transform);
                    chrome.draw_content(session, &theme, scale, &mut bitmaps, bw, bh, &content)
                })
            }
            ItemLayout::Rows { metrics, .. } => {
                let cols = self.columns_for(width_dip);
                let rows: Vec<RowCell<'_>> = cells
                    .iter()
                    .zip(keys.iter())
                    .map(|((i, c), key)| {
                        let item = &self.items[*i];
                        RowCell {
                            y: c.y - t,
                            h: c.h,
                            icon_key: key,
                            icon: item.icon.as_deref(),
                            name: item.label.as_deref().unwrap_or(&item.name),
                            date: item.date_label.as_deref().unwrap_or(""),
                            type_name: item.type_label.as_deref().unwrap_or(""),
                            size: item.size_label.as_deref().unwrap_or(""),
                            hover: 0.0,
                            selection: 0.0,
                            pressed: false,
                            focused: false,
                            dimmed: false,
                            failed: item.icon_failed,
                            drop_target: 0.0,
                            alpha: 1.0,
                            icon_alpha: 1.0,
                        }
                    })
                    .collect();
                let draw = RowsDraw {
                    rows: &rows,
                    group_headers: &[],
                    columns: RowColumns {
                        name_x: cols.name_x - l,
                        name_w: cols.name_w,
                        date: cols.date.map(|(x, w)| (x - l, w)),
                        type_: cols.type_.map(|(x, w)| (x - l, w)),
                        size: cols.size.map(|(x, w)| (x - l, w)),
                    },
                    icon_size: metrics.icon,
                    header_h: 0.0,
                    sort_column: None,
                    sort_descending: false,
                    header_hover: &[],
                    header_pressed: None,
                    scrollbar: None,
                    empty_text: None,
                    drop_highlight: 0.0,
                    marquee: None,
                    insert_caret: None,
                    insert_caret_alpha: 0.0,
                    selection_inactive: 0.0,
                };
                rt.draw(|session| {
                    session.set_transform(&transform);
                    chrome.draw_rows(
                        session,
                        None,
                        &theme,
                        scale,
                        &mut bitmaps,
                        bw,
                        bh,
                        &draw,
                        None,
                    )
                })
            }
        };
        if let Err(e) = drawn {
            tracing::debug!(error = %e, "drag image render failed; shell default used");
            return None;
        }
        let bgra = rt.read_pixels().ok()?;
        // The shell places the image at cursor - offset: with the offset taken at the press
        // point the copy lands pixel-exactly over the source, displaced by whatever the pointer
        // has moved since (a ListView's LVN_BEGINDRAG anchor).
        Some(DragImage {
            width: w_px,
            height: h_px,
            offset: (press_x - origin_x, press_y - origin_y),
            bgra,
        })
    }
}

#[cfg(test)]
mod launch_tests {
    use super::*;

    fn opens(path: &str, is_folder: bool, target: Option<&str>) -> bool {
        opens_dropped_files(Path::new(path), is_folder, || target.map(str::to_string))
    }

    #[test]
    fn programs_and_shortcuts_to_programs_open_dropped_files() {
        assert!(opens(r"C:\Tools\app.exe", false, None));
        assert!(opens(r"C:\Tools\RUN.CMD", false, None));
        assert!(opens(
            r"C:\Users\u\Desktop\Notepad.lnk",
            false,
            Some(r"%windir%\system32\notepad.exe")
        ));
        assert!(opens(
            r"C:\Users\u\Desktop\Tool.LNK",
            false,
            Some(r"\\server\share\tool.exe")
        ));
    }

    #[test]
    fn documents_folders_and_namespace_targets_do_not() {
        assert!(!opens(r"C:\Docs\report.pdf", false, None));
        assert!(
            !opens(r"C:\Tools\app.exe", true, None),
            "a folder named *.exe"
        );
        assert!(!opens(
            r"C:\Desktop\Report.lnk",
            false,
            Some(r"C:\Docs\report.docx")
        ));
        assert!(!opens(
            r"C:\Desktop\Projects.lnk",
            false,
            Some(r"D:\Projects")
        ));
        assert!(!opens(r"C:\Desktop\Broken.lnk", false, None));
        assert!(!opens(
            r"C:\Desktop\Shell.lnk",
            false,
            Some("::{20D04FE0-3AEA-1069-A2D8-08002B30309D}")
        ));
        assert!(!opens(r"C:\Desktop\Site.url", false, None));
        assert!(!opens(
            r"C:\Desktop\Odd.lnk",
            false,
            Some("https://example.com/a.exe")
        ));
    }

    #[test]
    fn a_program_target_never_reports_move() {
        let (copy, link) = (DropEffect::Copy.to_raw(), DropEffect::Link.to_raw());
        assert_eq!(launch_effect(dragdrop::ALL_EFFECTS), DropEffect::Copy);
        assert_eq!(launch_effect(link), DropEffect::Link);
        assert_eq!(launch_effect(copy | link), DropEffect::Copy);
        assert_eq!(launch_effect(DropEffect::Move.to_raw()), DropEffect::None);
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn modal_drop_requires_the_same_live_target_before_queuing_or_reporting_move() {
        let queue = CommandQueue::new();
        let original = DropCommitIdentity {
            active: ContentId::from_u128(1),
            tab: None,
            folder: Some((ItemId::from_u128(2), PathBuf::from(r"C:\Folder"))),
            launch: None,
            insertion: None,
            self_drop: false,
        };
        let command = || {
            Some(Command::MoveItems {
                items: vec![ItemId::from_u128(10)],
                to: original.active,
            })
        };
        assert_eq!(
            commit_drop(
                command(),
                DropEffect::Move,
                Some(&original),
                || Some(original.clone()),
                &queue
            ),
            DropEffect::Move
        );
        assert!(
            matches!(queue.drain().as_slice(), [Command::MoveItems { to, .. }] if *to == original.active)
        );
        // None represents a hidden, retired, removed or inaccessible target after a native callback.
        assert_eq!(
            commit_drop(
                command(),
                DropEffect::Move,
                Some(&original),
                || None,
                &queue
            ),
            DropEffect::None
        );
        assert!(queue.drain().is_empty());
        let mut changed = original.clone();
        changed.folder.as_mut().unwrap().0 = ItemId::from_u128((1 << 96) | 2);
        assert_eq!(
            commit_drop(
                command(),
                DropEffect::Move,
                Some(&original),
                || Some(changed),
                &queue
            ),
            DropEffect::None
        );
        assert!(queue.drain().is_empty());
        let mut renamed = original.clone();
        renamed.folder.as_mut().unwrap().1 = PathBuf::from(r"C:\Renamed");
        assert_eq!(
            commit_drop(
                command(),
                DropEffect::Move,
                Some(&original),
                || Some(renamed),
                &queue
            ),
            DropEffect::None
        );
        assert!(queue.drain().is_empty());
        let mut insertion = original.clone();
        insertion.folder = None;
        insertion.insertion = Some((1, vec![ItemId::from_u128(4), ItemId::from_u128(5)]));
        let mut reordered = insertion.clone();
        reordered.insertion.as_mut().unwrap().1.reverse();
        assert_eq!(
            commit_drop(
                command(),
                DropEffect::Move,
                Some(&insertion),
                || Some(reordered),
                &queue
            ),
            DropEffect::None
        );
        assert!(
            queue.drain().is_empty(),
            "The same slot now has another meaning"
        );
        assert_eq!(
            commit_drop(
                None,
                DropEffect::Move,
                Some(&original),
                || Some(original.clone()),
                &queue
            ),
            DropEffect::None
        );
        assert!(queue.drain().is_empty(), "No Move without an action");
    }
}

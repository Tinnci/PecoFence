use super::*;
use crate::board::text::Heuristic;
use std::cell::{Cell, RefCell};
#[derive(Default)]
struct Calls {
    copied: RefCell<Vec<String>>,
    opened: Cell<u64>,
}

struct Mock(Rc<Calls>, Option<ThemeSnapshot>);

impl ScopeLease for Mock {
    fn id(&self) -> ScopeId {
        ScopeId(1)
    }
    fn generation(&self) -> u64 {
        1
    }
    fn is_open(&self) -> bool {
        true
    }
}
impl IpcService for Mock {
    fn subscribe(&self, _: &ScopeHandle, _: Query) -> Result<Token> {
        Ok(Token(1))
    }
    fn send(&self, _: &ScopeHandle, _: Arc<[u8]>) -> Result<Token> {
        Ok(Token(2))
    }
    fn cancel(&self, _: Token) -> Result<()> {
        Ok(())
    }
}
impl ClipboardService for Mock {
    fn write_text(&self, _: &ScopeHandle, text: String) -> Result<Token> {
        self.0.copied.borrow_mut().push(text);
        Ok(Token(99))
    }
}
impl NavigationService for Mock {
    fn open(&self, _: &ScopeHandle, _: ExternalTarget) -> Result<Token> {
        self.0.opened.set(self.0.opened.get() + 1);
        Ok(Token(99))
    }
}
impl RenderService for Mock {
    fn measure(&self, _: &TextSpec, _: f32) -> Result<TextMetrics> {
        Err(Error::Revoked)
    }
    fn invalidate(&self, _: MountKey, _: Option<RectDip>) -> Result<()> {
        Ok(())
    }
}
impl ThemeService for Mock {
    fn snapshot(&self) -> Result<Arc<ThemeSnapshot>> {
        self.1.clone().map(Arc::new).ok_or(Error::Revoked)
    }
}
impl DesktopService for Mock {
    fn request_mode(&self, _: InstanceKey, _: Presentation) -> Result<()> {
        Ok(())
    }
}
impl StorageService for Mock {
    fn read(&self, _: &ScopeHandle, _: &str) -> Result<Token> {
        Err(Error::Revoked)
    }
    fn compare_and_set(
        &self,
        _: &ScopeHandle,
        _: &str,
        _: Option<u64>,
        _: Arc<[u8]>,
    ) -> Result<Token> {
        Err(Error::Revoked)
    }
}

fn capability<S: ?Sized + 'static>(
    service: Box<S>,
    scope: &ScopeHandle,
    keep: &mut Vec<Rc<dyn std::any::Any>>,
) -> Capability<S> {
    let cell = Rc::new(ServiceCell::new(1, service));
    let capability = Capability::new(&cell, scope.clone());
    keep.push(cell);
    capability
}

fn harness(paper: PaperStyle, hc: bool) -> (Panel, Vec<Rc<dyn std::any::Any>>) {
    let mut theme = ThemeSnapshot::dark();
    theme.paper.style = paper;
    theme.high_contrast = hc;
    if hc {
        theme.surface_panel = [0.0, 0.0, 0.0, 1.0];
        theme.text_primary = [1.0; 4];
        theme.text_secondary = [1.0; 4];
        theme.accent = [1.0, 1.0, 0.0, 1.0];
    }
    let calls = Rc::new(Calls::default());
    let lease: Rc<dyn ScopeLease> = Rc::new(Mock(calls.clone(), None));
    let scope = ScopeHandle::from_lease(&lease);
    let mut keep: Vec<Rc<dyn std::any::Any>> = vec![Rc::new(lease)];
    let m = || Box::new(Mock(calls.clone(), Some(theme.clone())));
    let ctx = PluginContext {
        scope: scope.clone(),
        render: capability::<dyn RenderService>(m(), &scope, &mut keep),
        theme: capability::<dyn ThemeService>(m(), &scope, &mut keep),
        desktop: capability::<dyn DesktopService>(m(), &scope, &mut keep),
        ipc: capability::<dyn IpcService>(m(), &scope, &mut keep),
        storage: capability::<dyn StorageService>(m(), &scope, &mut keep),
        clipboard: None,
        navigation: Some(capability::<dyn NavigationService>(m(), &scope, &mut keep)),
    };
    let key = InstanceKey {
        id: 7,
        activation: 1,
    };
    let mut panel = Panel::new(ctx, key, serde_json::from_value(demo_config().1).unwrap()).unwrap();
    panel.mount = Some(MountKey {
        instance: key,
        generation: 1,
    });
    (panel, keep)
}
fn frame(panel: &mut Panel, w: f32, h: f32) -> Scene {
    panel
        .prepare(
            FrameInput {
                viewport: RectDip {
                    x: 0.0,
                    y: 0.0,
                    w,
                    h,
                },
                locale: "zh-CN",
                presentation: Presentation::Workspace,
                grouping: Grouping::Single,
                exposure: Exposure::Desktop,
                active: true,
                text_scale: 1.0,
                theme_epoch: 1,
                device_epoch: 1,
            },
            &Heuristic,
        )
        .unwrap()
}
#[test]
fn cicd_pipeline_release_and_watch_views() {
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    let scene = frame(&mut panel, 480.0, 744.0);
    let names: Vec<&str> = scene.nodes().iter().map(|n| n.name.as_str()).collect();
    // Each change names its six stages and a headline word, not colour alone.
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("18234 ") && n.contains("需修复"))
    );
    assert!(names.iter().any(|n| n.contains("验证 × #913, 失败")));
    assert!(names.iter().any(|n| n.contains("验证中")));
    assert!(
        names
            .iter()
            .any(|n| n.contains("已进版本") || n.contains("待进版本"))
    );
    assert!(
        names
            .iter()
            .any(|n| n.contains("verify #913 失败 · 上次成功 #912"))
    );
    assert!(names.iter().any(|n| n.contains("与基线冲突，需变基")));
    assert!(names.iter().any(|n| n.contains("#482 约 12 分")));
    assert!(names.iter().any(|n| n.contains("已合入")));
    // Release train: running daily, scheduled release and last finished release.
    assert!(
        names
            .iter()
            .any(|n| n.contains("orion/launcher-daily-userdebug #482") && n.contains("构建中"))
    );
    assert!(
        names
            .iter()
            .any(|n| n.contains("release/ir-1113-full") && n.contains("计划"))
    );
    assert!(
        names
            .iter()
            .any(|n| n.contains("release/ir-1113-full #57") && n.contains("成功"))
    );
    for hit in scene.hit_tree() {
        assert!(hit.rect.x >= 0.0 && hit.rect.x + hit.rect.w <= 480.0);
        assert!(hit.rect.y >= 0.0 && hit.rect.y + hit.rect.h <= 744.0);
        assert!(
            !scene
                .semantics()
                .iter()
                .find(|s| s.id == hit.id)
                .unwrap()
                .name
                .is_empty()
        );
    }
    panel.invoke(Target::View(View::Release)).unwrap();
    let scene = frame(&mut panel, 480.0, 744.0);
    assert!(
        scene
            .nodes()
            .iter()
            .any(|n| n.name.contains("发布列车") || n.name.contains("Release train"))
    );
    assert!(
        scene
            .nodes()
            .iter()
            .any(|n| n.name.contains("orion/launcher-daily-userdebug #482"))
    );
    assert!(
        scene
            .nodes()
            .iter()
            .any(|n| n.name.contains("待进入 1 个合入（你的 1 个）"))
    );
    panel.invoke(Target::View(View::Watchlist)).unwrap();
    let scene = frame(&mut panel, 480.0, 744.0);
    assert!(scene.nodes().iter().any(|n| n.role == "combobox"));
    assert!(
        scene
            .nodes()
            .iter()
            .any(|n| n.role == "switch" && n.name.contains("角色"))
    );
    assert!(
        scene
            .nodes()
            .iter()
            .any(|n| n.role == "switch" && n.name.contains("失败提醒"))
    );
}

#[test]
fn local_demo_toggles_roles_add_remove_and_keyboard() {
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    let id = panel.snapshot.as_ref().unwrap().jobs[0].id.clone();
    panel
        .invoke(Target::Toggle(id.clone(), CiWatchFlag::OnlyMine, true))
        .unwrap();
    assert!(panel.snapshot.as_ref().unwrap().jobs[0].only_mine);
    panel
        .invoke(Target::Role(id.clone(), CiJobRole::Verify))
        .unwrap();
    assert_eq!(
        panel.snapshot.as_ref().unwrap().jobs[0].role,
        CiJobRole::Verify
    );
    panel.invoke(Target::View(View::Watchlist)).unwrap();
    panel.query = "orion/car".into();
    panel.suggestions().unwrap();
    assert_eq!(panel.suggestions.items.len(), 3);
    panel.selected_suggestion = Some(0);
    panel.add().unwrap();
    assert_eq!(panel.snapshot.as_ref().unwrap().jobs.len(), 5);
    panel.invoke(Target::Remove(id)).unwrap();
    assert_eq!(panel.snapshot.as_ref().unwrap().jobs.len(), 4);
    frame(&mut panel, 480.0, 744.0);
    panel
        .handle(PanelEvent::Key {
            key: PanelKey::Down,
            shift: false,
        })
        .unwrap();
    assert!(panel.focus.is_some());
    panel
        .handle(PanelEvent::Key {
            key: PanelKey::Escape,
            shift: false,
        })
        .unwrap();
    assert_eq!(panel.view, View::Pipeline);
    panel
        .handle(PanelEvent::Key {
            key: PanelKey::Right,
            shift: false,
        })
        .unwrap();
    assert_eq!(panel.view, View::Release);
    panel
        .handle(PanelEvent::Key {
            key: PanelKey::Left,
            shift: false,
        })
        .unwrap();
    assert_eq!(panel.view, View::Pipeline);
    assert_eq!(panel.cadence(), Duration::from_secs(30));
    for b in &mut panel.snapshot.as_mut().unwrap().my_builds {
        b.state = CiBuildState::Success;
    }
    for j in &mut panel.snapshot.as_mut().unwrap().jobs {
        if let Some(b) = &mut j.last {
            b.state = CiBuildState::Success;
        }
    }
    for b in &mut panel.snapshot.as_mut().unwrap().branches {
        for c in &mut b.changes {
            if let Some(build) = &mut c.ci {
                build.state = CiBuildState::Success;
            }
        }
    }
    assert_eq!(panel.cadence(), Duration::from_secs(120));
}

#[test]
fn rejects_stale_frame_actions_and_retains_good_snapshot_on_bad_payload() {
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    let scene = frame(&mut panel, 480.0, 744.0);
    let action = scene
        .hit_tree()
        .iter()
        .find(|h| h.action == 17)
        .unwrap()
        .action;
    panel.changed();
    panel
        .handle(PanelEvent::Invoke {
            action,
            layout_revision: scene.identity().layout_revision,
        })
        .unwrap();
    assert_eq!(panel.view, View::Pipeline);
    let old = panel.snapshot.clone();
    assert!(panel.completion(b"invalid").is_err());
    assert_eq!(panel.snapshot, old);
}

#[test]
fn good_ink_contrast_and_system_high_contrast() {
    for style in [PaperStyle::Light, PaperStyle::Dark, PaperStyle::Soft] {
        let (mut panel, _keep) = harness(style, false);
        let scene = frame(&mut panel, 480.0, 744.0);
        assert!(paper::contrast(panel.palette.success, panel.palette.base) >= 4.5);
        assert!(paper::contrast(panel.palette.success, panel.palette.card) >= 4.5);
        assert!(
            scene
                .nodes()
                .iter()
                .all(|n| n.rect.w >= 0.0 && n.rect.h >= 0.0)
        );
    }
    let (mut panel, _keep) = harness(PaperStyle::Dark, true);
    let scene = frame(&mut panel, 480.0, 744.0);
    assert!(panel.palette.hc);
    assert!(
        scene
            .nodes()
            .iter()
            .filter(|n| n.action.is_some())
            .all(|n| n.style.stroke.is_some())
    );
}

#[test]
fn cicd_pixel_snapshots() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    for (name, style, hc, view) in [
        (
            "cicd-pipeline-light",
            PaperStyle::Light,
            false,
            View::Pipeline,
        ),
        (
            "cicd-pipeline-dark",
            PaperStyle::Dark,
            false,
            View::Pipeline,
        ),
        (
            "cicd-release-light",
            PaperStyle::Light,
            false,
            View::Release,
        ),
        (
            "cicd-watchlist-light",
            PaperStyle::Light,
            false,
            View::Watchlist,
        ),
        ("cicd-hc", PaperStyle::Dark, true, View::Pipeline),
        (
            "cicd-workspace-light",
            PaperStyle::Light,
            false,
            View::Workspace,
        ),
        (
            "cicd-workspace-dark",
            PaperStyle::Dark,
            false,
            View::Workspace,
        ),
        ("cicd-workspace-hc", PaperStyle::Dark, true, View::Workspace),
    ] {
        let (mut panel, _keep) = harness(style, hc);
        panel.view = view;
        if view == View::Watchlist {
            panel.query = "orion/car".into();
            panel.suggestions().unwrap();
        }
        let (width, height) = if view == View::Workspace {
            (1040, 1400)
        } else {
            (480, 744)
        };
        let scene = frame(&mut panel, width as f32, height as f32);
        let mut canvas = pecofence_render::HeadlessCanvas::default();
        scene.paint(&mut canvas).unwrap();
        let pixmap = pecofence_render::raster::rasterize(
            canvas.commands(),
            width,
            height,
            panel.palette.base,
        );
        pecofence_render::raster::assert_snapshot(&pixmap, &dir.join(format!("{name}.png")));
    }
}

#[test]
fn workspace_view_names_unavailable_medians_triage_and_actions() {
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    panel.view = View::Workspace;
    let scene = frame(&mut panel, 1040.0, 1400.0);
    let names: Vec<&str> = scene.nodes().iter().map(|n| n.name.as_str()).collect();
    let has = |needle: &str| names.iter().any(|n| n.contains(needle));
    // Medians the observations cannot produce are dashes with an accessible word.
    assert!(has("提交到合入 · 中位, 不可用: —"));
    assert!(has("合入到进版本 · 中位, 不可用: —"));
    // Scope switch and the four KPIs.
    assert!(has("我的") && has("团队"));
    assert!(has("验证通过 · 7 天: 6/7"));
    assert!(has("关注任务当前失败: 1 / 4"));
    // Stage cards name the change, its state word and whether it is the user's.
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("18234 ") && n.contains("你的变更"))
    );
    // Triage: no steps in the record, so a note instead of a list; the byte window; the comparison.
    assert!(has("构建记录未提供步骤，不列出步骤。"));
    assert!(has("字节 1 204 880–1 206 912 / 3 100 000"));
    assert!(has("#912 ✓ → #913 ×：只多了 18234 PS3"));
    assert!(has(
        "比较的是构建记录中的变更和提交，不包含环境与参数差异。"
    ));
    // Actions only open things: the log and Gerrit, no retry or cancel.
    assert!(has("打开完整日志"));
    assert!(has("在 Gerrit 中打开 18234"));
    let buttons: Vec<&str> = scene
        .nodes()
        .iter()
        .filter(|n| n.role == "button")
        .map(|n| n.name.as_str())
        .collect();
    assert!(
        !buttons
            .iter()
            .any(|n| n.contains("重试") || n.contains("取消"))
    );
    // Hit targets stay inside the viewport.
    for hit in scene.hit_tree() {
        assert!(hit.rect.x >= 0.0 && hit.rect.x + hit.rect.w <= 1040.0);
        assert!(hit.rect.y >= 0.0 && hit.rect.y + hit.rect.h <= 1400.0);
    }
}

fn empty_watchlist(panel: &mut Panel) {
    panel.snapshot = Some(CiWatchSnapshot {
        user: CiUser {
            jenkins_user: "enterp".into(),
            gerrit_user: "enterp".into(),
        },
        ..Default::default()
    });
}

#[test]
fn first_run_offers_seeds_and_follows_only_the_ticked_ones() {
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    empty_watchlist(&mut panel);
    panel.ensure_seeds().unwrap();
    assert!(panel.seeding());
    assert_eq!(panel.seeds.items.len(), 5);
    assert_eq!(panel.selected_seeds.len(), 5, "all seeds start ticked");

    panel.invoke(Target::Seed(2)).unwrap();
    assert_eq!(panel.selected_seeds.len(), 4);
    frame(&mut panel, 480.0, 780.0);
    let names: Vec<&str> = panel.actions.iter().map(|(k, _, _)| k.as_str()).collect();
    assert!(names.contains(&"seed:apply") && names.contains(&"seed:manual"));

    panel.invoke(Target::ApplySeeds).unwrap();
    let watched = panel.snapshot.as_ref().unwrap();
    assert_eq!(watched.jobs.len(), 2, "two jobs ticked");
    assert_eq!(watched.branches.len(), 2, "two branches ticked");
    assert!(!panel.seeding(), "a watched item ends the first run");
}

#[test]
fn manual_add_leaves_the_first_run_for_the_watchlist_editor() {
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    empty_watchlist(&mut panel);
    panel.ensure_seeds().unwrap();
    panel.invoke(Target::ManualAdd).unwrap();
    assert_eq!(panel.view, View::Watchlist);
    assert!(!panel.seeding());
    assert!(
        panel.snapshot.as_ref().unwrap().jobs.is_empty(),
        "nothing followed"
    );
}

#[test]
fn seeds_are_requested_once_per_empty_watchlist() {
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    empty_watchlist(&mut panel);
    panel.ensure_seeds().unwrap();
    panel.selected_seeds.clear();
    panel.ensure_seeds().unwrap();
    assert!(
        panel.selected_seeds.is_empty(),
        "a second call does not reseed"
    );
}

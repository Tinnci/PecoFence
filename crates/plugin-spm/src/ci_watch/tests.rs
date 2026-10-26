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
fn ci_geometry_views_and_accessible_status() {
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    let scene = frame(&mut panel, 480.0, 744.0);
    assert!(
        scene
            .nodes()
            .iter()
            .any(|n| n.role == "progressbar" && n.name.contains("launcher"))
    );
    assert!(scene.nodes().iter().any(|n| n.name.contains("#913")));
    assert!(scene.nodes().iter().any(|n| n.name.contains("14:33")));
    assert!(scene.nodes().iter().any(|n| n.text == "CR+1"));
    assert!(scene.nodes().iter().any(|n| n.name.contains("超出预计")));
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
    let bar = scene
        .nodes()
        .iter()
        .find(|n| n.name.starts_with("orion/launcher-daily-userdebug ·") && n.role == "progressbar")
        .unwrap();
    let fill = scene
        .nodes()
        .iter()
        .find(|n| {
            n.rect.y == bar.rect.y
                && n.rect.w < bar.rect.w
                && n.style.fill == Some(panel.palette.accent)
        })
        .unwrap();
    assert!((fill.rect.w / bar.rect.w - 21.0 / 33.0).abs() < 0.001);
    panel.invoke(Target::View(View::Watchlist)).unwrap();
    let scene = frame(&mut panel, 480.0, 744.0);
    assert!(scene.nodes().iter().any(|n| n.role == "combobox"));
    assert!(
        scene
            .nodes()
            .iter()
            .any(|n| n.role == "switch" && n.name.contains("失败提醒"))
    );
}
#[test]
fn local_demo_toggles_add_remove_and_keyboard() {
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    let id = panel.snapshot.as_ref().unwrap().jobs[0].id.clone();
    panel
        .invoke(Target::Toggle(id.clone(), CiWatchFlag::OnlyMine, true))
        .unwrap();
    assert!(panel.snapshot.as_ref().unwrap().jobs[0].only_mine);
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
    assert_eq!(panel.view, View::Builds);
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
    assert_eq!(panel.view, View::Builds);
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
fn ci_pixel_snapshots() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    for (name, style, hc, view) in [
        ("ci-builds-light", PaperStyle::Light, false, View::Builds),
        ("ci-builds-dark", PaperStyle::Dark, false, View::Builds),
        ("ci-watchlist", PaperStyle::Light, false, View::Watchlist),
        ("ci-hc", PaperStyle::Dark, true, View::Builds),
    ] {
        let (mut panel, _keep) = harness(style, hc);
        panel.view = view;
        if view == View::Watchlist {
            panel.query = "orion/car".into();
            panel.suggestions().unwrap();
        }
        let scene = frame(&mut panel, 480.0, 744.0);
        let mut canvas = pecofence_render::HeadlessCanvas::default();
        scene.paint(&mut canvas).unwrap();
        let pixmap =
            pecofence_render::raster::rasterize(canvas.commands(), 480, 744, panel.palette.base);
        pecofence_render::raster::assert_snapshot(&pixmap, &dir.join(format!("{name}.png")));
    }
}

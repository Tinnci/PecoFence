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

fn names(scene: &Scene) -> Vec<String> {
    scene.nodes().iter().map(|n| n.name.clone()).collect()
}
fn has(names: &[String], needle: &str) -> bool {
    names.iter().any(|n| n.contains(needle))
}
fn click(panel: &mut Panel, w: f32, h: f32, target: &Target) {
    frame(panel, w, h);
    let index = panel
        .actions
        .iter()
        .position(|(_, t, _)| t == target)
        .unwrap_or_else(|| panic!("no action {target:?}"));
    panel
        .handle(PanelEvent::Invoke {
            action: 16 + index as u64,
            layout_revision: panel.layout_revision,
        })
        .unwrap();
}

#[test]
fn demo_bundle_matches_the_contract() {
    let demo = demo_bundle();
    demo.query.validate().unwrap();
    demo.readiness.validate().unwrap();
    demo.what_if.validate().unwrap();
    demo.search.validate().unwrap();
    for item in &demo.query.items {
        demo.details[&item.id].validate().unwrap();
    }
    assert_eq!(demo.readiness.di.value, 14.5);
    let path: Vec<f64> = demo.what_if.steps.iter().map(|s| s.di_after).collect();
    assert_eq!(path, vec![11.5, 10.5, 9.5]);
}

#[test]
fn narrow_inbox_groups_items_by_queue_with_the_gate_on_top() {
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    let n = names(&frame(&mut panel, 480.0, 780.0));
    assert!(has(&n, "TR5 DI 14.5 · ≤ 10 未就绪"));
    assert!(has(&n, "需要我回应 2"));
    assert!(has(&n, "待确认分配"));
    assert!(has(&n, "阻塞 TR5"));
    // Each row names its type, id, flag word and title: status is never colour alone.
    assert!(has(
        &n,
        "AR R-102 Move the Search box to the bottom, ? 负责人未确认"
    ));
    assert!(has(&n, "DTS D-2291"));
    assert!(has(&n, "严重"));
    // R-102 needs a reply and a confirmation; it is listed once, in the first group.
    assert_eq!(n.iter().filter(|x| x.starts_with("AR R-102 ")).count(), 1);
}

#[test]
fn narrow_select_opens_the_issue_and_back_returns() {
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    click(&mut panel, 480.0, 780.0, &Target::Select("D-2291".into()));
    assert_eq!(panel.state.selected.as_deref(), Some("D-2291"));
    let n = names(&frame(&mut panel, 480.0, 780.0));
    assert!(has(&n, "返回我的问题"));
    assert!(n.iter().any(|x| x.starts_with("D-2291 ")));
    click(&mut panel, 480.0, 780.0, &Target::Back);
    assert!(!panel.state.narrow_detail);
}

#[test]
fn workspace_shows_gate_detail_proposal_messages_and_people() {
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    let n = names(&frame(&mut panel, 1440.0, 1200.0));
    // Phase strip: the current gate is named, not only filled.
    assert!(has(&n, "验证 TR5 当前"));
    assert!(has(&n, "开发 TR4 已过"));
    // Criteria carry value, threshold and a pass word.
    assert!(has(&n, "DI 不超过阈值 14.5 / 10 未通过"));
    // R-102 opens first: the Lark proposal with both standings unknown, and its actions.
    assert!(has(&n, "Lark 中的分配 Gale 待你确认"));
    for action in [
        "确认并更新 Meegle",
        "仅本地记录",
        "不是这样",
        "在 Lark 中回复",
    ] {
        assert!(n.iter().any(|x| x == action), "{action}");
    }
    assert!(has(&n, "Gale 10-26"));
    assert!(has(&n, "业务编号 · 回复链"));
    assert!(has(&n, "Jordan PDT 经理") || has(&n, "PDT 经理"));
    assert!(has(&n, "Kai 相关代码变更") || has(&n, "Kai "));
    assert!(has(&n, "排序分，不是置信度"));
    // The what-if path, step by step.
    assert!(has(&n, "D-2291 严重 −3 DI 11.5"));
    assert!(has(&n, "DI 9.5"));
    // The suggestion strip leads to search.
    assert!(has(&n, "建议 Kai 10-19 提到 Pad 横屏裁切"));
}

#[test]
fn proposal_verdicts_move_the_proposal_and_never_write_remotely() {
    let pid = "proposal:R-102:gale".to_string();
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    click(
        &mut panel,
        1440.0,
        1200.0,
        &Target::Proposal(pid.clone(), ProposalAction::ConfirmMeegle),
    );
    let d = panel.state.detail.as_ref().unwrap();
    assert_eq!(d.proposals[0].status, ProposalStatus::Confirmed);
    assert!(d.proposals[0].reviewed_at.is_some());
    // No Meegle window yet: the confirmation is local, and the panel says so.
    assert!(
        panel
            .state
            .notice
            .as_deref()
            .unwrap()
            .contains("Meegle 更新暂不可用")
    );
    let n = names(&frame(&mut panel, 1440.0, 1200.0));
    assert!(has(&n, "Lark 中的分配 Gale 已确认"));
    assert!(!n.iter().any(|x| x == "不是这样"));

    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    click(
        &mut panel,
        1440.0,
        1200.0,
        &Target::Proposal(pid.clone(), ProposalAction::LocalOnly),
    );
    assert_eq!(
        panel.state.detail.as_ref().unwrap().proposals[0].status,
        ProposalStatus::LocalOnly
    );

    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    click(
        &mut panel,
        1440.0,
        1200.0,
        &Target::Proposal(pid.clone(), ProposalAction::NotThis),
    );
    let d = panel.state.detail.as_ref().unwrap();
    assert!(d.proposals.is_empty());
    assert_eq!(d.proposals_rejected, 1);
    assert!(has(
        &names(&frame(&mut panel, 1440.0, 1200.0)),
        "已否认 1 条分配"
    ));

    // Reply in Lark only navigates; without a Lark origin it explains, and records nothing.
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    click(
        &mut panel,
        1440.0,
        1200.0,
        &Target::Proposal(pid, ProposalAction::ReplyInLark),
    );
    assert_eq!(
        panel.state.detail.as_ref().unwrap().proposals[0].status,
        ProposalStatus::Proposed
    );
    assert!(panel.state.notice.as_deref().unwrap().contains("Lark"));
}

#[test]
fn search_marks_channels_similarity_and_scope() {
    let (mut panel, _keep) = harness(PaperStyle::Light, false);
    panel.handle(PanelEvent::Command { id: 2 }).unwrap();
    assert_eq!(panel.state.view, View::Search);
    let n = names(&frame(&mut panel, 1440.0, 900.0));
    assert!(has(&n, "混合, selected"));
    assert!(has(&n, "语义相似度，不是置信度"));
    assert!(has(&n, "Kai 10-19 提到 Pad 横屏裁切"));
    // Judged suggestions have accept and dismiss; rule suggestions are shown, never judged.
    assert!(has(&n, "关联 Kai 10-19"));
    assert!(!has(&n, "关联 R-102 没有验收标准"));
    click(
        &mut panel,
        1440.0,
        900.0,
        &Target::Suggestion(0, SuggestionVerdict::Dismiss),
    );
    assert_eq!(
        panel.state.detail.as_ref().unwrap().suggestions_dismissed,
        1
    );
    assert!(has(
        &names(&frame(&mut panel, 1440.0, 900.0)),
        "已忽略 1 条不再出现"
    ));
    click(
        &mut panel,
        1440.0,
        900.0,
        &Target::Scope(SearchScope::Messages),
    );
    let kinds: Vec<SearchKind> = panel
        .state
        .search
        .as_ref()
        .unwrap()
        .results
        .iter()
        .map(|r| r.kind)
        .collect();
    assert!(!kinds.is_empty() && kinds.iter().all(|k| *k == SearchKind::Message));
    // A result that is an issue opens it.
    click(&mut panel, 1440.0, 900.0, &Target::Scope(SearchScope::All));
    click(&mut panel, 1440.0, 900.0, &Target::Select("D-2301".into()));
    assert_eq!(panel.state.view, View::Issues);
    assert_eq!(panel.state.selected.as_deref(), Some("D-2301"));
}

#[test]
fn chrome_names_the_gate_state_and_commands() {
    let (panel, _keep) = harness(PaperStyle::Light, false);
    let chrome = panel.chrome();
    assert_eq!(chrome.title, "问题");
    assert!(chrome.subtitle.contains("DI 14.5"));
    assert_eq!(chrome.status.unwrap().text, "× TR5 未就绪");
    assert_eq!(chrome.commands.len(), 2);
}

#[test]
fn issue_raster_snapshots() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    for (name, style, hc, view, (width, height)) in [
        (
            "issues-inbox-light",
            PaperStyle::Light,
            false,
            View::Issues,
            (480, 780),
        ),
        (
            "issues-workspace-light",
            PaperStyle::Light,
            false,
            View::Issues,
            (1440, 1100),
        ),
        (
            "issues-workspace-dark",
            PaperStyle::Dark,
            false,
            View::Issues,
            (1440, 1100),
        ),
        (
            "issues-workspace-hc",
            PaperStyle::Dark,
            true,
            View::Issues,
            (1440, 1100),
        ),
        (
            "issues-search-light",
            PaperStyle::Light,
            false,
            View::Search,
            (1440, 900),
        ),
    ] {
        let (mut panel, _keep) = harness(style, hc);
        if view == View::Search {
            panel.invoke(Target::View(View::Search)).unwrap();
        }
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

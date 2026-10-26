//! Board behavior and layout checks against the demo backend and fixtures.

use super::layout::{Mode, Target};
use super::model::{self, DisplayColumn, Signal};
use super::text::{Heuristic, Lang};
use super::*;
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

const KEY: InstanceKey = InstanceKey {
    id: 7,
    activation: 1,
};

struct Harness {
    panel: BoardPanel,
    calls: Rc<Calls>,
    now: Instant,
    _keep: Vec<Rc<dyn std::any::Any>>,
}

fn light() -> ThemeSnapshot {
    let mut theme = ThemeSnapshot::dark();
    theme.text_primary = [0.0, 0.0, 0.0, 0.894];
    theme.text_secondary = [0.0, 0.0, 0.0, 0.62];
    theme.surface_panel = [0.957, 0.969, 0.973, 0.95];
    theme.surface_subtle = [0.0, 0.0, 0.0, 0.035];
    theme.stroke = [0.137, 0.259, 0.325, 0.1];
    theme.accent = [0.0, 0.373, 0.722, 1.0];
    theme.danger = [0.863, 0.148, 0.148, 1.0];
    theme.warning = [0.918, 0.345, 0.043, 1.0];
    theme.success = [0.086, 0.639, 0.29, 1.0];
    theme
}

fn high_contrast() -> ThemeSnapshot {
    let mut theme = ThemeSnapshot::dark();
    theme.high_contrast = true;
    let fg = [1.0, 1.0, 1.0, 1.0];
    theme.surface_panel = [0.0, 0.0, 0.0, 1.0];
    for c in [
        &mut theme.text_primary,
        &mut theme.text_secondary,
        &mut theme.stroke,
        &mut theme.accent,
        &mut theme.danger,
        &mut theme.warning,
        &mut theme.success,
    ] {
        *c = fg;
    }
    theme
}

fn harness_with(theme: Option<ThemeSnapshot>) -> Harness {
    let calls = Rc::new(Calls::default());
    let lease: Rc<dyn ScopeLease> = Rc::new(Mock(calls.clone(), None));
    let scope = ScopeHandle::from_lease(&lease);
    let mut keep: Vec<Rc<dyn std::any::Any>> = vec![Rc::new(lease)];
    let m = || Box::new(Mock(calls.clone(), theme.clone()));
    let ctx = PluginContext {
        scope: scope.clone(),
        render: capability::<dyn RenderService>(m(), &scope, &mut keep),
        theme: capability::<dyn ThemeService>(m(), &scope, &mut keep),
        desktop: capability::<dyn DesktopService>(m(), &scope, &mut keep),
        ipc: capability::<dyn IpcService>(m(), &scope, &mut keep),
        storage: capability::<dyn StorageService>(m(), &scope, &mut keep),
        clipboard: Some(capability::<dyn ClipboardService>(m(), &scope, &mut keep)),
        navigation: Some(capability::<dyn NavigationService>(m(), &scope, &mut keep)),
    };
    let (_, config) = demo_config();
    let config: BoardConfig = serde_json::from_value(config).unwrap();
    let now = Instant::now();
    let mut panel = BoardPanel::new(ctx, KEY, config, now).unwrap();
    panel
        .mount(MountContext {
            key: MountKey {
                instance: KEY,
                generation: 1,
            },
            scope,
            viewport: RectDip::default(),
            dpi: 96,
        })
        .unwrap();
    Harness {
        panel,
        calls,
        now,
        _keep: keep,
    }
}

fn harness() -> Harness {
    let mut h = harness_with(None);
    h.settle();
    h
}

fn input(w: f32, h: f32, text_scale: f32) -> FrameInput {
    FrameInput {
        viewport: RectDip {
            x: 0.0,
            y: 0.0,
            w,
            h,
        },
        locale: "en",
        presentation: Presentation::Workspace,
        grouping: Grouping::Single,
        exposure: Exposure::Desktop,
        active: true,
        text_scale,
        theme_epoch: 1,
        device_epoch: 1,
    }
}

impl Harness {
    /// Deliver every demo event that is due within the next few seconds.
    fn settle(&mut self) {
        for _ in 0..8 {
            self.now += Duration::from_millis(500);
            self.panel.handle(PanelEvent::Wake, self.now).unwrap();
        }
    }

    fn frame(&mut self, w: f32, h: f32) -> Scene {
        self.panel.prepare(input(w, h, 1.0), &Heuristic).unwrap()
    }

    fn click(&mut self, target: &Target) {
        // Header commands live in the host's title row; press them there, by label.
        let chrome_label = match target {
            Target::Refresh => Some("Refresh"),
            Target::CopySummary => Some("Copy summary"),
            Target::RestartDemo => Some("Restart demo"),
            _ => None,
        };
        if let Some(label) = chrome_label {
            let command = self
                .panel
                .chrome()
                .commands
                .into_iter()
                .find(|c| c.label == label)
                .unwrap_or_else(|| panic!("no chrome command {label}"));
            assert!(command.enabled, "{label} is disabled");
            self.panel
                .handle(PanelEvent::Command { id: command.id }, self.now)
                .unwrap();
            return;
        }
        let action = self
            .panel
            .action_for(target)
            .unwrap_or_else(|| panic!("no control for {target:?}"));
        let revision = self.panel.layout_revision();
        self.panel
            .handle(
                PanelEvent::Invoke {
                    action,
                    layout_revision: revision,
                },
                self.now,
            )
            .unwrap();
    }

    fn key(&mut self, key: PanelKey) {
        self.panel
            .handle(PanelEvent::Key { key, shift: false }, self.now)
            .unwrap();
    }

    fn card(&self, number: &str) -> RecordId {
        self.panel
            .board()
            .unwrap()
            .card(number)
            .unwrap()
            .card_id
            .clone()
    }

    fn refresh(&mut self, w: f32, h: f32) {
        self.frame(w, h);
        self.click(&Target::Refresh);
        self.settle();
    }
}

fn texts(scene: &Scene) -> Vec<String> {
    scene
        .nodes()
        .iter()
        .filter(|n| !n.text.is_empty())
        .map(|n| n.text.clone())
        .collect()
}

#[test]
fn demo_fixtures_are_valid_contract_data() {
    for step in 0..demo::step_count() {
        let board = demo::step_board(step);
        board.validate().unwrap();
        let evidence = demo::step_evidence(step);
        assert_eq!(evidence.len(), board.cards.len());
        for response in &evidence {
            response.validate().unwrap();
            let card = board
                .cards
                .iter()
                .find(|c| c.card_id == response.card_id)
                .unwrap();
            assert_eq!(
                response.items.len() as u32,
                card.evidence_count,
                "{:?}",
                card.business_number
            );
            assert_eq!(response.revision, board.revision);
        }
    }
    assert!(!demo::step_board(2).totals_complete);
}

#[test]
fn task_two_is_unassigned_before_the_issue_update_and_casey_after_it() {
    let lang = Lang { zh: false };
    let before = demo::step_board(0);
    let r102 = before.card("ORION-R-102").unwrap();
    assert!(model::signals(r102, &[]).contains(&Signal::Unassigned));
    assert_eq!(DisplayColumn::of(r102.stage.stage), DisplayColumn::Todo);
    let rows = model::people_rows(r102, "Asia/Shanghai", lang);
    assert_eq!(
        (rows[0].role, rows[0].name.as_str()),
        ("Responsible now", "Unassigned")
    );
    // Morgan's earlier change makes Morgan a contact, never the assignee.
    assert!(
        rows.iter()
            .any(|r| r.role == "Contact" && r.name == "Morgan")
    );
    assert!(
        rows.iter()
            .any(|r| r.role == "Code history" && r.name == "Morgan")
    );
    for number in ["ORION-R-101", "ORION-R-103", "ORION-R-104"] {
        let card = before.card(number).unwrap();
        assert!(
            model::signals(card, &[]).contains(&Signal::NotInTracker),
            "{number}"
        );
    }

    let after = demo::step_board(1);
    for number in ["ORION-R-101", "ORION-R-102"] {
        let card = after.card(number).unwrap();
        let rows = model::people_rows(card, "Asia/Shanghai", lang);
        assert_eq!(rows[0].name, "Casey", "{number}");
        assert!(
            rows[0].detail.starts_with("Meegle field"),
            "{}",
            rows[0].detail
        );
        assert_eq!(DisplayColumn::of(card.stage.stage), DisplayColumn::Develop);
        assert!(
            !rows
                .iter()
                .any(|r| r.role == "Responsible now" && r.name == "Morgan")
        );
    }
    // Gale's Lark statement for task 1 stays visible as an earlier statement.
    let r101 = model::people_rows(after.card("ORION-R-101").unwrap(), "Asia/Shanghai", lang);
    assert!(r101.iter().any(|r| r.role == "Earlier statement" && r.detail.contains("Gale named Casey in Lark")));
    assert!(
        r101.iter()
            .any(|r| r.role == "Reviewer" && r.name == "Gale" && r.detail.contains("(copied)"))
    );
}

#[test]
fn merged_code_regression_pending_and_a_separate_reviewer_read_as_distinct_facts() {
    let lang = Lang { zh: false };
    let board = demo::step_board(1);
    let r087 = board.card("ORION-R-087").unwrap();
    assert_eq!(
        model::fact_line(r087, lang),
        "Regression · Code merged · Test running"
    );
    assert_eq!(DisplayColumn::of(r087.stage.stage), DisplayColumn::Verify);
    let rows = model::people_rows(r087, "Asia/Shanghai", lang);
    let role = |name: &str| {
        rows.iter()
            .filter(|r| r.name == name)
            .map(|r| r.role)
            .collect::<Vec<_>>()
    };
    assert_eq!(role("司马云澜"), ["Responsible now"]);
    assert_eq!(role("Riley"), ["Also assigned", "Developer"]);
    assert_eq!(role("Jordan"), ["Reviewer", "Submitter"]);
    assert_eq!(
        model::acceptance_text(r087, "Asia/Shanghai", lang),
        "Not accepted"
    );
    // A failed regression after merge is a danger signal, not a finished card.
    let r124 = board.card("ORION-R-124").unwrap();
    assert_eq!(model::signals(r124, &[])[0], Signal::TestFailed);
}

#[test]
fn refresh_reorders_cards_and_keeps_the_selection_by_identity() {
    let mut h = harness();
    h.refresh(1440.0, 900.0); // step 2
    assert_eq!(h.panel.board().unwrap().revision, Revision(57));
    let r108 = h.card("ORION-R-108");
    h.frame(1440.0, 900.0);
    h.click(&Target::Select(r108.clone()));
    h.settle();
    let scene = h.frame(1440.0, 900.0);
    assert!(
        texts(&scene)
            .iter()
            .any(|t| t == "Accept or reject the notification summary")
    );
    h.refresh(1440.0, 900.0); // step 3: R-108 accepted, Gerrit failed
    assert_eq!(h.panel.board().unwrap().revision, Revision(58));
    assert_eq!(h.panel.selected(), Some(&r108));
    let status = h.panel.status().unwrap();
    assert!(
        status.contains("ORION-R-108 moved Verify → Done"),
        "{status}"
    );
    assert!(status.contains("Gerrit did not refresh"), "{status}");
    // The detail follows the card into Done and loads evidence for the new revision.
    let scene = h.frame(1440.0, 900.0);
    let texts = texts(&scene);
    assert!(texts.iter().any(|t| t.contains("by Taylor")), "{texts:?}");
    assert!(
        texts
            .iter()
            .any(|t| t == "Taylor completes the acceptance node")
    );
    // Partial totals are marked as a lower bound.
    assert!(texts.iter().any(|t| t.starts_with('≥')));
    assert!(texts.iter().any(|t| t.contains("Gerrit did not refresh")));
    // The outage is reported once; it does not turn every card into attention work.
    assert!(texts.iter().any(|t| t == "Needs attention 5"), "{texts:?}");
    // R-087's facts come partly from Gerrit; its card keeps its progress facts.
    assert!(
        texts
            .iter()
            .any(|t| t.starts_with("Regression · Code merged")),
        "{texts:?}"
    );
    // A refresh at the last step reports that nothing newer arrived.
    h.refresh(1440.0, 900.0);
    assert_eq!(h.panel.status(), Some("Refresh finished. No newer data."));
}

#[test]
fn a_second_refresh_while_one_runs_is_reported_not_queued() {
    let mut h = harness();
    h.frame(1440.0, 900.0);
    h.click(&Target::Refresh);
    h.frame(1440.0, 900.0);
    // The title-row command is disabled while it runs; F5 still asks and is told why.
    let chrome = h.panel.chrome();
    assert!(
        !chrome
            .commands
            .iter()
            .any(|c| c.label == "Refresh" && c.enabled)
    );
    assert_eq!(
        chrome.status.map(|s| s.text).as_deref(),
        Some("Refreshing…")
    );
    h.key(PanelKey::Refresh);
    assert_eq!(h.panel.status(), Some("A refresh is already running."));
    h.settle();
    assert_eq!(h.panel.board().unwrap().revision, Revision(57));
}

#[test]
fn evidence_loads_lazily_and_a_failed_load_can_be_retried() {
    let mut h = harness();
    h.frame(1440.0, 900.0);
    // R-121 sits in the last lane, below the fold: select it as the keyboard would.
    let r121 = h.card("ORION-R-121");
    h.panel.select(r121, true, h.now).unwrap();
    let scene = h.frame(1440.0, 900.0);
    assert!(texts(&scene).iter().any(|t| t == "Loading evidence…"));
    h.settle();
    let scene = h.frame(1440.0, 900.0);
    assert!(texts(&scene).iter().any(|t| t == "Evidence did not load."));
    h.click(&Target::RetryEvidence);
    h.settle();
    let scene = h.frame(1440.0, 900.0);
    assert!(
        texts(&scene)
            .iter()
            .any(|t| t.contains("has no board mapping"))
    );
}

#[test]
fn keyboard_moves_through_columns_and_lanes() {
    let mut h = harness();
    h.refresh(1440.0, 900.0);
    h.frame(1440.0, 900.0);
    h.key(PanelKey::Down); // nothing selected: first card in reading order
    let first = h.panel.selected().cloned().unwrap();
    assert_eq!(first, h.card("ORION-R-103"));
    h.frame(1440.0, 900.0);
    h.key(PanelKey::Right);
    let develop = h
        .panel
        .board()
        .unwrap()
        .cards
        .iter()
        .find(|c| Some(&c.card_id) == h.panel.selected())
        .unwrap();
    assert_eq!(
        DisplayColumn::of(develop.stage.stage),
        DisplayColumn::Develop
    );
    assert_eq!(develop.swimlane.model.as_deref(), Some("orion-ir"));
    h.frame(1440.0, 900.0);
    h.key(PanelKey::Enter);
    let scene = h.frame(1440.0, 900.0);
    assert!(h.panel.layout().detail.is_some());
    assert!(texts(&scene).iter().any(|t| t == "Responsible now"));
    h.key(PanelKey::Escape);
    h.frame(1440.0, 900.0);
    assert!(h.panel.layout().detail.is_none());
    assert!(h.panel.selected().is_some());
    h.key(PanelKey::Escape);
    assert!(h.panel.selected().is_none());
}

#[test]
fn filters_narrow_the_board_and_clear_together() {
    let mut h = harness();
    h.refresh(1440.0, 900.0);
    h.frame(1440.0, 900.0);
    h.click(&Target::Attention);
    let scene = h.frame(1440.0, 900.0);
    let numbers: Vec<String> = h
        .panel
        .layout()
        .nav
        .iter()
        .map(|n| {
            h.panel
                .board()
                .unwrap()
                .cards
                .iter()
                .find(|c| c.card_id == n.id)
                .unwrap()
                .business_number
                .clone()
                .unwrap()
        })
        .collect();
    for expected in [
        "ORION-R-093",
        "ORION-R-099",
        "ORION-R-124",
        "ORION-R-090",
        "ORION-R-121",
    ] {
        assert!(
            numbers.iter().any(|n| n == expected),
            "{expected} in {numbers:?}"
        );
    }
    assert!(!numbers.iter().any(|n| n == "ORION-R-115"));
    drop(scene);
    h.click(&Target::Attention);
    h.panel
        .handle(
            PanelEvent::TextSubmitted {
                action: ACTION_FIND_TEXT,
                text: "kai".into(),
            },
            h.now,
        )
        .unwrap();
    h.frame(1440.0, 900.0);
    assert!(h.panel.layout().nav.len() >= 2);
    h.panel.filter_mut().query = Some("no such person".into());
    let scene = h.frame(1440.0, 900.0);
    assert!(
        texts(&scene)
            .iter()
            .any(|t| t == "No cards match the filters.")
    );
    h.click(&Target::ClearFilters);
    h.frame(1440.0, 900.0);
    assert_eq!(h.panel.layout().nav.len(), 15);
}

#[test]
fn summary_copy_uses_the_backend_text_for_the_shown_revision() {
    let mut h = harness();
    h.refresh(1440.0, 900.0);
    h.frame(1440.0, 900.0);
    h.click(&Target::CopySummary);
    h.settle();
    let copied = h.calls.copied.borrow();
    assert_eq!(copied.len(), 1);
    let text = &copied[0];
    assert!(text.contains("rev 57"));
    assert!(
        text.contains("ORION-R-093 Support the LC3 audio codec in Car Mode — Blocked"),
        "{text}"
    );
    assert!(
        text.contains(
            "Taylor\n  - ORION-R-101: Confirm whether change 4711 covers this requirement"
        ),
        "{text}"
    );
    assert_eq!(h.panel.status(), Some("Summary copied (rev 57)."));
}

#[test]
fn open_source_says_why_nothing_opened_and_copy_id_copies() {
    let mut h = harness();
    h.frame(1440.0, 900.0);
    let r102 = h.card("ORION-R-102");
    h.click(&Target::Select(r102.clone()));
    h.settle();
    h.frame(1440.0, 900.0);
    let source = h
        .panel
        .board()
        .unwrap()
        .card("ORION-R-102")
        .unwrap()
        .source
        .clone();
    h.click(&Target::OpenSource(source));
    h.settle();
    assert_eq!(
        h.panel.status(),
        Some("Meegle has no configured link. Nothing opened.")
    );
    assert_eq!(h.calls.opened.get(), 0);
    h.frame(1440.0, 900.0);
    h.click(&Target::Copy(r102.0.clone()));
    assert_eq!(
        h.calls.copied.borrow().last().map(String::as_str),
        Some("meegle:acme/orion/story/88232")
    );
}

#[test]
fn stale_layout_actions_are_ignored() {
    let mut h = harness();
    h.frame(1440.0, 900.0);
    let action = h.panel.action_for(&Target::Attention).unwrap();
    let old = h.panel.layout_revision();
    h.panel
        .handle(
            PanelEvent::Scroll {
                x: 400.0,
                y: 400.0,
                lines: 3.0,
            },
            h.now,
        )
        .unwrap();
    h.panel
        .handle(
            PanelEvent::Invoke {
                action,
                layout_revision: old,
            },
            h.now,
        )
        .unwrap();
    h.frame(1440.0, 900.0);
    assert!(!h.panel.filter_mut().attention);
}

#[test]
fn width_and_content_choose_the_composition() {
    let mut h = harness();
    h.refresh(1440.0, 900.0);
    let modes = |h: &mut Harness, w: f32, ht: f32| {
        h.frame(w, ht);
        h.panel.layout().mode
    };
    assert_eq!(modes(&mut h, 1440.0, 900.0), Mode::Board);
    assert_eq!(modes(&mut h, 900.0, 700.0), Mode::Board);
    // Done collapses to a rail before columns get narrower than a card.
    let scene = h.frame(900.0, 700.0);
    assert!(
        scene
            .nodes()
            .iter()
            .any(|n| n.name == "Done 1" && n.action.is_some())
    );
    assert_eq!(modes(&mut h, 700.0, 700.0), Mode::Tabs);
    assert_eq!(modes(&mut h, 480.0, 320.0), Mode::Compact);
    let r093 = h.card("ORION-R-093");
    h.frame(1440.0, 900.0);
    h.click(&Target::Select(r093.clone()));
    h.settle();
    assert_eq!(modes(&mut h, 1440.0, 900.0), Mode::Board);
    assert!(h.panel.layout().detail.is_some());
    assert_eq!(modes(&mut h, 1000.0, 760.0), Mode::Detail);
    assert_eq!(modes(&mut h, 480.0, 320.0), Mode::CompactDetail);
    // The compact list ranks the blocked card first.
    h.key(PanelKey::Escape);
    h.key(PanelKey::Escape);
    h.frame(480.0, 320.0);
    assert_eq!(h.panel.layout().nav[0].id, r093);
    let mut capsule = input(360.0, 48.0, 1.0);
    capsule.presentation = Presentation::Capsule;
    h.panel.prepare(capsule, &Heuristic).unwrap();
    assert_eq!(h.panel.layout().mode, Mode::Capsule);
}

#[test]
fn many_cards_scroll_and_keyboard_selection_stays_visible() {
    let mut h = harness();
    let mut board = h.panel.board().unwrap().clone();
    let template = board.cards.clone();
    for copy in 1..4 {
        for card in &template {
            let mut card = card.clone();
            card.card_id = RecordId::new(format!("{}-{copy}", card.card_id.0)).unwrap();
            card.source.record_id = format!("{}{copy}", card.source.record_id);
            card.business_number = card.business_number.map(|n| format!("{n}.{copy}"));
            board.cards.push(card);
        }
    }
    board.totals_complete = false;
    board.revision = Revision(99);
    let cards = board.cards.len();
    h.panel.accept_board(board, None);
    h.frame(1440.0, 900.0);
    let layout = h.panel.layout().clone();
    assert_eq!(layout.nav.len(), cards);
    let region = layout.board.unwrap();
    assert!(layout.board_content > region.h * 2.0);
    for _ in 0..12 {
        h.key(PanelKey::Down);
        h.frame(1440.0, 900.0);
    }
    let id = h.panel.selected().cloned().unwrap();
    let nav = h
        .panel
        .layout()
        .nav
        .iter()
        .find(|n| n.id == id)
        .unwrap()
        .clone();
    let scroll = h.panel.scroll;
    assert!(
        nav.y >= scroll - 0.5 && nav.y + nav.h <= scroll + region.h + 0.5,
        "{nav:?} {scroll}"
    );
    h.panel
        .handle(
            PanelEvent::Scroll {
                x: 200.0,
                y: 400.0,
                lines: 10_000.0,
            },
            h.now,
        )
        .unwrap();
    h.frame(1440.0, 900.0);
    assert!((h.panel.scroll - (h.panel.layout().board_content - region.h)).abs() < 1.0);
}

/// Geometry invariants for every composition, theme and text scale: text stays
/// inside the viewport, and no two text runs on one card overlap.
#[test]
fn layouts_hold_their_geometry_across_themes_and_text_scales() {
    for theme in [None, Some(light()), Some(high_contrast())] {
        let mut h = harness_with(theme);
        h.settle();
        h.refresh(1440.0, 900.0);
        let r112 = h.card("ORION-R-112");
        for scale in [1.0, 1.5, 2.0] {
            for (w, ht, select) in [
                (1440.0, 900.0, true),
                (1120.0, 760.0, false),
                (900.0, 700.0, false),
                (700.0, 640.0, false),
                (480.0, 320.0, false),
            ] {
                h.panel.selected = None;
                h.panel.detail_open = false;
                if select {
                    h.panel.select(r112.clone(), true, h.now).unwrap();
                    h.settle();
                }
                let scene = h
                    .panel
                    .prepare(input(w * scale, ht * scale, scale), &Heuristic)
                    .unwrap();
                let view = scene.viewport();
                let text: Vec<_> = scene
                    .nodes()
                    .iter()
                    .filter(|n| !n.text.is_empty())
                    .collect();
                for node in &text {
                    let r = node.rect;
                    assert!(
                        r.x >= -0.5
                            && r.y >= -0.5
                            && r.x + r.w <= view.w + 0.5
                            && r.y + r.h <= view.h + 0.5,
                        "{} at {r:?} in {w}x{ht}@{scale}",
                        node.text
                    );
                }
                for (i, a) in text.iter().enumerate() {
                    for b in &text[i + 1..] {
                        let (ra, rb) = (a.rect, b.rect);
                        let overlap = ra.x + 0.5 < rb.x + rb.w
                            && rb.x + 0.5 < ra.x + ra.w
                            && ra.y + 0.5 < rb.y + rb.h
                            && rb.y + 0.5 < ra.y + ra.h;
                        assert!(
                            !overlap,
                            "{:?} overlaps {:?} in {w}x{ht}@{scale}",
                            a.text, b.text
                        );
                    }
                }
                let mut canvas = pecofence_render::HeadlessCanvas::default();
                scene.paint(&mut canvas).unwrap();
                assert!(canvas.is_balanced());
            }
        }
    }
}

#[test]
fn long_titles_and_names_are_clamped_inside_their_card() {
    let mut h = harness();
    h.refresh(1440.0, 900.0);
    let scene = h.frame(1120.0, 1400.0);
    let title = scene
        .nodes()
        .iter()
        .find(|n| n.text.starts_with("桌面搜索框"))
        .unwrap();
    let lines = (title.rect.h / (14.0 * 1.33)).round();
    assert!(lines <= 2.0, "{lines}");
    // The blocked card's contact has a long name; the next-action line ellipsizes it.
    let next = scene
        .nodes()
        .iter()
        .find(|n| n.text.starts_with("→ Alexandria"))
        .unwrap();
    assert!(next.text.ends_with('…'), "{}", next.text);
}

/// Pixel snapshots (text as placeholder bars) of the main views for review.
/// `UPDATE_SNAPSHOTS=1` accepts a deliberate change.
#[test]
fn board_snapshots_hold_their_layout() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    let cases: [(&str, Option<ThemeSnapshot>, f32, f32, bool); 5] = [
        ("board-dark-1440", None, 1440.0, 900.0, true),
        ("board-light-1120", Some(light()), 1120.0, 760.0, false),
        ("board-hc-900", Some(high_contrast()), 900.0, 700.0, false),
        ("board-tabs-700", Some(light()), 700.0, 640.0, false),
        ("board-compact-480", None, 480.0, 320.0, false),
    ];
    for (name, theme, w, ht, select) in cases {
        let background = theme
            .as_ref()
            .map_or(ThemeSnapshot::dark().surface_panel, |t| t.surface_panel);
        let mut h = harness_with(theme);
        h.settle();
        h.refresh(w, ht);
        if select {
            let r102 = h.card("ORION-R-102");
            h.panel.select(r102, true, h.now).unwrap();
            h.settle();
        }
        let scene = h.frame(w, ht);
        let mut canvas = pecofence_render::HeadlessCanvas::default();
        scene.paint(&mut canvas).unwrap();
        let pixmap = pecofence_render::raster::rasterize(
            canvas.commands(),
            w as u32,
            ht as u32,
            [background[0], background[1], background[2], 1.0],
        );
        pecofence_render::raster::assert_snapshot(&pixmap, &dir.join(format!("{name}.png")));
    }
}

#[test]
fn every_timeline_entry_links_to_its_own_source() {
    let mut h = harness();
    h.frame(1440.0, 900.0);
    let r102 = h.card("ORION-R-102");
    h.click(&Target::Select(r102));
    h.settle();
    let scene = h.frame(1440.0, 900.0);
    let links: Vec<_> = scene
        .nodes()
        .iter()
        .filter(|n| n.role == "link" && n.id != 0)
        .collect();
    assert!(!links.is_empty(), "the timeline shows no source links");
    // Each link opens a source record; none of them selects the entry instead.
    let opens = h
        .panel
        .targets()
        .iter()
        .filter(|t| matches!(t, Target::OpenSource(_)))
        .count();
    assert!(
        opens > links.len(),
        "{opens} open targets for {} links",
        links.len()
    );
}

#[test]
fn the_header_is_published_as_chrome_once_and_again_only_when_it_changes() {
    let mut h = harness();
    let sent = |update: &PanelUpdate| {
        update
            .commands
            .iter()
            .filter_map(|c| match c {
                HostCommand::SetChrome(chrome) => Some(chrome.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    // The harness has already handled events, so the chrome went out with them.
    let chrome = h.panel.chrome();
    assert_eq!(chrome.title, "orion · orion-ir-2026-11");
    assert!(
        chrome.subtitle.starts_with("Demo data · Step 1 of 3"),
        "{}",
        chrome.subtitle
    );
    let labels: Vec<_> = chrome.commands.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, ["Refresh", "Copy summary", "Restart demo"]);
    // Nothing changed: no second copy.
    let again = h.panel.handle(PanelEvent::Wake, h.now).unwrap();
    assert!(sent(&again).is_empty());
    // A refresh changes the status and the Refresh command, so the chrome is resent.
    let id = chrome.commands[0].id;
    let update = h.panel.handle(PanelEvent::Command { id }, h.now).unwrap();
    let refreshed = sent(&update).pop().expect("refresh changes the chrome");
    assert!(!refreshed.commands[0].enabled);
    // The board no longer draws a title row of its own.
    let scene = h.frame(1440.0, 900.0);
    assert!(!texts(&scene).iter().any(|t| t.starts_with("orion · ")));
}

#[test]
fn a_chrome_command_runs_the_same_action_as_its_old_button() {
    let mut h = harness();
    h.settle();
    h.frame(1440.0, 900.0);
    h.click(&Target::CopySummary);
    h.settle();
    assert!(
        h.calls.copied.borrow().iter().any(|t| t.contains("ORION")),
        "{:?}",
        h.calls.copied.borrow()
    );
}

//! Evidence-backed work board: one SPM board revision as a Kanban workspace,
//! a ranked compact list, a capsule and a detail view with lazy evidence.
//!
//! Projection and business rules stay in the SPM backend; this panel shows the
//! board it receives and keeps view state (selection, filters, scroll) by card
//! identity. With `"source": "demo"` a deterministic in-process backend
//! ([`demo`]) serves the same contract, because the SPM daemon has no board
//! producer yet.

pub mod demo;
pub mod layout;
pub mod model;
pub mod palette;
pub mod text;

use crate::{decode_event, encode_request};
use layout::{BoardView, Draft, EvidenceView, Input, Layout, Mode, NavCard, Palette, Target, Zoom};
use model::{DisplayColumn, Filter, Tone};
use pecofence_plugin_api::design::{ContentScale, LayoutTracker};
use pecofence_plugin_api::scene::{NodeStyle, Scene, SceneNode};
use pecofence_plugin_api::*;
use serde::{Deserialize, Serialize};
use spm_contracts::{
    BoardSnapshot, Body, BuildBriefingRequest, CardEvidenceResponse, DaemonSessionId,
    DeliveryScopeId, DetailLevel, Event, GetCardEvidenceRequest, IdempotencyKey, OperationId,
    ProjectId, ProjectQuery, RecordId, RefreshOutcome, RefreshRequest, Request,
    ResolveNavigationRequest, Response, Revision, SourceRef, SubscriptionId, Swimlane, ViewKind,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use text::{Fitter, HostMeasure, Lang};

pub const PROVIDER_ID: &str = "pecofence.spm.board";
pub const CONFIG_VERSION: u16 = 1;
const ACTION_BASE: u64 = 16;
const ACTION_FIND_TEXT: u64 = 2;
/// Ids of the commands the board publishes in its chrome.
const CHROME_REFRESH: u64 = 1;
const CHROME_COPY: u64 = 2;
const CHROME_RESTART_DEMO: u64 = 3;
/// How long a live board may take before the panel says SPM has not answered.
const LIVE_WAIT: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardSource {
    #[default]
    Spm,
    /// Deterministic fixtures; the panel says so in its header.
    Demo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardConfig {
    pub project_id: ProjectId,
    pub delivery_scope_id: DeliveryScopeId,
    #[serde(default)]
    pub source: BoardSource,
    /// Where "Open in …" may lead; see [`crate::navigation`].
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub navigation_origins: crate::navigation::NavigationOrigins,
}

/// Panel configuration for the demo board (version, JSON).
pub fn demo_config() -> (u16, serde_json::Value) {
    (
        CONFIG_VERSION,
        serde_json::json!({
            "project_id": "orion",
            "delivery_scope_id": "orion-ir-2026-11",
            "source": "demo",
        }),
    )
}

pub struct BoardPlugin;

impl PanelProvider for BoardPlugin {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: PROVIDER_ID,
            api_major: 1,
            config_major: CONFIG_VERSION,
            required_services: &["render", "theme", "desktop", "ipc", "storage"],
        }
    }

    fn validate(&self, config: &PanelConfig) -> Result<()> {
        if config.version != CONFIG_VERSION {
            return Err(Error::Invalid("unsupported board config version".into()));
        }
        let parsed: BoardConfig = serde_json::from_slice(&config.bytes)
            .map_err(|error| Error::Invalid(error.to_string()))?;
        crate::navigation::validate_origins(&parsed.navigation_origins)
    }

    fn create(&self, ctx: PluginContext, input: CreatePanel) -> Result<Box<dyn PanelInstance>> {
        self.validate(&input.config)?;
        let config: BoardConfig = serde_json::from_slice(&input.config.bytes)
            .map_err(|error| Error::Invalid(error.to_string()))?;
        Ok(Box::new(BoardPanel::new(
            ctx,
            input.key,
            config,
            Instant::now(),
        )?))
    }
}

enum Backend {
    Host,
    Demo(demo::DemoBackend),
}

#[derive(Clone, Debug, PartialEq)]
enum Pending {
    Refresh,
    Evidence(RecordId, Revision),
    Briefing(Revision),
    Navigate(SourceRef),
}

#[derive(Clone, Debug)]
enum Evidence {
    Loading,
    Failed(String),
    Loaded(CardEvidenceResponse),
}

/// Accepts board revisions in order within a daemon session; a new session
/// starts over, as for project snapshots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardCursor {
    session: Option<DaemonSessionId>,
    revision: Option<Revision>,
}

impl BoardCursor {
    pub fn accept(&mut self, config: &BoardConfig, board: &BoardSnapshot) -> bool {
        if board.project_id != config.project_id
            || board.delivery_scope_id != config.delivery_scope_id
        {
            return false;
        }
        if self.session != Some(board.daemon_session) {
            self.session = Some(board.daemon_session);
            self.revision = None;
        }
        if self.revision.is_some_and(|r| board.revision <= r) {
            return false;
        }
        self.revision = Some(board.revision);
        true
    }
}

pub struct BoardPanel {
    ctx: PluginContext,
    key: InstanceKey,
    config: BoardConfig,
    /// The monitor's CI snapshot (context from the composition root).
    ci: spm_contracts::CiWatchSnapshot,
    /// First-party events raised by this panel, returned with its next update.
    outbox: Vec<HostCommand>,
    backend: Backend,
    subscription: Token,
    server_subscription: Option<SubscriptionId>,
    started: Instant,
    mount: Option<MountKey>,
    board: Option<BoardSnapshot>,
    cursor: BoardCursor,
    load_error: Option<String>,
    view: BoardView,
    zoom: Zoom,
    evidence_open: bool,
    reveal_after_layout: bool,
    selected: Option<RecordId>,
    detail_open: bool,
    filter: Filter,
    collapsed: Vec<Swimlane>,
    tab: DisplayColumn,
    done_open: bool,
    scroll: f32,
    detail_scroll: f32,
    evidence: HashMap<(RecordId, Revision), Evidence>,
    selected_evidence: Option<String>,
    preview_event: Option<(RecordId, String)>,
    pending: HashMap<Token, Pending>,
    refresh_operation: Option<OperationId>,
    board_since_refresh: bool,
    status: Option<(String, Option<Tone>)>,
    palette: Palette,
    fit_cache: RefCell<HashMap<text::FitKey, (String, u32)>>,
    targets: Vec<Target>,
    last: Layout,
    find_bounds: Option<RectDip>,
    view_revision: u64,
    prepared_view_revision: u64,
    layout_revision: u64,
    layout_tracker: LayoutTracker,
    frame_revision: u64,
    ids: HashMap<String, u64>,
    next_id: u64,
    locale: &'static str,
    stopped: bool,
    /// The header last sent to the host, so an unchanged one is not sent again.
    published_chrome: Option<PanelChrome>,
}

impl BoardPanel {
    pub fn new(
        ctx: PluginContext,
        key: InstanceKey,
        config: BoardConfig,
        now: Instant,
    ) -> Result<Self> {
        let theme = ctx
            .theme
            .with(|theme| theme.snapshot())
            .map(|s| (*s).clone())
            .unwrap_or_else(|_| ThemeSnapshot::dark());
        let mut backend = match config.source {
            BoardSource::Spm => Backend::Host,
            BoardSource::Demo => Backend::Demo(demo::DemoBackend::new()),
        };
        let subscription = match &mut backend {
            Backend::Host => {
                let query = ProjectQuery {
                    project_id: config.project_id.clone(),
                    delivery_scope_id: config.delivery_scope_id.clone(),
                    view: ViewKind::Board,
                    filter: None,
                    detail_level: DetailLevel::Standard,
                    sort: Vec::new(),
                };
                let payload =
                    serde_json::to_vec(&query).map_err(|e| Error::Invalid(e.to_string()))?;
                ctx.ipc.with(|ipc| {
                    ipc.subscribe(
                        &ctx.scope,
                        Query {
                            endpoint: "spm.v2/read-model".into(),
                            payload: Arc::from(payload),
                        },
                    )
                })?
            }
            Backend::Demo(demo) => demo.subscribe(now),
        };
        Ok(Self {
            ctx,
            key,
            config,
            ci: Default::default(),
            outbox: vec![],
            backend,
            subscription,
            server_subscription: None,
            started: now,
            mount: None,
            board: None,
            cursor: BoardCursor::default(),
            load_error: None,
            view: BoardView::default(),
            zoom: Zoom::default(),
            evidence_open: false,
            reveal_after_layout: false,
            selected: None,
            detail_open: false,
            filter: Filter::default(),
            collapsed: Vec::new(),
            tab: DisplayColumn::Develop,
            done_open: false,
            scroll: 0.0,
            detail_scroll: 0.0,
            evidence: HashMap::new(),
            selected_evidence: None,
            preview_event: None,
            pending: HashMap::new(),
            refresh_operation: None,
            board_since_refresh: false,
            status: None,
            palette: Palette::from_theme(&theme),
            fit_cache: RefCell::default(),
            targets: Vec::new(),
            last: Layout::default(),
            find_bounds: None,
            view_revision: 1,
            prepared_view_revision: 0,
            layout_revision: 0,
            layout_tracker: LayoutTracker::default(),
            frame_revision: 0,
            ids: HashMap::new(),
            next_id: 2,
            locale: "en",
            stopped: false,
            published_chrome: None,
        })
    }

    fn lang(&self) -> Lang {
        Lang::for_locale(self.locale)
    }

    pub fn board(&self) -> Option<&BoardSnapshot> {
        self.board.as_ref()
    }

    pub fn selected(&self) -> Option<&RecordId> {
        self.selected.as_ref()
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_ref().map(|(s, _)| s.as_str())
    }

    pub fn filter_mut(&mut self) -> &mut Filter {
        self.changed();
        &mut self.filter
    }

    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    pub fn layout(&self) -> &Layout {
        &self.last
    }

    pub fn layout_revision(&self) -> u64 {
        self.layout_revision
    }

    /// Action id of the first control with this target in the last frame.
    pub fn action_for(&self, target: &Target) -> Option<u64> {
        self.targets
            .iter()
            .position(|t| t == target)
            .map(|i| ACTION_BASE + i as u64)
    }

    fn changed(&mut self) {
        self.view_revision += 1;
    }

    fn redraw(&mut self) -> PanelUpdate {
        self.changed();
        PanelUpdate {
            commands: vec![HostCommand::Invalidate],
            relayout: true,
        }
    }

    fn say(&mut self, text: String, tone: Option<Tone>) {
        self.status = Some((text, tone));
    }

    fn send(&mut self, request: Request, pending: Pending, now: Instant) -> Result<()> {
        let payload = encode_request(&request)?;
        let sent = match &mut self.backend {
            Backend::Host => self.ctx.ipc.with(|ipc| ipc.send(&self.ctx.scope, payload)),
            Backend::Demo(demo) => demo.send(&payload, now),
        };
        match sent {
            Ok(token) => {
                self.pending.insert(token, pending);
            }
            Err(error) => {
                let lang = self.lang();
                if pending == Pending::Refresh {
                    self.refresh_operation = None;
                }
                self.say(
                    lang.f("Refresh failed: {}", &[&error.to_string()]),
                    Some(Tone::Danger),
                );
            }
        }
        Ok(())
    }

    fn refreshing(&self) -> bool {
        self.pending.values().any(|p| *p == Pending::Refresh) || self.refresh_operation.is_some()
    }

    fn refresh(&mut self, now: Instant) -> Result<()> {
        let lang = self.lang();
        if self.refreshing() {
            self.say(lang.t("A refresh is already running.").into(), None);
            return Ok(());
        }
        self.board_since_refresh = false;
        self.send(
            Request::Refresh(RefreshRequest {
                project_id: self.config.project_id.clone(),
                delivery_scope_id: self.config.delivery_scope_id.clone(),
                idempotency_key: IdempotencyKey::new(),
            }),
            Pending::Refresh,
            now,
        )
    }

    /// Load the selected card's evidence for the current revision once.
    fn request_evidence(&mut self, now: Instant, retry: bool) -> Result<()> {
        let Some(id) = self.selected.clone() else {
            return Ok(());
        };
        self.request_card_evidence(&id, now, retry)
    }

    fn request_card_evidence(&mut self, id: &RecordId, now: Instant, retry: bool) -> Result<()> {
        let Some(board) = &self.board else {
            return Ok(());
        };
        if model::find(board, id).is_none() {
            return Ok(());
        }
        let key = (id.clone(), board.revision);
        let cached = self.evidence.get(&key);
        if matches!(cached, Some(Evidence::Loaded(_)) | Some(Evidence::Loading))
            || (matches!(cached, Some(Evidence::Failed(_))) && !retry)
        {
            return Ok(());
        }
        let Some(subscription_id) = self.server_subscription else {
            return Ok(());
        };
        let request = Request::GetCardEvidence(GetCardEvidenceRequest {
            subscription_id,
            card_id: id.clone(),
            revision: board.revision,
            cursor: None,
            page_size: 200,
        });
        self.evidence.insert(key.clone(), Evidence::Loading);
        self.send(request, Pending::Evidence(key.0, key.1), now)
    }

    fn select(&mut self, id: RecordId, open: bool, now: Instant) -> Result<()> {
        if self.selected.as_ref() != Some(&id) {
            self.detail_scroll = 0.0;
            self.selected_evidence = None;
        }
        self.selected = Some(id);
        self.detail_open |= open;
        if self.view == BoardView::Timeline && self.detail_open {
            self.reveal_after_layout = true;
        }
        if self.detail_open {
            self.request_evidence(now, false)?;
        }
        self.reveal_selected();
        Ok(())
    }

    fn reveal_selected(&mut self) {
        let (Some(id), Some(region)) = (&self.selected, self.last.board) else {
            return;
        };
        if let Some(card) = self.last.nav.iter().find(|n| &n.id == id) {
            if card.y < self.scroll {
                self.scroll = (card.y - 8.0).max(0.0);
            } else if card.y + card.h > self.scroll + region.h {
                self.scroll = card.y + card.h - region.h + 8.0;
            }
        }
    }

    fn accept_board(&mut self, board: BoardSnapshot, subscription: Option<SubscriptionId>) {
        let lang = self.lang();
        if !self.cursor.accept(&self.config, &board) {
            return;
        }
        if subscription.is_some() {
            self.server_subscription = subscription;
        }
        let mut notes = Vec::new();
        if let Some(previous) = &self.board {
            for (number, from, to) in model::moves(previous, &board) {
                notes.push(lang.f(
                    "{} moved {} → {}",
                    &[&number, from.label(lang), to.label(lang)],
                ));
            }
            for number in model::departures(previous, &board) {
                notes.push(lang.f("{} left the board", &[&number]));
            }
            if notes.is_empty() {
                notes.push(lang.t("no card moved").into());
            }
            for source in model::degraded_sources(&board) {
                notes.push(lang.f("{} did not refresh", &[model::system_label(source)]));
            }
            let tone = if model::degraded_sources(&board).is_empty() {
                None
            } else {
                Some(Tone::Warning)
            };
            self.say(
                lang.f(
                    "Revision {}: {}",
                    &[&board.revision.0.to_string(), &notes.join(" · ")],
                ),
                tone,
            );
        }
        // Selection follows the card identity, wherever the card moved.
        if self
            .selected
            .as_ref()
            .is_some_and(|id| model::find(&board, id).is_none())
        {
            self.selected = None;
            self.detail_open = false;
        }
        self.evidence
            .retain(|(_, revision), _| *revision == board.revision);
        self.load_error = None;
        self.preview_event = None;
        self.board = Some(board);
        self.board_since_refresh = true;
    }

    fn handle_envelope(&mut self, bytes: &[u8], now: Instant) -> Result<PanelUpdate> {
        let envelope = decode_event(bytes)?;
        let subscription = envelope.subscription_id;
        match envelope.body {
            Body::Event(Event::Board(board)) => {
                self.accept_board(board, subscription);
                if self.detail_open {
                    self.request_evidence(now, false)?;
                }
            }
            Body::Event(Event::OperationCompleted(done))
                if Some(done.operation_id) == self.refresh_operation =>
            {
                self.refresh_operation = None;
                let lang = self.lang();
                match done.outcome {
                    RefreshOutcome::Completed if !self.board_since_refresh => {
                        self.say(lang.t("Refresh finished. No newer data.").into(), None)
                    }
                    RefreshOutcome::Completed => {}
                    _ => self.say(
                        lang.f(
                            "Refresh failed: {}",
                            &[done.error.as_deref().unwrap_or("—")],
                        ),
                        Some(Tone::Danger),
                    ),
                }
            }
            _ => return Ok(PanelUpdate::default()),
        }
        Ok(self.redraw())
    }

    fn handle_completion(&mut self, pending: Pending, result: Result<Arc<[u8]>>) -> Result<()> {
        let lang = self.lang();
        let envelope = result.and_then(|bytes| decode_event(&bytes));
        let body = envelope.map(|e| e.body);
        match (pending, body) {
            (Pending::Refresh, Ok(Body::Response(Response::Refresh(response)))) => {
                self.refresh_operation = Some(response.operation_id);
                self.say(
                    lang.t("Refresh accepted. Waiting for new data.").into(),
                    None,
                );
            }
            (
                Pending::Evidence(id, revision),
                Ok(Body::Response(Response::GetCardEvidence(response))),
            ) => {
                let valid = response.card_id == id
                    && response.revision == revision
                    && response.validate().is_ok();
                self.evidence.insert(
                    (id, revision),
                    if valid {
                        Evidence::Loaded(response)
                    } else {
                        Evidence::Failed("the response belongs to another card or revision".into())
                    },
                );
            }
            (Pending::Evidence(id, revision), Ok(Body::Error(error))) => {
                self.evidence
                    .insert((id, revision), Evidence::Failed(error.message));
            }
            (
                Pending::Briefing(revision),
                Ok(Body::Response(Response::BuildBriefing(response))),
            ) => {
                let current = self.board.as_ref().map(|b| b.revision);
                if response.revision != revision || current != Some(revision) {
                    self.say(
                        lang.t("The summary is for an older revision. Copy it again.")
                            .into(),
                        Some(Tone::Warning),
                    );
                } else {
                    match self
                        .ctx
                        .clipboard
                        .as_ref()
                        .ok_or_else(|| Error::Backend("clipboard unavailable".into()))
                        .and_then(|c| c.with(|c| c.write_text(&self.ctx.scope, response.text)))
                    {
                        Ok(_) => self.say(
                            lang.f("Summary copied (rev {}).", &[&revision.0.to_string()]),
                            Some(Tone::Info),
                        ),
                        Err(error) => self.say(error.to_string(), Some(Tone::Danger)),
                    }
                }
            }
            (
                Pending::Navigate(source),
                Ok(Body::Response(Response::ResolveNavigation(response))),
            ) => {
                // The daemon names a configured origin and a record path; only an
                // origin in this panel's config can become a link.
                let system = model::system_label(source.system);
                use crate::navigation::{LinkError, OpenError};
                match crate::navigation::open(
                    &self.ctx,
                    &self.config.navigation_origins,
                    &response.targets,
                ) {
                    Ok(link) => self.say(
                        lang.f("Opened {}", &[model::system_label(link.system)]),
                        Some(Tone::Info),
                    ),
                    Err(OpenError::Host(error)) => self.say(
                        lang.f("{} did not open: {}", &[system, &error.to_string()]),
                        Some(Tone::Danger),
                    ),
                    Err(OpenError::Link(LinkError::BadPath)) => self.say(
                        lang.f("{} sent an invalid link. Nothing opened.", &[system]),
                        Some(Tone::Danger),
                    ),
                    Err(OpenError::Link(_)) => self.say(
                        lang.f("{} has no configured link. Nothing opened.", &[system]),
                        Some(Tone::Warning),
                    ),
                }
            }
            (Pending::Refresh, other) => {
                self.refresh_operation = None;
                let message = match other {
                    Ok(Body::Error(error)) => error.message,
                    Err(error) => error.to_string(),
                    _ => "unexpected response".into(),
                };
                self.say(
                    lang.f("Refresh failed: {}", &[&message]),
                    Some(Tone::Danger),
                );
            }
            (Pending::Evidence(id, revision), other) => {
                let message = match other {
                    Err(error) => error.to_string(),
                    _ => "unexpected response".into(),
                };
                self.evidence
                    .insert((id, revision), Evidence::Failed(message));
            }
            (_, Ok(Body::Error(error))) => self.say(error.message, Some(Tone::Danger)),
            (_, Err(error)) => self.say(error.to_string(), Some(Tone::Danger)),
            _ => {}
        }
        Ok(())
    }

    fn invoke(&mut self, target: Target, now: Instant) -> Result<PanelUpdate> {
        let lang = self.lang();
        match target {
            Target::Zoom(zoom) => self.zoom = zoom,
            Target::Event(id, evidence) => {
                self.preview_event = Some((id.clone(), evidence.clone()));
                self.select(id, self.detail_open, now)?;
                self.selected_evidence = Some(evidence);
                self.request_evidence(now, false)?;
            }
            Target::OpenEvidence => {
                self.evidence_open = true;
                self.detail_open = true;
                self.request_evidence(now, false)?;
            }
            Target::ShowOnBoard => {
                self.reveal_after_layout = true;
                self.view = BoardView::Board;
                self.detail_open = false;
                self.evidence_open = false;
                self.scroll = 0.0;
                self.filter = Filter::default();
                self.done_open = true;
                self.collapsed.clear();
                if let (Some(board), Some(id)) = (&self.board, &self.selected) {
                    if let Some(card) = model::find(board, id) {
                        self.tab = DisplayColumn::of(card.stage.stage);
                    }
                }
            }
            Target::View(view) => {
                self.preview_event = None;
                self.view = view;
                self.evidence_open = false;
                self.scroll = 0.0;
                self.detail_open = false;
            }
            Target::Refresh => self.refresh(now)?,
            Target::CopySummary => {
                if let Some(board) = &self.board {
                    let request = Request::BuildBriefing(BuildBriefingRequest {
                        project_id: board.project_id.clone(),
                        delivery_scope_id: board.delivery_scope_id.clone(),
                        revision: board.revision,
                        locale: if lang.zh { "zh-CN" } else { "en" }.into(),
                        timezone: board.project_timezone.clone(),
                    });
                    let revision = board.revision;
                    self.send(request, Pending::Briefing(revision), now)?;
                }
            }
            Target::RestartDemo | Target::RetryLoad => {
                self.status = None;
                match &mut self.backend {
                    Backend::Demo(demo) => demo.restart(now),
                    Backend::Host => {
                        let _ = self.ctx.ipc.with(|ipc| ipc.cancel(self.subscription));
                        let query = ProjectQuery {
                            project_id: self.config.project_id.clone(),
                            delivery_scope_id: self.config.delivery_scope_id.clone(),
                            view: ViewKind::Board,
                            filter: None,
                            detail_level: DetailLevel::Standard,
                            sort: Vec::new(),
                        };
                        let payload = serde_json::to_vec(&query)
                            .map_err(|e| Error::Invalid(e.to_string()))?;
                        self.subscription = self.ctx.ipc.with(|ipc| {
                            ipc.subscribe(
                                &self.ctx.scope,
                                Query {
                                    endpoint: "spm.v2/read-model".into(),
                                    payload: Arc::from(payload),
                                },
                            )
                        })?;
                        self.started = now;
                        self.load_error = None;
                    }
                }
            }
            Target::Model(model) => {
                self.filter.model = model;
                self.scroll = 0.0;
            }
            Target::CycleModel => {
                let models: Vec<String> = self
                    .board
                    .iter()
                    .flat_map(|b| b.lanes.iter().filter_map(|l| l.swimlane.model.clone()))
                    .collect();
                let next = match &self.filter.model {
                    None => models.first().cloned(),
                    Some(m) => models
                        .iter()
                        .position(|x| x == m)
                        .and_then(|i| models.get(i + 1))
                        .cloned(),
                };
                self.filter.model = next;
                self.scroll = 0.0;
            }
            Target::Attention => {
                self.filter.attention = !self.filter.attention;
                self.scroll = 0.0;
            }
            Target::Find => {
                let style = self.find_bounds.map(|bounds| {
                    let mut style = TextInputStyle::new(
                        RectDip {
                            w: bounds.w.max(260.0),
                            ..bounds
                        },
                        self.palette.primary,
                        self.palette.card,
                        13.0,
                    );
                    style.padding = [10.0, 0.0, 10.0, 0.0];
                    style.radius = bounds.h / 2.0;
                    style.accent = Some(self.palette.accent);
                    style.placeholder = Some(lang.t("Number, title or person"));
                    style
                });
                return Ok(PanelUpdate {
                    commands: vec![HostCommand::RequestTextInput {
                        action: ACTION_FIND_TEXT,
                        initial: self.filter.query.clone().unwrap_or_default(),
                        multiline: false,
                        style,
                    }],
                    relayout: false,
                });
            }
            Target::ClearQuery => self.filter.query = None,
            Target::ClearFilters => {
                self.filter = Filter::default();
                self.scroll = 0.0;
            }
            Target::Select(id) => {
                let reopen = self.selected.as_ref() == Some(&id) && self.detail_open;
                if reopen {
                    // A second click on the open card closes the detail.
                    self.detail_open = false;
                } else {
                    self.select(id, true, now)?;
                }
            }
            Target::CloseDetail => {
                self.detail_open = false;
                self.evidence_open = false;
            }
            Target::ToggleLane(lane) => match self.collapsed.iter().position(|l| *l == lane) {
                Some(i) => {
                    self.collapsed.remove(i);
                }
                None => self.collapsed.push(lane),
            },
            Target::Tab(column) => {
                self.tab = column;
                self.scroll = 0.0;
            }
            Target::ToggleDone => self.done_open = !self.done_open,
            Target::OpenCi => {
                let intent = crate::ci_watch::entry::OpenIntent {
                    focus: crate::ci_watch::entry::Summary::from_snapshot(&self.ci).focus,
                    edit: None,
                };
                self.outbox.push(HostCommand::AppEvent {
                    topic: "ci.open".into(),
                    json: serde_json::to_string(&intent)
                        .map_err(|e| Error::Invalid(e.to_string()))?,
                });
            }
            Target::OpenSource(source) => {
                if let Some(board) = &self.board {
                    let revision = board.revision;
                    self.send(
                        Request::ResolveNavigation(ResolveNavigationRequest {
                            source: source.clone(),
                            revision,
                        }),
                        Pending::Navigate(source),
                        now,
                    )?;
                }
            }
            Target::Copy(text) => {
                let written = self
                    .ctx
                    .clipboard
                    .as_ref()
                    .ok_or_else(|| Error::Backend("clipboard unavailable".into()))
                    .and_then(|c| c.with(|c| c.write_text(&self.ctx.scope, text.clone())));
                match written {
                    Ok(_) => self.say(lang.f("Copied {}", &[&text]), Some(Tone::Info)),
                    Err(error) => self.say(error.to_string(), Some(Tone::Danger)),
                }
            }
            Target::SelectEvidence(id) => {
                self.selected_evidence = if self.selected_evidence.as_ref() == Some(&id) {
                    None
                } else {
                    Some(id)
                };
            }
            Target::RetryEvidence => self.request_evidence(now, true)?,
        }
        Ok(self.redraw())
    }

    fn key(&mut self, key: PanelKey, now: Instant) -> Result<PanelUpdate> {
        let nav = self.last.nav.clone();
        let current = self
            .selected
            .as_ref()
            .and_then(|id| nav.iter().position(|n| &n.id == id));
        let in_column = |column: usize| -> Vec<&NavCard> {
            let mut cards: Vec<&NavCard> = nav.iter().filter(|n| n.column == column).collect();
            cards.sort_by(|a, b| a.y.total_cmp(&b.y));
            cards
        };
        let page = self.last.board.map_or(300.0, |r| r.h * 0.9);
        let detail_mode = matches!(self.last.mode, Mode::Detail | Mode::CompactDetail);
        let target: Option<RecordId> = match key {
            PanelKey::Refresh => {
                self.refresh(now)?;
                None
            }
            PanelKey::Escape => {
                self.preview_event = None;
                if self.detail_open {
                    self.detail_open = false;
                    self.evidence_open = false;
                } else if self.selected.is_some() {
                    self.selected = None;
                } else {
                    return Ok(PanelUpdate::default());
                }
                None
            }
            PanelKey::Enter => {
                if let Some(id) = self.selected.clone() {
                    self.select(id, true, now)?;
                }
                None
            }
            PanelKey::PageUp | PanelKey::PageDown
                if detail_mode
                    || (self.detail_open && self.last.detail.is_some() && current.is_none()) =>
            {
                let delta = if key == PanelKey::PageUp { -page } else { page };
                self.detail_scroll = (self.detail_scroll + delta).max(0.0);
                None
            }
            PanelKey::PageUp | PanelKey::PageDown => {
                let delta = if key == PanelKey::PageUp { -page } else { page };
                self.scroll = (self.scroll + delta).max(0.0);
                None
            }
            PanelKey::Up | PanelKey::Down if detail_mode => {
                let delta = if key == PanelKey::Up { -48.0 } else { 48.0 };
                self.detail_scroll = (self.detail_scroll + delta).max(0.0);
                None
            }
            _ if nav.is_empty() => None,
            _ => match current {
                None => Some(nav[0].id.clone()),
                Some(index) => {
                    let me = &nav[index];
                    let column = in_column(me.column);
                    let at = column.iter().position(|n| n.id == me.id).unwrap_or(0);
                    match key {
                        PanelKey::Left | PanelKey::Right if self.view == BoardView::Timeline => {
                            let next = if key == PanelKey::Left {
                                index.checked_sub(1)
                            } else {
                                Some(index + 1)
                            };
                            next.and_then(|i| nav.get(i)).map(|n| n.id.clone())
                        }
                        PanelKey::Up => column.get(at.wrapping_sub(1)).map(|n| n.id.clone()),
                        PanelKey::Down => column.get(at + 1).map(|n| n.id.clone()),
                        PanelKey::Home => column.first().map(|n| n.id.clone()),
                        PanelKey::End => column.last().map(|n| n.id.clone()),
                        PanelKey::Left | PanelKey::Right if self.last.mode == Mode::Tabs => {
                            let tabs = &self.last.tabs;
                            let i = tabs.iter().position(|c| *c == self.tab).unwrap_or(0);
                            let next = if key == PanelKey::Left {
                                i.checked_sub(1)
                            } else {
                                Some(i + 1)
                            };
                            if let Some(column) = next.and_then(|i| tabs.get(i)) {
                                self.tab = *column;
                                self.scroll = 0.0;
                                self.selected = None;
                            }
                            None
                        }
                        PanelKey::Left | PanelKey::Right => {
                            let step: isize = if key == PanelKey::Left { -1 } else { 1 };
                            let columns = nav.iter().map(|n| n.column).max().unwrap_or(0) as isize;
                            let mut c = me.column as isize + step;
                            let mut found = None;
                            while (0..=columns).contains(&c) && found.is_none() {
                                let candidates = in_column(c as usize);
                                let centre = me.y + me.h / 2.0;
                                let distance = |n: &&NavCard| {
                                    (n.y + n.h / 2.0 - centre).abs()
                                        + if n.lane == me.lane { 0.0 } else { 10_000.0 }
                                };
                                found = candidates
                                    .iter()
                                    .min_by(|a, b| distance(a).total_cmp(&distance(b)))
                                    .map(|n| n.id.clone());
                                c += step;
                            }
                            found
                        }
                        _ => None,
                    }
                }
            },
        };
        if let Some(id) = target {
            let open = self.detail_open;
            self.select(id, open, now)?;
        }
        Ok(self.redraw())
    }

    fn scroll_by(&mut self, x: f32, y: f32, lines: f32) -> PanelUpdate {
        let delta = lines * 16.0;
        let in_detail = self.last.detail.is_some_and(|r| r.contains(x, y));
        if in_detail || matches!(self.last.mode, Mode::Detail | Mode::CompactDetail) {
            self.detail_scroll = (self.detail_scroll + delta).max(0.0);
        } else {
            self.scroll = (self.scroll + delta).max(0.0);
        }
        self.redraw()
    }

    fn deliver_demo(&mut self, now: Instant) -> Result<PanelUpdate> {
        let events = match &mut self.backend {
            Backend::Demo(demo) => demo.due(now),
            Backend::Host => Vec::new(),
        };
        let mut update = PanelUpdate::default();
        for event in events {
            update.merge(self.handle(event, now)?);
        }
        Ok(update)
    }

    /// Event handling with an explicit clock (tests drive the demo with it).
    /// The board's header as data: the host draws it in the fence title strip or a
    /// window title bar, and the board draws only its content.
    pub fn chrome(&self) -> PanelChrome {
        let lang = self.lang();
        let (project, scope) = match &self.board {
            Some(board) => (&board.project_id, &board.delivery_scope_id),
            None => (&self.config.project_id, &self.config.delivery_scope_id),
        };
        let demo = match &self.backend {
            Backend::Demo(demo) => Some(demo.step()),
            Backend::Host => None,
        };
        let subtitle = demo.map_or_else(String::new, |step| {
            format!(
                "{} · {} · {}",
                lang.t("Demo data"),
                lang.f(
                    "Step {} of {}",
                    &[&(step + 1).to_string(), &demo::step_count().to_string()]
                ),
                demo::STEP_NOTES[step]
            )
        });
        let refreshing = self.refreshing();
        let degraded = self
            .board
            .as_ref()
            .map(model::degraded_sources)
            .unwrap_or_default();
        let status = if refreshing {
            Some(ChromeStatus {
                text: lang.t("Refreshing…").to_owned(),
                severity: ChromeSeverity::Informational,
            })
        } else {
            degraded.first().map(|system| ChromeStatus {
                text: lang.f("{} did not refresh", &[model::system_label(*system)]),
                severity: ChromeSeverity::Warning,
            })
        };
        let mut commands = vec![
            ChromeCommand {
                id: CHROME_REFRESH,
                label: lang.t("Refresh").to_owned(),
                glyph: '\u{E72C}',
                enabled: !refreshing,
            },
            ChromeCommand {
                id: CHROME_COPY,
                label: lang.t("Copy summary").to_owned(),
                glyph: '\u{E8C8}',
                enabled: self.board.is_some(),
            },
        ];
        if demo.is_some() {
            commands.push(ChromeCommand {
                id: CHROME_RESTART_DEMO,
                label: lang.t("Restart demo").to_owned(),
                glyph: '\u{E777}',
                enabled: true,
            });
        }
        PanelChrome {
            title: format!("{project} · {scope}"),
            subtitle,
            status,
            commands,
            surface: self.palette.ground,
        }
    }

    pub fn handle(&mut self, event: PanelEvent, now: Instant) -> Result<PanelUpdate> {
        let mut update = self.handle_event(event, now)?;
        let chrome = self.chrome();
        if self.published_chrome.as_ref() != Some(&chrome) {
            self.published_chrome = Some(chrome.clone());
            update.merge(PanelUpdate {
                commands: vec![HostCommand::SetChrome(chrome)],
                relayout: false,
            });
        }
        update.commands.append(&mut self.outbox);
        Ok(update)
    }

    fn handle_event(&mut self, event: PanelEvent, now: Instant) -> Result<PanelUpdate> {
        if self.stopped {
            return Err(Error::Closed);
        }
        match event {
            PanelEvent::Context { topic, .. } if topic == "board.timeline" => {
                self.view = BoardView::Timeline;
                Ok(self.redraw())
            }
            PanelEvent::Context { topic, json } if topic == "ci.snapshot" => {
                // A snapshot the monitor already validated; a bad one keeps the previous view.
                if let Ok(snapshot) = serde_json::from_str::<spm_contracts::CiWatchSnapshot>(&json)
                    && snapshot.validate().is_ok()
                {
                    self.ci = snapshot;
                    return Ok(self.redraw());
                }
                Ok(PanelUpdate::default())
            }
            PanelEvent::Inspect {
                action,
                layout_revision,
            } => {
                if layout_revision != self.layout_revision
                    || self.prepared_view_revision != self.view_revision
                {
                    return Ok(PanelUpdate::default());
                }
                let target = action
                    .and_then(|a| a.checked_sub(ACTION_BASE))
                    .and_then(|a| self.targets.get(a as usize));
                let preview = match target {
                    Some(Target::Event(id, e)) => Some((id.clone(), e.clone())),
                    _ => None,
                };
                if preview == self.preview_event {
                    return Ok(PanelUpdate::default());
                }
                self.preview_event = preview;
                if let Some((id, _)) = self.preview_event.clone() {
                    self.request_card_evidence(&id, now, false)?;
                }
                Ok(self.redraw())
            }
            PanelEvent::Command { id } => {
                let target = match id {
                    CHROME_REFRESH => Target::Refresh,
                    CHROME_COPY => Target::CopySummary,
                    CHROME_RESTART_DEMO => Target::RestartDemo,
                    _ => return Ok(PanelUpdate::default()),
                };
                self.invoke(target, now)
            }
            PanelEvent::Wake => {
                let mut update = self.deliver_demo(now)?;
                if matches!(self.backend, Backend::Host)
                    && self.board.is_none()
                    && now.duration_since(self.started) >= LIVE_WAIT
                {
                    update.merge(self.redraw());
                }
                Ok(update)
            }
            PanelEvent::Snapshot {
                subscription,
                bytes,
            } if subscription == self.subscription => {
                match self.handle_envelope(&bytes, now) {
                    Ok(update) => Ok(update),
                    Err(error) => {
                        // A rejected revision keeps the last good board.
                        if self.board.is_none() {
                            self.load_error = Some(error.to_string());
                        }
                        Ok(self.redraw())
                    }
                }
            }
            PanelEvent::Completion { operation, result } => {
                let Some(pending) = self.pending.remove(&operation) else {
                    return Ok(PanelUpdate::default());
                };
                self.handle_completion(pending, result)?;
                Ok(self.redraw())
            }
            PanelEvent::Invoke {
                action,
                layout_revision,
            } => {
                if layout_revision != self.layout_revision
                    || self.prepared_view_revision != self.view_revision
                {
                    return Ok(PanelUpdate::default());
                }
                let Some(target) = action
                    .checked_sub(ACTION_BASE)
                    .and_then(|i| self.targets.get(i as usize))
                    .cloned()
                else {
                    return Ok(PanelUpdate::default());
                };
                self.invoke(target, now)
            }
            PanelEvent::TextSubmitted {
                action: ACTION_FIND_TEXT,
                text,
            } => {
                let text = text.trim();
                self.filter.query = (!text.is_empty()).then(|| text.to_owned());
                self.scroll = 0.0;
                Ok(self.redraw())
            }
            PanelEvent::Key { key, .. } => self.key(key, now),
            PanelEvent::Scroll { x, y, lines } => Ok(self.scroll_by(x, y, lines)),
            PanelEvent::ThemeChanged { revision } => {
                if let Ok(snapshot) = self.ctx.theme.with(|theme| theme.snapshot())
                    && snapshot.revision == revision
                {
                    self.palette = Palette::from_theme(&snapshot);
                }
                Ok(self.redraw())
            }
            _ => Ok(PanelUpdate::default()),
        }
    }

    /// Build the frame with an explicit clock and measurer (tests use a heuristic).
    pub fn prepare(&mut self, input: FrameInput, measure: &dyn text::Measure) -> Result<Scene> {
        let mount_key = self.mount.ok_or(Error::Closed)?;
        self.locale = input.locale;
        let lang = self.lang();
        self.frame_revision += 1;
        let scale = ContentScale::new(input.viewport, input.text_scale);
        let viewport = scale.logical_viewport();
        let fitter = Fitter::new(measure, scale.factor(), &self.fit_cache);
        let demo = match &self.backend {
            Backend::Demo(demo) => Some((
                demo.step(),
                demo::step_count(),
                demo::STEP_NOTES[demo.step()],
            )),
            Backend::Host => None,
        };
        let now = match (&self.backend, &self.board) {
            (Backend::Demo(_), Some(board)) => board.computed_at,
            _ => chrono::Utc::now(),
        };
        let waiting = matches!(self.backend, Backend::Host)
            && self.board.is_none()
            && self.started.elapsed() >= LIVE_WAIT;
        let visible = input.exposure != Exposure::Hidden && input.active;
        let mut layout = Layout::default();
        // Clamp the scroll offsets to the content, then lay out again if they moved.
        for _ in 0..2 {
            let evidence = match (&self.board, &self.selected) {
                (Some(board), Some(id)) => match self.evidence.get(&(id.clone(), board.revision)) {
                    Some(Evidence::Loading) | None => EvidenceView::Loading,
                    Some(Evidence::Failed(e)) => EvidenceView::Failed(e),
                    Some(Evidence::Loaded(e)) => EvidenceView::Loaded(e),
                },
                _ => EvidenceView::None,
            };
            let status = self.status.as_ref().map(|(s, t)| (s.as_str(), *t));
            let layout_input = Input {
                board: self.board.as_ref(),
                ci: &self.ci,
                view: self.view,
                zoom: self.zoom,
                evidence_open: self.evidence_open,
                previous_error: self.load_error.as_deref(),
                loading_hint: waiting
                    .then(|| lang.t("Waiting for SPM. It has not sent a board yet.")),
                filter: &self.filter,
                selected: self.selected.as_ref(),
                detail_open: self.detail_open,
                collapsed: &self.collapsed,
                tab: self.tab,
                done_open: self.done_open,
                scroll: self.scroll,
                detail_scroll: self.detail_scroll,
                evidence,
                preview_event: self.preview_event.as_ref().map(|(_, e)| e.as_str()),
                preview_evidence: match (&self.board, &self.preview_event) {
                    (Some(board), Some((id, _))) => {
                        match self.evidence.get(&(id.clone(), board.revision)) {
                            Some(Evidence::Loaded(e)) => EvidenceView::Loaded(e),
                            Some(Evidence::Failed(e)) => EvidenceView::Failed(e),
                            _ => EvidenceView::Loading,
                        }
                    }
                    _ => EvidenceView::None,
                },
                selected_evidence: self.selected_evidence.as_deref(),
                status,
                demo,
                refreshing: self.refreshing(),
                capsule: input.presentation == Presentation::Capsule,
                now,
                lang,
            };
            layout = if visible {
                layout::build(&layout_input, viewport, &self.palette, &fitter)
            } else {
                Layout::default()
            };
            if self.reveal_after_layout && visible {
                self.reveal_after_layout = false;
                if let (Some(id), Some(region)) = (&self.selected, layout.board) {
                    if let Some(card) = layout.nav.iter().find(|n| &n.id == id) {
                        if card.y < self.scroll {
                            self.scroll = card.y.max(0.0);
                        } else if card.y + card.h > self.scroll + region.h {
                            self.scroll = (card.y + card.h - region.h + 8.0).max(0.0);
                        }
                    }
                }
                continue;
            }
            let max_scroll = layout
                .board
                .map_or(0.0, |r| (layout.board_content - r.h).max(0.0));
            let detail_region = layout.detail.map_or(0.0, |r| r.h - 120.0);
            let max_detail = (layout.detail_content - detail_region.max(0.0)).max(0.0);
            let (scroll, detail_scroll) = (
                self.scroll.min(max_scroll),
                self.detail_scroll.min(max_detail),
            );
            if scroll == self.scroll && detail_scroll == self.detail_scroll {
                break;
            }
            self.scroll = scroll;
            self.detail_scroll = detail_scroll;
        }
        self.find_bounds = layout
            .nodes
            .iter()
            .find(|n| n.key == "find:bg")
            .map(|n| scale.rect(n.rect));
        let nodes = self.scene_nodes(&layout.nodes, &scale, viewport);
        self.targets = layout.targets.clone();
        self.last = layout;
        let scene = Scene::new(
            FrameIdentity {
                mount_key,
                frame_revision: self.frame_revision,
                layout_revision: self.layout_revision,
                theme_epoch: input.theme_epoch,
                device_epoch: input.device_epoch,
            },
            input.viewport,
            nodes,
        )?;
        self.layout_revision = self
            .layout_tracker
            .update(scene.hit_tree(), self.view_revision)?;
        self.prepared_view_revision = self.view_revision;
        Ok(scene.with_layout_revision(self.layout_revision))
    }

    /// Stable scene ids per node key, never reused for another key.
    fn scene_nodes(
        &mut self,
        drafts: &[Draft],
        scale: &ContentScale,
        viewport: RectDip,
    ) -> Vec<SceneNode> {
        let mut nodes = vec![SceneNode {
            id: 1,
            parent: None,
            rect: scale.rect(viewport),
            role: "pane".into(),
            name: "Evidence board".into(),
            text: String::new(),
            action: None,
            style: NodeStyle {
                size: design::BODY,
                weight: 400,
                font: FontRole::Text,
                foreground: self.palette.primary,
                // Dense text needs a calm surface: other windows must not show through.
                fill: Some({
                    let [r, g, b, _] = self.palette.base;
                    [r, g, b, if self.palette.hc { 1.0 } else { 0.97 }]
                }),
                stroke: None,
                radius: 0.0,
                text_inset: [0.0; 2],
                align: TextAlign::TopLeading,
                path: None,
            },
        }];
        let mut seen = std::collections::HashSet::new();
        for draft in drafts {
            let id = match self.ids.get(&draft.key) {
                Some(id) => *id,
                None => {
                    let id = self.next_id;
                    self.next_id += 1;
                    self.ids.insert(draft.key.clone(), id);
                    id
                }
            };
            if !seen.insert(id) {
                continue;
            }
            let mut style = draft.style.clone();
            style.size *= scale.factor();
            style.radius *= scale.factor();
            nodes.push(SceneNode {
                id,
                parent: Some(1),
                rect: scale.rect(draft.rect),
                role: draft.role.into(),
                name: draft.name.clone(),
                text: draft.text.clone(),
                action: draft.action.map(|i| ACTION_BASE + i as u64),
                style,
            });
        }
        self.ids
            .retain(|key, _| drafts.iter().any(|d| &d.key == key));
        nodes
    }
}

impl PanelInstance for BoardPanel {
    fn mount(&mut self, ctx: MountContext) -> Result<()> {
        if ctx.key.instance != self.key {
            return Err(Error::Invalid("mount belongs to another instance".into()));
        }
        self.mount = Some(ctx.key);
        Ok(())
    }

    fn event(&mut self, event: PanelEvent) -> Result<PanelUpdate> {
        self.handle(event, Instant::now())
    }

    fn prepare_frame(&mut self, input: FrameInput) -> Result<Rc<dyn PreparedFrame>> {
        let render = self.ctx.render.clone();
        let measure = move |spec: &TextSpec, width: f32| -> Option<(f32, f32)> {
            render
                .with(|r| r.measure(spec, width))
                .ok()
                .map(|m| (m.width, m.height))
        };
        let host = HostMeasure(&measure);
        let scene = self.prepare(input, &host)?;
        Ok(Rc::new(scene))
    }

    fn unmount(&mut self, key: MountKey) {
        if self.mount == Some(key) {
            self.mount = None;
        }
    }

    fn begin_stop(&mut self, _reason: StopReason) {
        self.stopped = true;
        if matches!(self.backend, Backend::Host) {
            for (operation, _) in self.pending.drain() {
                let _ = self.ctx.ipc.with(|ipc| ipc.cancel(operation));
            }
            let _ = self.ctx.ipc.with(|ipc| ipc.cancel(self.subscription));
        } else if let Backend::Demo(demo) = &mut self.backend {
            for (operation, _) in self.pending.drain() {
                demo.cancel(operation);
            }
        }
    }

    fn next_wake_after(&self) -> Option<Duration> {
        let now = Instant::now();
        match &self.backend {
            Backend::Demo(demo) => demo.next_due(now),
            // One wake when the wait ends, to say that SPM has not answered.
            Backend::Host if self.board.is_none() => {
                LIVE_WAIT.checked_sub(now.duration_since(self.started))
            }
            Backend::Host => None,
        }
    }
}

#[cfg(test)]
mod tests;

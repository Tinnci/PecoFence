//! CI presentation only. SPM owns observation, attribution, ETA and local watchlists.
pub mod layout;
pub mod text;
use crate::board::{
    palette::Palette,
    text::{HostMeasure, Measure},
};
use pecofence_plugin_api::{
    design::{ContentScale, LayoutTracker},
    scene::Scene,
    *,
};
use serde::{Deserialize, Serialize};
use spm_contracts::*;
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use text::Lang;
fn palette(theme: &ThemeSnapshot) -> Palette {
    let mut p = Palette::from_theme(theme);
    if !theme.high_contrast {
        let paper = paper::resolve(Some(theme));
        p.success = paper.tone(paper::GOOD.0, paper::GOOD.1);
    }
    p
}
pub const PROVIDER_ID: &str = "pecofence.spm.ci-watch";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub demo: bool,
    #[serde(default = "timezone")]
    pub timezone: String,
    #[serde(default)]
    pub navigation_origins: crate::navigation::NavigationOrigins,
}
fn timezone() -> String {
    "Asia/Shanghai".into()
}
pub fn demo_config() -> (u16, serde_json::Value) {
    (
        1,
        serde_json::json!({"demo":true,"timezone":"Asia/Shanghai"}),
    )
}
pub fn live_config() -> (u16, serde_json::Value) {
    (
        1,
        serde_json::json!({"demo":false,"timezone":"Asia/Shanghai"}),
    )
}
pub fn demo_snapshot() -> CiWatchSnapshot {
    serde_json::from_str(include_str!("../../fixtures/ci-watch-demo.json"))
        .expect("verified deterministic CI fixture")
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Pipeline,
    Release,
    Watchlist,
}
impl View {
    fn shift(self, delta: i32) -> Self {
        const ORDER: [View; 3] = [View::Pipeline, View::Release, View::Watchlist];
        let i = ORDER.iter().position(|v| *v == self).unwrap_or(0) as i32 + delta;
        ORDER[i.clamp(0, ORDER.len() as i32 - 1) as usize]
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    View(View),
    Find,
    Add,
    Suggestion(usize),
    Toggle(String, CiWatchFlag, bool),
    Role(String, CiJobRole),
    Remove(String),
    Open(String, String, SourceSystem),
}
pub struct CiWatchPlugin;
impl PanelProvider for CiWatchPlugin {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: PROVIDER_ID,
            api_major: 1,
            config_major: 1,
            required_services: &["render", "theme", "ipc"],
        }
    }
    fn validate(&self, config: &PanelConfig) -> Result<()> {
        if config.version != 1 {
            return Err(Error::Invalid("unsupported CI config version".into()));
        }
        let parsed: Config =
            serde_json::from_slice(&config.bytes).map_err(|e| Error::Invalid(e.to_string()))?;
        if parsed.timezone.len() > 64 {
            return Err(Error::Invalid("invalid timezone".into()));
        }
        crate::navigation::validate_origins(&parsed.navigation_origins)
    }
    fn create(&self, ctx: PluginContext, input: CreatePanel) -> Result<Box<dyn PanelInstance>> {
        self.validate(&input.config)?;
        let config = serde_json::from_slice(&input.config.bytes)
            .map_err(|e| Error::Invalid(e.to_string()))?;
        Ok(Box::new(Panel::new(ctx, input.key, config)?))
    }
}
pub struct Panel {
    ctx: PluginContext,
    key: InstanceKey,
    config: Config,
    mount: Option<MountKey>,
    snapshot: Option<CiWatchSnapshot>,
    suggestions: CiSuggestions,
    view: View,
    query: String,
    selected_suggestion: Option<usize>,
    focus: Option<String>,
    scroll: f32,
    content_height: f32,
    viewport_height: f32,
    field: RectDip,
    palette: Palette,
    locale: &'static str,
    actions: Vec<(String, Target, RectDip)>,
    ids: HashMap<String, u64>,
    next_id: u64,
    tracker: LayoutTracker,
    layout_revision: u64,
    view_revision: u64,
    prepared_view: u64,
    frame_revision: u64,
    published_chrome: bool,
    chrome_warmup_until: Instant,
    pending: HashSet<Token>,
    last_refresh: Instant,
    stopped: bool,
    status: Option<String>,
}
impl Panel {
    pub fn new(ctx: PluginContext, key: InstanceKey, config: Config) -> Result<Self> {
        let theme = ctx
            .theme
            .with(|t| t.snapshot())
            .unwrap_or_else(|_| Arc::new(ThemeSnapshot::dark()));
        let mut panel = Self {
            ctx,
            key,
            snapshot: config.demo.then(demo_snapshot),
            config,
            mount: None,
            suggestions: CiSuggestions::default(),
            view: View::Pipeline,
            query: String::new(),
            selected_suggestion: None,
            focus: None,
            scroll: 0.0,
            content_height: 0.0,
            viewport_height: 780.0,
            field: RectDip::default(),
            palette: palette(&theme),
            locale: "zh-CN",
            actions: vec![],
            ids: HashMap::new(),
            next_id: 2,
            tracker: LayoutTracker::default(),
            layout_revision: 0,
            view_revision: 1,
            prepared_view: 0,
            frame_revision: 0,
            published_chrome: false,
            chrome_warmup_until: Instant::now() + Duration::from_secs(3),
            pending: HashSet::new(),
            last_refresh: Instant::now(),
            stopped: false,
            status: None,
        };
        if !panel.config.demo {
            panel.send(Request::GetCiWatch(GetCiWatchRequest::default()))?;
        }
        Ok(panel)
    }
    fn lang(&self) -> Lang {
        Lang::new(self.locale)
    }
    fn changed(&mut self) {
        self.view_revision += 1;
    }
    fn send(&mut self, request: Request) -> Result<()> {
        let bytes = crate::encode_request(&request)?;
        match self.ctx.ipc.with(|ipc| ipc.send(&self.ctx.scope, bytes)) {
            Ok(token) => {
                self.pending.insert(token);
            }
            Err(_) => {
                self.status = Some(self.lang().t("Refresh failed").into());
            }
        }
        Ok(())
    }
    pub fn cadence(&self) -> Duration {
        Duration::from_secs(
            if self.snapshot.as_ref().is_some_and(|s| {
                s.my_builds
                    .iter()
                    .chain(s.jobs.iter().filter_map(|j| j.last.as_ref()))
                    .chain(
                        s.branches
                            .iter()
                            .flat_map(|b| b.changes.iter().filter_map(|c| c.ci.as_ref())),
                    )
                    .any(|b| b.state == CiBuildState::Running)
            }) {
                30
            } else {
                120
            },
        )
    }
    fn refresh(&mut self, force: bool) -> Result<()> {
        if !self.config.demo && self.pending.is_empty() {
            self.send(Request::GetCiWatch(GetCiWatchRequest { refresh: force }))?;
        }
        self.last_refresh = Instant::now();
        Ok(())
    }
    fn suggestions(&mut self) -> Result<()> {
        validate_ci_query(&self.query).map_err(|e| Error::Invalid(e.to_string()))?;
        self.selected_suggestion = None;
        if self.config.demo {
            let candidates = [
                (CiSuggestionKind::Job, "orion/carmode-lc3-smoke"),
                (CiSuggestionKind::Job, "orion/carmode-full"),
                (CiSuggestionKind::Branch, "orion/carmode:master"),
            ];
            self.suggestions = CiSuggestions {
                items: candidates
                    .into_iter()
                    .filter(|(_, value)| value.contains(&self.query))
                    .map(|(kind, value)| CiSuggestion {
                        kind,
                        value: value.into(),
                        label: value.into(),
                        already_watched: false,
                    })
                    .collect(),
                truncated: false,
            };
            self.selected_suggestion = (!self.suggestions.items.is_empty()).then_some(0);
        } else {
            self.send(Request::SuggestCiWatch(SuggestCiWatchRequest {
                query: self.query.clone(),
            }))?;
        }
        Ok(())
    }
    fn edit(&mut self, edit: UpdateCiWatchlistRequest) -> Result<()> {
        if self.config.demo {
            if let Some(snapshot) = &mut self.snapshot {
                let mut list = snapshot.watchlist();
                list.apply(edit)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                snapshot.apply_watchlist(&list);
            }
        } else {
            self.send(Request::UpdateCiWatchlist(edit))?;
        }
        Ok(())
    }
    fn add(&mut self) -> Result<()> {
        let Some(suggestion) = self
            .selected_suggestion
            .and_then(|i| self.suggestions.items.get(i))
            .cloned()
        else {
            return Ok(());
        };
        if suggestion.already_watched {
            return Ok(());
        }
        let edit = match suggestion.kind {
            CiSuggestionKind::Job => UpdateCiWatchlistRequest::AddJob {
                job: suggestion.value,
            },
            CiSuggestionKind::Branch => {
                let (project, branch) = suggestion
                    .value
                    .split_once(':')
                    .ok_or_else(|| Error::Invalid("missing project:branch".into()))?;
                UpdateCiWatchlistRequest::AddBranch {
                    project: project.into(),
                    branch: branch.into(),
                }
            }
        };
        self.edit(edit)?;
        self.query.clear();
        self.suggestions = CiSuggestions::default();
        self.selected_suggestion = None;
        Ok(())
    }
    fn invoke(&mut self, target: Target) -> Result<PanelUpdate> {
        match target {
            Target::View(view) => {
                self.view = view;
                self.scroll = 0.0;
                self.focus = None;
            }
            Target::Find => {
                let mut style =
                    TextInputStyle::new(self.field, self.palette.primary, self.palette.card, 13.0);
                style.accent = Some(self.palette.accent);
                style.placeholder = Some(self.lang().t("Job path or project:branch"));
                return Ok(PanelUpdate {
                    commands: vec![HostCommand::RequestTextInput {
                        action: 3,
                        initial: self.query.clone(),
                        multiline: false,
                        style: Some(style),
                    }],
                    relayout: false,
                });
            }
            Target::Add => self.add()?,
            Target::Suggestion(i) => {
                self.selected_suggestion = Some(i);
                self.focus = self
                    .suggestions
                    .items
                    .get(i)
                    .map(|s| format!("suggest:{}", s.value));
            }
            Target::Toggle(id, flag, value) => {
                self.edit(UpdateCiWatchlistRequest::Toggle { id, flag, value })?
            }
            Target::Role(id, role) => self.edit(UpdateCiWatchlistRequest::SetRole { id, role })?,
            Target::Remove(id) => self.edit(UpdateCiWatchlistRequest::Remove { id })?,
            Target::Open(origin, path, system) => {
                if crate::navigation::open(
                    &self.ctx,
                    &self.config.navigation_origins,
                    &[NavigationTarget {
                        system,
                        configured_origin_id: origin,
                        relative_record_path: path,
                    }],
                )
                .is_err()
                {
                    self.status = Some(self.lang().t("Open source unavailable").into());
                }
            }
        }
        self.changed();
        Ok(self.update())
    }
    fn update(&mut self) -> PanelUpdate {
        self.published_chrome = true;
        PanelUpdate {
            commands: vec![
                HostCommand::Invalidate,
                HostCommand::SetChrome(self.chrome()),
            ],
            relayout: true,
        }
    }
    pub fn chrome(&self) -> PanelChrome {
        let lang = self.lang();
        let failures = self.snapshot.as_ref().map_or(0, |s| {
            s.branches
                .iter()
                .flat_map(|b| b.changes.iter())
                .filter(|c| c.pipeline.iter().any(|st| st.state == CiStageState::Failed))
                .count()
        });
        let gaps = self
            .snapshot
            .as_ref()
            .is_some_and(|s| s.sources.iter().any(|s| s.health != WatchHealth::Ok));
        let status = if let Some(text) = &self.status {
            Some(ChromeStatus {
                text: text.clone(),
                severity: ChromeSeverity::Warning,
            })
        } else if failures > 0 {
            Some(ChromeStatus {
                text: format!("× {failures} {}", if lang.zh { "失败" } else { "failed" }),
                severity: ChromeSeverity::Error,
            })
        } else if gaps {
            Some(ChromeStatus {
                text: lang.t("Partial results").into(),
                severity: ChromeSeverity::Warning,
            })
        } else {
            None
        };
        let subtitle = self.snapshot.as_ref().map_or_else(
            || lang.t("Loading CI…").into(),
            |s| {
                format!(
                    "{} · {}{}",
                    s.user.jenkins_user,
                    s.observed_through.map_or_else(
                        || lang.t("Not observed").into(),
                        |at| text::freshness(
                            at,
                            if self.config.demo {
                                at
                            } else {
                                chrono::Utc::now()
                            },
                            lang
                        )
                    ),
                    if self.config.demo {
                        format!(" · {}", lang.t("Demo data"))
                    } else {
                        String::new()
                    }
                )
            },
        );
        PanelChrome {
            title: lang.t("CI/CD").into(),
            subtitle,
            status,
            commands: vec![
                ChromeCommand {
                    id: 1,
                    label: lang.t("Refresh").into(),
                    glyph: '\u{E72C}',
                    enabled: self.pending.is_empty(),
                },
                ChromeCommand {
                    id: 2,
                    label: lang.t("Watchlist").into(),
                    glyph: '\u{E713}',
                    enabled: true,
                },
            ],
            surface: self.palette.ground,
        }
    }
    fn completion(&mut self, bytes: &[u8]) -> Result<()> {
        let envelope = crate::decode_event(bytes)?;
        match envelope.body {
            Body::Response(
                Response::GetCiWatch(snapshot) | Response::UpdateCiWatchlist(snapshot),
            ) => {
                snapshot
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                self.snapshot = Some(snapshot);
                self.status = None;
            }
            Body::Response(Response::SuggestCiWatch(suggestions)) => {
                suggestions
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                self.selected_suggestion = (!suggestions.items.is_empty()).then_some(0);
                self.suggestions = suggestions;
            }
            Body::Error(_) => self.status = Some(self.lang().t("Refresh failed").into()),
            _ => {}
        }
        self.changed();
        Ok(())
    }
    pub fn handle(&mut self, event: PanelEvent) -> Result<PanelUpdate> {
        if self.stopped {
            return Err(Error::Closed);
        }
        match event {
            PanelEvent::Command { id: 1 } => {
                self.refresh(true)?;
                self.changed();
            }
            PanelEvent::Command { id: 2 } => {
                return self.invoke(Target::View(if self.view == View::Watchlist {
                    View::Pipeline
                } else {
                    View::Watchlist
                }));
            }
            PanelEvent::Wake => {
                if self.last_refresh.elapsed() >= self.cadence() {
                    self.refresh(false)?;
                    self.changed();
                }
            }
            PanelEvent::Invoke {
                action,
                layout_revision,
            } => {
                if layout_revision == self.layout_revision
                    && self.prepared_view == self.view_revision
                {
                    if let Some((key, target, _)) = action
                        .checked_sub(16)
                        .and_then(|i| self.actions.get(i as usize))
                        .cloned()
                    {
                        self.focus = Some(key);
                        return self.invoke(target);
                    }
                }
            }
            PanelEvent::Completion { operation, result } => {
                if self.pending.remove(&operation) {
                    if result.and_then(|bytes| self.completion(&bytes)).is_err() {
                        self.status = Some(self.lang().t("Refresh failed").into());
                    }
                    self.changed();
                }
            }
            PanelEvent::TextSubmitted { action: 3, text } => {
                self.query = text.trim().into();
                self.scroll = 0.0;
                self.suggestions()?;
                self.changed();
            }
            PanelEvent::Key { key, .. } => {
                match key {
                    PanelKey::Escape => {
                        if self.view == View::Watchlist {
                            self.view = View::Pipeline;
                        }
                        self.focus = None;
                        self.scroll = 0.0;
                    }
                    PanelKey::Refresh => self.refresh(true)?,
                    PanelKey::Left | PanelKey::Right => {
                        let delta = if key == PanelKey::Left { -1 } else { 1 };
                        self.view = self.view.shift(delta);
                        self.scroll = 0.0;
                        self.focus = None;
                    }
                    PanelKey::Enter => {
                        if let Some((_, target, _)) = self
                            .actions
                            .iter()
                            .find(|(key, _, _)| Some(key) == self.focus.as_ref())
                            .cloned()
                        {
                            if let Target::Suggestion(i) = target {
                                self.selected_suggestion = Some(i);
                                return self.invoke(Target::Add);
                            }
                            return self.invoke(target);
                        }
                    }
                    PanelKey::Up | PanelKey::Down => {
                        let index = self
                            .actions
                            .iter()
                            .position(|(k, _, _)| Some(k) == self.focus.as_ref());
                        let next = if key == PanelKey::Down {
                            index.map_or(0, |i| (i + 1).min(self.actions.len().saturating_sub(1)))
                        } else {
                            index.unwrap_or(1).saturating_sub(1)
                        };
                        self.focus = self.actions.get(next).map(|(k, _, _)| k.clone());
                        if let Some((_, Target::Suggestion(i), _)) = self.actions.get(next) {
                            self.selected_suggestion = Some(*i);
                        }
                    }
                    PanelKey::PageDown => {
                        self.scroll = (self.scroll + self.viewport_height * 0.6)
                            .min((self.content_height - self.viewport_height + 82.0).max(0.0))
                    }
                    PanelKey::PageUp => {
                        self.scroll = (self.scroll - self.viewport_height * 0.6).max(0.0)
                    }
                    PanelKey::Home => self.scroll = 0.0,
                    PanelKey::End => {
                        self.scroll = (self.content_height - self.viewport_height + 82.0).max(0.0)
                    }
                }
                self.changed();
            }
            PanelEvent::Scroll { lines, .. } => {
                self.scroll = (self.scroll + lines * 36.0).clamp(
                    0.0,
                    (self.content_height - self.viewport_height + 82.0).max(0.0),
                );
                self.changed();
            }
            PanelEvent::ThemeChanged { .. } => {
                if let Ok(theme) = self.ctx.theme.with(|t| t.snapshot()) {
                    self.palette = palette(&theme);
                }
                self.changed();
            }
            _ => {}
        }
        Ok(self.update())
    }
    pub fn prepare(&mut self, input: FrameInput, measure: &dyn Measure) -> Result<Scene> {
        let mount_key = self.mount.ok_or(Error::Closed)?;
        self.locale = input.locale;
        self.frame_revision += 1;
        let scale = ContentScale::new(input.viewport, input.text_scale);
        let viewport = scale.logical_viewport();
        self.viewport_height = viewport.h;
        let mut layout = layout::build(
            self.snapshot.as_ref(),
            &self.suggestions,
            self.view,
            &self.query,
            self.focus.as_deref(),
            self.scroll,
            &self.config,
            self.lang(),
            viewport,
            &self.palette,
            measure,
            &mut self.ids,
            &mut self.next_id,
        );
        self.content_height = layout.content_height;
        self.field = scale.rect(layout.field);
        self.actions = layout.actions;
        for node in &mut layout.nodes {
            node.rect = scale.rect(node.rect);
            node.style.size *= scale.factor();
            node.style.radius *= scale.factor();
        }
        let scene = Scene::new(
            FrameIdentity {
                mount_key,
                frame_revision: self.frame_revision,
                layout_revision: self.layout_revision,
                theme_epoch: input.theme_epoch,
                device_epoch: input.device_epoch,
            },
            input.viewport,
            layout.nodes,
        )?;
        self.layout_revision = self.tracker.update(scene.hit_tree(), self.view_revision)?;
        self.prepared_view = self.view_revision;
        Ok(scene.with_layout_revision(self.layout_revision))
    }
}
impl PanelInstance for Panel {
    fn mount(&mut self, ctx: MountContext) -> Result<()> {
        if ctx.key.instance != self.key {
            return Err(Error::Invalid("wrong CI instance".into()));
        }
        self.mount = Some(ctx.key);
        Ok(())
    }
    fn event(&mut self, event: PanelEvent) -> Result<PanelUpdate> {
        self.handle(event)
    }
    fn prepare_frame(&mut self, input: FrameInput) -> Result<Rc<dyn PreparedFrame>> {
        let render = self.ctx.render.clone();
        let measure = move |spec: &TextSpec, width: f32| {
            render
                .with(|r| r.measure(spec, width))
                .ok()
                .map(|m| (m.width, m.height))
        };
        Ok(Rc::new(self.prepare(input, &HostMeasure(&measure))?))
    }
    fn unmount(&mut self, key: MountKey) {
        if self.mount == Some(key) {
            self.mount = None;
        }
    }
    fn begin_stop(&mut self, _: StopReason) {
        self.stopped = true;
        for token in self.pending.drain() {
            let _ = self.ctx.ipc.with(|ipc| ipc.cancel(token));
        }
    }
    fn next_wake_after(&self) -> Option<Duration> {
        // The first host wake can precede the fence HWND. Publish again after it
        // mounts, so an early chrome update cannot leave the generic title behind.
        if !self.stopped && Instant::now() < self.chrome_warmup_until {
            return Some(Duration::from_millis(500));
        }
        if !self.published_chrome && !self.stopped {
            return Some(Duration::from_millis(10));
        }
        (!self.config.demo && !self.stopped).then(|| {
            self.cadence()
                .saturating_sub(self.last_refresh.elapsed())
                .max(Duration::from_secs(1))
        })
    }
}
#[cfg(test)]
mod tests;

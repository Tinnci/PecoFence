//! Read-only delivery plans. Calculations and evidence remain in SPM.
use crate::board::{
    palette::Palette,
    text::{Fitter, HostMeasure, Measure},
};
use crate::scene_builder::{Builder, r};
use pecofence_plugin_api::{design::ContentScale, scene::Scene, *};
use serde::{Deserialize, Serialize};
use spm_contracts::*;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
pub const PROVIDER_ID: &str = "pecofence.spm.planning";
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub demo: bool,
    pub selection: Option<Selection>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub project_id: ProjectId,
    pub delivery_scope_id: DeliveryScopeId,
    pub plan_id: String,
}
pub fn live_config() -> (u16, serde_json::Value) {
    (1, serde_json::json!({"selection":null}))
}
pub fn demo_snapshot() -> DeliveryPlanView {
    serde_json::from_str(demo_fixture!("../fixtures/planning-demo.json"))
        .expect("synthetic plan fixture")
}
pub fn demo_config() -> (u16, serde_json::Value) {
    (1, serde_json::json!({"selection":null,"demo":true}))
}
pub struct PlanningPlugin;
impl PanelProvider for PlanningPlugin {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: PROVIDER_ID,
            api_major: 1,
            config_major: 1,
            required_services: &["render", "theme", "ipc"],
        }
    }
    fn validate(&self, c: &PanelConfig) -> Result<()> {
        if c.version != 1 {
            return Err(Error::Invalid("unsupported planning config".into()));
        }
        let config: Config =
            serde_json::from_slice(&c.bytes).map_err(|e| Error::Invalid(e.to_string()))?;
        crate::validate_demo(config.demo)?;
        if let Some(s) = config.selection {
            request(&s, None, 0)
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
        }
        Ok(())
    }
    fn create(&self, ctx: PluginContext, input: CreatePanel) -> Result<Box<dyn PanelInstance>> {
        self.validate(&input.config)?;
        let config: Config = serde_json::from_slice(&input.config.bytes)
            .map_err(|e| Error::Invalid(e.to_string()))?;
        Ok(Box::new(Panel {
            ctx,
            key: input.key,
            config: config.clone(),
            mount: None,
            pending: None,
            assembling: None,
            snapshot: config
                .demo
                .then(|| PlanSnapshot::from_page(demo_snapshot())),
            status: None,
            scroll: 0.0,
            height: 0.0,
            viewport: 0.0,
            revision: 0,
            stopped: false,
            chrome_published: false,
            chrome_warmup_until: Instant::now() + Duration::from_secs(3),
            last: Instant::now() - Duration::from_secs(120),
        }))
    }
}
fn request(s: &Selection, rev: Option<u64>, offset: u32) -> GetDeliveryPlanRequest {
    GetDeliveryPlanRequest {
        project_id: s.project_id.clone(),
        delivery_scope_id: s.delivery_scope_id.clone(),
        plan_id: s.plan_id.clone(),
        plan_revision: rev,
        offset,
        limit: 200,
    }
}
/// A refresh is published atomically after all pages share one immutable plan revision.
#[derive(Default)]
struct Pages {
    view: Option<PlanSnapshot>,
    next_offset: Option<u32>,
}
impl Pages {
    fn accept(
        &mut self,
        page: DeliveryPlanView,
        expected: &GetDeliveryPlanRequest,
    ) -> Result<bool> {
        page.validate().map_err(|e| Error::Invalid(e.to_string()))?;
        if page.project_id != expected.project_id
            || page.delivery_scope_id != expected.delivery_scope_id
            || page.plan_id != expected.plan_id
            || page.offset != expected.offset
            || expected
                .plan_revision
                .is_some_and(|n| n != page.plan_revision)
        {
            return Err(Error::Invalid(
                "planning page scope/revision mismatch".into(),
            ));
        }
        if let Some(v) = &mut self.view {
            let mut summary = v.page.clone();
            summary.tasks = page.tasks.clone();
            summary.offset = page.offset;
            summary.next_offset = page.next_offset;
            summary.resource_waits = page.resource_waits.clone();
            if page.offset as usize != v.tasks.len()
                || v.resource_waits.len() + page.resource_waits.len() > 32000
                || summary != page
                || v.tasks
                    .iter()
                    .any(|t| page.tasks.iter().any(|n| n.id == t.id))
            {
                return Err(Error::Invalid("inconsistent planning pages".into()));
            }
            v.tasks.extend(page.tasks);
            v.resource_waits.extend(page.resource_waits);
            self.next_offset = page.next_offset;
        } else {
            if page.offset != 0 {
                return Err(Error::Invalid("missing first plan page".into()));
            }
            self.next_offset = page.next_offset;
            self.view = Some(PlanSnapshot::from_page(page));
        }
        Ok(self.next_offset.is_none())
    }
}
/// Local aggregate. Wire pages remain valid and are never repurposed as oversized envelopes.
struct PlanSnapshot {
    page: DeliveryPlanView,
    tasks: Vec<PlanTaskView>,
    resource_waits: Vec<PlanResourceWait>,
}
impl PlanSnapshot {
    fn from_page(page: DeliveryPlanView) -> Self {
        Self {
            tasks: page.tasks.clone(),
            resource_waits: page.resource_waits.clone(),
            page,
        }
    }
}
struct Panel {
    ctx: PluginContext,
    key: InstanceKey,
    config: Config,
    mount: Option<MountKey>,
    pending: Option<(Token, Option<GetDeliveryPlanRequest>)>,
    assembling: Option<Pages>,
    snapshot: Option<PlanSnapshot>,
    status: Option<String>,
    scroll: f32,
    height: f32,
    viewport: f32,
    revision: u64,
    stopped: bool,
    last: Instant,
    chrome_published: bool,
    chrome_warmup_until: Instant,
}
impl Panel {
    fn send(&mut self, request: Request, page: Option<GetDeliveryPlanRequest>) -> Result<()> {
        let token = self
            .ctx
            .ipc
            .with(|ipc| ipc.send(&self.ctx.scope, crate::encode_request(&request)?))?;
        self.pending = Some((token, page));
        Ok(())
    }
    fn refresh(&mut self) -> Result<()> {
        if !self.config.demo && self.pending.is_none() && self.config.selection.is_some() {
            self.assembling = Some(Pages::default());
            self.send(Request::GetCapabilities(GetCapabilitiesRequest {}), None)?;
            self.last = Instant::now();
        }
        Ok(())
    }
    fn completion(&mut self, bytes: &[u8], expected: Option<GetDeliveryPlanRequest>) -> Result<()> {
        match crate::decode_event(bytes)?.body {
            Body::Response(Response::GetCapabilities(c)) if expected.is_none() => {
                if !c.methods.contains(&RpcMethod::GetDeliveryPlan) {
                    return Err(Error::Invalid(
                        "Backend does not support delivery plans".into(),
                    ));
                }
                let q = request(
                    self.config.selection.as_ref().ok_or(Error::Closed)?,
                    None,
                    0,
                );
                self.send(Request::GetDeliveryPlan(q.clone()), Some(q))?;
            }
            Body::Response(Response::GetDeliveryPlan(page)) => {
                let q = expected
                    .ok_or_else(|| Error::Invalid("unexpected planning response".into()))?;
                let pages = self.assembling.as_mut().ok_or(Error::Closed)?;
                if pages.accept(page, &q)? {
                    self.snapshot = pages.view.take();
                    self.assembling = None;
                    self.status = None;
                } else {
                    let v = pages.view.as_ref().unwrap();
                    let q = request(
                        self.config.selection.as_ref().ok_or(Error::Closed)?,
                        Some(v.page.plan_revision),
                        pages.next_offset.unwrap(),
                    );
                    self.send(Request::GetDeliveryPlan(q.clone()), Some(q))?;
                }
            }
            Body::Error(_) => {
                return Err(Error::Invalid("Plan unavailable in selected scope".into()));
            }
            _ => return Err(Error::Invalid("Unexpected planning response".into())),
        }
        Ok(())
    }
    fn update(&self) -> PanelUpdate {
        PanelUpdate {
            commands: vec![
                HostCommand::Invalidate,
                HostCommand::SetChrome(PanelChrome {
                    title: if self.config.demo {
                        "交付计划 · 演示"
                    } else {
                        "交付计划"
                    }
                    .into(),
                    subtitle: self
                        .snapshot
                        .as_ref()
                        .map(|snapshot| {
                            let v = &snapshot.page;
                            format!(
                                "{} · {} · r{} · UTC",
                                v.project_id.as_str(),
                                v.plan_id,
                                v.plan_revision
                            )
                        })
                        .unwrap_or_default(),
                    status: self.status.as_ref().map(|s| ChromeStatus {
                        text: s.clone(),
                        severity: ChromeSeverity::Warning,
                    }),
                    commands: vec![
                        ChromeCommand {
                            id: 1,
                            label: "刷新".into(),
                            glyph: '\u{E72C}',
                            enabled: self.pending.is_none(),
                        },
                        ChromeCommand {
                            id: 2,
                            label: "选择计划".into(),
                            glyph: '\u{E713}',
                            enabled: true,
                        },
                    ],
                    surface: self.palette().ground,
                }),
            ],
            relayout: true,
        }
    }
    fn palette(&self) -> Palette {
        Palette::from_theme(
            &self
                .ctx
                .theme
                .with(|t| t.snapshot())
                .unwrap_or_else(|_| Arc::new(ThemeSnapshot::dark())),
        )
    }
}
impl PanelInstance for Panel {
    fn mount(&mut self, c: MountContext) -> Result<()> {
        if c.key.instance != self.key {
            return Err(Error::Invalid("wrong plan instance".into()));
        }
        self.mount = Some(c.key);
        if self.refresh().is_err() {
            self.status = Some("无法读取计划".into());
        }
        Ok(())
    }
    fn event(&mut self, event: PanelEvent) -> Result<PanelUpdate> {
        if self.stopped {
            return Err(Error::Closed);
        }
        self.chrome_published = true;
        match event {
            PanelEvent::Command { id: 1 } => {
                if self.refresh().is_err() {
                    self.status = Some("无法读取计划".into());
                }
            }
            PanelEvent::Command { id: 2 } => {
                return Ok(PanelUpdate {
                    commands: vec![HostCommand::RequestTextInput {
                        action: 2,
                        initial: self
                            .config
                            .selection
                            .as_ref()
                            .map(|s| {
                                format!(
                                    "{} {} {}",
                                    s.project_id.as_str(),
                                    s.delivery_scope_id.as_str(),
                                    s.plan_id
                                )
                            })
                            .unwrap_or_default(),
                        multiline: false,
                        style: None,
                    }],
                    relayout: false,
                });
            }
            PanelEvent::TextSubmitted { action: 2, text } => {
                let values: Vec<_> = text.split_whitespace().collect();
                if values.len() != 3 {
                    self.status = Some("输入：项目 交付范围 计划ID（空格分隔）".into());
                } else {
                    let parsed = (|| {
                        let s = Selection {
                            project_id: ProjectId::new(values[0])
                                .map_err(|e| Error::Invalid(e.to_string()))?,
                            delivery_scope_id: DeliveryScopeId::new(values[1])
                                .map_err(|e| Error::Invalid(e.to_string()))?,
                            plan_id: values[2].into(),
                        };
                        request(&s, None, 0)
                            .validate()
                            .map_err(|e| Error::Invalid(e.to_string()))?;
                        Ok::<_, Error>(s)
                    })();
                    match parsed {
                        Ok(s) => {
                            if let Some((t, _)) = self.pending.take() {
                                let _ = self.ctx.ipc.with(|i| i.cancel(t));
                            }
                            self.config.demo = false;
                            self.config.selection = Some(s);
                            self.snapshot = None;
                            self.assembling = None;
                            self.scroll = 0.0;
                            self.status = None;
                            let mut update = self.update();
                            update.commands.push(HostCommand::PersistConfig(
                                serde_json::to_vec(&self.config)
                                    .map_err(|e| Error::Invalid(e.to_string()))?
                                    .into(),
                            ));
                            if self.refresh().is_err() {
                                self.status = Some("无法读取计划".into());
                            }
                            return Ok(update);
                        }
                        Err(_) => self.status = Some("项目、交付范围或计划ID无效".into()),
                    }
                }
            }
            PanelEvent::Completion { operation, result } => {
                if self.pending.as_ref().is_some_and(|(t, _)| *t == operation) {
                    let (_, expected) = self.pending.take().unwrap();
                    if result.and_then(|b| self.completion(&b, expected)).is_err() {
                        self.status = Some("计划读取失败；保留上次版本".into());
                        self.assembling = None;
                    }
                }
            }
            PanelEvent::Wake => {
                if self.last.elapsed() >= Duration::from_secs(120) && self.refresh().is_err() {
                    self.status = Some("无法读取计划".into());
                }
            }
            PanelEvent::Scroll { lines, .. } => {
                self.scroll =
                    (self.scroll - lines * 32.0).clamp(0.0, (self.height - self.viewport).max(0.0));
            }
            _ => {}
        }
        Ok(self.update())
    }
    fn prepare_frame(&mut self, input: FrameInput) -> Result<Rc<dyn PreparedFrame>> {
        self.revision += 1;
        let render = self.ctx.render.clone();
        let measure = move |s: &TextSpec, w: f32| {
            render
                .with(|r| r.measure(s, w))
                .ok()
                .map(|m| (m.width, m.height))
        };
        let scale = ContentScale::new(input.viewport, input.text_scale);
        let viewport = scale.logical_viewport();
        self.viewport = viewport.h;
        let (mut nodes, height) = layout(
            self.snapshot.as_ref(),
            viewport,
            self.scroll,
            &self.palette(),
            &HostMeasure(&measure),
        );
        self.height = height;
        for n in &mut nodes {
            n.rect = scale.rect(n.rect);
            n.style.size *= scale.factor();
            n.style.radius *= scale.factor();
        }
        Ok(Rc::new(Scene::new(
            FrameIdentity {
                mount_key: self.mount.ok_or(Error::Closed)?,
                frame_revision: self.revision,
                layout_revision: self.revision,
                theme_epoch: input.theme_epoch,
                device_epoch: input.device_epoch,
            },
            input.viewport,
            nodes,
        )?))
    }
    fn unmount(&mut self, k: MountKey) {
        if self.mount == Some(k) {
            self.mount = None;
        }
    }
    fn begin_stop(&mut self, _: StopReason) {
        self.stopped = true;
        if let Some((t, _)) = self.pending.take() {
            let _ = self.ctx.ipc.with(|i| i.cancel(t));
        }
    }
    fn next_wake_after(&self) -> Option<Duration> {
        if !self.stopped && Instant::now() < self.chrome_warmup_until {
            return Some(Duration::from_millis(500));
        }
        if !self.stopped && !self.chrome_published {
            return Some(Duration::from_millis(10));
        }
        (!self.stopped && self.config.selection.is_some()).then(|| {
            Duration::from_secs(120)
                .saturating_sub(self.last.elapsed())
                .max(Duration::from_secs(1))
        })
    }
}
fn date(v: &DeliveryPlanView, t: Option<u32>) -> String {
    t.and_then(|n| {
        v.epoch_utc.checked_add_signed(chrono::Duration::minutes(
            i64::from(n) * i64::from(v.slot_minutes),
        ))
    })
    .map(|d| d.format("%m-%d %H:%M").to_string())
    .unwrap_or_else(|| "未知".into())
}
fn layout(
    snapshot: Option<&PlanSnapshot>,
    viewport: RectDip,
    scroll: f32,
    p: &Palette,
    measure: &dyn Measure,
) -> (Vec<scene::SceneNode>, f32) {
    let mut ids = HashMap::new();
    let mut next = 2;
    let cache = RefCell::new(HashMap::new());
    let mut b = Builder::<()> {
        nodes: vec![],
        actions: vec![],
        rows: vec![],
        scroll,
        ids: &mut ids,
        next: &mut next,
        p,
        fitter: Fitter::new(measure, 1.0, &cache),
        focus: None,
        top: 0.0,
        bottom: viewport.h,
    };
    // Root uses the same scene geometry as text and timeline bars.
    b.node(
        "root",
        viewport,
        "group",
        "交付计划".into(),
        String::new(),
        FontRole::Text,
        12.0,
        400,
        p.primary,
        p.ground,
        None,
    );
    if let Some(root) = b.nodes.first_mut() {
        root.id = 1;
        root.parent = None;
    }
    let mut y = 12.0 - scroll;
    let w = viewport.w;
    let Some(snapshot) = snapshot else {
        b.label(
            "empty",
            r(8.0, y, w - 16.0, 28.0),
            "选择计划：项目 交付范围 计划ID（空格分隔）".into(),
            p.secondary,
            13.0,
            FontRole::Text,
        );
        return (b.nodes, 100.0);
    };
    let v = &snapshot.page;
    {
        let mut row = |b: &mut Builder<'_, ()>, key: &str, text: String, color: [f32; 4]| {
            b.label(
                key,
                r(8.0, y, w - 16.0, 28.0),
                text,
                color,
                13.0,
                FontRole::Text,
            );
            y += 30.0;
        };
        row(
            &mut b,
            "finish",
            format!(
                "{} {} · 含缓冲 {}",
                if v.conditional {
                    "条件预测"
                } else {
                    "预计完成"
                },
                date(v, v.finish),
                date(v, v.protected_finish)
            ),
            p.primary,
        );
        for m in &v.milestones {
            let color = if m.target.zip(m.forecast).is_some_and(|(t, f)| f > t) {
                p.danger
            } else {
                p.primary
            };
            let primary = format!(
                "◆ {}  {}{}",
                m.label,
                date(v, m.actual.or(m.forecast)),
                if m.actual.is_some() {
                    "（完成）"
                } else if m.conditional {
                    "（条件）"
                } else {
                    ""
                }
            );
            row(
                &mut b,
                &format!("milestone:{}", m.id),
                if w < 520.0 {
                    primary
                } else {
                    format!("{primary} · 目标 {}", date(v, m.target))
                },
                color,
            );
            if w < 520.0 {
                row(
                    &mut b,
                    &format!("target:{}", m.id),
                    format!("目标 {}", date(v, m.target)),
                    p.secondary,
                );
            }
            if m.baseline != m.target {
                row(
                    &mut b,
                    &format!("baseline:{}", m.id),
                    format!("基线 {}", date(v, m.baseline)),
                    p.secondary,
                );
            }
        }
        for g in &v.gates {
            let state = match g.readiness {
                PlanReadiness::Satisfied => "✓ 已满足",
                PlanReadiness::Unsatisfied => "× 未满足",
                PlanReadiness::Unknown => "? 待确认",
            };
            row(
                &mut b,
                &format!("gate:{}", g.id),
                format!("{}  {}", g.id, state),
                if g.readiness == PlanReadiness::Satisfied {
                    p.success
                } else {
                    p.warning
                },
            );
            for (i, reason) in g.reasons.iter().enumerate() {
                row(
                    &mut b,
                    &format!("gate:{}:{i}", g.id),
                    evidence_reason(reason),
                    p.secondary,
                );
            }
        }
        for f in &v.flow {
            row(
                &mut b,
                &format!("flow:{}", f.lane),
                format!(
                    "{}  进行中 {}{} · 阻塞 {}",
                    f.lane,
                    f.wip,
                    f.limit.map(|n| format!("/{n}")).unwrap_or_default(),
                    f.blocked_wip
                ),
                if f.limit.is_some_and(|n| f.wip > n) {
                    p.danger
                } else {
                    p.secondary
                },
            );
        }
        for buffer in &v.buffers {
            row(
                &mut b,
                &format!("buffer:{}", buffer.id),
                format!(
                    "缓冲 {} · {} 工作时段 · 消耗 {}",
                    buffer.id,
                    buffer.slots,
                    buffer
                        .consumed
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "未比较基线".into())
                ),
                p.secondary,
            );
        }
        for warning in &v.warnings {
            row(
                &mut b,
                &format!("warning:{warning}"),
                reason(warning).into(),
                p.warning,
            );
        }
    }
    let end = v.finish.unwrap_or(v.horizon).max(1);
    let timeline_x = (w * 0.45).min(250.0);
    let tw = (w - timeline_x - 12.0).max(1.0);
    for (i, tick) in [0, end / 2, end].into_iter().enumerate() {
        let label =
            v.epoch_utc + chrono::Duration::minutes(i64::from(tick) * i64::from(v.slot_minutes));
        b.label(
            &format!("axis:{i}"),
            r(
                timeline_x + (tw - (tw / 3.0).min(80.0)) * i as f32 / 2.0,
                y,
                (tw / 3.0).min(80.0),
                26.0,
            ),
            label
                .format(if tw < 220.0 { "%d %Hh" } else { "%m-%d %Hh" })
                .to_string(),
            p.secondary,
            10.0,
            FontRole::Text,
        );
    }
    y += 30.0;
    for t in &snapshot.tasks {
        let state = if t.blocked {
            "阻塞"
        } else {
            match t.state {
                PlanWorkState::Todo => "待办",
                PlanWorkState::Doing => "进行中",
                PlanWorkState::Done => "完成",
            }
        };
        let color = if t.blocked || t.unscheduled_reason.is_some() {
            p.warning
        } else if t.driving {
            p.danger
        } else {
            p.accent_ink
        };
        b.label(
            &format!("task:{}", t.id),
            r(8.0, y, timeline_x - 12.0, 26.0),
            format!(
                "{} · {}{}",
                t.label,
                state,
                if t.driving { " ◆" } else { "" }
            ),
            color,
            12.0,
            FontRole::Text,
        );
        if let Some((a, z)) = t.start.zip(t.finish) {
            let x = timeline_x + tw * (a as f32 / end as f32);
            let width = (tw * (z - a) as f32 / end as f32).max(3.0);
            b.node(
                &format!("bar:{}", t.id),
                r(x, y + 4.0, width, 18.0),
                "img",
                format!(
                    "{} {} 至 {}{}{}",
                    t.label,
                    date(v, Some(a)),
                    date(v, Some(z)),
                    if t.conditional { " 条件预测" } else { "" },
                    if t.driving { " 驱动链" } else { "" }
                ),
                String::new(),
                FontRole::Text,
                11.0,
                400,
                color,
                Some(color),
                None,
            );
        } else {
            b.label(
                &format!("unknown:{}", t.id),
                r(timeline_x, y, tw, 26.0),
                format!(
                    "? {}",
                    reason(t.unscheduled_reason.as_deref().unwrap_or("未知"))
                ),
                p.warning,
                12.0,
                FontRole::Text,
            );
        }
        y += 30.0;
    }
    for wait in &snapshot.resource_waits {
        b.label(
            &format!("wait:{}:{}:{}", wait.resource, wait.before, wait.after),
            r(8.0, y, w - 16.0, 26.0),
            format!("{}：{} → {}", wait.resource, wait.before, wait.after),
            p.secondary,
            12.0,
            FontRole::Text,
        );
        y += 28.0;
    }
    (b.nodes, y + scroll + 12.0)
}

#[cfg(test)]
#[path = "planning/tests.rs"]
mod tests;

fn reason(value: &str) -> &str {
    match value {
        "unknown_duration" => "工期待确认",
        "unknown_gate" => "准入证据待确认",
        "unknown_blocker" => "阻塞解除时间待确认",
        "unknown_resource_release" => "资源释放时间待确认",
        "predecessor" => "前置任务尚无交期",
        "horizon" => "超出排程范围",
        "forecast_assumes_future_gate_release_not_approval" => "日期包含未来准入假设，尚未获准执行",
        "deadline_missed" => "预计超过目标日期",
        "project_buffer_exceeds_horizon" => "缓冲结束超出排程范围",
        "logical_cpm_incomplete" => "依赖数据不完整，关键路径待确认",
        "resource_schedule_is_deterministic_heuristic_not_optimality_proof" => {
            "调整资源优先级可能改变交期"
        }
        "Global WIP limit exceeded" => "全局在制任务超过限制",
        other => other,
    }
}

fn evidence_reason(value: &str) -> String {
    if let Some(id) = value
        .strip_prefix("check:")
        .and_then(|v| v.strip_suffix(":missing_stale_unconfirmed_or_wrong_scope"))
    {
        return format!("{id}：缺少本版本有效的确认信息");
    }
    match value {
        "approval_missing_stale_unconfirmed_or_wrong_policy" => "尚无本版本有效的审批记录".into(),
        "required_condition_failed_or_approval_denied" => "必要条件未通过或审批被拒绝".into(),
        other => other.into(),
    }
}

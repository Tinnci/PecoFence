//! First-party SPM panel adapter.
//!
//! Business decisions remain daemon-owned. This crate consumes versioned read-model snapshots and
//! turns them into layout, hit-test, semantic, and paint data without depending on Win32.

use pecofence_plugin_api::*;
use serde::{Deserialize, Serialize};
use spm_contracts::{
    Body, BuildBriefingRequest, DaemonSessionId, DeliveryScopeId, DetailLevel, Envelope, Event,
    GateStatus, IdempotencyKey, PROTOCOL_MAJOR, PROTOCOL_MINOR, ProjectId, ProjectQuery,
    ProjectSnapshot, RefreshRequest, Request, RequestId, ResolveNavigationRequest, Revision,
    ViewKind,
};
use std::sync::Arc;

pub const PROVIDER_ID: &str = "pecofence.spm";
const ACTION_REFRESH: u64 = 1;
const ACTION_COPY_BRIEFING: u64 = 2;
const ACTION_OPEN_BASE: u64 = 1_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpmConfig {
    pub project_id: ProjectId,
    pub delivery_scope_id: DeliveryScopeId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewNode {
    pub id: u64,
    pub rect: RectDip,
    pub role: &'static str,
    pub text: String,
    pub action: Option<u64>,
    pub fill: Option<[f32; 4]>,
    pub foreground: [f32; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PanelView {
    pub nodes: Vec<ViewNode>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SnapshotCursor {
    accepted_session: Option<DaemonSessionId>,
    query: Option<ProjectQuery>,
    revision: Option<Revision>,
}

impl SnapshotCursor {
    pub fn for_query(query: ProjectQuery) -> Self {
        Self {
            accepted_session: None,
            query: Some(query),
            revision: None,
        }
    }

    pub fn accept(&mut self, envelope: &Envelope) -> bool {
        let Body::Event(Event::Snapshot(snapshot)) = &envelope.body else {
            return false;
        };
        let Some(subscription) = envelope.subscription_id else {
            return false;
        };
        let _ = subscription;
        if envelope.daemon_session != Some(snapshot.daemon_session)
            || envelope.revision != Some(snapshot.revision)
            || self
                .accepted_session
                .is_some_and(|session| session != snapshot.daemon_session)
            || self
                .revision
                .is_some_and(|revision| snapshot.revision <= revision)
            || self.query.as_ref().is_some_and(|query| {
                query.project_id != snapshot.project_id
                    || query.delivery_scope_id != snapshot.delivery_scope_id
            })
        {
            return false;
        }
        self.accepted_session.get_or_insert(snapshot.daemon_session);
        self.revision = Some(snapshot.revision);
        true
    }

    pub fn accepted_session(&self) -> Option<DaemonSessionId> {
        self.accepted_session
    }
}

pub fn encode_request(envelope: &Envelope) -> Result<Arc<[u8]>> {
    envelope
        .validate(spm_contracts::Direction::ClientToServer)
        .map_err(|error| Error::Invalid(error.to_string()))?;
    serde_json::to_vec(envelope)
        .map(Arc::from)
        .map_err(|error| Error::Invalid(error.to_string()))
}

pub fn decode_event(bytes: &[u8]) -> Result<Envelope> {
    let envelope: Envelope =
        serde_json::from_slice(bytes).map_err(|error| Error::Invalid(error.to_string()))?;
    envelope
        .validate(spm_contracts::Direction::ServerToClient)
        .map_err(|error| Error::Invalid(error.to_string()))?;
    Ok(envelope)
}

/// Builds presentation data solely from a read-model snapshot and viewport.
pub fn build_view(snapshot: Option<&ProjectSnapshot>, viewport: RectDip) -> PanelView {
    let mut nodes = Vec::new();
    let panel = [0.125, 0.149, 0.188, 1.0];
    let subtle = [0.165, 0.2, 0.251, 1.0];
    let primary = [0.949, 0.961, 0.98, 1.0];
    let secondary = [0.725, 0.773, 0.839, 1.0];
    let accent = [0.545, 0.765, 1.0, 1.0];
    let mut push = |id, rect, role, text: String, action, fill, foreground| {
        nodes.push(ViewNode {
            id,
            rect,
            role,
            text,
            action,
            fill,
            foreground,
        });
    };
    push(
        1,
        viewport,
        "pane",
        String::new(),
        None,
        Some(panel),
        primary,
    );
    let Some(snapshot) = snapshot else {
        push(
            2,
            RectDip {
                x: 16.0,
                y: 16.0,
                w: (viewport.w - 32.0).max(0.0),
                h: 64.0,
            },
            "status",
            "后台连接已断开；尚无可用快照".into(),
            None,
            Some(subtle),
            primary,
        );
        push(
            3,
            RectDip {
                x: 16.0,
                y: 92.0,
                w: 120.0,
                h: 32.0,
            },
            "button",
            "重试".into(),
            Some(ACTION_REFRESH),
            Some(subtle),
            accent,
        );
        return PanelView { nodes };
    };
    push(
        10,
        RectDip {
            x: 16.0,
            y: 12.0,
            w: (viewport.w - 32.0).max(0.0),
            h: 32.0,
        },
        "heading",
        format!("{} · {}", snapshot.project_id, snapshot.delivery_scope_id),
        None,
        None,
        primary,
    );
    push(
        11,
        RectDip {
            x: 16.0,
            y: 48.0,
            w: (viewport.w - 32.0).max(0.0),
            h: 28.0,
        },
        "status",
        format!(
            "{} · {} · 未满足 {} · 未知 {}",
            snapshot
                .next_milestone
                .as_ref()
                .map(|item| item.name.as_str())
                .unwrap_or("—"),
            gate_status_label(snapshot.overall_gate.state),
            snapshot.overall_gate.unsatisfied_count,
            snapshot.overall_gate.unknown_count
        ),
        None,
        Some(subtle),
        primary,
    );
    let metric_width = ((viewport.w - 68.0) / 4.0).max(80.0);
    let metrics = [
        ("条件未满足", snapshot.overall_gate.unsatisfied_count),
        ("严重缺陷", snapshot.work_summary.severe_open),
        ("待验证", snapshot.work_summary.pending_verification),
        ("客户待验收", snapshot.customer_obligations.pending),
    ];
    for (index, (label, value)) in metrics.into_iter().enumerate() {
        push(
            20 + index as u64,
            RectDip {
                x: 16.0 + index as f32 * (metric_width + 12.0),
                y: 88.0,
                w: metric_width,
                h: 64.0,
            },
            "group",
            format!("{label}\n{value}"),
            None,
            Some(subtle),
            primary,
        );
    }
    push(
        30,
        RectDip {
            x: 16.0,
            y: 164.0,
            w: (viewport.w - 152.0).max(0.0),
            h: 28.0,
        },
        "status",
        format!("数据时间 {}", snapshot.computed_at.to_rfc3339()),
        None,
        None,
        secondary,
    );
    push(
        31,
        RectDip {
            x: (viewport.w - 120.0).max(16.0),
            y: 160.0,
            w: 104.0,
            h: 32.0,
        },
        "button",
        "刷新数据".into(),
        Some(ACTION_REFRESH),
        Some(subtle),
        accent,
    );
    let row_top = 204.0;
    let row_height = 58.0;
    let visible = ((viewport.h - row_top - 52.0).max(0.0) / row_height).floor() as usize;
    for (index, item) in snapshot.preview_items.iter().take(visible).enumerate() {
        let y = row_top + index as f32 * row_height;
        let action = (!item.source_refs.is_empty()).then_some(ACTION_OPEN_BASE + index as u64);
        push(
            100 + index as u64,
            RectDip {
                x: 16.0,
                y,
                w: (viewport.w - 32.0).max(0.0),
                h: row_height - 6.0,
            },
            "listitem",
            format!(
                "{} · {} · {}\n{} · {}",
                item.record_id,
                item.owner.as_deref().unwrap_or("—"),
                item.due_at
                    .map(|value| value.to_rfc3339())
                    .unwrap_or_else(|| "—".into()),
                item.title,
                item.next_step.as_deref().unwrap_or("—")
            ),
            action,
            Some(subtle),
            primary,
        );
    }
    push(
        900,
        RectDip {
            x: (viewport.w - 152.0).max(16.0),
            y: (viewport.h - 40.0).max(row_top),
            w: 136.0,
            h: 32.0,
        },
        "button",
        "复制简报".into(),
        Some(ACTION_COPY_BRIEFING),
        Some(subtle),
        accent,
    );
    PanelView { nodes }
}

fn gate_status_label(status: GateStatus) -> &'static str {
    match status {
        GateStatus::Satisfied => "已满足",
        GateStatus::Unsatisfied => "未满足",
        GateStatus::Unknown => "未知",
        GateStatus::NotApplicable => "不适用",
    }
}

pub fn layout_snapshot(view: &PanelView, revision: u64) -> LayoutSnapshot {
    LayoutSnapshot {
        revision,
        hits: view
            .nodes
            .iter()
            .filter_map(|node| {
                node.action.map(|action| HitNode {
                    id: node.id,
                    rect: node.rect,
                    action,
                })
            })
            .collect(),
        semantics: view
            .nodes
            .iter()
            .map(|node| SemanticNode {
                id: node.id,
                parent: (node.id != 1).then_some(1),
                role: node.role.into(),
                name: node.text.clone(),
                rect: node.rect,
                action: node.action,
            })
            .collect(),
    }
}

pub fn hit_test(layout: &LayoutSnapshot, x: f32, y: f32) -> Option<u64> {
    layout
        .hits
        .iter()
        .rev()
        .find(|node| node.rect.contains(x, y))
        .map(|node| node.action)
}

pub struct SpmPlugin;

impl PanelProvider for SpmPlugin {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: PROVIDER_ID,
            api_major: 1,
            config_major: 2,
            required_services: &["render", "theme", "desktop", "ipc", "storage"],
        }
    }

    fn validate(&self, config: &PanelConfig) -> Result<()> {
        if config.version != 2 {
            return Err(Error::Invalid(
                "unsupported SPM panel config version".into(),
            ));
        }
        let _parsed: SpmConfig = serde_json::from_slice(&config.bytes)
            .map_err(|error| Error::Invalid(error.to_string()))?;
        Ok(())
    }

    fn create(&self, ctx: PluginContext, input: CreatePanel) -> Result<Box<dyn PanelInstance>> {
        self.validate(&input.config)?;
        let config: SpmConfig = serde_json::from_slice(&input.config.bytes)
            .map_err(|error| Error::Invalid(error.to_string()))?;
        let project_query = ProjectQuery {
            project_id: config.project_id.clone(),
            delivery_scope_id: config.delivery_scope_id.clone(),
            view: ViewKind::Summary,
            filter: None,
            detail_level: DetailLevel::Standard,
            sort: Vec::new(),
        };
        let query = Query {
            endpoint: "spm.v2/read-model".into(),
            payload: Arc::from(
                serde_json::to_vec(&project_query)
                    .map_err(|error| Error::Invalid(error.to_string()))?,
            ),
        };
        let subscription = ctx.ipc.with(|ipc| ipc.subscribe(&ctx.scope, query))?;
        Ok(Box::new(SpmPanel {
            ctx,
            key: input.key,
            config,
            subscription,
            mount: None,
            snapshot: None,
            snapshot_cursor: SnapshotCursor::for_query(project_query),
            view: PanelView::default(),
            layout: LayoutSnapshot::default(),
            layout_revision: 0,
            stopped: false,
        }))
    }
}

pub struct SpmPanel {
    ctx: PluginContext,
    key: InstanceKey,
    config: SpmConfig,
    subscription: Token,
    mount: Option<MountKey>,
    snapshot: Option<ProjectSnapshot>,
    snapshot_cursor: SnapshotCursor,
    view: PanelView,
    layout: LayoutSnapshot,
    layout_revision: u64,
    stopped: bool,
}

impl PanelInstance for SpmPanel {
    fn mount(&mut self, ctx: MountContext) -> Result<()> {
        if ctx.key.instance != self.key {
            return Err(Error::Invalid("mount belongs to another instance".into()));
        }
        self.mount = Some(ctx.key);
        Ok(())
    }

    fn event(&mut self, event: PanelEvent) -> Result<PanelUpdate> {
        if self.stopped {
            return Err(Error::Closed);
        }
        match event {
            PanelEvent::Snapshot {
                subscription,
                bytes,
            } if subscription == self.subscription => {
                let envelope = decode_event(&bytes)?;
                if !self.snapshot_cursor.accept(&envelope) {
                    return Ok(PanelUpdate::default());
                }
                let Body::Event(Event::Snapshot(snapshot)) = envelope.body else {
                    return Ok(PanelUpdate::default());
                };
                self.snapshot = Some(snapshot);
                Ok(PanelUpdate {
                    commands: vec![HostCommand::Invalidate],
                    relayout: true,
                })
            }
            PanelEvent::Invoke {
                action,
                layout_revision,
            } if layout_revision == self.layout.revision => {
                if action == ACTION_REFRESH {
                    let payload = self.action_request(Request::Refresh(RefreshRequest {
                        project_id: self.config.project_id.clone(),
                        delivery_scope_id: self.config.delivery_scope_id.clone(),
                        idempotency_key: IdempotencyKey::new(),
                    }))?;
                    self.ctx
                        .ipc
                        .with(|ipc| ipc.send(&self.ctx.scope, payload))?;
                } else if action == ACTION_COPY_BRIEFING {
                    if let Some(snapshot) = &self.snapshot {
                        let payload =
                            self.action_request(Request::BuildBriefing(BuildBriefingRequest {
                                project_id: snapshot.project_id.clone(),
                                delivery_scope_id: snapshot.delivery_scope_id.clone(),
                                revision: snapshot.revision,
                                locale: "zh-CN".into(),
                                timezone: snapshot.project_timezone.clone(),
                            }))?;
                        self.ctx
                            .ipc
                            .with(|ipc| ipc.send(&self.ctx.scope, payload))?;
                    }
                } else if let Some(index) = action
                    .checked_sub(ACTION_OPEN_BASE)
                    .map(|value| value as usize)
                    && let Some(target) = self
                        .snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.preview_items.get(index))
                        .and_then(|item| item.source_refs.first())
                {
                    if let Some(snapshot) = &self.snapshot {
                        let payload = self.action_request(Request::ResolveNavigation(
                            ResolveNavigationRequest {
                                source: target.clone(),
                                revision: snapshot.revision,
                            },
                        ))?;
                        self.ctx
                            .ipc
                            .with(|ipc| ipc.send(&self.ctx.scope, payload))?;
                    }
                }
                Ok(PanelUpdate {
                    commands: vec![HostCommand::Invalidate],
                    relayout: false,
                })
            }
            PanelEvent::SuspendInput => Ok(PanelUpdate::default()),
            _ => Ok(PanelUpdate::default()),
        }
    }

    fn layout(&mut self, input: LayoutInput) -> Result<LayoutSnapshot> {
        self.layout_revision = self
            .layout_revision
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        self.view = build_view(self.snapshot.as_ref(), input.viewport);
        self.layout = layout_snapshot(&self.view, self.layout_revision);
        Ok(self.layout.clone())
    }

    fn paint(&self, canvas: &mut dyn Canvas, layout: &LayoutSnapshot) -> Result<()> {
        if layout.revision != self.layout.revision {
            return Err(Error::Revoked);
        }
        for node in &self.view.nodes {
            if let Some(fill) = node.fill {
                canvas.fill(node.rect, fill)?;
            }
            if !node.text.is_empty() {
                canvas.text(
                    node.rect,
                    &TextSpec {
                        text: node.text.clone(),
                        size_dip: if node.role == "heading" { 20.0 } else { 14.0 },
                        weight: if node.action.is_some() { 600 } else { 400 },
                    },
                    node.foreground,
                )?;
            }
        }
        Ok(())
    }

    fn unmount(&mut self, key: MountKey) {
        if self.mount == Some(key) {
            self.mount = None;
        }
    }

    fn begin_stop(&mut self, _reason: StopReason) {
        self.stopped = true;
        let _ = self.ctx.ipc.with(|ipc| ipc.cancel(self.subscription));
    }
}

impl SpmPanel {
    fn action_request(&self, request: Request) -> Result<Arc<[u8]>> {
        let daemon_session = self
            .snapshot_cursor
            .accepted_session()
            .ok_or(Error::Revoked)?;
        encode_request(&Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            daemon_session: Some(daemon_session),
            request_id: Some(RequestId::new()),
            subscription_id: None,
            revision: None,
            body: Body::Request(request),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spm_contracts::*;

    fn snapshot(session: DaemonSessionId, revision: u64) -> ProjectSnapshot {
        ProjectSnapshot {
            project_id: ProjectId::new("project-atlas").unwrap(),
            delivery_scope_id: DeliveryScopeId::new("scope-eu").unwrap(),
            baseline_id: Some(BaselineId::new("build-7").unwrap()),
            policy_revision: 1,
            daemon_session: session,
            revision: Revision(revision),
            computed_at: "2026-09-21T01:20:00Z".parse().unwrap(),
            project_timezone: "Asia/Shanghai".into(),
            freshness_ttl_secs: 300,
            sources: vec![],
            overall_gate: OverallGate {
                state: GateStatus::Unsatisfied,
                unsatisfied_count: 2,
                unknown_count: 1,
                applicable_count: 3,
            },
            gates: vec![],
            next_milestone: Some(MilestoneSummary {
                id: "tr5".into(),
                name: "TR5".into(),
                due_at: None,
            }),
            work_summary: WorkSummary {
                total: 1,
                severe_open: 5,
                pending_verification: 11,
            },
            preview_items: vec![WorkItem {
                record_id: RecordId::new("C-104").unwrap(),
                kind: WorkItemKind::Obligation,
                title: "确认验收".into(),
                owner: Some("Owner".into()),
                due_at: None,
                next_step: Some("确认结果".into()),
                source_refs: vec![SourceRef {
                    system: SourceSystem::Jira,
                    tenant: "main".into(),
                    project: "ATLAS".into(),
                    record_kind: "issue".into(),
                    record_id: "C-104".into(),
                }],
                relation_state: RelationState::Confirmed,
            }],
            customer_obligations: CustomerObligationSummary {
                total: 3,
                accepted: 0,
                pending: 3,
            },
            verification: VerificationSummary::default(),
            merge: MergeSummary::default(),
            trend: TrendSummary {
                basis: TrendBasis::LiveScope,
                points: vec![],
                has_plan_line: false,
            },
        }
    }

    fn envelope(snapshot: ProjectSnapshot) -> Envelope {
        Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            daemon_session: Some(snapshot.daemon_session),
            request_id: None,
            subscription_id: Some(SubscriptionId::new()),
            revision: Some(snapshot.revision),
            body: Body::Event(Event::Snapshot(snapshot)),
        }
    }

    #[test]
    fn layout_and_hit_testing_are_pure_and_revision_bound() {
        let viewport = RectDip {
            x: 0.0,
            y: 0.0,
            w: 1120.0,
            h: 760.0,
        };
        let data = snapshot(DaemonSessionId::new(), 1);
        let a = build_view(Some(&data), viewport);
        let b = build_view(Some(&data), viewport);
        assert_eq!(a, b);
        let layout = layout_snapshot(&a, 4);
        assert_eq!(hit_test(&layout, 1020.0, 176.0), Some(ACTION_REFRESH));
        assert!(layout.semantics.iter().any(|node| node.role == "listitem"));
    }

    #[test]
    fn snapshot_revision_is_monotone_within_a_daemon_session() {
        let session = DaemonSessionId::new();
        let query = ProjectQuery {
            project_id: ProjectId::new("project-atlas").unwrap(),
            delivery_scope_id: DeliveryScopeId::new("scope-eu").unwrap(),
            view: ViewKind::Summary,
            filter: None,
            detail_level: DetailLevel::Standard,
            sort: vec![],
        };
        let mut cursor = SnapshotCursor::for_query(query);
        let current = envelope(snapshot(session, 1));
        assert!(cursor.accept(&current));
        assert!(!cursor.accept(&current));
        assert!(!cursor.accept(&envelope(snapshot(session, 0))));
        assert!(!cursor.accept(&envelope(snapshot(DaemonSessionId::new(), 2))));
    }
}

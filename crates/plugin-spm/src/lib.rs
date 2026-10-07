//! First-party SPM panel adapter.
//!
//! Business decisions remain daemon-owned. This crate consumes versioned read-model snapshots and
//! turns them into layout, hit-test, semantic, and paint data without depending on Win32.

use pecofence_plugin_api::*;
use serde::{Deserialize, Serialize};
use spm_contracts::{
    Body, BuildBriefingRequest, DaemonSessionId, DeliveryScopeId, DetailLevel, Envelope, Event,
    GateStatus, IdempotencyKey, ProjectId, ProjectQuery, ProjectSnapshot, RefreshRequest, Request,
    ResolveNavigationRequest, Response, Revision, RpcMethod, ViewKind,
};
use std::sync::Arc;
use std::{collections::HashMap, rc::Rc};

pub const PROVIDER_ID: &str = "pecofence.spm";
const ACTION_REFRESH: u64 = 1;
const ACTION_COPY_BRIEFING: u64 = 2;
const ACTION_EXPAND: u64 = 3;
const ACTION_BACK_TO_LIST: u64 = 4;
const ACTION_OPEN_BASE: u64 = 1_000;
const ACTION_SELECT_BASE: u64 = 2_000;

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
            || self.query.as_ref().is_some_and(|query| {
                query.project_id != snapshot.project_id
                    || query.delivery_scope_id != snapshot.delivery_scope_id
            })
        {
            return false;
        }
        if self.accepted_session != Some(snapshot.daemon_session) {
            self.accepted_session = Some(snapshot.daemon_session);
            self.revision = None;
        }
        if self
            .revision
            .is_some_and(|revision| snapshot.revision <= revision)
        {
            return false;
        }
        self.revision = Some(snapshot.revision);
        true
    }

    pub fn accepted_session(&self) -> Option<DaemonSessionId> {
        self.accepted_session
    }
}

pub fn encode_request(request: &Request) -> Result<Arc<[u8]>> {
    request
        .validate()
        .map_err(|error| Error::Invalid(error.to_string()))?;
    serde_json::to_vec(request)
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

/// Semantic colour tokens the view builders consume. Values are derived from
/// the host `ThemeSnapshot` — the design system owns colours, the panel only
/// maps business state onto roles, never onto concrete RGBA values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelTokens {
    pub panel: [f32; 4],
    pub subtle: [f32; 4],
    pub primary: [f32; 4],
    pub secondary: [f32; 4],
    pub accent: [f32; 4],
    pub stroke: [f32; 4],
    pub danger: [f32; 4],
    pub warning: [f32; 4],
    pub success: [f32; 4],
    pub unknown: [f32; 4],
}

impl PanelTokens {
    pub fn from_snapshot(snapshot: &ThemeSnapshot) -> Self {
        Self {
            panel: snapshot.surface_panel,
            subtle: snapshot.surface_subtle,
            primary: snapshot.text_primary,
            secondary: snapshot.text_secondary,
            accent: snapshot.accent,
            stroke: snapshot.stroke,
            danger: snapshot.danger,
            warning: snapshot.warning,
            success: snapshot.success,
            unknown: snapshot.unknown,
        }
    }

    /// Dark-mode fallback used before the first theme snapshot arrives and by
    /// layout tests; mirrors the host's dark palette.
    pub fn dark() -> Self {
        Self {
            panel: [0.125, 0.149, 0.188, 1.0],
            subtle: [0.165, 0.2, 0.251, 1.0],
            primary: [0.949, 0.961, 0.98, 1.0],
            secondary: [0.725, 0.773, 0.839, 1.0],
            accent: [0.545, 0.765, 1.0, 1.0],
            stroke: [0.196, 0.267, 0.314, 1.0],
            danger: [0.973, 0.443, 0.443, 1.0],
            warning: [0.984, 0.573, 0.235, 1.0],
            success: [0.29, 0.871, 0.533, 1.0],
            unknown: [0.725, 0.773, 0.839, 1.0],
        }
    }
}

/// Builds presentation data solely from a read-model snapshot and viewport.
pub fn build_view(
    snapshot: Option<&ProjectSnapshot>,
    viewport: RectDip,
    tokens: &PanelTokens,
) -> PanelView {
    build_workspace_view(snapshot, viewport, None, tokens)
}

fn build_workspace_view(
    snapshot: Option<&ProjectSnapshot>,
    viewport: RectDip,
    selected_item: Option<usize>,
    tokens: &PanelTokens,
) -> PanelView {
    let mut nodes = Vec::new();
    let panel = tokens.panel;
    let subtle = tokens.subtle;
    let primary = tokens.primary;
    let secondary = tokens.secondary;
    let accent = tokens.accent;
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
    let columns = if WorkspaceBreakpoint::for_width(viewport.w) == WorkspaceBreakpoint::Narrow {
        2
    } else {
        4
    };
    let metric_width =
        ((viewport.w - 32.0 - 12.0 * (columns - 1) as f32) / columns as f32).max(80.0);
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
                x: 16.0 + (index % columns) as f32 * (metric_width + 12.0),
                y: 88.0 + (index / columns) as f32 * 76.0,
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
    let narrow = columns == 2;
    let status_top = if narrow { 240.0 } else { 164.0 };
    push(
        30,
        RectDip {
            x: 16.0,
            y: status_top,
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
            y: status_top - 4.0,
            w: 104.0,
            h: 32.0,
        },
        "button",
        "刷新数据".into(),
        Some(ACTION_REFRESH),
        Some(subtle),
        accent,
    );
    let row_top = status_top + 40.0;
    let row_height = 58.0;
    let breakpoint = WorkspaceBreakpoint::for_width(viewport.w);
    if breakpoint != WorkspaceBreakpoint::Wide
        && let Some(index) = selected_item
        && let Some(item) = snapshot.preview_items.get(index)
    {
        push(
            40,
            RectDip {
                x: 16.0,
                y: row_top,
                w: 104.0,
                h: 32.0,
            },
            "button",
            "返回事项".into(),
            Some(ACTION_BACK_TO_LIST),
            Some(subtle),
            accent,
        );
        push(
            41,
            RectDip {
                x: 16.0,
                y: row_top + 44.0,
                w: (viewport.w - 32.0).max(0.0),
                h: (viewport.h - row_top - 60.0).max(80.0),
            },
            "article",
            format!(
                "{}\n{}\n负责人 {}\n下一步 {}",
                item.record_id,
                item.title,
                item.owner.as_deref().unwrap_or("—"),
                item.next_step.as_deref().unwrap_or("—")
            ),
            (!item.source_refs.is_empty()).then_some(ACTION_OPEN_BASE + index as u64),
            Some(subtle),
            primary,
        );
        return PanelView { nodes };
    }
    let list_width = if breakpoint == WorkspaceBreakpoint::Wide {
        (viewport.w - 408.0).max(320.0)
    } else {
        (viewport.w - 32.0).max(0.0)
    };
    let visible = ((viewport.h - row_top - 52.0).max(0.0) / row_height).floor() as usize;
    for (index, item) in snapshot.preview_items.iter().take(visible).enumerate() {
        let y = row_top + index as f32 * row_height;
        let action = Some(ACTION_SELECT_BASE + index as u64);
        push(
            100 + index as u64,
            RectDip {
                x: 16.0,
                y,
                w: list_width,
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
    if breakpoint == WorkspaceBreakpoint::Wide {
        let detail = selected_item
            .and_then(|index| snapshot.preview_items.get(index))
            .or_else(|| snapshot.preview_items.first());
        push(
            800,
            RectDip {
                x: viewport.w - 376.0,
                y: row_top,
                w: 360.0,
                h: (viewport.h - row_top - 52.0).max(80.0),
            },
            "complementary",
            detail
                .map(|item| format!("事项详情\n{}\n{}", item.record_id, item.title))
                .unwrap_or_else(|| "事项详情\n暂无事项".into()),
            None,
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

fn build_compact_view(
    snapshot: Option<&ProjectSnapshot>,
    viewport: RectDip,
    tokens: &PanelTokens,
) -> PanelView {
    let panel = tokens.panel;
    let subtle = tokens.subtle;
    let primary = tokens.primary;
    let accent = tokens.accent;
    let mut nodes = vec![ViewNode {
        id: 1,
        rect: viewport,
        role: "pane",
        text: String::new(),
        action: None,
        fill: Some(panel),
        foreground: primary,
    }];
    let inset = 12.0;
    let width = (viewport.w - inset * 2.0).max(0.0);
    let mut push = |id, rect, role, text, action, fill, foreground| {
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
    let Some(snapshot) = snapshot else {
        push(
            2,
            RectDip {
                x: inset,
                y: 12.0,
                w: width,
                h: 44.0,
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
                x: inset,
                y: (viewport.h - 40.0).max(60.0),
                w: 112.0,
                h: 28.0,
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
            x: inset,
            y: 8.0,
            w: width,
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
            x: inset,
            y: 48.0,
            w: width,
            h: 44.0,
        },
        "status",
        format!(
            "门槛 {} · 未满足 {} · 未知 {}",
            gate_status_label(snapshot.overall_gate.state),
            snapshot.overall_gate.unsatisfied_count,
            snapshot.overall_gate.unknown_count
        ),
        None,
        Some(subtle),
        primary,
    );
    let items_top = 100.0;
    for (index, item) in snapshot.preview_items.iter().take(3).enumerate() {
        push(
            100 + index as u64,
            RectDip {
                x: inset,
                y: items_top + index as f32 * 44.0,
                w: width,
                h: 40.0,
            },
            "listitem",
            format!("{} · {}", item.record_id, item.title),
            (!item.source_refs.is_empty()).then_some(ACTION_OPEN_BASE + index as u64),
            Some(subtle),
            primary,
        );
    }
    let footer_y = (viewport.h - 36.0).max(items_top + 132.0);
    let due = snapshot
        .next_milestone
        .as_ref()
        .map(|milestone| {
            milestone
                .due_at
                .map(|date| date.to_rfc3339())
                .unwrap_or_else(|| "日期未知".into())
        })
        .unwrap_or_else(|| "无里程碑".into());
    push(
        20,
        RectDip {
            x: inset,
            y: footer_y,
            w: (width - 112.0).max(80.0),
            h: 28.0,
        },
        "status",
        format!("下一里程碑 {due}"),
        None,
        None,
        primary,
    );
    push(
        21,
        RectDip {
            x: (viewport.w - 108.0).max(inset),
            y: footer_y,
            w: 96.0,
            h: 28.0,
        },
        "button",
        "展开".into(),
        Some(ACTION_EXPAND),
        Some(subtle),
        accent,
    );
    PanelView { nodes }
}

fn build_capsule_view(
    snapshot: Option<&ProjectSnapshot>,
    viewport: RectDip,
    tokens: &PanelTokens,
) -> PanelView {
    let panel = tokens.panel;
    let badge = tokens.subtle;
    let primary = tokens.primary;
    let accent = tokens.accent;
    let height = viewport.h.max(48.0);
    let project_width = if viewport.w < 360.0 { 88.0 } else { 112.0 };
    let status_width = if viewport.w < 360.0 { 88.0 } else { 112.0 };
    let (project, status, pending) = snapshot
        .map(|snapshot| {
            let unknown = snapshot.overall_gate.unknown_count;
            let unsatisfied = snapshot.overall_gate.unsatisfied_count;
            let label = if unknown > 0 {
                format!("未知 {unknown}")
            } else if unsatisfied > 0 {
                format!("未满足 {unsatisfied}")
            } else {
                gate_status_label(snapshot.overall_gate.state).into()
            };
            (
                snapshot.project_id.to_string(),
                label,
                unknown + unsatisfied,
            )
        })
        .unwrap_or_else(|| ("SPM".into(), "未知".into(), 0));
    let mut nodes = vec![ViewNode {
        id: 1,
        rect: viewport,
        role: "toolbar",
        text: String::new(),
        action: None,
        fill: Some(panel),
        foreground: primary,
    }];
    nodes.push(ViewNode {
        id: 10,
        rect: RectDip {
            x: 8.0,
            y: (height - 32.0) / 2.0,
            w: 32.0,
            h: 32.0,
        },
        role: "grip",
        text: "⋮⋮".into(),
        action: None,
        fill: None,
        foreground: primary,
    });
    nodes.push(ViewNode {
        id: 11,
        rect: RectDip {
            x: 44.0,
            y: (height - 32.0) / 2.0,
            w: project_width,
            h: 32.0,
        },
        role: "heading",
        text: project,
        action: None,
        fill: None,
        foreground: primary,
    });
    nodes.push(ViewNode {
        id: 12,
        rect: RectDip {
            x: 48.0 + project_width,
            y: (height - 32.0) / 2.0,
            w: status_width,
            h: 32.0,
        },
        role: "status",
        text: format!("{status} · {pending}"),
        action: None,
        fill: Some(badge),
        foreground: primary,
    });
    nodes.push(ViewNode {
        id: 13,
        rect: RectDip {
            x: (viewport.w - 44.0).max(232.0),
            y: (height - 32.0) / 2.0,
            w: 32.0,
            h: 32.0,
        },
        role: "button",
        text: "↗".into(),
        action: Some(ACTION_EXPAND),
        fill: Some(badge),
        foreground: accent,
    });
    PanelView { nodes }
}

pub fn build_presentation_view(
    snapshot: Option<&ProjectSnapshot>,
    presentation: Presentation,
    viewport: RectDip,
    tokens: &PanelTokens,
) -> PanelView {
    build_presentation_view_with_selection(snapshot, presentation, viewport, None, tokens)
}

fn build_presentation_view_with_selection(
    snapshot: Option<&ProjectSnapshot>,
    presentation: Presentation,
    viewport: RectDip,
    selected_item: Option<usize>,
    tokens: &PanelTokens,
) -> PanelView {
    match presentation {
        Presentation::Workspace => build_workspace_view(snapshot, viewport, selected_item, tokens),
        Presentation::Compact => build_compact_view(snapshot, viewport, tokens),
        Presentation::Capsule => build_capsule_view(snapshot, viewport, tokens),
    }
}

fn gate_status_label(status: GateStatus) -> &'static str {
    match status {
        GateStatus::Satisfied => "已满足",
        GateStatus::Unsatisfied => "未满足",
        GateStatus::Unknown => "未知",
        GateStatus::NotApplicable => "未完成",
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

#[derive(Clone)]
pub struct SpmPreparedFrame {
    identity: FrameIdentity,
    view: PanelView,
    layout: LayoutSnapshot,
}

impl PreparedFrame for SpmPreparedFrame {
    fn identity(&self) -> FrameIdentity {
        self.identity
    }

    fn hit_tree(&self) -> &[HitNode] {
        &self.layout.hits
    }

    fn semantics(&self) -> &[SemanticNode] {
        &self.layout.semantics
    }

    fn paint(&self, canvas: &mut dyn Canvas) -> Result<()> {
        canvas.push_clip(
            self.view
                .nodes
                .first()
                .map(|node| node.rect)
                .unwrap_or_default(),
        )?;
        let result = (|| {
            for node in &self.view.nodes {
                if let Some(fill) = node.fill {
                    let radius = match node.role {
                        "toolbar" => node.rect.h / 2.0,
                        "button" | "status" => 6.0,
                        _ => 4.0,
                    };
                    canvas.fill_rounded_rect(node.rect, radius, fill)?;
                }
                if !node.text.is_empty() {
                    canvas.draw_text(
                        RectDip {
                            x: node.rect.x + 6.0,
                            y: node.rect.y + 4.0,
                            w: (node.rect.w - 12.0).max(0.0),
                            h: (node.rect.h - 8.0).max(0.0),
                        },
                        &TextSpec {
                            text: node.text.clone(),
                            size_dip: if node.role == "heading" { 20.0 } else { 14.0 },
                            weight: if node.action.is_some() { 600 } else { 400 },
                            font: FontRole::Text,
                        },
                        node.foreground,
                    )?;
                }
            }
            Ok(())
        })();
        let pop = canvas.pop_clip();
        result.and(pop)
    }
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
        let tokens = ctx
            .theme
            .with(|theme| {
                theme
                    .snapshot()
                    .map(|snapshot| PanelTokens::from_snapshot(&snapshot))
            })
            .unwrap_or_else(|_| PanelTokens::dark());
        Ok(Box::new(SpmPanel {
            ctx,
            key: input.key,
            config,
            subscription,
            mount: None,
            snapshot: None,
            snapshot_cursor: SnapshotCursor::for_query(project_query),
            layout: LayoutSnapshot::default(),
            layout_revision: 0,
            frame_revision: 0,
            selected_item: None,
            pending_actions: HashMap::new(),
            action_status: None,
            tokens,
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
    layout: LayoutSnapshot,
    layout_revision: u64,
    frame_revision: u64,
    selected_item: Option<usize>,
    pending_actions: HashMap<Token, (RpcMethod, Option<Revision>)>,
    action_status: Option<String>,
    tokens: PanelTokens,
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
            PanelEvent::Completion { operation, result } => {
                let Some((method, revision)) = self.pending_actions.remove(&operation) else {
                    return Ok(PanelUpdate::default());
                };
                let outcome = result.and_then(|bytes| self.finish_action(method, revision, &bytes));
                self.action_status = Some(match outcome {
                    Ok(message) => message,
                    Err(error) => format!("Action failed: {error}"),
                });
                Ok(PanelUpdate {
                    commands: vec![HostCommand::Invalidate],
                    relayout: false,
                })
            }
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
            PanelEvent::ThemeChanged { revision } => {
                // Colours changed but geometry did not: refresh tokens and
                // repaint without advancing the layout revision (keeps any
                // in-progress gesture's hit targets stable).
                if let Ok(snapshot) = self.ctx.theme.with(|theme| theme.snapshot()) {
                    if snapshot.revision == revision {
                        self.tokens = PanelTokens::from_snapshot(&snapshot);
                    }
                }
                Ok(PanelUpdate {
                    commands: vec![HostCommand::Invalidate],
                    relayout: false,
                })
            }
            PanelEvent::Invoke { action, .. } => {
                if action == ACTION_REFRESH {
                    self.queue_action(Request::Refresh(RefreshRequest {
                        project_id: self.config.project_id.clone(),
                        delivery_scope_id: self.config.delivery_scope_id.clone(),
                        idempotency_key: IdempotencyKey::new(),
                    }))?;
                } else if action == ACTION_EXPAND {
                    return Ok(PanelUpdate {
                        commands: vec![HostCommand::RequestMode(Presentation::Workspace)],
                        relayout: true,
                    });
                } else if action == ACTION_BACK_TO_LIST {
                    self.selected_item = None;
                    return Ok(PanelUpdate {
                        commands: vec![HostCommand::Invalidate],
                        relayout: true,
                    });
                } else if let Some(index) = action
                    .checked_sub(ACTION_SELECT_BASE)
                    .map(|value| value as usize)
                    && self
                        .snapshot
                        .as_ref()
                        .is_some_and(|snapshot| index < snapshot.preview_items.len())
                {
                    self.selected_item = Some(index);
                    return Ok(PanelUpdate {
                        commands: vec![HostCommand::Invalidate],
                        relayout: true,
                    });
                } else if action == ACTION_COPY_BRIEFING {
                    if let Some(snapshot) = &self.snapshot {
                        self.queue_action(Request::BuildBriefing(BuildBriefingRequest {
                            project_id: snapshot.project_id.clone(),
                            delivery_scope_id: snapshot.delivery_scope_id.clone(),
                            revision: snapshot.revision,
                            locale: "zh-CN".into(),
                            timezone: snapshot.project_timezone.clone(),
                        }))?;
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
                        self.queue_action(Request::ResolveNavigation(ResolveNavigationRequest {
                            source: target.clone(),
                            revision: snapshot.revision,
                        }))?;
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

    fn prepare_frame(&mut self, input: FrameInput) -> Result<Rc<dyn PreparedFrame>> {
        let mount_key = self.mount.ok_or(Error::Closed)?;
        self.frame_revision = self.frame_revision.checked_add(1).ok_or(Error::Exhausted)?;
        let visible = input.exposure != Exposure::Hidden && input.active;
        let mut view = if visible {
            build_presentation_view_with_selection(
                self.snapshot.as_ref(),
                input.presentation,
                input.viewport,
                self.selected_item,
                &self.tokens,
            )
        } else {
            PanelView::default()
        };
        if visible && let Some(status) = &self.action_status {
            view.nodes.push(ViewNode {
                id: u64::MAX - 1,
                role: "status",
                text: status.clone(),
                action: None,
                rect: RectDip {
                    x: input.viewport.x + 12.0,
                    y: (input.viewport.y + input.viewport.h - 24.0).max(input.viewport.y),
                    w: (input.viewport.w - 24.0).max(0.0),
                    h: 22.0,
                },
                fill: Some(self.tokens.panel),
                foreground: self.tokens.secondary,
            });
        }
        let candidate = layout_snapshot(&view, self.layout_revision);
        // Text, colors, freshness, and semantic labels may change without invalidating a
        // gesture. Only geometry/action mapping advances the layout revision.
        if candidate.hits != self.layout.hits {
            self.layout_revision = self
                .layout_revision
                .checked_add(1)
                .ok_or(Error::Exhausted)?;
        }
        self.layout = layout_snapshot(&view, self.layout_revision);
        Ok(Rc::new(SpmPreparedFrame {
            identity: FrameIdentity {
                mount_key,
                frame_revision: self.frame_revision,
                layout_revision: self.layout_revision,
                theme_epoch: input.theme_epoch,
                device_epoch: input.device_epoch,
            },
            view,
            layout: self.layout.clone(),
        }))
    }

    fn unmount(&mut self, key: MountKey) {
        if self.mount == Some(key) {
            self.mount = None;
        }
    }

    fn begin_stop(&mut self, _reason: StopReason) {
        self.stopped = true;
        for (operation, _) in self.pending_actions.drain() {
            let _ = self.ctx.ipc.with(|ipc| ipc.cancel(operation));
        }
        let _ = self.ctx.ipc.with(|ipc| ipc.cancel(self.subscription));
    }
}

impl SpmPanel {
    fn queue_action(&mut self, request: Request) -> Result<()> {
        let method = request.method();
        if self
            .pending_actions
            .values()
            .any(|(pending, _)| *pending == method)
        {
            self.action_status = Some("Action already pending".into());
            return Ok(());
        }
        let revision = match &request {
            Request::BuildBriefing(request) => Some(request.revision),
            Request::ResolveNavigation(request) => Some(request.revision),
            _ => None,
        };
        let payload = encode_request(&request)?;
        match self.ctx.ipc.with(|ipc| ipc.send(&self.ctx.scope, payload)) {
            Ok(operation) => {
                self.pending_actions.insert(operation, (method, revision));
                self.action_status = Some("Request pending".into());
            }
            Err(error) => self.action_status = Some(format!("Action failed: {error}")),
        }
        Ok(())
    }

    fn finish_action(
        &self,
        method: RpcMethod,
        revision: Option<Revision>,
        bytes: &[u8],
    ) -> Result<String> {
        let envelope = decode_event(bytes)?;
        match (method, envelope.body) {
            (RpcMethod::BuildBriefing, Body::Response(Response::BuildBriefing(response)))
                if revision == Some(response.revision) =>
            {
                self.ctx
                    .clipboard
                    .as_ref()
                    .ok_or_else(|| Error::Backend("clipboard unavailable".into()))?
                    .with(|clipboard| clipboard.write_text(&self.ctx.scope, response.text))?;
                Ok("Briefing copied".into())
            }
            (RpcMethod::Refresh, Body::Response(Response::Refresh(response))) => Ok(match response
                .disposition
            {
                spm_contracts::RefreshDisposition::Accepted => "Refresh accepted; awaiting data",
                spm_contracts::RefreshDisposition::Coalesced => "Refresh coalesced; awaiting data",
            }
            .into()),
            (RpcMethod::ResolveNavigation, Body::Response(Response::ResolveNavigation(_))) => Err(
                Error::Backend("navigation origin not configured; no link opened".into()),
            ),
            _ => Err(Error::Invalid(
                "unexpected action response or revision".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spm_contracts::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct Calls {
        requests: RefCell<Vec<Request>>,
        copied: RefCell<Vec<String>>,
        cancelled: RefCell<Vec<Token>>,
        opened: Cell<u64>,
    }
    struct Mock(Rc<Calls>);
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
        fn send(&self, scope: &ScopeHandle, bytes: Arc<[u8]>) -> Result<Token> {
            scope.check()?;
            let request =
                serde_json::from_slice(&bytes).map_err(|e| Error::Invalid(e.to_string()))?;
            let mut requests = self.0.requests.borrow_mut();
            requests.push(request);
            Ok(Token(requests.len() as u64 + 1))
        }
        fn cancel(&self, token: Token) -> Result<()> {
            self.0.cancelled.borrow_mut().push(token);
            Ok(())
        }
    }
    impl ClipboardService for Mock {
        fn write_text(&self, scope: &ScopeHandle, text: String) -> Result<Token> {
            scope.check()?;
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
            Err(Error::Revoked)
        }
    }
    impl ThemeService for Mock {
        fn snapshot(&self) -> Result<Arc<ThemeSnapshot>> {
            Err(Error::Revoked)
        }
    }
    impl DesktopService for Mock {
        fn request_mode(&self, _: InstanceKey, _: Presentation) -> Result<()> {
            Err(Error::Revoked)
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
    fn panel() -> (SpmPanel, Rc<Calls>, Vec<Rc<dyn std::any::Any>>) {
        let calls = Rc::new(Calls::default());
        let lease: Rc<dyn ScopeLease> = Rc::new(Mock(calls.clone()));
        let scope = ScopeHandle::from_lease(&lease);
        let mut keep: Vec<Rc<dyn std::any::Any>> = vec![Rc::new(lease)];
        let ctx = PluginContext {
            scope: scope.clone(),
            render: capability::<dyn RenderService>(
                Box::new(Mock(calls.clone())),
                &scope,
                &mut keep,
            ),
            theme: capability::<dyn ThemeService>(Box::new(Mock(calls.clone())), &scope, &mut keep),
            desktop: capability::<dyn DesktopService>(
                Box::new(Mock(calls.clone())),
                &scope,
                &mut keep,
            ),
            ipc: capability::<dyn IpcService>(Box::new(Mock(calls.clone())), &scope, &mut keep),
            storage: capability::<dyn StorageService>(
                Box::new(Mock(calls.clone())),
                &scope,
                &mut keep,
            ),
            clipboard: Some(capability::<dyn ClipboardService>(
                Box::new(Mock(calls.clone())),
                &scope,
                &mut keep,
            )),
            navigation: Some(capability::<dyn NavigationService>(
                Box::new(Mock(calls.clone())),
                &scope,
                &mut keep,
            )),
        };
        let config = SpmConfig {
            project_id: ProjectId::new("fictional-project").unwrap(),
            delivery_scope_id: DeliveryScopeId::new("fictional-scope").unwrap(),
        };
        (
            SpmPanel {
                ctx,
                key: InstanceKey {
                    id: 1,
                    activation: 1,
                },
                config,
                subscription: Token(1),
                mount: None,
                snapshot: None,
                snapshot_cursor: SnapshotCursor::default(),
                layout: LayoutSnapshot::default(),
                layout_revision: 0,
                frame_revision: 0,
                selected_item: None,
                pending_actions: HashMap::new(),
                action_status: None,
                tokens: PanelTokens::dark(),
                stopped: false,
            },
            calls,
            keep,
        )
    }

    fn completion(operation: Token, response: Response) -> PanelEvent {
        PanelEvent::Completion {
            operation,
            result: Ok(serde_json::to_vec(&Envelope {
                protocol_major: PROTOCOL_MAJOR,
                protocol_minor: PROTOCOL_MINOR,
                daemon_session: Some(DaemonSessionId::new()),
                request_id: Some(RequestId::new()),
                subscription_id: None,
                revision: None,
                body: Body::Response(response),
            })
            .unwrap()
            .into()),
        }
    }
    fn briefing(revision: u64) -> Response {
        Response::BuildBriefing(BuildBriefingResponse {
            briefing_id: BriefingId::new(),
            revision: Revision(revision),
            text: "Fictional briefing".into(),
            html: None,
            evidence_markers: vec![],
        })
    }
    fn request_briefing(panel: &mut SpmPanel) {
        panel
            .queue_action(Request::BuildBriefing(BuildBriefingRequest {
                project_id: panel.config.project_id.clone(),
                delivery_scope_id: panel.config.delivery_scope_id.clone(),
                revision: Revision(4),
                locale: "en".into(),
                timezone: "Etc/UTC".into(),
            }))
            .unwrap();
    }

    #[test]
    fn briefing_is_copied_only_after_its_matching_result_and_never_twice() {
        let (mut panel, calls, _keep) = panel();
        request_briefing(&mut panel);
        request_briefing(&mut panel);
        assert_eq!(calls.requests.borrow().len(), 1);
        assert!(calls.copied.borrow().is_empty());
        panel.event(completion(Token(99), briefing(4))).unwrap();
        assert!(calls.copied.borrow().is_empty());
        panel.event(completion(Token(2), briefing(4))).unwrap();
        panel.event(completion(Token(2), briefing(4))).unwrap();
        assert_eq!(&*calls.copied.borrow(), &["Fictional briefing"]);
        assert_eq!(panel.action_status.as_deref(), Some("Briefing copied"));
    }

    #[test]
    fn wrong_briefing_revision_never_writes_the_clipboard() {
        let (mut panel, calls, _keep) = panel();
        request_briefing(&mut panel);
        panel.event(completion(Token(2), briefing(3))).unwrap();
        assert!(calls.copied.borrow().is_empty());
        assert!(
            panel
                .action_status
                .unwrap()
                .contains("unexpected action response or revision")
        );
    }

    #[test]
    fn refresh_acceptance_is_not_reported_as_completed_data_and_stop_revokes_work() {
        let (mut panel, calls, _keep) = panel();
        panel
            .event(PanelEvent::Invoke {
                action: ACTION_REFRESH,
                layout_revision: 0,
            })
            .unwrap();
        panel
            .event(completion(
                Token(2),
                Response::Refresh(RefreshResponse {
                    operation_id: OperationId::new(),
                    disposition: RefreshDisposition::Accepted,
                }),
            ))
            .unwrap();
        assert_eq!(
            panel.action_status.as_deref(),
            Some("Refresh accepted; awaiting data")
        );
        assert!(panel.snapshot.is_none());
        request_briefing(&mut panel);
        panel.begin_stop(StopReason::Shutdown);
        assert!(calls.cancelled.borrow().contains(&Token(3)));
        assert!(calls.cancelled.borrow().contains(&Token(1)));
        assert_eq!(
            panel.event(completion(Token(3), briefing(4))).unwrap_err(),
            Error::Closed
        );
        assert!(calls.copied.borrow().is_empty());
    }

    #[test]
    fn navigation_resolution_never_opens_an_unconfigured_origin() {
        let (mut panel, calls, _keep) = panel();
        panel
            .pending_actions
            .insert(Token(2), (RpcMethod::ResolveNavigation, Some(Revision(4))));
        panel
            .event(completion(
                Token(2),
                Response::ResolveNavigation(ResolveNavigationResponse {
                    targets: vec![NavigationTarget {
                        system: SourceSystem::Jira,
                        configured_origin_id: "fictional-origin".into(),
                        relative_record_path: "//unapproved.example/record".into(),
                    }],
                }),
            ))
            .unwrap();
        assert_eq!(calls.opened.get(), 0);
        assert!(panel.action_status.unwrap().contains("no link opened"));
    }

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
        let a = build_view(Some(&data), viewport, &PanelTokens::dark());
        let b = build_view(Some(&data), viewport, &PanelTokens::dark());
        assert_eq!(a, b);
        let layout = layout_snapshot(&a, 4);
        assert_eq!(hit_test(&layout, 1020.0, 176.0), Some(ACTION_REFRESH));
        assert!(layout.semantics.iter().any(|node| node.role == "listitem"));
    }

    #[test]
    fn snapshot_revision_is_monotone_and_resets_for_a_new_daemon_session() {
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
        let restarted = DaemonSessionId::new();
        assert!(cursor.accept(&envelope(snapshot(restarted, 1))));
        assert!(!cursor.accept(&envelope(snapshot(restarted, 1))));
    }

    #[test]
    fn compact_contains_only_three_items_and_an_expand_action() {
        let mut data = snapshot(DaemonSessionId::new(), 1);
        let template = data.preview_items[0].clone();
        data.preview_items = (0..5)
            .map(|index| {
                let mut item = template.clone();
                item.title = format!("事项 {index}");
                item
            })
            .collect();
        let view = build_presentation_view(
            Some(&data),
            Presentation::Compact,
            RectDip {
                x: 0.0,
                y: 0.0,
                w: 480.0,
                h: 320.0,
            },
            &PanelTokens::dark(),
        );
        assert_eq!(
            view.nodes
                .iter()
                .filter(|node| node.role == "listitem")
                .count(),
            3
        );
        assert!(
            view.nodes
                .iter()
                .any(|node| node.action == Some(ACTION_EXPAND))
        );
        assert!(!view.nodes.iter().any(|node| node.role == "complementary"));
    }

    #[test]
    fn capsule_prioritizes_unknown_count_and_has_no_body_nodes() {
        let data = snapshot(DaemonSessionId::new(), 1);
        let view = build_presentation_view(
            Some(&data),
            Presentation::Capsule,
            RectDip {
                x: 0.0,
                y: 0.0,
                w: 360.0,
                h: 48.0,
            },
            &PanelTokens::dark(),
        );
        assert!(view.nodes.iter().any(|node| node.text.contains("未知 1")));
        assert!(!view.nodes.iter().any(|node| node.role == "listitem"));
        assert_eq!(
            view.nodes
                .iter()
                .filter(|node| node.action.is_some())
                .count(),
            1
        );
    }

    #[test]
    fn workspace_breakpoints_change_metrics_and_detail_layout() {
        let data = snapshot(DaemonSessionId::new(), 1);
        let wide = build_workspace_view(
            Some(&data),
            RectDip {
                x: 0.0,
                y: 0.0,
                w: 1120.0,
                h: 760.0,
            },
            Some(0),
            &PanelTokens::dark(),
        );
        assert!(wide.nodes.iter().any(|node| node.role == "complementary"));
        let narrow = build_workspace_view(
            Some(&data),
            RectDip {
                x: 0.0,
                y: 0.0,
                w: 600.0,
                h: 520.0,
            },
            Some(0),
            &PanelTokens::dark(),
        );
        assert!(narrow.nodes.iter().any(|node| node.role == "article"));
        assert!(!narrow.nodes.iter().any(|node| node.role == "listitem"));
        let metric_rows: Vec<f32> = narrow
            .nodes
            .iter()
            .filter(|node| node.role == "group")
            .map(|node| node.rect.y)
            .collect();
        assert_eq!(metric_rows, vec![88.0, 88.0, 164.0, 164.0]);
    }

    #[test]
    fn prepared_frame_paints_to_headless_canvas() {
        let data = snapshot(DaemonSessionId::new(), 1);
        let view = build_presentation_view(
            Some(&data),
            Presentation::Capsule,
            RectDip {
                x: 0.0,
                y: 0.0,
                w: 360.0,
                h: 48.0,
            },
            &PanelTokens::dark(),
        );
        let frame = SpmPreparedFrame {
            identity: FrameIdentity {
                mount_key: MountKey {
                    instance: InstanceKey {
                        id: 1,
                        activation: 1,
                    },
                    generation: 1,
                },
                frame_revision: 1,
                layout_revision: 1,
                theme_epoch: 1,
                device_epoch: 1,
            },
            layout: layout_snapshot(&view, 1),
            view,
        };
        let mut canvas = pecofence_render::HeadlessCanvas::default();
        frame.paint(&mut canvas).unwrap();
        assert!(canvas.is_balanced());
        assert!(!canvas.commands().is_empty());
    }
}

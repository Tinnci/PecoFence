//! First-party SPM panel adapter.
//!
//! Business decisions remain daemon-owned. This crate consumes versioned read-model snapshots and
//! turns them into layout, hit-test, semantic, and paint data without depending on Win32.

use pecofence_plugin_api::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const PROVIDER_ID: &str = "pecofence.spm";
const ACTION_REFRESH: u64 = 1;
const ACTION_COPY_BRIEFING: u64 = 2;
const ACTION_OPEN_BASE: u64 = 1_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpmConfig {
    pub project: String,
    pub delivery_scope: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpmSnapshot {
    pub daemon_session: String,
    pub revision: u64,
    pub project: String,
    pub delivery_scope: String,
    pub next_milestone: String,
    pub gate_status: String,
    pub unsatisfied: u32,
    pub unknown: u32,
    pub severe_open: u32,
    pub pending_verification: u32,
    pub customer_pending: u32,
    pub freshness: String,
    pub briefing: String,
    #[serde(default)]
    pub work: Vec<WorkItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: String,
    pub title: String,
    pub owner: String,
    pub due: String,
    pub next_step: String,
    #[serde(default)]
    pub targets: Vec<NavigationTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavigationTarget {
    pub system: String,
    pub https_uri: String,
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
    session: Option<String>,
    revision: u64,
}

impl SnapshotCursor {
    pub fn accept(&mut self, snapshot: &SpmSnapshot) -> bool {
        if self.session.as_deref() == Some(&snapshot.daemon_session)
            && snapshot.revision <= self.revision
        {
            return false;
        }
        self.session = Some(snapshot.daemon_session.clone());
        self.revision = snapshot.revision;
        true
    }
}

/// Builds presentation data solely from a read-model snapshot and viewport.
pub fn build_view(snapshot: Option<&SpmSnapshot>, viewport: RectDip) -> PanelView {
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
        format!("{} · {}", snapshot.project, snapshot.delivery_scope),
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
            snapshot.next_milestone, snapshot.gate_status, snapshot.unsatisfied, snapshot.unknown
        ),
        None,
        Some(subtle),
        primary,
    );
    let metric_width = ((viewport.w - 68.0) / 4.0).max(80.0);
    let metrics = [
        ("条件未满足", snapshot.unsatisfied),
        ("严重缺陷", snapshot.severe_open),
        ("待验证", snapshot.pending_verification),
        ("客户待验收", snapshot.customer_pending),
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
        snapshot.freshness.clone(),
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
    for (index, item) in snapshot.work.iter().take(visible).enumerate() {
        let y = row_top + index as f32 * row_height;
        let action = (!item.targets.is_empty()).then_some(ACTION_OPEN_BASE + index as u64);
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
                item.id, item.owner, item.due, item.title, item.next_step
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
            config_major: 1,
            required_services: &["render", "theme", "desktop", "ipc", "storage"],
        }
    }

    fn validate(&self, config: &PanelConfig) -> Result<()> {
        if config.version != 1 {
            return Err(Error::Invalid(
                "unsupported SPM panel config version".into(),
            ));
        }
        let parsed: SpmConfig = serde_json::from_slice(&config.bytes)
            .map_err(|error| Error::Invalid(error.to_string()))?;
        if parsed.project.trim().is_empty() || parsed.delivery_scope.trim().is_empty() {
            return Err(Error::Invalid(
                "project and delivery scope are required".into(),
            ));
        }
        Ok(())
    }

    fn create(&self, ctx: PluginContext, input: CreatePanel) -> Result<Box<dyn PanelInstance>> {
        self.validate(&input.config)?;
        let config: SpmConfig = serde_json::from_slice(&input.config.bytes)
            .map_err(|error| Error::Invalid(error.to_string()))?;
        let query = Query {
            endpoint: "spm.v2/read-model".into(),
            payload: Arc::from(input.config.bytes.as_ref()),
        };
        let subscription = ctx.ipc.with(|ipc| ipc.subscribe(&ctx.scope, query))?;
        Ok(Box::new(SpmPanel {
            ctx,
            key: input.key,
            config,
            subscription,
            mount: None,
            snapshot: None,
            snapshot_cursor: SnapshotCursor::default(),
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
    snapshot: Option<SpmSnapshot>,
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
                let snapshot: SpmSnapshot = serde_json::from_slice(&bytes)
                    .map_err(|error| Error::Invalid(error.to_string()))?;
                if !self.snapshot_cursor.accept(&snapshot) {
                    return Ok(PanelUpdate::default());
                }
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
                    let payload = Arc::from(
                        format!(
                            "{{\"project\":{:?},\"deliveryScope\":{:?}}}",
                            self.config.project, self.config.delivery_scope
                        )
                        .into_bytes(),
                    );
                    self.ctx
                        .ipc
                        .with(|ipc| ipc.send(&self.ctx.scope, payload))?;
                } else if action == ACTION_COPY_BRIEFING {
                    if let (Some(clipboard), Some(snapshot)) = (&self.ctx.clipboard, &self.snapshot)
                    {
                        clipboard.with(|service| {
                            service.write_text(&self.ctx.scope, snapshot.briefing.clone())
                        })?;
                    }
                } else if let Some(index) = action
                    .checked_sub(ACTION_OPEN_BASE)
                    .map(|value| value as usize)
                    && let Some(target) = self
                        .snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.work.get(index))
                        .and_then(|item| item.targets.first())
                    && let Some(navigation) = &self.ctx.navigation
                {
                    navigation.with(|service| {
                        service.open(
                            &self.ctx.scope,
                            ExternalTarget {
                                system: target.system.clone(),
                                https_uri: target.https_uri.clone(),
                            },
                        )
                    })?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> SpmSnapshot {
        SpmSnapshot {
            daemon_session: "test".into(),
            revision: 1,
            project: "Atlas".into(),
            delivery_scope: "EU".into(),
            next_milestone: "TR5".into(),
            gate_status: "Unknown".into(),
            unsatisfied: 2,
            unknown: 1,
            severe_open: 5,
            pending_verification: 11,
            customer_pending: 3,
            freshness: "Jira 09:20 · Gerrit 覆盖未知".into(),
            briefing: "brief".into(),
            work: vec![WorkItem {
                id: "C-104".into(),
                title: "确认验收".into(),
                owner: "Owner".into(),
                due: "09-25".into(),
                next_step: "确认结果".into(),
                targets: vec![],
            }],
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
        let a = build_view(Some(&snapshot()), viewport);
        let b = build_view(Some(&snapshot()), viewport);
        assert_eq!(a, b);
        let layout = layout_snapshot(&a, 4);
        assert_eq!(hit_test(&layout, 1020.0, 176.0), Some(ACTION_REFRESH));
        assert!(layout.semantics.iter().any(|node| node.role == "listitem"));
    }

    #[test]
    fn snapshot_revision_is_monotone_within_a_daemon_session() {
        let mut cursor = SnapshotCursor::default();
        let mut current = snapshot();
        assert!(cursor.accept(&current));
        assert!(!cursor.accept(&current));
        current.revision = 0;
        assert!(!cursor.accept(&current));
        current.daemon_session = "restarted".into();
        assert!(cursor.accept(&current));
    }
}

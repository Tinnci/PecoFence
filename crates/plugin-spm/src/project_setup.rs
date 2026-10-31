//! A live panel starts without an invented project. The existing host editor
//! collects identifiers until the backend exposes a project-discovery contract.
use pecofence_plugin_api::{
    scene::{NodeStyle, Scene, SceneNode},
    *,
};
use serde::Deserialize;
use std::{rc::Rc, sync::Arc};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Unselected {
    selection: Option<()>,
}

pub fn unselected(bytes: &[u8]) -> bool {
    // Require the explicit marker; malformed configured panels must still fail.
    serde_json::from_slice::<serde_json::Value>(bytes).is_ok_and(|v| v.get("selection").is_some())
        && serde_json::from_slice::<Unselected>(bytes).is_ok_and(|v| v.selection.is_none())
}

pub fn live_config() -> (u16, serde_json::Value) {
    (1, serde_json::json!({"selection": null}))
}

#[derive(Clone, Copy)]
pub enum Kind {
    Board,
    Issues,
}
impl Kind {
    fn config(self, text: &str) -> Result<serde_json::Value> {
        let ids: Vec<_> = text.split_whitespace().collect();
        if ids.len()
            != match self {
                Self::Board => 2,
                Self::Issues => 1,
            }
        {
            return Err(Error::Invalid(
                "Enter the project and, for a board, its delivery scope".into(),
            ));
        }
        let project =
            spm_contracts::ProjectId::new(ids[0]).map_err(|e| Error::Invalid(e.to_string()))?;
        Ok(match self {
            Self::Board => {
                let scope = spm_contracts::DeliveryScopeId::new(ids[1])
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                serde_json::json!({"project_id": project, "delivery_scope_id": scope, "source": "spm"})
            }
            Self::Issues => serde_json::json!({"project_id": project, "demo": false}),
        })
    }
}

pub struct ProjectSetup {
    ctx: Option<PluginContext>,
    key: InstanceKey,
    kind: Kind,
    mount: Option<MountContext>,
    inner: Option<Box<dyn PanelInstance>>,
    notice: Option<String>,
    revision: u64,
    stopped: bool,
    timeline: bool,
}
impl ProjectSetup {
    pub fn new(ctx: PluginContext, key: InstanceKey, kind: Kind) -> Self {
        Self {
            ctx: Some(ctx),
            key,
            kind,
            mount: None,
            inner: None,
            notice: None,
            revision: 0,
            stopped: false,
            timeline: false,
        }
    }
}
impl PanelInstance for ProjectSetup {
    fn mount(&mut self, ctx: MountContext) -> Result<()> {
        if self.stopped {
            return Err(Error::Closed);
        }
        if ctx.key.instance != self.key {
            return Err(Error::Invalid("wrong setup instance".into()));
        }
        if let Some(inner) = &mut self.inner {
            return inner.mount(ctx);
        }
        self.mount = Some(ctx);
        Ok(())
    }
    fn event(&mut self, event: PanelEvent) -> Result<PanelUpdate> {
        if self.stopped {
            return Err(Error::Closed);
        }
        if let Some(inner) = &mut self.inner {
            return inner.event(event);
        }
        match event {
            PanelEvent::Invoke {
                layout_revision, ..
            } if layout_revision != self.revision => return Ok(PanelUpdate::default()),
            PanelEvent::Context { topic, .. } if topic == "board.timeline" => self.timeline = true,
            PanelEvent::Command { id: 1 }
            | PanelEvent::Invoke {
                action: 1,
                layout_revision: _,
            } => {
                return Ok(PanelUpdate {
                    commands: vec![HostCommand::RequestTextInput {
                        action: 1,
                        initial: String::new(),
                        multiline: false,
                        style: None,
                    }],
                    relayout: false,
                });
            }
            PanelEvent::TextSubmitted { action: 1, text } => match self.kind.config(&text) {
                Ok(config) => {
                    let bytes: Arc<[u8]> = serde_json::to_vec(&config)
                        .map_err(|e| Error::Invalid(e.to_string()))?
                        .into();
                    let saved = self.ctx.as_ref().ok_or(Error::Closed)?;
                    let ctx = PluginContext {
                        scope: saved.scope.clone(),
                        render: saved.render.clone(),
                        theme: saved.theme.clone(),
                        desktop: saved.desktop.clone(),
                        ipc: saved.ipc.clone(),
                        storage: saved.storage.clone(),
                        navigation: saved.navigation.clone(),
                        clipboard: saved.clipboard.clone(),
                    };
                    let input = CreatePanel {
                        key: self.key,
                        config: PanelConfig {
                            version: 1,
                            bytes: bytes.clone(),
                        },
                    };
                    let mut inner = match self.kind {
                        Kind::Board => crate::board::BoardPlugin.create(ctx, input)?,
                        Kind::Issues => crate::issues::IssuesPlugin.create(ctx, input)?,
                    };
                    if let Some(mount) = &self.mount {
                        inner.mount(MountContext {
                            key: mount.key,
                            scope: mount.scope.clone(),
                            viewport: mount.viewport,
                            dpi: mount.dpi,
                        })?;
                    }
                    let mut update = PanelUpdate {
                        commands: vec![HostCommand::PersistConfig(bytes), HostCommand::Invalidate],
                        relayout: true,
                    };
                    if self.timeline {
                        update.merge(inner.event(PanelEvent::Context {
                            topic: "board.timeline".into(),
                            json: "{}".into(),
                        })?);
                    }
                    self.inner = Some(inner);
                    self.ctx = None;
                    return Ok(update);
                }
                Err(_) => {
                    self.notice =
                        Some("项目标识无效，请重新选择 / Invalid project identifiers".into())
                }
            },
            _ => {}
        }
        Ok(PanelUpdate {
            commands: vec![
                HostCommand::SetChrome(PanelChrome {
                    title: match self.kind {
                        Kind::Board => "交付看板",
                        Kind::Issues => "我的问题",
                    }
                    .into(),
                    subtitle: "选择项目 / Choose a project".into(),
                    status: None,
                    surface: None,
                    commands: vec![ChromeCommand {
                        id: 1,
                        label: "选择项目 / Choose project".into(),
                        glyph: '\u{E8B7}',
                        enabled: true,
                    }],
                }),
                HostCommand::Invalidate,
            ],
            relayout: true,
        })
    }
    fn prepare_frame(&mut self, input: FrameInput) -> Result<Rc<dyn PreparedFrame>> {
        if self.stopped {
            return Err(Error::Closed);
        }
        if let Some(inner) = &mut self.inner {
            return inner.prepare_frame(input);
        }
        self.revision += 1;
        let zh = input.locale.starts_with("zh");
        let text = self.notice.clone().unwrap_or_else(|| {
            match (zh, self.kind) {
                (true, Kind::Board) => "选择项目：输入项目标识和交付范围（空格分隔）",
                (true, Kind::Issues) => "选择项目：输入项目标识",
                (false, Kind::Board) => {
                    "Choose a project: enter project ID and delivery scope, separated by a space"
                }
                (false, Kind::Issues) => "Choose a project: enter project ID",
            }
            .into()
        });
        let theme = self
            .ctx
            .as_ref()
            .ok_or(Error::Closed)?
            .theme
            .with(|t| t.snapshot())
            .unwrap_or_else(|_| Arc::new(ThemeSnapshot::dark()));
        let rect = input.viewport;
        Ok(Rc::new(Scene::new(
            FrameIdentity {
                mount_key: self.mount.as_ref().ok_or(Error::Closed)?.key,
                frame_revision: self.revision,
                layout_revision: self.revision,
                theme_epoch: input.theme_epoch,
                device_epoch: input.device_epoch,
            },
            rect,
            vec![SceneNode {
                id: 1,
                parent: None,
                rect,
                role: "button".into(),
                name: text.clone(),
                text,
                action: Some(1),
                style: NodeStyle {
                    size: 16.0 * input.text_scale.max(1.0),
                    weight: 400,
                    font: FontRole::Text,
                    foreground: theme.text_primary,
                    fill: None,
                    stroke: None,
                    radius: 0.0,
                    text_inset: [16.0, 12.0],
                    align: TextAlign::MiddleLeading,
                    path: None,
                },
            }],
        )?))
    }
    fn unmount(&mut self, key: MountKey) {
        if let Some(inner) = &mut self.inner {
            inner.unmount(key);
        }
        if self.mount.as_ref().is_some_and(|m| m.key == key) {
            self.mount = None;
        }
    }
    fn begin_stop(&mut self, reason: StopReason) {
        self.stopped = true;
        if let Some(inner) = &mut self.inner {
            inner.begin_stop(reason);
        }
        self.ctx = None;
    }
    fn next_wake_after(&self) -> Option<std::time::Duration> {
        self.inner.as_ref().and_then(|p| p.next_wake_after())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_empty_selection_is_unconfigured() {
        assert!(unselected(br#"{"selection":null}"#));
        for bad in [
            br#"{}"#.as_slice(),
            br#"{"selection":"x"}"#,
            br#"{"selection":null,"source":"demo"}"#,
        ] {
            assert!(!unselected(bad));
        }
    }
    #[test]
    fn selection_builds_live_configs_and_validates_identifiers() {
        let board = Kind::Board.config("project-a scope-a").unwrap();
        assert_eq!(board["source"], "spm");
        assert_eq!(board["project_id"], "project-a");
        assert_eq!(Kind::Issues.config("project-a").unwrap()["demo"], false);
        assert!(Kind::Board.config("project-a").is_err());
        assert!(Kind::Issues.config("a b").is_err());
    }
}

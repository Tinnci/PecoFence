//! UI-thread adapter from committed scenes to UIA queries and deferred plugin actions.
use super::*;
use pecofence_platform::accessibility::{Bounds, Node, NodeId, Snapshot};
use pecofence_plugin_api::{RectDip, SemanticNode};

fn identity(namespace: i32, value: u128) -> NodeId {
    let mut id = vec![namespace];
    append128(&mut id, value);
    id
}
fn append128(id: &mut NodeId, value: u128) {
    for part in value.to_le_bytes().as_chunks::<4>().0 {
        id.push(i32::from_le_bytes(*part));
    }
}
fn append64(id: &mut NodeId, value: u64) {
    id.extend([value as i32, (value >> 32) as i32]);
}
fn clipped_bounds(
    rect: RectDip,
    viewport: RectDip,
    origin: (f64, f64),
    scale: f64,
    window: Bounds,
) -> Bounds {
    let x = f64::from(rect.x.max(viewport.x)) * scale + origin.0;
    let y = f64::from(rect.y.max(viewport.y)) * scale + origin.1;
    let right = f64::from((rect.x + rect.w).min(viewport.x + viewport.w)) * scale + origin.0;
    let bottom = f64::from((rect.y + rect.h).min(viewport.y + viewport.h)) * scale + origin.1;
    let left = x.max(window.x);
    let top = y.max(window.y);
    Bounds {
        x: left,
        y: top,
        width: (right.min(window.x + window.width) - left).max(0.0),
        height: (bottom.min(window.y + window.height) - top).max(0.0),
    }
}
fn semantic_node<Id>(
    semantic: &SemanticNode<Id>,
    id: NodeId,
    parent: Option<NodeId>,
    bounds: Bounds,
    enabled: bool,
) -> Node {
    Node {
        id,
        parent,
        role: semantic.role.clone(),
        name: semantic.name.clone(),
        bounds,
        enabled,
        invoke: None,
    }
}

impl FenceViewState {
    pub(super) fn publish_accessibility(&self) {
        if !self.accessibility.is_connected() {
            return;
        }
        let result = self.accessibility.publish(self.accessibility_snapshot());
        if let Err(error) = result {
            tracing::warn!(error, "accessibility snapshot rejected");
        }
    }
    fn accessibility_snapshot(&self) -> Snapshot {
        let window_rect = window::window_rect(self.hwnd);
        let bounds = Bounds {
            x: f64::from(window_rect.left),
            y: f64::from(window_rect.top),
            width: f64::from((window_rect.right - window_rect.left).max(0)),
            height: f64::from((window_rect.bottom - window_rect.top).max(0)),
        };
        let lifecycle = self.input_lifecycle();
        let mut snapshot = Snapshot {
            name: self
                .committed_chrome_scene()
                .and_then(|s| s.semantics().first())
                .map_or_else(|| self.title.clone(), |n| n.name.clone()),
            bounds,
            visible: lifecycle.window(),
            nodes: Vec::new(),
        };
        if !snapshot.visible {
            return snapshot;
        }
        let client = window::client_to_screen(self.hwnd, pecofence_platform::POINT::default());
        let origin = (f64::from(client.x), f64::from(client.y));
        let scale = f64::from(self.scale());
        if let Some(scene) = self.committed_chrome_scene() {
            let key = |target: super::chrome_scene::ChromeTarget| {
                use super::chrome_scene::ChromeTarget;
                let (content, kind) = match target {
                    ChromeTarget::Title(id) => (id, 0),
                    ChromeTarget::Tab(id) => (id, 1),
                    ChromeTarget::Up => (self.active, 2),
                    ChromeTarget::Chevron => (self.active, 3),
                };
                let mut id = identity(1, content.as_u128());
                id.push(kind);
                id
            };
            for node in scene.semantics() {
                snapshot.nodes.push(semantic_node(
                    node,
                    key(node.id),
                    node.parent.map(key),
                    clipped_bounds(node.rect, scene.viewport(), origin, scale, bounds),
                    true,
                ));
            }
        }
        if !lifecycle.body() {
            return snapshot;
        }
        let origin = (origin.0, origin.1 + f64::from(self.title_h_px()));
        if let Some(panel) = &self.plugin_panel {
            let device = (u64::from(self.stack.device_generation()) << 32) | u64::from(self.dpi);
            if let Some(frame) =
                panel.accessibility_frame(self.content_panel.surface_epoch(), self.dpi, device)
            {
                let mount = frame.identity().mount_key;
                let mut prefix = identity(3, self.active.as_u128());
                append128(&mut prefix, mount.instance.id);
                append64(&mut prefix, mount.instance.activation);
                append64(&mut prefix, mount.generation);
                let key = |node_id: u64| {
                    let mut id = prefix.clone();
                    append64(&mut id, node_id);
                    id
                };
                let (width, height) = self.content_size_px();
                let viewport = RectDip {
                    x: 0.0,
                    y: 0.0,
                    w: width as f32 / self.scale(),
                    h: height as f32 / self.scale(),
                };
                for node in frame.semantics() {
                    let mut exported = semantic_node(
                        node,
                        key(node.id),
                        node.parent.map(key),
                        clipped_bounds(node.rect, viewport, origin, scale, bounds),
                        true,
                    );
                    if node.role == "button" {
                        exported.invoke = node
                            .action
                            .filter(|action| {
                                frame
                                    .hit_tree()
                                    .iter()
                                    .any(|hit| hit.id == node.id && hit.action == *action)
                            })
                            .map(|action| invocation_token(frame.identity(), action));
                        exported.enabled = exported.invoke.is_some();
                    }
                    snapshot.nodes.push(exported);
                }
            }
        } else if let Some(scene) = self.committed_file_scene() {
            let group = identity(2, self.active.as_u128());
            snapshot.nodes.push(Node {
                id: group.clone(),
                parent: None,
                role: "list".into(),
                name: snapshot.name.clone(),
                bounds: clipped_bounds(scene.viewport, scene.viewport, origin, scale, bounds),
                enabled: true,
                invoke: None,
            });
            for node in scene.accessibility_nodes() {
                let mut id = group.clone();
                append128(&mut id, node.id.as_u128());
                snapshot.nodes.push(semantic_node(
                    node,
                    id,
                    Some(group.clone()),
                    clipped_bounds(node.rect, scene.viewport, origin, scale, bounds),
                    true,
                ));
            }
        }
        snapshot
    }
}

fn invocation_token(identity: pecofence_plugin_api::FrameIdentity, action: u64) -> Vec<i32> {
    let mut token = Vec::new();
    for value in [action, identity.layout_revision, identity.device_epoch] {
        append64(&mut token, value);
    }
    token
}
fn matches_invocation(
    snapshot: &Snapshot,
    request: &pecofence_platform::accessibility::InvokeRequest,
) -> bool {
    snapshot.visible
        && request.token.len() == 6
        && snapshot.nodes.iter().any(|node| {
            node.id == request.id
                && node.enabled
                && node.bounds.width > 0.0
                && node.bounds.height > 0.0
                && node.invoke.as_ref() == Some(&request.token)
        })
}

impl FenceViewState {
    /// One accepted activation path for pointer, keyboard and UIA. The provider
    /// updates its state first; its notice is queued before presentation can
    /// remove the source node. Native event delivery happens outside the borrow.
    pub(super) fn invoke_plugin_control(
        &mut self,
        control: u64,
        action: u64,
        layout_revision: u64,
    ) -> pecofence_plugin_api::Result<()> {
        if !self.input_lifecycle().body() {
            return Err(pecofence_plugin_api::Error::Invalid(
                "inactive plugin input".into(),
            ));
        }
        let suffix = [control as i32, (control >> 32) as i32];
        let notice = self
            .accessibility_snapshot()
            .nodes
            .into_iter()
            .find(|node| {
                node.id.first() == Some(&3) && node.id.ends_with(&suffix) && node.invoke.is_some()
            })
            .map(|node| node.id);
        let panel = self
            .plugin_panel
            .as_ref()
            .ok_or(pecofence_plugin_api::Error::Closed)?;
        let update = panel.invoke_committed(control, action, layout_revision)?;
        if let Some(id) = notice {
            self.accessibility.queue_invoked(&id);
        }
        self.apply_plugin_update(update)
            .map_err(|error| pecofence_plugin_api::Error::Backend(error.to_string()))
    }
    fn invoke_accessible(&mut self, request: pecofence_platform::accessibility::InvokeRequest) {
        if !self.input_lifecycle().body()
            || !matches_invocation(&self.accessibility_snapshot(), &request)
        {
            return;
        }
        let value = |offset| {
            u64::from(request.token[offset] as u32)
                | (u64::from(request.token[offset + 1] as u32) << 32)
        };
        let action = value(0);
        let layout_revision = value(2);
        let Some(&[low, high]) = request.id.last_chunk::<2>() else {
            return;
        };
        let control = u64::from(low as u32) | (u64::from(high as u32) << 32);
        if let Err(error) = self.invoke_plugin_control(control, action, layout_revision) {
            tracing::warn!(%error, "accessible plugin action failed");
        }
    }
}

pub(super) fn handle_message(
    h: &super::handler::HandlerCtx,
    hwnd: HWND,
    message: u32,
    wparam: usize,
) -> Option<isize> {
    const RETRY_TIMER: usize = 45;
    if message != pecofence_platform::accessibility::ACTION_MESSAGE
        && !(message == msg::WM_TIMER && wparam == RETRY_TIMER)
    {
        return None;
    }
    window::kill_timer(hwnd, RETRY_TIMER);
    let Ok(mut guard) = h.view.try_borrow_mut() else {
        window::set_timer(hwnd, RETRY_TIMER, 16);
        return Some(0);
    };
    if let Some(view) = guard.as_mut() {
        for request in view.accessibility.take_invocations() {
            view.invoke_accessible(request);
        }
    }
    Some(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accessible_geometry_clips_in_dips_then_converts_only_display_scale() {
        let viewport = RectDip {
            x: 0.0,
            y: 28.0,
            w: 200.0,
            h: 72.0,
        };
        let rect = RectDip {
            x: 10.0,
            y: 20.0,
            w: 50.0,
            h: 40.0,
        };
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let window = Bounds {
                x: -500.0,
                y: 100.0,
                width: 500.0,
                height: 500.0,
            };
            let result = clipped_bounds(rect, viewport, (-500.0, 100.0), scale, window);
            assert_eq!(result.x, -500.0 + 10.0 * scale);
            assert_eq!(result.y, 100.0 + 28.0 * scale);
            assert_eq!(result.width, 50.0 * scale);
            assert_eq!(result.height, 32.0 * scale);
        }
    }
    #[test]
    fn runtime_identity_keeps_uuid_and_generation_bits() {
        assert_ne!(identity(1, 1), identity(1, (1 << 96) | 1));
        assert_ne!(identity(1, 1), identity(2, 1));
        let mut a = identity(3, 1);
        let mut b = a.clone();
        append64(&mut a, 1);
        append64(&mut b, (1 << 40) | 1);
        assert_ne!(a, b);
    }
    #[test]
    fn queued_invocation_requires_current_identity_action_epochs_and_visible_geometry() {
        use pecofence_platform::accessibility::InvokeRequest;
        let token = vec![7, 0, 10, 0, 30, 0];
        let mut snapshot = Snapshot {
            visible: true,
            nodes: vec![Node {
                id: vec![3, 1, 2],
                parent: None,
                role: "button".into(),
                name: "Action".into(),
                bounds: Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
                enabled: true,
                invoke: Some(token.clone()),
            }],
            ..Default::default()
        };
        let request = InvokeRequest {
            id: snapshot.nodes[0].id.clone(),
            token,
        };
        assert!(matches_invocation(&snapshot, &request));
        for index in 0..6 {
            let mut stale = request.clone();
            stale.token[index] ^= 1;
            assert!(!matches_invocation(&snapshot, &stale));
        }
        let mut other = request.clone();
        other.id.push(1);
        assert!(!matches_invocation(&snapshot, &other));
        snapshot.nodes[0].enabled = false;
        assert!(!matches_invocation(&snapshot, &request));
        snapshot.nodes[0].enabled = true;
        snapshot.nodes[0].bounds.height = 0.0;
        assert!(!matches_invocation(&snapshot, &request));
        snapshot.nodes[0].bounds.height = 10.0;
        snapshot.visible = false;
        assert!(!matches_invocation(&snapshot, &request));
    }
    #[test]
    fn invocation_identity_preserves_theme_only_updates_but_rejects_action_layout_and_device_changes()
     {
        use pecofence_plugin_api::{FrameIdentity, InstanceKey, MountKey};
        let identity = FrameIdentity {
            mount_key: MountKey {
                instance: InstanceKey {
                    id: 1,
                    activation: 1,
                },
                generation: 1,
            },
            frame_revision: 1,
            layout_revision: 2,
            theme_epoch: 3,
            device_epoch: 4,
        };
        let original = invocation_token(identity, 7);
        assert_eq!(
            original,
            invocation_token(
                FrameIdentity {
                    theme_epoch: 99,
                    frame_revision: 99,
                    ..identity
                },
                7
            )
        );
        assert_ne!(
            original,
            invocation_token(
                FrameIdentity {
                    layout_revision: 99,
                    ..identity
                },
                7
            )
        );
        assert_ne!(
            original,
            invocation_token(
                FrameIdentity {
                    device_epoch: 99,
                    ..identity
                },
                7
            )
        );
        assert_ne!(original, invocation_token(identity, 8));
    }
}

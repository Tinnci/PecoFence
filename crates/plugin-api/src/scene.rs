//! An immutable content frame. Providers describe nodes once; painting, hit
//! geometry and accessibility are projections of that same description.
use crate::design::Component;
use crate::{
    Canvas, Error, FontRole, FrameIdentity, HitNode, PreparedFrame, RectDip, Result, SemanticNode,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq)]
pub struct NodeStyle {
    pub size: f32,
    pub weight: u16,
    pub font: FontRole,
    pub foreground: [f32; 4],
    pub fill: Option<[f32; 4]>,
    pub stroke: Option<crate::StrokeStyle>,
    pub radius: f32,
    pub text_inset: [f32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneNode {
    pub id: u64,
    pub parent: Option<u64>,
    pub rect: RectDip,
    pub role: String,
    pub name: String,
    pub text: String,
    pub action: Option<u64>,
    pub style: NodeStyle,
}

#[derive(Clone, Debug)]
pub struct Scene {
    identity: FrameIdentity,
    viewport: RectDip,
    nodes: Vec<SceneNode>,
    hits: Vec<HitNode>,
    semantics: Vec<SemanticNode>,
}

fn valid_rect(rect: RectDip) -> bool {
    [
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        rect.x + rect.w,
        rect.y + rect.h,
    ]
    .iter()
    .all(|n| n.is_finite())
        && rect.w >= 0.0
        && rect.h >= 0.0
}

impl Scene {
    /// Validate before touching a visible surface. A rejected candidate must not
    /// replace the committed frame or leave a partially updated input tree.
    pub fn new(identity: FrameIdentity, viewport: RectDip, nodes: Vec<SceneNode>) -> Result<Self> {
        if !valid_rect(viewport) {
            return Err(Error::Invalid(
                "non-finite or negative scene viewport".into(),
            ));
        }
        let mut ids = HashSet::new();
        for node in &nodes {
            let style = &node.style;
            let color_valid = |rgba: [f32; 4]| {
                rgba.iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            };
            if !ids.insert(node.id) {
                return Err(Error::Invalid(format!("duplicate scene node {}", node.id)));
            }
            if !valid_rect(node.rect)
                || !style.size.is_finite()
                || style.size <= 0.0
                || !style.radius.is_finite()
                || style.radius < 0.0
                || !style.text_inset.iter().all(|v| v.is_finite() && *v >= 0.0)
                || !color_valid(style.foreground)
                || style.fill.is_some_and(|color| !color_valid(color))
                || style.stroke.is_some_and(|stroke| {
                    !color_valid(stroke.rgba) || !stroke.width.is_finite() || stroke.width <= 0.0
                })
            {
                return Err(Error::Invalid(format!(
                    "invalid scene geometry/style for node {}",
                    node.id
                )));
            }
        }
        for node in &nodes {
            if node
                .parent
                .is_some_and(|parent| parent == node.id || !ids.contains(&parent))
            {
                return Err(Error::Invalid(format!(
                    "invalid parent for scene node {}",
                    node.id
                )));
            }
        }
        let parents: HashMap<_, _> = nodes.iter().map(|node| (node.id, node.parent)).collect();
        let mut checked = HashSet::new();
        for node in &nodes {
            let mut path = HashSet::new();
            let mut cursor = Some(node.id);
            while let Some(id) = cursor {
                if checked.contains(&id) {
                    break;
                }
                if !path.insert(id) {
                    return Err(Error::Invalid(format!(
                        "cycle in scene parents at node {id}"
                    )));
                }
                cursor = parents[&id];
            }
            checked.extend(path);
        }
        let hits = nodes
            .iter()
            .filter_map(|node| {
                node.action
                    .filter(|_| node.rect.w > 0.0 && node.rect.h > 0.0)
                    .map(|action| HitNode {
                        id: node.id,
                        rect: node.rect,
                        action,
                    })
            })
            .collect();
        let semantics = nodes
            .iter()
            .map(|node| SemanticNode {
                id: node.id,
                parent: node.parent,
                role: node.role.clone(),
                name: node.name.clone(),
                rect: node.rect,
                action: node.action,
            })
            .collect();
        Ok(Self {
            identity,
            viewport,
            nodes,
            hits,
            semantics,
        })
    }

    pub fn nodes(&self) -> &[SceneNode] {
        &self.nodes
    }
    pub fn viewport(&self) -> RectDip {
        self.viewport
    }

    pub fn with_layout_revision(mut self, revision: u64) -> Self {
        self.identity.layout_revision = revision;
        self
    }
}

impl PreparedFrame for Scene {
    fn identity(&self) -> FrameIdentity {
        self.identity
    }
    fn hit_tree(&self) -> &[HitNode] {
        &self.hits
    }
    fn semantics(&self) -> &[SemanticNode] {
        &self.semantics
    }

    fn control_corner_radius(&self, id: u64) -> Option<f32> {
        self.nodes
            .iter()
            .find(|node| node.id == id && node.action.is_some())
            .map(|node| {
                node.style
                    .radius
                    .min(node.rect.w / 2.0)
                    .min(node.rect.h / 2.0)
            })
    }

    fn paint(&self, canvas: &mut dyn Canvas) -> Result<()> {
        canvas.push_clip(self.viewport)?;
        let painted = (|| {
            for node in &self.nodes {
                let style = &node.style;
                Component {
                    rect: node.rect,
                    text: &node.text,
                    size: style.size,
                    weight: style.weight,
                    font: style.font,
                    foreground: style.foreground,
                    fill: style.fill,
                    stroke: style.stroke,
                    radius: style.radius,
                    text_inset: style.text_inset,
                }
                .paint(canvas)?;
            }
            Ok(())
        })();
        painted.and(canvas.pop_clip())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InstanceKey, MountKey};

    fn identity() -> FrameIdentity {
        FrameIdentity {
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
        }
    }
    fn node(id: u64) -> SceneNode {
        SceneNode {
            id,
            parent: None,
            rect: RectDip {
                x: 2.0,
                y: 3.0,
                w: 80.0,
                h: 32.0,
            },
            role: "button".into(),
            name: "Accessible action".into(),
            text: "Action".into(),
            action: Some(7),
            style: NodeStyle {
                stroke: None,
                size: 14.0,
                weight: 400,
                font: FontRole::Text,
                foreground: [1.0; 4],
                fill: None,
                radius: 4.0,
                text_inset: [0.0; 2],
            },
        }
    }
    fn scene(nodes: Vec<SceneNode>) -> Result<Scene> {
        Scene::new(
            identity(),
            RectDip {
                x: 0.0,
                y: 0.0,
                w: 200.0,
                h: 100.0,
            },
            nodes,
        )
    }
    #[test]
    fn projections_keep_node_identity_geometry_and_accessible_name() {
        let mut input = node(1);
        let frame = scene(vec![input.clone()]).unwrap();
        input.rect.x = 99.0;
        assert_eq!(frame.hit_tree()[0].rect, frame.nodes()[0].rect);
        assert_eq!(frame.semantics()[0].rect, frame.nodes()[0].rect);
        assert_eq!(frame.hit_tree()[0].id, frame.semantics()[0].id);
        assert_eq!(frame.semantics()[0].name, "Accessible action");
        assert_eq!(frame.nodes()[0].rect.x, 2.0);
        assert_eq!(frame.with_layout_revision(8).identity().layout_revision, 8);
    }
    #[test]
    fn malformed_geometry_style_and_duplicate_ids_are_rejected() {
        assert!(scene(vec![node(1), node(1)]).is_err());
        let mut invalid = node(1);
        invalid.rect.w = f32::NAN;
        assert!(scene(vec![invalid]).is_err());
        let mut invalid = node(1);
        invalid.style.fill = Some([2.0; 4]);
        assert!(scene(vec![invalid]).is_err());
        let mut invalid = node(1);
        invalid.style.text_inset[0] = -1.0;
        assert!(scene(vec![invalid]).is_err());
        for width in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let mut invalid = node(1);
            invalid.style.stroke = Some(crate::StrokeStyle {
                rgba: [1.0; 4],
                width,
            });
            assert!(scene(vec![invalid]).is_err());
        }
        let mut invalid = node(1);
        invalid.style.stroke = Some(crate::StrokeStyle {
            rgba: [2.0; 4],
            width: 1.0,
        });
        assert!(scene(vec![invalid]).is_err());
    }
    #[test]
    fn missing_parents_and_cycles_are_rejected_but_unordered_trees_are_valid() {
        let mut child = node(2);
        child.parent = Some(1);
        assert!(scene(vec![child.clone()]).is_err());
        assert!(scene(vec![child.clone(), node(1)]).is_ok());
        let mut parent = node(1);
        parent.parent = Some(2);
        assert!(scene(vec![parent, child]).is_err());
    }
    #[test]
    fn zero_area_nodes_remain_semantic_but_are_not_pointer_targets() {
        let mut empty = node(1);
        empty.rect.w = 0.0;
        let frame = scene(vec![empty]).unwrap();
        assert!(frame.hit_tree().is_empty());
        assert_eq!(frame.semantics().len(), 1);
    }
}

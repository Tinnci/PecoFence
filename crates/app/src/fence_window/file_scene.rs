//! Geometry committed alongside a file surface. Animated paint positions, pointer
//! targets and native label editors share one frozen, stable-ID description.
use super::*;
use pecofence_plugin_api::{
    RectDip, SemanticNode,
    file_geometry::{FileItemGeometry, IconMetrics},
};

#[derive(Clone, Copy)]
pub(super) struct FileNode {
    pub id: ItemId,
    pub geometry: FileItemGeometry,
    pub alpha: f32,
}

pub(super) struct FileScene {
    active: ContentId,
    dpi: u32,
    device_epoch: u32,
    surface_epoch: u64,
    size: (i32, i32),
    pub viewport: RectDip,
    columns: Option<DetailColumns>,
    pub scroll: f32,
    pub nodes: Vec<FileNode>,
    visible: Vec<usize>,
    by_id: HashMap<ItemId, usize>,
    overlay: Option<usize>,
    semantics: Vec<SemanticNode<ItemId>>,
}

fn intersects(a: RectDip, b: RectDip) -> bool {
    a.w > 0.0
        && a.h > 0.0
        && b.w > 0.0
        && b.h > 0.0
        && a.x < b.x + b.w
        && a.x + a.w > b.x
        && a.y < b.y + b.h
        && a.y + a.h > b.y
}

impl FileScene {
    fn index_nodes(&mut self) {
        self.visible = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.alpha > 0.0 && intersects(n.geometry.bounds, self.viewport))
            .map(|(i, _)| i)
            .collect();
        self.by_id = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id, i))
            .collect();
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<(usize, ItemId)> {
        if !self.viewport.contains(x, y) {
            return None;
        }
        if let Some(index) = self.overlay
            && self.semantics[index].rect.contains(x, y)
        {
            return Some((index, self.nodes[index].id));
        }
        self.visible_nodes().rev().find_map(|(index, node)| {
            self.semantics[index]
                .rect
                .contains(x, y)
                .then_some((index, node.id))
        })
    }

    /// Expanded selected labels are painted after all other cells.
    pub fn promote(&mut self, index: usize, extra_height: f32) {
        if self.visible.contains(&index) {
            let mut bounds = self.nodes[index].geometry.bounds;
            bounds.h += extra_height.max(0.0);
            self.semantics[index].rect = bounds;
            self.overlay = Some(index);
        }
    }

    pub fn accessibility_nodes(&self) -> impl Iterator<Item = &SemanticNode<ItemId>> {
        self.visible
            .iter()
            .copied()
            .filter(|index| Some(*index) != self.overlay)
            .chain(self.overlay)
            .map(|index| &self.semantics[index])
    }

    pub fn semantics(&self) -> &[SemanticNode<ItemId>] {
        &self.semantics
    }

    pub fn semantic(&self, id: ItemId) -> Option<&SemanticNode<ItemId>> {
        self.by_id.get(&id).map(|&index| &self.semantics[index])
    }

    pub fn header(&self) -> Option<(f32, DetailColumns)> {
        self.columns.map(|columns| (self.viewport.y, columns))
    }

    pub fn node(&self, id: ItemId) -> Option<&FileNode> {
        self.by_id.get(&id).map(|&i| &self.nodes[i])
    }

    fn matches(
        &self,
        active: ContentId,
        dpi: u32,
        device_epoch: u32,
        surface_epoch: u64,
        size: (i32, i32),
    ) -> bool {
        self.active == active
            && self.dpi == dpi
            && self.device_epoch == device_epoch
            && self.surface_epoch == surface_epoch
            && self.size == size
    }

    pub fn visible_nodes(&self) -> impl DoubleEndedIterator<Item = (usize, &FileNode)> {
        self.visible
            .iter()
            .map(|&index| (index, &self.nodes[index]))
    }
}

impl FenceViewState {
    pub(super) fn prepare_file_scene(&self, layout: &ItemLayout, now: Instant) -> FileScene {
        let size = self.content_size_px();
        let width = size.0 as f32 / self.scale();
        let fixed = layout.fixed_top();
        let columns = self.columns_for(width);
        let nodes: Vec<FileNode> = self
            .file_view
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let cell = layout.cell(i);
                let (x, y, alpha) = self.file_view.item_motion.get(&item.id).map_or(
                    (cell.x, cell.y, 1.0),
                    |motion| {
                        (
                            motion.x.value_at(now),
                            motion.y.value_at(now),
                            motion.alpha.value_at(now),
                        )
                    },
                );
                let bounds = RectDip {
                    x,
                    y: y - self.file_view.scroll_y + fixed,
                    w: cell.w,
                    h: cell.h,
                };
                let geometry = match layout {
                    ItemLayout::Grid(grid) => FileItemGeometry::grid(
                        bounds,
                        IconMetrics {
                            size: grid.metrics.icon,
                            top: grid.metrics.icon_top,
                            label_gap: grid.metrics.label_gap,
                            line_height: grid.metrics.line_h,
                            label_lines: grid.metrics.label_lines as f32,
                        },
                    ),
                    ItemLayout::Rows { metrics, .. } => FileItemGeometry::row(
                        RectDip { x: 0.0, ..bounds },
                        columns.name_x,
                        columns.name_w,
                        metrics.icon,
                    ),
                };
                FileNode {
                    id: item.id,
                    geometry,
                    alpha,
                }
            })
            .collect();
        let semantics = nodes
            .iter()
            .zip(&self.file_view.items)
            .map(|(node, item)| SemanticNode {
                id: node.id,
                parent: None,
                role: "listitem".into(),
                name: item.name.clone(),
                rect: node.geometry.bounds,
                action: None,
            })
            .collect();
        let mut scene = FileScene {
            active: self.active,
            dpi: self.dpi,
            device_epoch: self.stack.device_generation(),
            surface_epoch: self.content_panel.surface_epoch(),
            size,
            viewport: RectDip {
                x: 0.0,
                y: fixed,
                w: width,
                h: (size.1 as f32 / self.scale() - fixed).max(0.0),
            },
            columns: (fixed > 0.0).then_some(columns),
            scroll: self.file_view.scroll_y,
            nodes,
            visible: Vec::new(),
            by_id: HashMap::new(),
            overlay: None,
            semantics,
        };
        scene.index_nodes();
        scene
    }

    pub(super) fn committed_file_scene(&self) -> Option<&FileScene> {
        self.file_view.file_scene.as_ref().filter(|scene| {
            scene.matches(
                self.active,
                self.dpi,
                self.stack.device_generation(),
                self.content_panel.surface_epoch(),
                self.content_size_px(),
            ) && self.panel_host.plugin_panel.is_none()
        })
    }

    pub(super) fn resolve_file_node(&self, index: usize, id: ItemId) -> Option<usize> {
        if self
            .file_view
            .items
            .get(index)
            .is_some_and(|item| item.id == id)
        {
            Some(index)
        } else {
            self.file_view.items.iter().position(|item| item.id == id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u128, x: f32, y: f32, alpha: f32) -> FileNode {
        FileNode {
            id: ItemId::from_u128(id),
            alpha,
            geometry: FileItemGeometry::row(
                RectDip {
                    x,
                    y,
                    w: 80.0,
                    h: 28.0,
                },
                x + 8.0,
                72.0,
                20.0,
            ),
        }
    }
    fn scene(nodes: Vec<FileNode>) -> FileScene {
        let semantics = nodes
            .iter()
            .map(|node| SemanticNode {
                id: node.id,
                parent: None,
                role: "listitem".into(),
                name: format!("完整文件名称 {}.txt", node.id),
                rect: node.geometry.bounds,
                action: None,
            })
            .collect();
        let mut scene = FileScene {
            active: ContentId::from_u128(1),
            dpi: 144,
            device_epoch: 1,
            surface_epoch: 3,
            size: (300, 150),
            viewport: RectDip {
                x: 0.0,
                y: 28.0,
                w: 200.0,
                h: 72.0,
            },
            columns: Some(DetailColumns::for_width(200.0)),
            scroll: 20.0,
            nodes,
            visible: Vec::new(),
            by_id: HashMap::new(),
            overlay: None,
            semantics,
        };
        scene.index_nodes();
        scene
    }

    #[test]
    fn hits_use_frozen_animated_positions_and_preserve_file_identity() {
        // Both cells are between their destination slots; painting consumes these same bounds.
        let frame = scene(vec![node(10, 80.0, 40.0, 1.0), node(20, 0.0, 40.0, 1.0)]);
        assert_eq!(frame.hit(10.0, 45.0), Some((1, ItemId::from_u128(20))));
        assert_eq!(frame.hit(90.0, 45.0), Some((0, ItemId::from_u128(10))));
        assert_eq!(
            frame.node(ItemId::from_u128(20)).unwrap().geometry.label.x,
            36.0
        );
        assert!(frame.node(ItemId::from_u128(99)).is_none());
        for (semantic, node) in frame.semantics().iter().zip(&frame.nodes) {
            assert_eq!(semantic.id, node.id);
            assert_eq!(semantic.rect, node.geometry.bounds);
            assert!(semantic.name.starts_with("完整文件名称"));
        }
    }

    #[test]
    fn clipped_headers_offscreen_and_invisible_nodes_cannot_receive_pointer_input() {
        let frame = scene(vec![
            node(1, 0.0, 10.0, 1.0),
            node(2, 80.0, 40.0, 0.0),
            node(3, 0.0, 120.0, 1.0),
        ]);
        assert_eq!(frame.visible_nodes().count(), 1);
        assert_eq!(frame.header().unwrap().0, 28.0);
        assert!(frame.hit(10.0, 20.0).is_none());
        assert_eq!(frame.hit(10.0, 30.0).unwrap().1, ItemId::from_u128(1));
        assert!(frame.hit(90.0, 45.0).is_none());
        assert!(frame.hit(10.0, 125.0).is_none());
        assert!(frame.hit(200.0, 50.0).is_none());
        assert!(frame.hit(f32::NAN, 50.0).is_none());
    }

    #[test]
    fn semantic_lookup_keeps_identity_across_reorder_and_rejects_reused_slots() {
        let original = scene(vec![node(1, 0.0, 40.0, 1.0), node(2, 80.0, 40.0, 1.0)]);
        let id = original.hit(10.0, 45.0).unwrap().1;
        let reordered = scene(vec![node(2, 0.0, 40.0, 1.0), node(1, 80.0, 40.0, 1.0)]);
        let semantic = reordered.semantic(id).unwrap();
        assert_eq!(semantic.rect.x, 80.0);
        assert_eq!(semantic.name, original.semantic(id).unwrap().name);
        let replaced = scene(vec![node(3, 0.0, 40.0, 1.0), node(2, 80.0, 40.0, 1.0)]);
        assert!(
            replaced.semantic(id).is_none(),
            "The old index now names another file"
        );
        // Stable identity includes all UUID bits, not just a narrowed integer key.
        let high = ItemId::from_u128((1 << 96) | 1);
        assert!(original.semantic(high).is_none());
    }
    #[test]
    fn overlapping_animated_cells_follow_paint_order() {
        let frame = scene(vec![node(1, 0.0, 40.0, 1.0), node(2, 0.0, 40.0, 1.0)]);
        assert_eq!(frame.hit(10.0, 45.0).unwrap().1, ItemId::from_u128(2));
    }
    #[test]
    fn expanded_label_is_the_topmost_pointer_target_without_changing_layout_bounds() {
        let mut frame = scene(vec![node(1, 0.0, 40.0, 1.0), node(2, 0.0, 68.0, 1.0)]);
        frame.promote(0, 28.0);
        assert_eq!(frame.nodes[0].geometry.bounds.h, 28.0);
        assert_eq!(frame.semantics()[0].rect.h, 56.0);
        assert_eq!(frame.hit(10.0, 80.0).unwrap().1, ItemId::from_u128(1));
        assert!(frame.hit(10.0, 100.0).is_none());
    }

    #[test]
    fn surface_resize_dpi_device_reset_and_tab_switch_reject_old_geometry() {
        let frame = scene(vec![node(1, 0.0, 40.0, 1.0)]);
        let active = ContentId::from_u128(1);
        assert!(frame.matches(active, 144, 1, 3, (300, 150)));
        assert!(!frame.matches(active, 144, 1, 4, (300, 150)));
        assert!(!frame.matches(active, 192, 1, 3, (300, 150)));
        assert!(!frame.matches(active, 144, 2, 3, (300, 150)));
        assert!(!frame.matches(active, 144, 1, 3, (600, 300)));
        assert!(!frame.matches(ContentId::from_u128(2), 144, 1, 3, (300, 150)));
    }
}

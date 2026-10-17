//! The title bar's published geometry. Input reads sampled animation positions,
//! stable tab identities and the same control boxes used by FenceChrome.
use super::*;
use pecofence_plugin_api::{RectDip, SemanticNode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ChromeTarget {
    Title(ContentId),
    Tab(ContentId),
    Up,
    Chevron,
}

pub(super) struct ChromeInput<'a> {
    pub active: ContentId,
    pub dpi: u32,
    pub device: u32,
    pub surface: u64,
    pub size: (i32, i32),
    pub title_height: f32,
    pub title: &'a str,
    pub tabs: &'a [TabDraw<'a>],
    pub deco: TitleDeco,
    pub rolled: bool,
}

pub(super) struct ChromeScene {
    active: ContentId,
    dpi: u32,
    device: u32,
    surface: u64,
    size: (i32, i32),
    viewport: RectDip,
    nodes: Vec<SemanticNode<ChromeTarget>>,
    clipped_captions: HashSet<ContentId>,
}

fn dip_rect(rect: Rect) -> RectDip {
    RectDip {
        x: rect.left,
        y: rect.top,
        w: rect.width(),
        h: rect.height(),
    }
}

impl ChromeScene {
    pub fn new(input: ChromeInput<'_>) -> Self {
        let scale = input.dpi.max(96) as f32 / 96.0;
        let width = input.size.0 as f32 / scale;
        let height = input.title_height.min(input.size.1 as f32 / scale).max(0.0);
        let viewport = RectDip {
            x: 0.0,
            y: 0.0,
            w: width,
            h: height,
        };
        let root = ChromeTarget::Title(input.active);
        let mut nodes = vec![SemanticNode {
            id: root,
            parent: None,
            role: "titlebar".into(),
            name: input.title.into(),
            rect: viewport,
            action: None,
        }];
        // The dragged tab is painted after all ordinary tabs. Reverse hit testing
        // must follow that exact order, including overlapping animation samples.
        for tab in input
            .tabs
            .iter()
            .filter(|tab| input.tabs.len() > 1 && !tab.dragging)
            .chain(
                input
                    .tabs
                    .iter()
                    .filter(|tab| input.tabs.len() > 1 && tab.dragging),
            )
        {
            nodes.push(SemanticNode {
                id: ChromeTarget::Tab(ContentId::from_u128(tab.key)),
                parent: Some(root),
                role: "tab".into(),
                name: tab.title.into(),
                rect: RectDip {
                    x: tab.x,
                    y: 0.0,
                    w: tab.w,
                    h: height,
                },
                action: None,
            });
        }
        if input.deco.up_button && input.tabs.len() <= 1 {
            nodes.push(SemanticNode {
                id: ChromeTarget::Up,
                parent: Some(root),
                role: "button".into(),
                name: pecofence_core::i18n::text("向上").into(),
                rect: dip_rect(TitleDeco::up_box(height)),
                action: None,
            });
        }
        nodes.push(SemanticNode {
            id: ChromeTarget::Chevron,
            parent: Some(root),
            role: "button".into(),
            name: pecofence_core::i18n::text(if input.rolled { "展开" } else { "卷起" }).into(),
            rect: dip_rect(TitleDeco::chevron_box(width, height)),
            action: None,
        });
        Self {
            active: input.active,
            dpi: input.dpi,
            device: input.device,
            surface: input.surface,
            size: input.size,
            viewport,
            nodes,
            clipped_captions: HashSet::new(),
        }
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<ChromeTarget> {
        if !self.viewport.contains(x, y) {
            return None;
        }
        self.nodes
            .iter()
            .skip(1)
            .rev()
            .find(|node| node.rect.contains(x, y))
            .map(|node| node.id)
    }

    pub fn with_clipped_captions(mut self, ids: impl IntoIterator<Item = ContentId>) -> Self {
        self.clipped_captions.extend(ids);
        self
    }

    pub fn tab_tip(&self, id: ContentId) -> Option<&str> {
        self.clipped_captions
            .contains(&id)
            .then(|| self.tab(id))
            .flatten()
            .map(|node| node.name.as_str())
    }

    pub fn tab(&self, id: ContentId) -> Option<&SemanticNode<ChromeTarget>> {
        self.nodes
            .iter()
            .find(|node| node.id == ChromeTarget::Tab(id))
    }

    pub fn height(&self) -> f32 {
        self.viewport.h
    }

    pub fn semantics(&self) -> &[SemanticNode<ChromeTarget>] {
        &self.nodes
    }

    fn matches(
        &self,
        active: ContentId,
        dpi: u32,
        device: u32,
        surface: u64,
        size: (i32, i32),
    ) -> bool {
        self.active == active
            && self.dpi == dpi
            && self.device == device
            && self.surface == surface
            && self.size == size
    }
}

impl FenceViewState {
    pub(super) fn committed_chrome_scene(&self) -> Option<&ChromeScene> {
        self.chrome_scene.as_ref().filter(|scene| {
            scene.matches(
                self.active,
                self.dpi,
                self.stack.device_generation(),
                self.chrome_panel.surface_epoch(),
                self.chrome_panel.size_px(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: u128, x: f32, dragging: bool) -> TabDraw<'static> {
        TabDraw {
            key: id,
            x,
            w: 80.0,
            title: "完整标签 Archives",
            title_size: 1,
            title_color: None,
            active: false,
            hover: 0.0,
            pressed: false,
            color: None,
            drop_target: 0.0,
            dragging,
            alpha: 1.0,
        }
    }
    fn scene(tabs: &[TabDraw<'_>], size: (i32, i32), title_height: f32) -> ChromeScene {
        ChromeScene::new(ChromeInput {
            active: ContentId::from_u128(7),
            dpi: 96,
            device: 1,
            surface: 9,
            size,
            title_height,
            title: "Title",
            tabs,
            deco: TitleDeco {
                folder_icon: true,
                up_button: true,
            },
            rolled: false,
        })
    }

    #[test]
    fn frozen_tab_positions_preserve_full_identity_and_caption_after_source_changes() {
        let id = (1u128 << 120) + 11;
        let mut tabs = [tab(id, 10.0, false), tab(20, 100.0, false)];
        let frame =
            scene(&tabs, (320, 120), 36.0).with_clipped_captions([ContentId::from_u128(id)]);
        tabs[0].x = 200.0;
        tabs[0].title = "New caption";
        assert_eq!(tabs[0].title, "New caption");
        assert_eq!(tabs[0].x, 200.0);
        assert_eq!(
            frame.hit(20.0, 10.0),
            Some(ChromeTarget::Tab(ContentId::from_u128(id)))
        );
        assert_eq!(frame.tab(ContentId::from_u128(id)).unwrap().rect.x, 10.0);
        assert_eq!(
            frame.tab_tip(ContentId::from_u128(id)),
            Some("完整标签 Archives")
        );
        assert!(frame.tab_tip(ContentId::from_u128(20)).is_none());
        assert!(frame.tab(ContentId::from_u128(99)).is_none());
        assert_eq!(
            frame.semantics()[1].parent,
            Some(ChromeTarget::Title(ContentId::from_u128(7)))
        );
        // Consumers remap by the frozen content identity, never the old slot.
        let reordered = [ContentId::from_u128(20), ContentId::from_u128(id)];
        let ChromeTarget::Tab(target) = frame.hit(20.0, 10.0).unwrap() else {
            panic!("expected tab");
        };
        assert_eq!(reordered.iter().position(|&id| id == target), Some(1));
    }

    #[test]
    fn dragged_tabs_and_ordinary_overlap_follow_the_renderer_paint_order() {
        let frame = scene(&[tab(1, 10.0, true), tab(2, 20.0, false)], (320, 120), 36.0);
        assert_eq!(
            frame.hit(30.0, 10.0),
            Some(ChromeTarget::Tab(ContentId::from_u128(1)))
        );
        let frame = scene(
            &[tab(1, 10.0, false), tab(2, 20.0, false)],
            (320, 120),
            36.0,
        );
        assert_eq!(
            frame.hit(30.0, 10.0),
            Some(ChromeTarget::Tab(ContentId::from_u128(2)))
        );
    }

    #[test]
    fn clipped_title_controls_share_render_boxes_and_reject_outside_points() {
        let frame = scene(&[], (180, 30), 60.0);
        let up = dip_rect(TitleDeco::up_box(30.0));
        assert_eq!(frame.hit(up.x + 1.0, up.y + 1.0), Some(ChromeTarget::Up));
        let chevron = dip_rect(TitleDeco::chevron_box(180.0, 30.0));
        assert_eq!(
            frame.hit(chevron.x + 1.0, chevron.y + 1.0),
            Some(ChromeTarget::Chevron)
        );
        assert_eq!(frame.height(), 30.0);
        for (x, y) in [(20.0, -1.0), (20.0, 30.0), (180.0, 10.0), (f32::NAN, 10.0)] {
            assert!(frame.hit(x, y).is_none());
        }
        let frame = scene(&[tab(1, 60.0, false)], (180, 60), 36.0);
        assert!(
            frame.tab(ContentId::from_u128(1)).is_none(),
            "one tab paints a plain title"
        );
    }

    #[test]
    fn storage_dpi_device_active_content_and_dimensions_bound_the_snapshot() {
        let frame = scene(&[], (320, 120), 36.0);
        let active = ContentId::from_u128(7);
        assert!(frame.matches(active, 96, 1, 9, (320, 120)));
        assert!(!frame.matches(active, 144, 1, 9, (320, 120)));
        assert!(!frame.matches(active, 96, 2, 9, (320, 120)));
        assert!(!frame.matches(active, 96, 1, 10, (320, 120)));
        assert!(!frame.matches(active, 96, 1, 9, (320, 160)));
        assert!(!frame.matches(ContentId::from_u128(8), 96, 1, 9, (320, 120)));
    }
}

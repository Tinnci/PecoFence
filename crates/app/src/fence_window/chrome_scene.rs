//! The title bar's published geometry. Input reads sampled animation positions,
//! stable tab identities and the same control boxes used by FenceChrome.
use super::*;
use pecofence_plugin_api::{RectDip, SemanticNode};
use pecofence_render::title_controls::{TitleControl, TitleControlId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ChromeTarget {
    Title(ContentId),
    Tab(ContentId),
    Up,
    Chevron,
    /// A panel command, by index in the published chrome.
    Command(usize),
    /// A Windows caption button (app-style fences).
    Caption(pecofence_render::fence_chrome::CaptionButton),
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
    /// Accessible names of the panel's chrome commands, nearest the chevron first.
    pub commands: &'a [&'a str],
    /// The fence fills its work area (names the middle caption button "Restore").
    pub maximized: bool,
    pub state: TitleState,
    pub command_states: &'a [pecofence_render::fence_chrome::TitleCommandDraw],
}

pub(super) struct ChromeScene {
    active: ContentId,
    dpi: u32,
    device: u32,
    surface: u64,
    size: (i32, i32),
    viewport: RectDip,
    nodes: Vec<SemanticNode<ChromeTarget>>,
    controls: Vec<TitleControl>,
    clipped_captions: HashSet<ContentId>,
}

#[cfg(test)]
fn dip_rect(rect: Rect) -> RectDip {
    RectDip {
        x: rect.left,
        y: rect.top,
        w: rect.width(),
        h: rect.height(),
    }
}

fn target(id: TitleControlId) -> ChromeTarget {
    match id {
        TitleControlId::Tab(id) => ChromeTarget::Tab(ContentId::from_u128(id)),
        TitleControlId::Up => ChromeTarget::Up,
        TitleControlId::Chevron => ChromeTarget::Chevron,
        TitleControlId::Command(i) => ChromeTarget::Command(i),
        TitleControlId::Caption(b) => ChromeTarget::Caption(b),
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
        let controls = TitleControl::layout(
            width,
            height,
            input.deco,
            input.tabs,
            input.command_states,
            input.commands,
            input.rolled,
            TitleState {
                maximized: input.maximized,
                ..input.state
            },
        );
        nodes.extend(controls.iter().map(|control| {
            SemanticNode {
                id: target(control.id),
                parent: Some(root),
                role: if matches!(control.id, TitleControlId::Tab(_)) {
                    "tab"
                } else {
                    "button"
                }
                .into(),
                name: match &control.label {
                    std::borrow::Cow::Borrowed(key) => pecofence_core::i18n::text(key).into(),
                    std::borrow::Cow::Owned(text) => text.clone(),
                },
                rect: control.rect,
                action: None,
            }
        }));
        Self {
            active: input.active,
            dpi: input.dpi,
            device: input.device,
            surface: input.surface,
            size: input.size,
            viewport,
            nodes,
            controls,
            clipped_captions: HashSet::new(),
        }
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<ChromeTarget> {
        if !self.viewport.contains(x, y) {
            return None;
        }
        self.controls
            .iter()
            .rev()
            .find(|control| control.rect.contains(x, y))
            .map(|control| target(control.id))
    }

    pub fn enabled(&self, id: ChromeTarget) -> bool {
        self.controls
            .iter()
            .find(|control| target(control.id) == id)
            .is_some_and(|control| control.enabled)
    }

    pub fn controls(&self) -> &[TitleControl] {
        &self.controls
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

    pub fn viewport(&self) -> RectDip {
        self.viewport
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

/// Caption boxes own resize-edge pixels too; all other controls retain the
/// resize border. This decision is shared with the native NC hit-test.
pub(super) fn control_owns_client(target: Option<ChromeTarget>, inside_border: bool) -> bool {
    target.is_some() && (matches!(target, Some(ChromeTarget::Caption(_))) || inside_border)
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
                captions: false,
            },
            rolled: false,
            commands: &[],
            maximized: false,
            state: TitleState::default(),
            command_states: &[],
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

    #[test]
    fn panel_commands_sit_left_of_the_chevron_and_hit_by_index() {
        let commands = ["Refresh", "Copy summary"];
        let frame = ChromeScene::new(ChromeInput {
            active: ContentId::from_u128(7),
            dpi: 96,
            device: 1,
            surface: 9,
            size: (600, 300),
            title_height: 36.0,
            title: "orion · orion-ir",
            tabs: &[],
            deco: TitleDeco::default(),
            rolled: false,
            commands: &commands,
            maximized: false,
            state: TitleState::default(),
            command_states: &[],
        });
        let chevron = TitleDeco::chevron_box(600.0, 36.0);
        let first = TitleDeco::default().command_box(600.0, 36.0, 0);
        let second = TitleDeco::default().command_box(600.0, 36.0, 1);
        assert!(first.right <= chevron.left && second.right <= first.left);
        assert_eq!(
            frame.hit(first.left + 2.0, first.top + 2.0),
            Some(ChromeTarget::Command(0))
        );
        assert_eq!(
            frame.hit(second.left + 2.0, second.top + 2.0),
            Some(ChromeTarget::Command(1))
        );
        let names: Vec<_> = frame
            .semantics()
            .iter()
            .filter(|n| matches!(n.id, ChromeTarget::Command(_)))
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(names, commands);
        // A rolled-up fence shows only its title and chevron.
        let rolled = ChromeScene::new(ChromeInput {
            active: ContentId::from_u128(7),
            dpi: 96,
            device: 1,
            surface: 9,
            size: (600, 300),
            title_height: 36.0,
            title: "orion",
            tabs: &[],
            deco: TitleDeco::default(),
            rolled: true,
            commands: &commands,
            maximized: false,
            state: TitleState::default(),
            command_states: &[],
        });
        assert_eq!(rolled.hit(first.left + 2.0, first.top + 2.0), None);
    }

    #[test]
    fn app_style_fences_get_windows_caption_buttons_instead_of_the_chevron() {
        use pecofence_render::fence_chrome::CaptionButton;
        let commands = ["Refresh"];
        let scene = |maximized| {
            ChromeScene::new(ChromeInput {
                active: ContentId::from_u128(7),
                dpi: 96,
                device: 1,
                surface: 9,
                size: (600, 300),
                title_height: 36.0,
                title: "orion",
                tabs: &[],
                deco: TitleDeco {
                    captions: true,
                    ..TitleDeco::default()
                },
                rolled: false,
                commands: &commands,
                maximized,
                state: TitleState::default(),
                command_states: &[],
            })
        };
        let frame = scene(false);
        let close = TitleDeco::caption_box(600.0, 36.0, CaptionButton::Close);
        let max = TitleDeco::caption_box(600.0, 36.0, CaptionButton::Maximize);
        let min = TitleDeco::caption_box(600.0, 36.0, CaptionButton::Minimize);
        // Windows order, Close at the edge.
        assert_eq!(close.right, 600.0);
        assert!(min.right <= max.left && max.right <= close.left);
        for (rect, button) in [
            (close, CaptionButton::Close),
            (max, CaptionButton::Maximize),
            (min, CaptionButton::Minimize),
        ] {
            assert_eq!(
                frame.hit(rect.left + 2.0, 18.0),
                Some(ChromeTarget::Caption(button))
            );
        }
        // No chevron; the panel command moves left of the caption buttons.
        assert!(
            !frame
                .semantics()
                .iter()
                .any(|n| n.id == ChromeTarget::Chevron)
        );
        let deco = TitleDeco {
            captions: true,
            ..TitleDeco::default()
        };
        let command = deco.command_box(600.0, 36.0, 0);
        assert!(command.right <= min.left);
        assert_eq!(
            frame.hit(command.left + 2.0, command.top + 2.0),
            Some(ChromeTarget::Command(0))
        );
        let name = |frame: &ChromeScene| {
            frame
                .semantics()
                .iter()
                .find(|n| n.id == ChromeTarget::Caption(CaptionButton::Maximize))
                .map(|n| n.name.clone())
                .unwrap()
        };
        assert_ne!(name(&frame), name(&scene(true)), "Maximize becomes Restore");
    }
    #[test]
    fn disabled_commands_reserve_their_geometry_but_cannot_be_pressed() {
        let command = pecofence_render::fence_chrome::TitleCommandDraw {
            glyph: '\u{E72C}',
            enabled: false,
            hover: 0.5,
            pressed: false,
        };
        let frame = ChromeScene::new(ChromeInput {
            active: ContentId::from_u128(7),
            dpi: 144,
            device: 1,
            surface: 9,
            size: (900, 450),
            title_height: 36.0,
            title: "Board",
            tabs: &[],
            deco: TitleDeco {
                captions: true,
                ..Default::default()
            },
            rolled: false,
            commands: &["Refresh"],
            maximized: true,
            state: TitleState::default(),
            command_states: &[command],
        });
        let control = frame
            .controls()
            .iter()
            .find(|c| c.id == TitleControlId::Command(0))
            .unwrap();
        let point = (control.rect.x + 1.0, control.rect.y + 1.0);
        assert_eq!(frame.hit(point.0, point.1), Some(ChromeTarget::Command(0)));
        assert!(!frame.enabled(ChromeTarget::Command(0)));
        assert_eq!(control.glyph, command.glyph);
        assert_eq!(control.state.hover, command.hover);
        let drawn = control.draw_rect();
        assert_eq!((drawn.left, drawn.top), (control.rect.x, control.rect.y));
        assert!(control_owns_client(frame.hit(point.0, point.1), true));
    }

    #[test]
    fn caption_edges_and_release_points_follow_the_committed_frame() {
        use pecofence_render::fence_chrome::CaptionButton;
        let frame = ChromeScene::new(ChromeInput {
            active: ContentId::from_u128(7),
            dpi: 192,
            device: 1,
            surface: 9,
            size: (1200, 600),
            title_height: 36.0,
            title: "Board",
            tabs: &[],
            deco: TitleDeco {
                captions: true,
                ..Default::default()
            },
            rolled: false,
            commands: &[],
            maximized: true,
            state: TitleState::default(),
            command_states: &[],
        });
        let max = TitleDeco::caption_box(600.0, 36.0, CaptionButton::Maximize);
        let pressed = ChromeTarget::Caption(CaptionButton::Maximize);
        // The final release point, not a cached hover, determines activation.
        assert_eq!(frame.hit(max.left + 1.0, 0.0), Some(pressed));
        assert_ne!(frame.hit(1.0, 1.0), Some(pressed));
        assert!(control_owns_client(frame.hit(max.left + 1.0, 0.0), false));
        assert!(control_owns_client(frame.hit(599.5, 0.0), false));
        assert!(!control_owns_client(None, false));
        assert!(!control_owns_client(Some(ChromeTarget::Up), false));
        let restore = frame
            .controls()
            .iter()
            .find(|c| c.id == TitleControlId::Caption(CaptionButton::Maximize))
            .unwrap();
        assert_eq!(restore.glyph, '\u{E923}');
    }
}

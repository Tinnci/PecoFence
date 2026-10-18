//! Platform-free presentation rules. Layout is in DIPs; display DPI is applied only
//! by the host surface. Accessibility text scaling reflows the layout before paint.
use crate::{Canvas, FontRole, HitNode, RectDip, Result, TextAlign, TextSpec, ThemeSnapshot};

pub const FONT_TEXT: &str = "Segoe UI Variable Text";
pub const FONT_SMALL: &str = "Segoe UI Variable Small";
pub const FONT_DISPLAY: &str = "Segoe UI Variable Display";
pub const FONT_ICONS: &str = "Segoe Fluent Icons";
/// Built-in Windows serif with optical sizes; Han text falls back through DirectWrite.
/// Fallback for [`FontRole::Serif`] when the bundled Noto Serif SC is missing.
pub const FONT_SERIF: &str = "Sitka Subheading";
/// Built-in DIN face for digital clock numerals (variable: Light…Bold), drawn at
/// condensed width; a normal-width fallback keeps the family if that fails.
pub const FONT_CONDENSED: &str = "Bahnschrift";
/// Bundled warm-paper title face (`fonts/` beside the EXE), semibold.
pub const FONT_PAPER_SERIF: &str = "Noto Serif SC";
/// Bundled warm-paper body face (`fonts/` beside the EXE), regular.
pub const FONT_PAPER_BODY: &str = "Noto Sans SC";
pub const CAPTION: f32 = 12.0;
pub const BODY: f32 = 14.0;
pub const SUBTITLE: f32 = 16.0;
pub const TITLE: f32 = 20.0;
pub const CONTROL_RADIUS: f32 = 4.0;
/// Corner radius for grouped surfaces such as note bodies and list rows.
pub const CARD_RADIUS: f32 = 8.0;
pub const CONTROL_HEIGHT: f32 = 32.0;
pub const CONTENT_INSET: f32 = 16.0;

/// Visual weight of a pill button; the theme decides the concrete colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// Primary action: accent text on an accent wash.
    Accent,
    /// Secondary action: primary text on the subtle surface.
    Subtle,
    /// Icon-like action with no resting fill, so host hover feedback is visible.
    Flat,
}

/// Pill radius for a control of the given height.
pub fn pill_radius(height: f32) -> f32 {
    height.max(0.0) / 2.0
}

pub fn control_visible(rect: RectDip, viewport: RectDip) -> bool {
    rect.w > 0.0
        && rect.h > 0.0
        && rect.x >= viewport.x
        && rect.y >= viewport.y
        && rect.x + rect.w <= viewport.x + viewport.w + 0.01
        && rect.y + rect.h <= viewport.y + viewport.h + 0.01
}

/// Inside-aligned contrast feedback shared by native chrome and plugin controls.
/// A stronger outline represents selection or press without tinting the text background.
pub fn contrast_outline(
    rect: RectDip,
    grid: PixelGrid,
    ink: [f32; 4],
    strong: bool,
) -> Option<(RectDip, crate::StrokeStyle)> {
    let width = grid.hairline() * if strong { 2.0 } else { 1.0 };
    let x = grid.snap(rect.x) + width / 2.0;
    let y = grid.snap(rect.y) + width / 2.0;
    let w = grid.snap(rect.x + rect.w) - width / 2.0 - x;
    let h = grid.snap(rect.y + rect.h) - width / 2.0 - y;
    if ![x, y, w, h].iter().all(|v| v.is_finite()) || w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some((
        RectDip { x, y, w, h },
        crate::StrokeStyle { rgba: ink, width },
    ))
}

/// Host-owned interaction state shared by all content providers. IDs identify
/// controls, not actions (two separate controls may invoke the same action).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ControlInteraction {
    pub hovered: Option<u64>,
    pub pressed: Option<u64>,
    pub focused: Option<u64>,
}

impl ControlInteraction {
    pub fn hover(&mut self, id: Option<u64>) -> bool {
        if self.hovered == id {
            return false;
        }
        self.hovered = id;
        true
    }

    pub fn focus_next(&mut self, hits: &[HitNode], reverse: bool) {
        if hits.is_empty() {
            self.focused = None;
            return;
        }
        let index = self
            .focused
            .and_then(|id| hits.iter().position(|hit| hit.id == id));
        let next = match index {
            Some(index) if reverse => (index + hits.len() - 1) % hits.len(),
            Some(index) => (index + 1) % hits.len(),
            None if reverse => hits.len() - 1,
            None => 0,
        };
        self.focused = Some(hits[next].id);
    }

    pub fn reconcile(&mut self, hits: &[HitNode]) {
        let exists = |id: &mut Option<u64>| {
            if !hits.iter().any(|hit| Some(hit.id) == *id) {
                *id = None;
            }
        };
        exists(&mut self.hovered);
        exists(&mut self.pressed);
        exists(&mut self.focused);
    }

    pub fn reconcile_viewport(&mut self, hits: &[HitNode], viewport: RectDip) {
        self.reconcile(hits);
        if !hits
            .iter()
            .any(|hit| Some(hit.id) == self.focused && control_visible(hit.rect, viewport))
        {
            self.focused = None;
        }
    }

    /// Paint feedback over the committed frame; never rebuild the provider's
    /// business/layout snapshot merely because the pointer moved.
    pub fn paint(
        &self,
        canvas: &mut dyn Canvas,
        hits: &[HitNode],
        theme: &ThemeSnapshot,
        grid: PixelGrid,
        radius: f32,
    ) -> Result<()> {
        self.paint_with_radii(canvas, hits, theme, grid, |_| radius)
    }

    /// Resolve shape metadata from the same immutable frame as the hit tree.
    pub fn paint_with_radii(
        &self,
        canvas: &mut dyn Canvas,
        hits: &[HitNode],
        theme: &ThemeSnapshot,
        grid: PixelGrid,
        radius_for: impl Fn(u64) -> f32,
    ) -> Result<()> {
        let find = |id: Option<u64>| hits.iter().find(|hit| Some(hit.id) == id);
        if let Some(hit) = find(self.hovered) {
            if theme.high_contrast {
                if let Some((rect, stroke)) = contrast_outline(
                    hit.rect,
                    grid,
                    theme.text_primary,
                    self.pressed == self.hovered,
                ) {
                    canvas.draw_rect(rect, stroke)?;
                }
            } else {
                let mut fill = if self.pressed == self.hovered {
                    theme.control_pressed
                } else {
                    theme.control_hover
                };
                // Feedback is over an already-painted frame, including its text.
                fill[3] = fill[3].min(0.16);
                let radius = radius_for(hit.id);
                let radius = if radius.is_finite() {
                    radius.max(0.0)
                } else {
                    0.0
                };
                canvas.fill_rounded_rect(
                    hit.rect,
                    radius.min(hit.rect.w / 2.0).min(hit.rect.h / 2.0),
                    fill,
                )?;
            }
        }
        if let Some(hit) = find(self.focused) {
            if let Some((rect, stroke)) =
                contrast_outline(hit.rect, grid, theme.focus_stroke, theme.high_contrast)
            {
                canvas.draw_rect(rect, stroke)?;
            }
        }
        Ok(())
    }
}

/// A click belongs to a committed control, not merely to an action number.
/// Used on release so DPI, remounts and provider reflow cancel stale gestures.
pub fn released_action(
    down: crate::FrameIdentity,
    current: crate::FrameIdentity,
    down_hits: &[HitNode],
    current_hits: &[HitNode],
    pressed_id: u64,
    release: Option<&HitNode>,
) -> Option<u64> {
    let released = release?;
    if down.mount_key != current.mount_key
        || down.layout_revision != current.layout_revision
        || down.device_epoch != current.device_epoch
        || down_hits != current_hits
        || released.id != pressed_id
    {
        return None;
    }
    down_hits
        .iter()
        .find(|hit| hit.id == pressed_id && hit.action == released.action)
        .map(|hit| hit.action)
}

/// Device-pixel alignment, separate from accessibility text scale.
#[derive(Clone, Copy, Debug)]
pub struct PixelGrid(f32);

impl PixelGrid {
    pub fn new(scale: f32) -> Self {
        Self(if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        })
    }
    pub fn scale(self) -> f32 {
        self.0
    }
    pub fn snap(self, dip: f32) -> f32 {
        (dip * self.0).round() / self.0
    }
    pub fn hairline(self) -> f32 {
        self.0.floor().max(1.0) / self.0
    }
}

/// Build in a smaller logical viewport, then map both paint and hit geometry
/// back to DIPs. This grows controls along with text and lets existing responsive
/// breakpoints see the actual available space. Never pass monitor DPI here.
#[derive(Clone, Copy, Debug)]
pub struct ContentScale {
    viewport: RectDip,
    factor: f32,
}

impl ContentScale {
    pub fn new(viewport: RectDip, text_scale: f32) -> Self {
        Self {
            viewport,
            factor: if text_scale.is_finite() {
                text_scale.clamp(1.0, 2.25)
            } else {
                1.0
            },
        }
    }
    pub fn factor(self) -> f32 {
        self.factor
    }
    pub fn logical_viewport(self) -> RectDip {
        RectDip {
            w: self.viewport.w / self.factor,
            h: self.viewport.h / self.factor,
            ..self.viewport
        }
    }
    pub fn rect(self, rect: RectDip) -> RectDip {
        RectDip {
            x: self.viewport.x + (rect.x - self.viewport.x) * self.factor,
            y: self.viewport.y + (rect.y - self.viewport.y) * self.factor,
            w: rect.w.max(0.0) * self.factor,
            h: rect.h.max(0.0) * self.factor,
        }
    }
    pub fn value(self, value: f32) -> f32 {
        value * self.factor
    }
}

/// One component painter, including clipping, shared by local widgets and SPM.
/// The supplied rect is also the component's hit/semantic geometry.
pub struct Component<'a> {
    pub rect: RectDip,
    pub text: &'a str,
    pub size: f32,
    pub weight: u16,
    pub font: FontRole,
    pub foreground: [f32; 4],
    pub fill: Option<[f32; 4]>,
    pub stroke: Option<crate::StrokeStyle>,
    pub radius: f32,
    pub text_inset: [f32; 2],
    pub align: TextAlign,
    pub path: Option<&'a crate::Path>,
}

impl Component<'_> {
    pub fn paint(&self, canvas: &mut dyn Canvas) -> Result<()> {
        if self.rect.w <= 0.0 || self.rect.h <= 0.0 {
            return Ok(());
        }
        canvas.push_clip(self.rect)?;
        let result = (|| {
            if let Some(fill) = self.fill {
                canvas.fill_rounded_rect(
                    self.rect,
                    self.radius.min(self.rect.w / 2.0).min(self.rect.h / 2.0),
                    fill,
                )?;
            }
            if let (Some(path), Some(stroke)) = (self.path, self.stroke) {
                canvas.stroke_path(path, stroke)?;
            } else if let Some(mut stroke) = self.stroke {
                stroke.width = stroke.width.min(self.rect.w).min(self.rect.h);
                let inset = stroke.width / 2.0;
                canvas.draw_rect(
                    RectDip {
                        x: self.rect.x + inset,
                        y: self.rect.y + inset,
                        w: (self.rect.w - stroke.width).max(0.0),
                        h: (self.rect.h - stroke.width).max(0.0),
                    },
                    stroke,
                )?;
            }
            if !self.text.is_empty() {
                let [x, y] = self.text_inset;
                canvas.draw_text(
                    RectDip {
                        x: self.rect.x + x,
                        y: self.rect.y + y,
                        w: (self.rect.w - x * 2.0).max(0.0),
                        h: (self.rect.h - y * 2.0).max(0.0),
                    },
                    &TextSpec {
                        text: self.text.into(),
                        size_dip: self.size,
                        weight: self.weight,
                        font: self.font,
                        align: self.align,
                    },
                    self.foreground,
                )?;
            }
            Ok(())
        })();
        result.and(canvas.pop_clip())
    }
}

/// Geometry revisions change when viewport/text scale changes, not just when
/// the business data changes. A pointer-down must not invoke an old layout.
#[derive(Default)]
pub struct LayoutTracker {
    revision: u64,
    source_revision: u64,
    hits: Vec<HitNode>,
}

impl LayoutTracker {
    pub fn update(&mut self, hits: &[HitNode], source_revision: u64) -> Result<u64> {
        if self.hits != hits || self.source_revision != source_revision {
            self.revision = self
                .revision
                .checked_add(1)
                .ok_or(crate::Error::Exhausted)?;
            self.hits = hits.to_vec();
            self.source_revision = source_revision;
        }
        Ok(self.revision)
    }
}

impl ThemeSnapshot {
    /// Only used before a host snapshot arrives; all providers share this fallback.
    pub fn dark() -> Self {
        Self {
            revision: 0,
            paper: crate::PaperEnvironment::default(),
            high_contrast: false,
            animations: true,
            text_primary: [0.949, 0.961, 0.98, 1.0],
            text_secondary: [0.725, 0.773, 0.839, 1.0],
            surface_panel: [0.125, 0.149, 0.188, 1.0],
            surface_subtle: [0.165, 0.2, 0.251, 1.0],
            control_hover: [1.0, 1.0, 1.0, 15.0 / 255.0],
            control_pressed: [1.0, 1.0, 1.0, 10.0 / 255.0],
            focus_stroke: [1.0; 4],
            stroke: [0.196, 0.267, 0.314, 1.0],
            accent: [0.545, 0.765, 1.0, 1.0],
            danger: [0.973, 0.443, 0.443, 1.0],
            warning: [0.984, 0.573, 0.235, 1.0],
            success: [0.29, 0.871, 0.533, 1.0],
            unknown: [0.725, 0.773, 0.839, 1.0],
            info: [0.545, 0.765, 1.0, 1.0],
        }
    }
}

/// Pending work is monotone until handled: feedback cannot erase a source or
/// layout update received while the content is hidden.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum FrameInvalidation {
    #[default]
    Clean,
    Feedback,
    Content,
    Layout,
}

impl FrameInvalidation {
    pub fn request(&mut self, requested: Self) {
        *self = (*self).max(requested);
    }
    pub fn needs_layout(self) -> bool {
        self == Self::Layout
    }
    pub fn is_pending(self) -> bool {
        self != Self::Clean
    }
    pub fn layout_resolved(&mut self) {
        if *self == Self::Layout {
            *self = Self::Content;
        }
    }
    pub fn feedback_committed(&mut self) {
        if *self == Self::Feedback {
            *self = Self::Clean;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits() -> Vec<HitNode> {
        vec![
            HitNode {
                id: 1,
                rect: RectDip {
                    x: 0.0,
                    y: 0.0,
                    w: 32.0,
                    h: 32.0,
                },
                action: 5,
            },
            HitNode {
                id: 2,
                rect: RectDip {
                    x: 40.0,
                    y: 0.0,
                    w: 32.0,
                    h: 32.0,
                },
                action: 5,
            },
        ]
    }

    #[test]
    fn outlines_cover_integer_device_pixels_and_stay_within_snapped_controls() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let grid = PixelGrid::new(scale);
            let rect = RectDip {
                x: 10.3,
                y: 7.2,
                w: 80.1,
                h: 31.8,
            };
            for strong in [false, true] {
                let (line, stroke) = contrast_outline(rect, grid, [1.0; 4], strong).unwrap();
                for edge in [
                    line.x - stroke.width / 2.0,
                    line.y - stroke.width / 2.0,
                    line.x + line.w + stroke.width / 2.0,
                    line.y + line.h + stroke.width / 2.0,
                ] {
                    assert!((edge * scale - (edge * scale).round()).abs() < 0.001);
                }
                assert_eq!(grid.snap(rect.x), line.x - stroke.width / 2.0);
                assert!(
                    (line.x + line.w + stroke.width / 2.0 - grid.snap(rect.x + rect.w)).abs()
                        < 0.001
                );
                assert!(line.w > 0.0 && line.h > 0.0);
            }
            assert!(contrast_outline(RectDip { w: 0.0, ..rect }, grid, [1.0; 4], true).is_none());
        }
    }

    #[test]
    fn deferred_layout_survives_feedback_until_the_layout_and_content_are_handled() {
        let mut invalidation = FrameInvalidation::Clean;
        invalidation.request(FrameInvalidation::Content);
        invalidation.request(FrameInvalidation::Layout);
        invalidation.request(FrameInvalidation::Feedback);
        invalidation.feedback_committed();
        assert!(invalidation.needs_layout());
        invalidation.layout_resolved();
        assert_eq!(invalidation, FrameInvalidation::Content);
        invalidation.feedback_committed();
        assert!(invalidation.is_pending());
        invalidation = FrameInvalidation::Clean;
        invalidation.request(FrameInvalidation::Feedback);
        invalidation.feedback_committed();
        assert!(!invalidation.is_pending());
    }

    #[test]
    fn keyboard_navigation_uses_controls_even_when_actions_are_the_same() {
        let hits = hits();
        let mut state = ControlInteraction::default();
        state.focus_next(&hits, false);
        assert_eq!(state.focused, Some(1));
        state.focus_next(&hits, false);
        assert_eq!(state.focused, Some(2));
        state.focus_next(&hits, false);
        assert_eq!(state.focused, Some(1));
        state.focus_next(&hits, true);
        assert_eq!(state.focused, Some(2));
        assert!(state.hover(Some(2)));
        assert!(
            !state.hover(Some(2)),
            "same control needs no feedback repaint"
        );
        state.pressed = Some(2);
        state.reconcile(&hits[..1]);
        assert_eq!(state, ControlInteraction::default());
        state.focus_next(&[], true);
        assert_eq!(state.focused, None);
    }

    #[test]
    fn shrinking_viewport_clears_focus_on_a_now_clipped_control() {
        let hits = hits();
        let mut state = ControlInteraction {
            focused: Some(2),
            ..Default::default()
        };
        state.reconcile_viewport(
            &hits,
            RectDip {
                x: 0.0,
                y: 0.0,
                w: 72.0,
                h: 32.0,
            },
        );
        assert_eq!(state.focused, Some(2));
        state.reconcile_viewport(
            &hits,
            RectDip {
                x: 0.0,
                y: 0.0,
                w: 60.0,
                h: 32.0,
            },
        );
        assert_eq!(state.focused, None);
    }

    #[test]
    fn release_on_a_different_control_or_after_dpi_change_does_not_invoke() {
        let hits = hits();
        let identity = crate::FrameIdentity {
            mount_key: crate::MountKey {
                instance: crate::InstanceKey {
                    id: 1,
                    activation: 1,
                },
                generation: 1,
            },
            frame_revision: 1,
            layout_revision: 1,
            theme_epoch: 1,
            device_epoch: 96,
        };
        let invoke =
            |current, release| released_action(identity, current, &hits, &hits, 1, release);
        assert_eq!(invoke(identity, Some(&hits[0])), Some(5));
        assert_eq!(invoke(identity, Some(&hits[1])), None);
        assert_eq!(invoke(identity, None), None);
        assert_eq!(
            invoke(
                crate::FrameIdentity {
                    device_epoch: 144,
                    ..identity
                },
                Some(&hits[0])
            ),
            None
        );
        assert_eq!(
            invoke(
                crate::FrameIdentity {
                    layout_revision: 2,
                    ..identity
                },
                Some(&hits[0])
            ),
            None
        );
        // Fresh paints and theme changes preserve an otherwise valid gesture.
        assert_eq!(
            invoke(
                crate::FrameIdentity {
                    frame_revision: 9,
                    theme_epoch: 2,
                    ..identity
                },
                Some(&hits[0])
            ),
            Some(5)
        );
    }

    #[test]
    fn text_scale_reflows_and_preserves_viewport_origin_without_applying_dpi() {
        let viewport = RectDip {
            x: 17.0,
            y: 23.0,
            w: 600.0,
            h: 400.0,
        };
        for factor in [1.0, 1.25, 1.5, 2.0, 2.25] {
            let scale = ContentScale::new(viewport, factor);
            let logical = scale.logical_viewport();
            assert_eq!((logical.x, logical.y), (17.0, 23.0));
            assert_eq!(scale.rect(logical), viewport);
            assert_eq!(scale.value(BODY), BODY * factor);
            // A physical monitor's scale only affects rasterisation, not layout DIPs.
            for dpi in [96.0, 120.0, 144.0, 192.0] {
                let grid = PixelGrid::new(dpi / 96.0);
                let position = grid.snap(17.3);
                assert!(
                    (position * grid.scale() - (position * grid.scale()).round()).abs() < 0.0001
                );
            }
        }
    }

    #[test]
    fn invalid_scale_cannot_poison_geometry() {
        let viewport = RectDip {
            w: 100.0,
            h: 80.0,
            ..RectDip::default()
        };
        for factor in [f32::NAN, f32::INFINITY, -2.0, 0.0] {
            let scale = ContentScale::new(viewport, factor);
            assert_eq!(scale.factor(), 1.0);
            assert_eq!(scale.rect(scale.logical_viewport()), viewport);
        }
    }

    #[test]
    fn resizing_or_replacing_actions_invalidates_a_pressed_layout() {
        let mut tracker = LayoutTracker::default();
        let mut hits = vec![HitNode {
            id: 1,
            rect: RectDip {
                w: 32.0,
                h: 32.0,
                ..RectDip::default()
            },
            action: 5,
        }];
        let first = tracker.update(&hits, 1).unwrap();
        assert_eq!(first, tracker.update(&hits, 1).unwrap());
        hits[0].rect.w = 64.0;
        let resized = tracker.update(&hits, 1).unwrap();
        assert!(resized > first);
        assert!(tracker.update(&hits, 2).unwrap() > resized);
    }
}

/// Fluent motion tokens shared by the host compositor animations and plugin
/// frame tweens. Platform-free so providers can ease values while preparing frames.
pub mod motion {
    use std::time::Duration;

    /// `ControlFasterAnimationDuration`: fades (hover in/out, tooltip, scrollbar opacity).
    pub const FASTER: Duration = Duration::from_millis(83);
    /// `ControlFastAnimationDuration`: small moves, exits, collapse, scrollbar expand/contract.
    pub const FAST: Duration = Duration::from_millis(167);
    /// `ControlNormalAnimationDuration`: medium moves, entrances.
    pub const NORMAL: Duration = Duration::from_millis(250);
    /// Large entrances / expand (WinUI Expander opens in 333 ms).
    pub const SLOW: Duration = Duration::from_millis(333);

    /// Fluent easing curves. Each maps to a CSS-style `cubic-bezier(x1, y1, x2, y2)`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum Curve {
        Linear,
        /// Fast-out slow-in `(0,0,0,1)`: entrances, things arriving under the pointer.
        Decelerate,
        /// Slow-out fast-in `(1,0,1,1)`: exits, dismissals.
        Accelerate,
        /// `(0.55,0.55,0,1)`: a value moving from one resting place to another (scroll, reorder).
        PointToPoint,
        /// `(1,1,0,1)`: WinUI Expander collapse.
        Collapse,
        /// Zero velocity at both ends, for a liquid plate's continuous silhouette change.
        Smooth,
    }

    impl Curve {
        /// `None` for [`Curve::Linear`].
        pub const fn control_points(self) -> Option<[f32; 4]> {
            match self {
                Curve::Linear => None,
                Curve::Decelerate => Some([0.0, 0.0, 0.0, 1.0]),
                Curve::Accelerate => Some([1.0, 0.0, 1.0, 1.0]),
                Curve::PointToPoint => Some([0.55, 0.55, 0.0, 1.0]),
                Curve::Collapse => Some([1.0, 1.0, 0.0, 1.0]),
                Curve::Smooth => Some([0.42, 0.0, 0.58, 1.0]),
            }
        }

        pub const ALL: [Curve; 6] = [
            Curve::Linear,
            Curve::Decelerate,
            Curve::Accelerate,
            Curve::PointToPoint,
            Curve::Collapse,
            Curve::Smooth,
        ];
    }

    /// Eased progress for `t` in `0..=1` (clamped) — the client-side twin of the compositor's
    /// `CubicBezierEasingFunction`.
    pub fn ease(curve: Curve, t: f32) -> f32 {
        let t = if t.is_nan() { 1.0 } else { t.clamp(0.0, 1.0) };
        match curve.control_points() {
            None => t,
            Some(_) => EASINGS.with(|e| e[curve as usize].sample(t)),
        }
    }

    thread_local! {
        static EASINGS: Vec<bezier_easing::BezierEasing<f32>> = Curve::ALL
            .iter()
            .map(|c| {
                let [x1, y1, x2, y2] = c.control_points().unwrap_or([0.0, 0.0, 1.0, 1.0]);
                bezier_easing::BezierEasing::new(x1, y1, x2, y2).expect("valid Fluent curve")
            })
            .collect();
    }
}

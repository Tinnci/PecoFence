//! Scene building shared by the CI/CD and issue panels: one geometry description feeds paint,
//! hit testing and accessibility. `T` is the panel's own action target.
use crate::board::{palette::Palette, text::Fitter};
use pecofence_plugin_api::{
    scene::{NodeStyle, SceneNode},
    *,
};
use std::collections::HashMap;

pub struct Builder<'a, T> {
    pub nodes: Vec<SceneNode>,
    pub actions: Vec<(String, T, RectDip)>,
    pub rows: Vec<(String, f32)>,
    /// The scroll offset this pass was laid out with, so rows can be recorded in document space.
    pub scroll: f32,
    pub ids: &'a mut HashMap<String, u64>,
    pub next: &'a mut u64,
    pub p: &'a Palette,
    pub fitter: Fitter<'a>,
    pub focus: Option<&'a str>,
    pub top: f32,
    pub bottom: f32,
}
impl<T> Builder<'_, T> {
    #[allow(clippy::too_many_arguments)]
    pub fn node(
        &mut self,
        key: &str,
        rect: RectDip,
        role: &str,
        name: String,
        text: String,
        font: FontRole,
        size: f32,
        weight: u16,
        color: [f32; 4],
        fill: Option<[f32; 4]>,
        target: Option<T>,
    ) {
        if rect.h <= 0.0 || rect.w <= 0.0 || rect.y < self.top || rect.y + rect.h > self.bottom {
            return;
        }
        let id = *self.ids.entry(key.into()).or_insert_with(|| {
            let n = *self.next;
            *self.next += 1;
            n
        });
        let action = target.map(|t| {
            self.actions.push((key.into(), t, rect));
            16 + self.actions.len() as u64 - 1
        });
        let fitted = self
            .fitter
            .with_font(font)
            .fit(&text, size, weight, (rect.w - 12.0).max(1.0), 1)
            .0;
        self.nodes.push(SceneNode {
            id,
            parent: Some(1),
            rect,
            role: role.into(),
            name,
            text: fitted,
            action,
            style: NodeStyle {
                font,
                size,
                weight,
                foreground: color,
                fill,
                stroke: if self.focus == Some(key) || self.p.hc && action.is_some() {
                    Some(StrokeStyle {
                        rgba: self.p.accent_ink,
                        width: 1.5,
                    })
                } else {
                    None
                },
                radius: 6.0,
                text_inset: [6.0, 0.0],
                align: TextAlign::MiddleLeading,
                path: None,
            },
        });
    }
    pub fn label(
        &mut self,
        key: &str,
        r: RectDip,
        text: String,
        color: [f32; 4],
        size: f32,
        font: FontRole,
    ) {
        self.node(
            key,
            r,
            "text",
            text.clone(),
            text,
            font,
            size,
            400,
            color,
            None,
            None,
        );
    }
    /// Diagonal hatch inside `rect`, drawn as stroked paths so running segments differ from done ones
    /// by pattern as well as colour. Lines stay inside the rectangle.
    pub fn hatch(&mut self, key: &str, rect: RectDip, color: [f32; 4]) {
        if rect.h <= 0.0 || rect.w <= 0.0 || rect.y < self.top || rect.y + rect.h > self.bottom {
            return;
        }
        let right = rect.x + rect.w;
        let mut k = 0u32;
        while rect.x + k as f32 * 4.0 + rect.h <= right {
            let x = rect.x + k as f32 * 4.0;
            let id = *self
                .ids
                .entry(format!("{key}:hatch:{k}"))
                .or_insert_with(|| {
                    let n = *self.next;
                    *self.next += 1;
                    n
                });
            self.nodes.push(SceneNode {
                id,
                parent: Some(1),
                rect,
                role: "decoration".into(),
                name: String::new(),
                text: String::new(),
                action: None,
                style: NodeStyle {
                    font: FontRole::Text,
                    size: 11.0,
                    weight: 400,
                    foreground: color,
                    fill: None,
                    stroke: Some(StrokeStyle {
                        rgba: color,
                        width: 1.2,
                    }),
                    radius: 0.0,
                    text_inset: [0.0, 0.0],
                    align: TextAlign::TopLeading,
                    path: Some(Path {
                        points: vec![
                            PointDip {
                                x,
                                y: rect.y + rect.h,
                            },
                            PointDip {
                                x: x + rect.h,
                                y: rect.y,
                            },
                        ],
                        closed: false,
                    }),
                },
            });
            k += 1;
        }
    }
    pub fn section(&mut self, key: &str, y: f32, w: f32, title: &str, meta: &str) {
        self.node(
            key,
            RectDip {
                x: 14.0,
                y,
                w: w - 28.0,
                h: 24.0,
            },
            "heading",
            format!("{title} {meta}"),
            format!("{title}   {meta}"),
            FontRole::Display,
            13.0,
            700,
            self.p.secondary,
            None,
            None,
        );
    }
}
pub fn r(x: f32, y: f32, w: f32, h: f32) -> RectDip {
    RectDip {
        x,
        y,
        w: w.max(0.0),
        h: h.max(0.0),
    }
}

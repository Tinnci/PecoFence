//! Everything that rasterises a surface: chrome panel, content grid, rows, backdrop crop, icon / label fitting and per-item fade sampling.

use crate::commands::DesktopCmd;

use super::*;

fn portal_empty_text(health: &pecofence_core::portal::PortalHealth) -> &'static str {
    use pecofence_core::portal::PortalHealth;
    match health {
        PortalHealth::Loading => pecofence_core::i18n::text("正在读取文件夹"),
        PortalHealth::Ready => pecofence_core::i18n::text("此文件夹为空。"),
        PortalHealth::Stale(_) => pecofence_core::i18n::text("文件夹不可用，内容可能已过期"),
    }
}

#[cfg(test)]
mod window_shape_tests {
    use super::corner_preference;
    use pecofence_platform::dwm::CornerPreference;
    use pecofence_render::{Theme, ThemeMode};

    #[test]
    fn dwm_rounding_matches_the_drawn_radius() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let fluent = Theme::for_mode(mode);
            assert_eq!(corner_preference(&fluent), CornerPreference::Round);
            let contrast = fluent.with_contrast_colors([0; 3], [255; 3]);
            assert_eq!(corner_preference(&contrast), CornerPreference::RoundSmall);
            assert_eq!(
                corner_preference(&fluent.with_liquid_glass()),
                CornerPreference::DoNotRound
            );
        }
    }
}

#[cfg(test)]
mod portal_presentation_tests {
    use super::portal_empty_text;
    use pecofence_core::portal::{PortalHealth, ReadFailure};

    #[test]
    fn unavailable_or_loading_is_not_reported_as_empty() {
        let empty = portal_empty_text(&PortalHealth::Ready);
        assert_ne!(portal_empty_text(&PortalHealth::Loading), empty);
        assert_ne!(
            portal_empty_text(&PortalHealth::Stale(ReadFailure::Open("offline".into()))),
            empty
        );
    }
}

pub(super) fn header_column(c: DetailColumn) -> HeaderColumn {
    match c {
        DetailColumn::Name => HeaderColumn::Name,
        DetailColumn::Date => HeaderColumn::Date,
        DetailColumn::Type => HeaderColumn::Type,
        DetailColumn::Size => HeaderColumn::Size,
    }
}

pub(super) fn sort_column(sort: SortMode) -> Option<HeaderColumn> {
    match sort {
        SortMode::Name => Some(HeaderColumn::Name),
        SortMode::Date => Some(HeaderColumn::Date),
        SortMode::Type => Some(HeaderColumn::Type),
        SortMode::Size => Some(HeaderColumn::Size),
        _ => None,
    }
}

pub(super) fn column_sort(c: DetailColumn) -> SortMode {
    match c {
        DetailColumn::Name => SortMode::Name,
        DetailColumn::Date => SortMode::Date,
        DetailColumn::Type => SortMode::Type,
        DetailColumn::Size => SortMode::Size,
    }
}

/// Title-row things whose state alpha fades (keys of the chrome [`Fades`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum ChromeKey {
    /// Title-row reveal (title-on-hover).
    Title,
    /// Title row hovered: row pill + chevron reveal.
    Hover,
    Up,
    Chevron,
    /// Rolled item count.
    Count,
    Tab(usize),
    /// A tab head with files dragged over it (drop-target look).
    TabDrop(usize),
    /// A tab that just joined the strip (merge, re-attach): 1 → 0 is its fade-in remainder,
    /// so the pill draws at alpha `1 - value` (Fluent Fade In, 83 ms linear).
    TabNew(ContentId),
}

/// The selected anchor's unfolded label: `text` is the full name fitted to `MAX_UNFOLD_LINES`
/// at `cell_w`, `lines` how many lines it takes (measured once, when the cache fills).
pub(super) struct UnfoldCache {
    pub(super) id: ItemId,
    pub(super) cell_w: f32,
    pub(super) name: String,
    pub(super) text: String,
    pub(super) lines: u32,
}

/// Applies (or removes) the DWM system backdrop for the chosen material.
pub(super) fn apply_system_backdrop(hwnd: HWND, mode: BackdropMode, dark: bool) {
    // DWM materials fall back to a solid colour on never-activated windows (verified on 26200),
    // so Acrylic is drawn by our own wallpaper pipeline; DWM is only told the colour mode.
    let _ = mode;
    let _ = dwm::set_system_backdrop(hwnd, dwm::SystemBackdrop::None);
    let _ = dwm::set_immersive_dark_mode(hwnd, dark);
}

/// DWM rounds Fluent windows at the theme's radius (Round = 8 DIP, RoundSmall = 4 DIP), so
/// the drawn rim and shadow meet its outline. Liquid Glass draws its own 24 DIP outline.
pub(super) fn apply_window_shape(hwnd: HWND, theme: &Theme) {
    let _ = dwm::set_nonclient_rendering(hwnd, !theme.is_liquid_glass());
    let _ = dwm::set_corner_preference(hwnd, corner_preference(theme));
}

pub(super) fn corner_preference(theme: &Theme) -> dwm::CornerPreference {
    if theme.is_liquid_glass() {
        dwm::CornerPreference::DoNotRound
    } else if theme.corner_radius >= 8.0 {
        dwm::CornerPreference::Round
    } else {
        dwm::CornerPreference::RoundSmall
    }
}

impl FenceViewState {
    /// The theme this fence draws with. A panel that publishes its own header
    /// (`PanelChrome`) is shown like a Windows Settings window: title bar and content
    /// are one matte paper sheet with no title fill or divider, whatever material the
    /// other fences use. Everything else follows the app-wide style.
    pub(super) fn sheet_theme(&self) -> Theme {
        let base = self.base_theme;
        let mut theme = match self.title_bar.panel_chrome.as_ref() {
            None => base,
            Some(_) if base.is_high_contrast() => base,
            Some(chrome) => match chrome.surface {
                None => base.with_paper(),
                // The panel's own ground decides light or dark paper (it follows the paper
                // preference, which can differ from the app mode), and the sheet takes
                // that exact colour.
                Some(ground) => {
                    let dark = pecofence_plugin_api::paper::luminance(ground) < 0.18;
                    let mode = if dark {
                        pecofence_render::ThemeMode::Dark
                    } else {
                        pecofence_render::ThemeMode::Light
                    };
                    let mut sheet = if mode == base.mode {
                        base
                    } else {
                        let mut other = Theme::for_mode(mode);
                        other.accent = base.accent;
                        other.title_height = base.title_height;
                        other
                    }
                    .with_paper();
                    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                    sheet.solid_fill =
                        ColorF::from_rgba8(byte(ground[0]), byte(ground[1]), byte(ground[2]), 0xFF);
                    sheet
                }
            },
        };
        if self.frameless {
            theme.title_height = 0.0;
        }
        theme
    }

    /// Title decorations as drawn: app-style fences (panel chrome) get Windows caption
    /// buttons instead of the roll-up chevron.
    pub(super) fn title_deco(&self) -> TitleDeco {
        TitleDeco {
            captions: self.title_bar.panel_chrome.is_some() && !self.frameless,
            ..self.title_bar.deco
        }
    }

    /// Re-derive the theme after the panel's chrome appeared or went away, and repaint.
    pub(super) fn refresh_sheet(&mut self) {
        let theme = self.sheet_theme();
        if theme == self.theme {
            return;
        }
        self.theme = theme;
        apply_window_shape(self.hwnd, &self.theme);
        if !self.theme.is_liquid_glass() {
            self.gpu_glass.borrow_mut().reset();
        }
        self.glass_dark_text.set(None);
        self.invalidate_backdrop();
        let (w, h) = window::window_rect_size(self.hwnd);
        let _ = self.layout_panels(w, h);
        let _ = self.redraw();
    }

    /// Liquid Glass and Paper render on the GPU from the shared wallpaper at the exact
    /// window rectangle; Fluent draws a CPU acrylic crop.
    pub(super) fn gpu_material(&self) -> bool {
        matches!(
            self.theme.material().backdrop_source(),
            pecofence_render::material::BackdropSource::Lens
                | pecofence_render::material::BackdropSource::Sheet
        )
    }

    /// Per-window paper fibre variation, stable across restarts.
    fn paper_seed(&self) -> f32 {
        (self.fence_id.as_u128() % 997) as f32
    }

    pub(super) fn material_opacity(&self) -> f32 {
        if self.behavior.floating.get() && self.gpu_material() {
            self.opacity.max(1.0)
        } else {
            self.opacity
        }
    }

    /// Adapt content to the clear plate instead of darkening the whole wallpaper.
    pub(super) fn material_foreground(&self) -> Theme {
        if self.theme.is_liquid_glass() && self.gpu_glass.borrow().failed() {
            return self.theme;
        }
        match &self.backdrop_crop {
            Some(image) if self.theme.is_liquid_glass() => {
                // The GPU renders a complete wallpaper-backed material, so floating
                // fences do not expose application text through a transparent centre.
                let theme = self.theme;
                let foreground = theme.over_glass_with_previous_ink(
                    image,
                    self.material_opacity(),
                    self.style.tint,
                    self.glass_dark_text.get(),
                );
                self.glass_dark_text
                    .set(Some(foreground.text_primary.r < 0.5));
                foreground
            }
            _ => self.theme,
        }
    }

    /// Drops the per-item fades: the view now shows a different list (tab switch, layout
    /// change), so nothing should cross-fade from the old one.
    pub(super) fn reset_item_fades(&mut self) {
        self.file_view.item_hover.clear();
        self.file_view.item_sel.clear();
        self.file_view.drop_fades.clear();
    }

    /// Syncs the per-item hover / selection / drop-target fades from the logical state and
    /// samples them: one `(hover, selection, drop_target)` alpha triple per item. Called at the
    /// top of every content draw.
    pub(super) fn item_alphas(&mut self, now: Instant) -> Vec<(f32, f32, f32)> {
        let animate = self.motion.enabled();
        let dragging = self.file_view.ole_drag;
        for (i, it) in self.file_view.items.iter().enumerate() {
            let hover = (self.file_view.hover == Some(i) && !dragging) as u8 as f32;
            self.file_view.item_hover.set(animate, it.id, hover, now);
            let sel = self.file_view.selected.contains(&i) as u8 as f32;
            self.file_view.item_sel.set(animate, it.id, sel, now);
            let drop = (self.file_view.drop_item == Some(i)) as u8 as f32;
            self.file_view.drop_fades.set(animate, it.id, drop, now);
        }
        let out = self
            .file_view
            .items
            .iter()
            .map(|it| {
                (
                    self.file_view.item_hover.value(&it.id, now),
                    self.file_view.item_sel.value(&it.id, now),
                    self.file_view.drop_fades.value(&it.id, now),
                )
            })
            .collect();
        // Fades that settled at 0 go; ids that left the list were dropped in `replace_items`
        // (no per-draw id set: this runs every frame of a glide).
        self.file_view.item_hover.prune(now, |_| true);
        self.file_view.item_sel.prune(now, |_| true);
        self.file_view.drop_fades.prune(now, |_| true);
        out
    }

    /// Drops finished layout / icon motion before a content draw (a finished tween sits at its
    /// end value, so nothing changes visually) and folds the caret away once it has faded out.
    pub(super) fn prune_content_motion(&mut self, now: Instant) {
        self.file_view.item_motion.retain(|_, m| !m.done(now));
        self.file_view.leaving.retain(|l| !l.alpha.is_done(now));
        for item in &mut self.file_view.items {
            if item.icon_fade.is_some_and(|t| t.is_done(now)) {
                item.icon_fade = None;
            }
        }
        if self.file_view.drop_insert.is_none() && self.file_view.caret_alpha.is_done(now) {
            self.file_view.caret_shown = None;
        }
    }

    /// Notes whether a content fade / motion is still running after a draw and keeps the frame
    /// clock ticking while it is.
    pub(super) fn note_content_fades(&mut self, now: Instant) {
        self.file_view.content_busy = self.file_view.item_hover.busy(now)
            || self.file_view.item_sel.busy(now)
            || self.file_view.header_fades.busy(now)
            || self.file_view.drop_fades.busy(now)
            || !self.file_view.selection_inactive.is_done(now)
            || !self.file_view.drop_inactive.is_done(now)
            || !self.file_view.drop_wash.is_done(now)
            || !self.file_view.caret_alpha.is_done(now)
            || self.file_view.item_motion.values().any(|m| !m.done(now))
            || self.file_view.leaving.iter().any(|l| !l.alpha.is_done(now))
            || self
                .file_view
                .items
                .iter()
                .any(|i| i.icon_fade.is_some_and(|t| !t.is_done(now)));
        if self.file_view.content_busy {
            self.frames.request();
        }
    }

    /// Selection colour blend for this draw: the activation cross-fade, pushed towards neutral
    /// grey (1) while a drag from elsewhere hovers the fence (`drop_inactive`, 83 ms) —
    /// Explorer's view is inactive then and only the drop target reads accent.
    pub(super) fn selection_inactive_now(&self, now: Instant) -> f32 {
        let base = self.file_view.selection_inactive.value_at(now);
        let foreign = self.file_view.drop_inactive.value_at(now).clamp(0.0, 1.0);
        base + (1.0 - base) * foreign
    }

    /// Empty-state caption for the fence being shown.
    pub(super) fn empty_text(&self) -> &'static str {
        if self.file_view.is_inbox {
            pecofence_core::i18n::text("没有未归类的项目")
        } else if self.file_view.drop_hover {
            pecofence_core::i18n::text("松开以放入")
        } else if self.file_view.is_portal {
            portal_empty_text(
                self.file_view
                    .portal_health
                    .as_ref()
                    .unwrap_or(&pecofence_core::portal::PortalHealth::Loading),
            )
        } else {
            pecofence_core::i18n::text("将项目拖到此处")
        }
    }

    /// Syncs the title-row fades from the logical state and samples them for this draw.
    pub(super) fn title_state(&mut self, now: Instant) -> TitleState {
        let animate = self.motion.enabled();
        let title_visible = self.title_visible_target();
        let rolled = self.roll_target();
        let f = &mut self.title_bar.chrome_fades;
        f.set(animate, ChromeKey::Title, title_visible as u8 as f32, now);
        f.set(
            animate,
            ChromeKey::Hover,
            self.title_bar.title_hover as u8 as f32,
            now,
        );
        f.set(
            animate,
            ChromeKey::Up,
            self.title_bar.up_hovered as u8 as f32,
            now,
        );
        f.set(
            animate,
            ChromeKey::Chevron,
            self.title_bar.chevron_hovered as u8 as f32,
            now,
        );
        f.set(animate, ChromeKey::Count, rolled as u8 as f32, now);
        let ntabs = if self.title_bar.tabs.len() > 1 {
            self.title_bar.tabs.len()
        } else {
            0
        };
        for i in 0..ntabs {
            f.set(
                animate,
                ChromeKey::Tab(i),
                (self.title_bar.tab_hover == Some(i)) as u8 as f32,
                now,
            );
            f.set(
                animate,
                ChromeKey::TabDrop(i),
                (self.title_bar.drop_tab == Some(i)) as u8 as f32,
                now,
            );
        }
        let inside = self.title_bar.press_inside;
        TitleState {
            title: f.value(&ChromeKey::Title, now),
            hover: f.value(&ChromeKey::Hover, now),
            up_hover: f.value(&ChromeKey::Up, now),
            up_pressed: self.title_bar.pressed == Some(PressTarget::Up) && inside,
            chevron_hover: f.value(&ChromeKey::Chevron, now),
            chevron_pressed: self.title_bar.pressed == Some(PressTarget::Chevron) && inside,
            count: f.value(&ChromeKey::Count, now),
            merge_hint: self.title_bar.merge_t.value_at(now),
            caption_hover: self.title_bar.caption_hovered,
            caption_pressed: match self.title_bar.pressed {
                Some(PressTarget::Caption(button)) if inside => Some(button),
                _ => None,
            },
            maximized: self.title_bar.maximized,
        }
    }

    /// Places the title-row fades at their rest values (first draw: nothing fades in).
    pub(super) fn snap_chrome_fades(&mut self) {
        let now = Instant::now();
        let title = self.title_visible_target() as u8 as f32;
        self.title_bar
            .chrome_fades
            .snap(ChromeKey::Title, title, now);
        self.title_bar
            .chrome_fades
            .snap(ChromeKey::Count, self.roll_target() as u8 as f32, now);
    }

    /// Draws the chrome (backdrop, tabs or title, rim, chevron).
    pub(super) fn draw_chrome_panel(&mut self) -> Result<()> {
        let now = Instant::now();
        let state = self.title_state(now);
        // DIP height the crop covers (taller than the surface mid height animation).
        let crop_h = self
            .backdrop_rect
            .map_or(0.0, |(_, _, _, h)| h as f32 / self.scale());
        let crop = self.backdrop_crop.as_ref().map(|img| BackdropCrop {
            image: img,
            key: &self.backdrop_key,
            height: crop_h,
        });
        let wallpaper = self.backdrops.for_mode(self.backdrop_mode()).clone();
        let backdrop = match (self.backdrop_mode(), crop) {
            (BackdropMode::Acrylic, _) if self.gpu_material() && !wallpaper.is_empty() => {
                let (x, y, w, h) = self.backdrop_rect.unwrap_or_else(|| {
                    let r = window::window_rect(self.hwnd);
                    (r.left, r.top, r.right - r.left, r.bottom - r.top)
                });
                Backdrop::GpuGlass {
                    material: &self.gpu_glass,
                    wallpaper: &wallpaper,
                    rect: [x, y, w, h],
                    scale: self.scale(),
                    seed: self.paper_seed(),
                }
            }
            (BackdropMode::Acrylic, Some(crop)) => Backdrop::Glass(crop, self.scale()),
            _ => Backdrop::Solid,
        };
        let bitmaps = self.bitmaps.clone();
        // A panel that publishes its own header names the title row.
        let title = self
            .title_bar
            .panel_chrome
            .as_ref()
            .filter(|c| !c.title.is_empty() && self.title_bar.tabs.len() <= 1)
            .map_or_else(|| self.title_bar.title.clone(), |c| c.title.clone());
        let commands: Vec<pecofence_render::fence_chrome::TitleCommandDraw> = self
            .title_bar
            .panel_chrome
            .as_ref()
            .map(|c| {
                c.commands
                    .iter()
                    .enumerate()
                    .map(
                        |(i, command)| pecofence_render::fence_chrome::TitleCommandDraw {
                            glyph: command.glyph,
                            enabled: command.enabled,
                            hover: (self.title_bar.command_hovered == Some(i)) as u8 as f32,
                            pressed: self.title_bar.pressed == Some(PressTarget::Command(i))
                                && self.title_bar.press_inside,
                        },
                    )
                    .collect()
            })
            .unwrap_or_default();
        let command_labels: Vec<&str> = self
            .title_bar
            .panel_chrome
            .as_ref()
            .map(|c| {
                c.commands
                    .iter()
                    .map(|command| command.label.as_str())
                    .collect()
            })
            .unwrap_or_default();
        let mut title_chrome = pecofence_render::fence_chrome::TitleChrome {
            subtitle: self
                .title_bar
                .panel_chrome
                .as_ref()
                .map_or("", |c| c.subtitle.as_str()),
            status: self
                .title_bar
                .panel_chrome
                .as_ref()
                .and_then(|c| c.status.as_ref())
                .map(|s| (s.text.as_str(), s.severity)),
            commands: &commands,
            controls: &[],
        };
        let rolled = self.roll_target();
        // A roll set / cancelled without the animation (reduced motion, retire, external
        // geometry) leaves the progress tween behind: rest it at the settled state.
        if self.animation.roll_anim.is_none()
            && (self.animation.roll_t.target() - self.animation.rolled_up as u8 as f32).abs()
                > f32::EPSILON
        {
            self.animation.roll_t = Tween::at(self.animation.rolled_up as u8 as f32, now);
        }
        let roll_t = self.animation.roll_t.value_at(now);
        // The count is drawn while it fades out after expanding, not only while rolled.
        let count =
            (self.panel_host.plugin_panel.is_none() && (rolled || state.count > 0.0)).then(|| {
                pecofence_core::i18n::format("{0} 项", &[format!("{}", self.file_view.items.len())])
            });
        let opacity = self.material_opacity();
        let theme = self.material_foreground();
        let scale = self.scale();
        let chrome = self.chrome.clone();
        let rects = self.tab_rects();
        let xs = self.tab_draw_xs(now);
        let active = self.active_index();
        // A pressed tab counts as dragged only past the drag threshold; a plain click keeps the
        // pressed caption and the SwitchTab pill glide.
        let dragging = self
            .title_bar
            .tab_drag
            .as_ref()
            .filter(|d| d.moved)
            .map(|d| d.index);
        // A dragged tab carries the active fill itself: no pill glide underneath it.
        if dragging.is_some() {
            self.title_bar.pill_anim = None;
        }
        // Active pill mid-glide: drawn at the tweened place instead of on the active tab.
        let pill = self
            .title_bar
            .pill_anim
            .map(|(x, w)| (x.value_at(now), w.value_at(now)));
        if self
            .title_bar
            .pill_anim
            .is_some_and(|(x, w)| x.is_done(now) && w.is_done(now))
        {
            self.title_bar.pill_anim = None;
        }
        let deco = self.title_deco();
        let style = self.style;
        let pressed_tab = match self.title_bar.pressed {
            Some(PressTarget::Tab(t)) if self.title_bar.press_inside => Some(t),
            _ => None,
        };
        let tabs: Vec<TabDraw<'_>> = self
            .title_bar
            .tabs
            .iter()
            .zip(rects.iter())
            .enumerate()
            .map(|(i, (t, (x, w)))| TabDraw {
                key: t.id.as_u128(),
                x: xs.get(i).copied().unwrap_or(*x),
                w: *w,
                title: &t.title,
                title_size: t.title_size,
                title_color: t.title_color,
                active: i == active,
                hover: self.title_bar.chrome_fades.value(&ChromeKey::Tab(i), now),
                pressed: pressed_tab == Some(i),
                color: t.color.map(|c| ColorF::from_rgba8(c[0], c[1], c[2], 0xFF)),
                drop_target: self
                    .title_bar
                    .chrome_fades
                    .value(&ChromeKey::TabDrop(i), now),
                dragging: dragging == Some(i),
                alpha: 1.0
                    - self
                        .title_bar
                        .chrome_fades
                        .value(&ChromeKey::TabNew(t.id), now),
            })
            .collect();
        let chrome_scene =
            super::chrome_scene::ChromeScene::new(super::chrome_scene::ChromeInput {
                active: self.active,
                dpi: self.dpi,
                device: self.stack.device_generation(),
                surface: self.chrome_panel.surface_epoch(),
                size: self.chrome_panel.size_px(),
                title_height: theme.title_height,
                title: &title,
                tabs: &tabs,
                deco,
                rolled,
                commands: &command_labels,
                maximized: self.title_bar.maximized,
                state,
                command_states: &commands,
            })
            .with_clipped_captions(
                tabs.iter()
                    .filter(|tab| !chrome.tab_title_fits(tab.title, tab.w, tab.title_size, &theme))
                    .map(|tab| ContentId::from_u128(tab.key)),
            );
        title_chrome.controls = chrome_scene.controls();
        let ok = self
            .chrome_panel
            .draw_candidate(self.dpi, |session, wd, hd| {
                chrome.draw(
                    session,
                    &mut bitmaps.borrow_mut(),
                    &theme,
                    scale,
                    wd,
                    hd,
                    backdrop,
                    &title,
                    count.as_deref(),
                    rolled,
                    roll_t,
                    opacity,
                    &tabs,
                    pill,
                    deco,
                    state,
                    style,
                    &title_chrome,
                )
            })?;
        if ok {
            self.title_bar.chrome_scene = Some(chrome_scene);
            self.publish_accessibility();
        }
        self.note_draw_result(ok);
        let ntabs = self.title_bar.tabs.len();
        self.title_bar.chrome_fades.prune(now, |k| match k {
            ChromeKey::Tab(i) | ChromeKey::TabDrop(i) => *i < ntabs,
            ChromeKey::TabNew(id) => self.title_bar.tabs.iter().any(|t| t.id == *id),
            _ => true,
        });
        self.title_bar.tab_slide.retain(|(_, t)| !t.is_done(now));
        if self
            .title_bar
            .tab_settle
            .is_some_and(|(_, t)| t.is_done(now))
        {
            self.title_bar.tab_settle = None;
        }
        self.title_bar.chrome_busy = self.title_bar.chrome_fades.busy(now)
            || self.title_bar.pill_anim.is_some()
            || !self.title_bar.merge_t.is_done(now)
            || !self.title_bar.tab_slide.is_empty()
            || self.title_bar.tab_settle.is_some();
        if self.title_bar.chrome_busy {
            self.frames.request();
        }
        Ok(())
    }

    /// Re-crops the wallpaper under the window when its rect changed. While the height
    /// animates the crop is cut once for the taller end of the tween in either direction —
    /// the taller of the current height, the tween's target and the crop already in hand —
    /// so the moving bottom edge only reveals or conceals finished pixels. Liquid Glass
    /// instead follows the curved bottom bezel each frame. Returns whether the crop changed.
    pub(super) fn update_backdrop(&mut self) -> bool {
        let r = window::window_rect(self.hwnd);
        self.update_backdrop_at(r)
    }

    /// Also accepts a pending pure move, so the GPU surface is prepared before Windows
    /// changes the HWND position. Only a tiny contrast sample is computed on the CPU.
    pub(super) fn update_backdrop_at(&mut self, r: RECT) -> bool {
        let w = r.right - r.left;
        let mut h = r.bottom - r.top;
        // Liquid Glass's displacement field is rebuilt by the GPU renderer only on
        // size/DPI changes; position changes retain every uploaded image.
        if !self.gpu_material()
            && let Some(target) = self.height_target_px()
        {
            h = h.max(target);
        }
        if self.backdrop_crop.is_some()
            && let Some((l, t, cw, ch)) = self.backdrop_rect
            && (l, t, cw) == (r.left, r.top, w)
            && (ch == h || (!self.gpu_material() && self.height_animating() && ch >= h))
        {
            // Same rect, or a collapse / shrink whose crop (cut for the taller start) still
            // covers the visible band: the surface clips it.
            return false;
        }
        let want = (r.left, r.top, w, h);
        let cx = r.left + w / 2;
        let cy = r.top + h.min(r.bottom - r.top) / 2;
        // The set for this fence's *effective* material, so an override crops the right tint.
        let set = self.backdrops.for_mode(self.backdrop_mode());
        let hit = set
            .iter()
            .find(|b| cx >= b.left && cx < b.left + b.width && cy >= b.top && cy < b.top + b.height)
            .or(set.first());
        self.backdrop_crop = if self.gpu_material() && !set.is_empty() {
            Some(pecofence_render::liquid_glass::foreground_sample(
                set,
                [r.left, r.top, w, h],
            ))
        } else {
            hit.map(|b| b.crop_screen_rect(r.left, r.top, w, h))
        };
        self.backdrop_rect = Some(want);
        // The uploaded bitmap belongs to the old crop.
        if !self.gpu_material() {
            self.bitmaps.borrow_mut().remove(&self.backdrop_key);
        }
        true
    }

    /// Forces a fresh crop (new wallpaper set, material or theme) even at the same rect.
    pub(super) fn invalidate_backdrop(&mut self) {
        self.backdrop_rect = None;
        self.update_backdrop();
    }

    /// Lays out the content panel below the title bar and resizes both surfaces.
    pub(super) fn layout_panels(&mut self, w: i32, h: i32) -> Result<()> {
        // A content swap still moving would be laid out at the wrong size: settle it first.
        self.finish_content_swap_now();
        // Entrance / exit scales pivot on the window centre (visual units are physical px).
        self.motion
            .set_center(&self.root, w as f32 / 2.0, h as f32 / 2.0);
        self.update_shape_clip(w, h)?;
        self.chrome_panel.resize(w, h)?;
        let title_h = self.title_h_px();
        let content_h = (h - title_h).max(1);
        self.content_panel.resize(w, content_h)?;
        self.content_panel
            .visual
            .set_offset(0.0, title_h as f32, 0.0);
        self.content_panel
            .visual
            .set_visible(!self.animation.rolled_up || self.animation.roll_anim.is_some());
        Ok(())
    }

    /// The clip must follow every height-animation frame even while content stays frozen.
    /// It sits 1 px outside the plate: the chrome draws the antialiased outline itself, and
    /// a second antialiasing pass on the same pixels would thin the corner arcs.
    pub(super) fn update_shape_clip(&self, w: i32, h: i32) -> Result<()> {
        if self.theme.is_liquid_glass() {
            self.motion.clip_rounded(
                &self.root,
                w as f32,
                h as f32,
                self.theme.corner_radius * self.scale(),
                1.0,
            )?;
        } else {
            self.motion.clear_clip(&self.root)?;
        }
        Ok(())
    }

    /// `label_w` is the width the label must fit: the cell width for icons, the name column's
    /// text width for rows. Changing it drops every fitted label.
    pub(super) fn ensure_icons_and_labels(&mut self, label_w: f32) {
        let icon_px = (self.icon_dip() * self.scale()).round() as u32;
        if (self.file_view.label_w - label_w).abs() > 0.5 {
            self.file_view.label_w = label_w;
            for item in &mut self.file_view.items {
                item.label = None;
            }
            self.file_view.unfold_cache = None;
        }
        let details = self.file_view.layout == ViewLayout::Details;
        let now = Instant::now();
        let mut started = false;
        let mut icons = self.file_view.icons.borrow_mut();
        for item in &mut self.file_view.items {
            if item.icon.is_none() && !item.icon_failed {
                match icons.get(&item.icon_key, &item.path, icon_px, item.icon_only) {
                    Lookup::Ready(Some(img)) => {
                        item.icon = Some(img);
                        if item.icon_waited {
                            // Delivered by the worker: cross-fade over the loading tile (167 ms
                            // linear); a synchronous cache hit shows at once.
                            item.icon_waited = false;
                            item.icon_fade =
                                Some(
                                    self.motion
                                        .tween(0.0, 1.0, motion::FAST, Curve::Linear, now),
                                );
                            started = true;
                        }
                    }
                    Lookup::Ready(None) => item.icon_failed = true,
                    Lookup::Pending => item.icon_waited = true,
                }
            }
            if item.label.is_none() {
                item.label = Some(match self.file_view.layout {
                    ViewLayout::Icons => pecofence_render::text::fit_lines(
                        &item.name,
                        &self.chrome.label_format(),
                        (label_w - pecofence_render::fence_chrome::ICON_LABEL_SIDE_INSET * 2.0)
                            .max(1.0),
                        self.file_view.label_lines as u32,
                    ),
                    _ => pecofence_render::text::fit_width(
                        &item.name,
                        &self.chrome.row_format(),
                        label_w,
                    ),
                });
            }
            if details {
                if item.date_label.is_none() {
                    item.date_label = Some(item.date_text());
                }
                if item.type_label.is_none() {
                    item.type_label =
                        Some(fileinfo::type_name(&item.path, item.is_folder).unwrap_or_default());
                }
                if item.size_label.is_none() {
                    item.size_label = Some(item.size_text());
                }
            }
        }
        drop(icons);
        if started {
            self.frames.request();
        }
    }

    /// True when some item is still waiting for its icon.
    pub fn has_pending_icons(&self) -> bool {
        self.file_view
            .items
            .iter()
            .any(|i| i.icon.is_none() && !i.icon_failed)
    }

    /// Redraws the chrome surface only (backdrop/title); the content surface is untouched.
    pub fn redraw_chrome_only(&mut self) -> Result<()> {
        self.draw_chrome_panel()
    }

    /// `Ok(false)` from a surface draw means the GPU device is gone: ask the app to rebuild the
    /// stack once, then recreate our surfaces (`recreate_surfaces`).
    pub(super) fn note_draw_result(&mut self, ok: bool) {
        if !ok && !self.device_lost {
            self.device_lost = true;
            tracing::warn!(fence = %self.fence_id, "composition surface lost its device");
            self.queue.push(Command::Desktop(DesktopCmd::DeviceLost));
        }
    }

    pub fn redraw(&mut self) -> Result<()> {
        self.draw_chrome_panel()?;
        if self.animation.rolled_up && self.animation.roll_anim.is_none() {
            return Ok(());
        }
        self.redraw_content()
    }

    pub(super) fn columns_for(&self, width_dip: f32) -> DetailColumns {
        match self.file_view.layout {
            ViewLayout::Details => DetailColumns::for_width_with(
                width_dip,
                self.file_view.column_widths,
                self.file_view.columns_visible,
            ),
            _ => DetailColumns {
                name_x: 8.0,
                name_w: (width_dip - 16.0).max(40.0),
                date: None,
                type_: None,
                size: None,
            },
        }
    }

    /// Fills `unfold_cache` for item `i` at `cell_w` unless it already holds that entry: one
    /// DirectWrite layout pass per anchor / width / name change instead of one per redraw.
    pub(super) fn refresh_unfold_cache(&mut self, i: usize, cell_w: f32) {
        let it = &self.file_view.items[i];
        let fresh = self
            .file_view
            .unfold_cache
            .as_ref()
            .is_some_and(|c| c.id == it.id && c.cell_w == cell_w && c.name == it.name);
        if fresh {
            return;
        }
        let (text, lines) = pecofence_render::text::fit_lines_counted(
            &it.name,
            &self.chrome.label_format(),
            (cell_w - pecofence_render::fence_chrome::ICON_LABEL_SIDE_INSET * 2.0).max(1.0),
            MAX_UNFOLD_LINES,
        );
        self.file_view.unfold_cache = Some(UnfoldCache {
            id: it.id,
            cell_w,
            name: it.name.clone(),
            text,
            lines,
        });
    }

    pub(super) fn redraw_content(&mut self) -> Result<()> {
        if self
            .panel_host
            .plugin_panel
            .as_ref()
            .is_some_and(|panel| panel.invalidation.needs_layout())
        {
            let (width, height) = window::window_rect_size(self.hwnd);
            self.layout_panels(width, height)?;
            if let Some(panel) = &mut self.panel_host.plugin_panel {
                panel.invalidation.layout_resolved();
            }
        }
        if self.panel_host.plugin_panel.is_some() {
            let foreground = self.material_foreground();
            let rect = window::window_rect(self.hwnd);
            let cx = (rect.left + rect.right) / 2;
            let cy = (rect.top + rect.bottom) / 2;
            let set = self.backdrops.for_mode(self.backdrop_mode());
            let wallpaper_luminance = set
                .iter()
                .find(|b| {
                    cx >= b.left && cx < b.left + b.width && cy >= b.top && cy < b.top + b.height
                })
                .or(set.first())
                .map(|b| b.wallpaper_luminance);
            let paper = pecofence_plugin_api::PaperEnvironment {
                style: match self.behavior.widget_paper_style.get() {
                    pecofence_core::WidgetPaperStyle::Adaptive => {
                        pecofence_plugin_api::PaperStyle::Adaptive
                    }
                    pecofence_core::WidgetPaperStyle::Light => {
                        pecofence_plugin_api::PaperStyle::Light
                    }
                    pecofence_core::WidgetPaperStyle::Soft => {
                        pecofence_plugin_api::PaperStyle::Soft
                    }
                    pecofence_core::WidgetPaperStyle::Dark => {
                        pecofence_plugin_api::PaperStyle::Dark
                    }
                },
                brightness: self.behavior.widget_paper_brightness.get(),
                wallpaper_luminance,
            };
            let panel = self
                .panel_host
                .plugin_panel
                .as_mut()
                .expect("plugin panel is present");
            let exposure = panel.exposure();
            panel.set_container_state(
                if self.title_bar.tabs.len() > 1 {
                    pecofence_plugin_api::Grouping::Tabbed
                } else {
                    pecofence_plugin_api::Grouping::Single
                },
                exposure,
                exposure != pecofence_plugin_api::Exposure::Hidden,
            );
            let drawn = panel.draw(
                &self.content_panel,
                self.dpi,
                self.behavior.text_scale.get(),
                (u64::from(self.stack.device_generation()) << 32) | u64::from(self.dpi),
                &foreground,
                paper,
            );
            panel.wake_capture_release(self.hwnd);
            self.note_draw_result(drawn?);
            self.publish_accessibility();
            return Ok(());
        }
        if self.row_metrics().is_some() {
            return self.redraw_rows();
        }
        let (cw, ch) = self.content_size_px();
        let scale = self.scale();
        let width_dip = cw as f32 / scale;
        let height_dip = ch as f32 / scale;
        let layout = self.layout(width_dip);
        let ItemLayout::Grid(grid) = &layout else {
            return Ok(());
        };
        let max_scroll = grid.max_scroll(height_dip);
        self.file_view.scroll_y = self.file_view.scroll_y.clamp(0.0, max_scroll);
        self.ensure_icons_and_labels(grid.metrics.cell_w);

        let now = Instant::now();
        self.prune_content_motion(now);
        let mut file_scene = self.prepare_file_scene(&layout, now);
        self.sync_scrollbar_alpha();
        let alphas = self.item_alphas(now);
        let scroll = self.file_view.scroll_y;
        let dragging = self.file_view.ole_drag;
        let cut = self.file_view.cut.clone();
        let pressed = match self.title_bar.pressed {
            Some(PressTarget::Item(i)) if self.title_bar.press_inside => Some(i),
            _ => None,
        };
        let focused = self
            .file_view
            .focus_visible
            .then_some(self.file_view.anchor_index)
            .flatten();
        // The selected anchor item shows its full name (Windows desktop): only when the fitted
        // label had to be shortened, and never under an open rename edit.
        let unfold = self.file_view.anchor_index.filter(|&a| {
            a < self.file_view.items.len()
                && self.file_view.selected.contains(&a)
                && !dragging
                && self.file_view.renaming != Some(self.file_view.items[a].id)
        });
        let shortened = unfold.filter(|&i| {
            let it = &self.file_view.items[i];
            it.label.as_deref().unwrap_or(&it.name) != it.name
        });
        if let Some(i) = shortened {
            self.refresh_unfold_cache(i, grid.metrics.cell_w);
        }
        let full: Option<(&str, u32)> = shortened.and_then(|i| {
            let id = self.file_view.items[i].id;
            self.file_view
                .unfold_cache
                .as_ref()
                .filter(|c| c.id == id)
                .map(|c| (c.text.as_str(), c.lines))
        });
        if let Some((index, (_, lines))) = shortened.zip(full) {
            file_scene.promote(
                index,
                (lines as f32 - self.file_view.label_lines as f32).max(0.0) * grid.metrics.line_h,
            );
        }
        let keys: Vec<String> = {
            let icons = self.file_view.icons.borrow();
            let px = (self.file_view.icon_size as f32 * scale).round() as u32;
            self.file_view
                .items
                .iter()
                .map(|i| icons.draw_key(&i.icon_key, px))
                .collect()
        };
        // Removed items fade out beneath everything else (painted first).
        let mut cells: Vec<ItemCell<'_>> = self
            .file_view
            .leaving
            .iter()
            .map(|l| ItemCell {
                x: l.x,
                y: l.y - scroll,
                w: l.w,
                h: l.h,
                icon_key: &l.icon_key,
                icon: l.icon.as_deref(),
                label: &l.label,
                hover: 0.0,
                selection: 0.0,
                pressed: false,
                focused: false,
                dimmed: false,
                failed: l.failed,
                drop_target: 0.0,
                alpha: l.alpha.value_at(now),
                icon_alpha: 1.0,
                full_label: None,
            })
            .collect();
        cells.extend(self.file_view.items.iter().enumerate().map(|(i, item)| {
            let node = &file_scene.nodes[i];
            let c = node.geometry.bounds;
            ItemCell {
                x: c.x,
                y: c.y,
                w: c.w,
                h: c.h,
                icon_key: &keys[i],
                icon: item.icon.as_deref(),
                label: item.label.as_deref().unwrap_or(&item.name),
                hover: alphas[i].0,
                selection: alphas[i].1,
                pressed: pressed == Some(i) && !dragging,
                focused: focused == Some(i),
                dimmed: cut.contains(&item.id),
                failed: item.icon_failed,
                drop_target: alphas[i].2,
                alpha: node.alpha,
                icon_alpha: item.icon_fade.map_or(1.0, |t| t.value_at(now)),
                full_label: if unfold == Some(i) { full } else { None },
            }
        }));
        let group_headers = group_header_draws(&layout, scroll, 0.0);
        let scrollbar = self.scrollbar_draw();
        let content = ContentDraw {
            items: &cells,
            group_headers: &group_headers,
            icon_size: self.file_view.icon_size as f32,
            label_lines: self.file_view.label_lines,
            line_h: grid.metrics.line_h,
            icon_top: grid.metrics.icon_top,
            label_gap: grid.metrics.label_gap,
            scrollbar,
            empty_text: Some(self.empty_text()),
            drop_highlight: self.file_view.drop_wash.value_at(now),
            marquee: self.marquee_rect_dip(),
            insert_caret: self.insert_caret_dip(0.0),
            insert_caret_alpha: self.file_view.caret_alpha.value_at(now),
            selection_inactive: self.selection_inactive_now(now),
        };
        let theme = self.material_foreground();
        let chrome = self.chrome.clone();
        let bitmaps = self.bitmaps.clone();
        let ok = self
            .content_panel
            .draw_candidate(self.dpi, |session, wd, hd| {
                chrome.draw_content(
                    session,
                    &theme,
                    scale,
                    &mut bitmaps.borrow_mut(),
                    wd,
                    hd,
                    &content,
                )
            })?;
        if ok {
            self.file_view.file_scene = Some(file_scene);
            self.publish_accessibility();
        }
        self.note_draw_result(ok);
        self.note_content_fades(now);
        Ok(())
    }

    /// List / Details: rows below an optional fixed header.
    pub(super) fn redraw_rows(&mut self) -> Result<()> {
        let Some(metrics) = self.row_metrics() else {
            return Ok(());
        };
        let (cw, ch) = self.content_size_px();
        let scale = self.scale();
        let width_dip = cw as f32 / scale;
        let height_dip = ch as f32 / scale;
        let layout = self.layout(width_dip);
        let view_h = (height_dip - metrics.header_h).max(1.0);
        let max_scroll = layout.max_scroll(view_h);
        self.file_view.scroll_y = self.file_view.scroll_y.clamp(0.0, max_scroll);
        let cols = self.columns_for(width_dip);
        let text_w = (cols.name_w - metrics.icon - 8.0 - 4.0).max(20.0);
        self.ensure_icons_and_labels(text_w);

        let now = Instant::now();
        self.prune_content_motion(now);
        let file_scene = self.prepare_file_scene(&layout, now);
        self.sync_scrollbar_alpha();
        let alphas = self.item_alphas(now);
        let scroll = self.file_view.scroll_y;
        let dragging = self.file_view.ole_drag;
        let cut = self.file_view.cut.clone();
        let pressed_item = match self.title_bar.pressed {
            Some(PressTarget::Item(i)) if self.title_bar.press_inside => Some(i),
            _ => None,
        };
        let header_pressed = match self.title_bar.pressed {
            Some(PressTarget::Header(c)) if self.title_bar.press_inside => Some(header_column(c)),
            _ => None,
        };
        let focused = self
            .file_view
            .focus_visible
            .then_some(self.file_view.anchor_index)
            .flatten();
        // Header cell hover fades (Details only).
        let header_alphas: Vec<(HeaderColumn, f32)> = if metrics.header_h > 0.0 {
            let animate = self.motion.enabled();
            let hovered = self.file_view.header_hover.map(header_column);
            let shown = [
                Some(HeaderColumn::Name),
                cols.date.map(|_| HeaderColumn::Date),
                cols.type_.map(|_| HeaderColumn::Type),
                cols.size.map(|_| HeaderColumn::Size),
            ];
            for c in shown.into_iter().flatten() {
                self.file_view
                    .header_fades
                    .set(animate, c, (hovered == Some(c)) as u8 as f32, now);
            }
            let out = shown
                .into_iter()
                .flatten()
                .map(|c| (c, self.file_view.header_fades.value(&c, now)))
                .collect();
            self.file_view.header_fades.prune(now, |_| true);
            out
        } else {
            self.file_view.header_fades.clear();
            Vec::new()
        };
        let icon_px = (metrics.icon * scale).round() as u32;
        let keys: Vec<String> = {
            let icons = self.file_view.icons.borrow();
            self.file_view
                .items
                .iter()
                .map(|i| icons.draw_key(&i.icon_key, icon_px))
                .collect()
        };
        // Removed rows fade out beneath everything else (painted first).
        let mut rows: Vec<RowCell<'_>> = self
            .file_view
            .leaving
            .iter()
            .map(|l| RowCell {
                y: l.y - scroll + metrics.header_h,
                h: l.h,
                icon_key: &l.icon_key,
                icon: l.icon.as_deref(),
                name: &l.label,
                date: &l.date,
                type_name: &l.type_,
                size: &l.size,
                hover: 0.0,
                selection: 0.0,
                pressed: false,
                focused: false,
                dimmed: false,
                failed: l.failed,
                drop_target: 0.0,
                alpha: l.alpha.value_at(now),
                icon_alpha: 1.0,
            })
            .collect();
        rows.extend(self.file_view.items.iter().enumerate().map(|(i, item)| {
            let node = &file_scene.nodes[i];
            let c = node.geometry.bounds;
            RowCell {
                y: c.y,
                h: c.h,
                icon_key: &keys[i],
                icon: item.icon.as_deref(),
                name: item.label.as_deref().unwrap_or(&item.name),
                date: item.date_label.as_deref().unwrap_or(""),
                type_name: item.type_label.as_deref().unwrap_or(""),
                size: item.size_label.as_deref().unwrap_or(""),
                hover: alphas[i].0,
                selection: alphas[i].1,
                pressed: pressed_item == Some(i) && !dragging,
                focused: focused == Some(i),
                dimmed: cut.contains(&item.id),
                failed: item.icon_failed,
                drop_target: alphas[i].2,
                alpha: node.alpha,
                icon_alpha: item.icon_fade.map_or(1.0, |t| t.value_at(now)),
            }
        }));
        let group_headers = group_header_draws(&layout, scroll, metrics.header_h);
        let scrollbar = self.scrollbar_draw();
        let empty_text = Some(self.empty_text());
        let marquee = self.marquee_rect_dip().map(|m| Rect {
            left: m.left,
            top: m.top + metrics.header_h,
            right: m.right,
            bottom: m.bottom + metrics.header_h,
        });
        let draw = RowsDraw {
            rows: &rows,
            group_headers: &group_headers,
            columns: RowColumns {
                name_x: cols.name_x,
                name_w: cols.name_w,
                date: cols.date,
                type_: cols.type_,
                size: cols.size,
            },
            icon_size: metrics.icon,
            header_h: metrics.header_h,
            sort_column: sort_column(self.file_view.sort),
            sort_descending: self.file_view.sort_reverse,
            header_hover: &header_alphas,
            header_pressed,
            scrollbar,
            empty_text,
            drop_highlight: self.file_view.drop_wash.value_at(now),
            marquee,
            insert_caret: self.insert_caret_dip(metrics.header_h),
            insert_caret_alpha: self.file_view.caret_alpha.value_at(now),
            selection_inactive: self.selection_inactive_now(now),
        };
        let theme = self.material_foreground();
        let chrome = self.chrome.clone();
        let bitmaps = self.bitmaps.clone();
        let wallpaper = self.backdrops.for_mode(self.backdrop_mode()).clone();
        let backdrop = if self.theme.is_liquid_glass() && !wallpaper.is_empty() {
            let r = self.backdrop_rect.unwrap_or_else(|| {
                let r = window::window_rect(self.hwnd);
                (r.left, r.top, r.right - r.left, r.bottom - r.top)
            });
            Some(Backdrop::GpuGlass {
                material: &self.gpu_glass,
                wallpaper: &wallpaper,
                rect: [r.0, r.1, r.2, r.3],
                scale,
                seed: self.paper_seed(),
            })
        } else {
            None
        };
        let opacity = self.material_opacity();
        let ok = self
            .content_panel
            .draw_candidate_ex(self.dpi, |session, clip, wd, hd| {
                chrome.draw_rows(
                    session,
                    Some(clip),
                    &theme,
                    scale,
                    &mut bitmaps.borrow_mut(),
                    wd,
                    hd,
                    &draw,
                    backdrop.as_ref().map(|b| (b, opacity, self.style.tint)),
                )
            })?;
        if ok {
            self.file_view.file_scene = Some(file_scene);
            self.publish_accessibility();
        }
        self.note_draw_result(ok);
        self.note_content_fades(now);
        Ok(())
    }
}

/// Localised caption of a "按时间分组" section (literal keys keep `check-locales.py` honest).
pub(super) fn bucket_caption(bucket: DateBucket) -> &'static str {
    match bucket {
        DateBucket::Today => pecofence_core::i18n::text("今天"),
        DateBucket::Yesterday => pecofence_core::i18n::text("昨天"),
        DateBucket::ThisWeek => pecofence_core::i18n::text("本周"),
        DateBucket::ThisMonth => pecofence_core::i18n::text("本月"),
        DateBucket::Earlier => pecofence_core::i18n::text("更早"),
    }
}

/// Section headers of `layout` in surface DIPs: scrolled by `scroll` and, for rows, pushed
/// below the fixed column header by `header_h` (the same offset the row cells get). Headers
/// are static: item motion glides the items, the bands stay where the new layout puts them.
pub(super) fn group_header_draws(
    layout: &ItemLayout,
    scroll: f32,
    header_h: f32,
) -> Vec<GroupHeaderDraw<'static>> {
    layout
        .headers()
        .into_iter()
        .map(|h| GroupHeaderDraw {
            y: h.rect.y - scroll + header_h,
            h: h.rect.h,
            text: bucket_caption(h.bucket),
        })
        .collect()
}

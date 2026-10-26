//! Infotip target / arming / show / hide (comctl32 TME_HOVER + TTDT_RESHOW + auto-pop emulation).

use super::*;

/// Infotip delay: the full hover delay normally; the short comctl32 TTDT_RESHOW delay when a
/// tip was hidden less than one hover delay ago (sweeping across neighbouring items).
pub(super) fn tip_delay_ms(hover_ms: u32, reshow_ms: u32, hidden_ago: Option<Duration>) -> u32 {
    match hidden_ago {
        Some(ago) if ago < Duration::from_millis(hover_ms as u64) => reshow_ms,
        _ => hover_ms,
    }
}

/// What the infotip describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TipTarget {
    Item(ContentId, ItemId),
    Tab(ContentId),
    Up(ContentId),
    /// A panel chrome command (icon only), by index.
    Command(ContentId, usize),
    /// A caption button: its name, from the committed chrome scene.
    Caption(ContentId, pecofence_render::fence_chrome::CaptionButton),
}

/// Pending native hover timer. A queued WM_TIMER can survive KillTimer and arrive
/// after a new target is armed, so the message itself is never proof of expiry.
#[derive(Clone, Copy, Debug)]
pub(super) struct TipWait {
    pub anchor: (i32, i32),
    pub deadline: Instant,
}

impl TipWait {
    pub fn remaining_ms(self, now: Instant) -> Option<u32> {
        tip_timer_remaining_ms(self.deadline, now)
    }
}

pub(super) fn tip_timer_remaining_ms(deadline: Instant, now: Instant) -> Option<u32> {
    let remaining = deadline.saturating_duration_since(now);
    (!remaining.is_zero()).then(|| {
        remaining
            .as_millis()
            .saturating_add(1)
            .min(u32::MAX as u128) as u32
    })
}

/// TME_HOVER rectangle test: has the pointer left the `rect` (SPI_GETMOUSEHOVERWIDTH x
/// SPI_GETMOUSEHOVERHEIGHT, physical px) centred on where the infotip timer was armed? The delay
/// restarts when it has, so a tip appears only once the pointer rests.
pub(super) fn tip_hover_moved(anchor: (i32, i32), pt: (i32, i32), rect: (i32, i32)) -> bool {
    (pt.0 - anchor.0).abs() >= (rect.0 / 2).max(1) || (pt.1 - anchor.1).abs() >= (rect.1 / 2).max(1)
}

impl FenceViewState {
    /// What an infotip at a client-pixel point would describe. `hit` is the hover-filtered
    /// item hit (None over the scrollbar hot zone / title controls), so the tip can never
    /// name an item the hover logic does not consider under the pointer.
    pub(super) fn tip_target_at(
        &self,
        x_px: i32,
        y_px: i32,
        hit: Option<usize>,
    ) -> Option<TipTarget> {
        if self.up_button_at(x_px, y_px) {
            return Some(TipTarget::Up(self.active));
        }
        if let Some(i) = self.command_at(x_px, y_px) {
            return Some(TipTarget::Command(self.active, i));
        }
        if let Some(b) = self.caption_at(x_px, y_px) {
            return Some(TipTarget::Caption(self.active, b));
        }
        if let Some(t) = self.tab_at(x_px, y_px) {
            return Some(TipTarget::Tab(self.title_bar.tabs.get(t)?.id));
        }
        hit.and_then(|i| self.file_view.items.get(i))
            .map(|item| TipTarget::Item(self.active, item.id))
    }

    /// NOTE: `TTM_TRACKACTIVATE` makes comctl32 send `WM_NOTIFY` (TTN_SHOW / TTN_POP) back to
    /// this window synchronously while the view is borrowed. That is only safe because the
    /// wndproc has no `WM_NOTIFY` arm (it falls through to DefWindowProc); do not add one that
    /// borrows `view`.
    pub(super) fn hide_tip(&mut self) {
        window::kill_timer(self.hwnd, TIMER_TIP);
        window::kill_timer(self.hwnd, TIMER_TIP_HIDE);
        if self.tip_expires.take().is_some() {
            if let Some(t) = self.tip.as_mut() {
                t.hide();
            }
            self.tip_hidden_at = Some(Instant::now());
        }
        self.tip_target = None;
        self.tip_wait = None;
    }

    /// Auto-pop (TIMER_TIP_HIDE): the tip goes away but the target stays, so it does not come
    /// back until the pointer moves onto something else (comctl32 after TTDT_AUTOPOP).
    pub(super) fn autopop_tip(&mut self) {
        if self.tip_expires.take().is_some() {
            if let Some(t) = self.tip.as_mut() {
                t.hide();
            }
            self.tip_hidden_at = Some(Instant::now());
        }
    }

    /// Pointer moved to client px (`x`, `y`): re-arm the infotip when it settles on something
    /// else, and restart the delay while it keeps moving over the same thing (TME_HOVER: the
    /// tip appears only after the pointer has rested inside the SM_C[XY]MOUSEHOVER box for
    /// SPI_GETMOUSEHOVERTIME). Moving straight from one tipped thing to the next re-shows
    /// after the short TTDT_RESHOW delay. A tip already shown / auto-popped never re-arms.
    pub(super) fn update_tip_target(&mut self, target: Option<TipTarget>, x: i32, y: i32) {
        if target == self.tip_target {
            if let Some(wait) = self.tip_wait
                && tip_hover_moved(
                    wait.anchor,
                    (x, y),
                    pecofence_platform::sysparams::mouse_hover_rect(),
                )
            {
                window::kill_timer(self.hwnd, TIMER_TIP);
                self.arm_tip_timer(x, y);
            }
            return;
        }
        self.hide_tip();
        self.tip_target = target;
        if target.is_some() {
            self.arm_tip_timer(x, y);
        }
    }

    pub(super) fn arm_tip_timer(&mut self, x: i32, y: i32) {
        let delay = tip_delay_ms(
            Tooltip::hover_delay_ms(),
            Tooltip::reshow_delay_ms(),
            self.tip_hidden_at.map(|t| t.elapsed()),
        );
        window::set_timer(self.hwnd, TIMER_TIP, delay);
        self.tip_wait = Some(TipWait {
            anchor: (x, y),
            deadline: Instant::now() + Duration::from_millis(u64::from(delay)),
        });
    }

    /// Infotip text: items get Explorer's name / 类型 / 大小 / 修改日期, a shrunken tab pill its
    /// full caption, the portal up button "向上".
    pub(super) fn tip_text(&self) -> Option<String> {
        match self.tip_target? {
            TipTarget::Up(content) => {
                (content == self.active).then(|| pecofence_core::i18n::text("向上").to_string())
            }
            TipTarget::Caption(content, button) => {
                if content != self.active {
                    return None;
                }
                self.committed_chrome_scene()?
                    .semantics()
                    .iter()
                    .find(|n| n.id == super::chrome_scene::ChromeTarget::Caption(button))
                    .map(|n| n.name.clone())
            }
            TipTarget::Command(content, i) => {
                if content != self.active {
                    return None;
                }
                self.committed_chrome_scene()?
                    .semantics()
                    .iter()
                    .find(|node| node.id == super::chrome_scene::ChromeTarget::Command(i))
                    .map(|node| node.name.clone())
            }
            TipTarget::Tab(id) => {
                if !self.title_bar.tabs.iter().any(|tab| tab.id == id) {
                    return None;
                }
                self.committed_chrome_scene()?
                    .tab_tip(id)
                    .map(str::to_owned)
            }
            TipTarget::Item(content, id) => {
                if content != self.active {
                    return None;
                }
                let it = self.file_view.items.iter().find(|item| item.id == id)?;
                let mut s = self.committed_file_scene()?.semantic(id)?.name.clone();
                if let Some(t) = fileinfo::type_name(&it.path, it.is_folder) {
                    s.push_str(&pecofence_core::i18n::format(
                        "\n类型: {0}",
                        std::slice::from_ref(&t),
                    ));
                }
                // Namespace items (Recycle Bin, ...) have neither a size nor a date.
                if !pecofence_platform::shell::is_namespace_path(&it.path) {
                    if !it.is_folder {
                        s.push_str(&pecofence_core::i18n::format(
                            "\n大小: {0}",
                            &[fileinfo::format_size_kb(it.size, false).to_string()],
                        ));
                    }
                    s.push_str(&pecofence_core::i18n::format(
                        "\n修改日期: {0}",
                        &[fileinfo::format_local_datetime(it.mtime).to_string()],
                    ));
                }
                Some(s)
            }
        }
    }

    pub(super) fn show_tip(&mut self, anchor: (i32, i32)) {
        // Resolve delayed work against the currently published surfaces, never the
        // index or pointer geometry from when the native timer was armed.
        let pt = window::cursor_pos();
        if !self.input_lifecycle().window()
            || desktop::root_ancestor(desktop::window_from_point(pt.x, pt.y)) != self.hwnd
        {
            self.hide_tip();
            return;
        }
        let client = window::screen_to_client(self.hwnd, pt);
        let hit =
            if !self.input_lifecycle().body() || self.scrollbar_hit(client.x, client.y).is_some() {
                None
            } else {
                self.hit_item(client.x, client.y)
            };
        let target = self.tip_target_at(client.x, client.y, hit);
        if target != self.tip_target {
            self.update_tip_target(target, client.x, client.y);
            return;
        }
        if tip_hover_moved(
            anchor,
            (client.x, client.y),
            pecofence_platform::sysparams::mouse_hover_rect(),
        ) {
            self.arm_tip_timer(client.x, client.y);
            return;
        }
        let Some(text) = self.tip_text() else {
            return;
        };
        if self.tip.is_none() {
            let dark = matches!(self.theme.mode, pecofence_render::ThemeMode::Dark);
            self.tip = Tooltip::create(self.hwnd, dark)
                .map_err(|e| tracing::warn!(error = %e, "tooltip window failed"))
                .ok();
        }
        let scale = self.scale();
        if let Some(t) = self.tip.as_mut() {
            t.show(
                &text,
                pt.x + (12.0 * scale).round() as i32,
                pt.y + (20.0 * scale).round() as i32,
            );
            let duration_ms =
                pecofence_platform::sysparams::message_duration_secs().saturating_mul(1000);
            self.tip_expires = Some(Instant::now() + Duration::from_millis(u64::from(duration_ms)));
            self.tip_wait = None;
            window::set_timer(self.hwnd, TIMER_TIP_HIDE, duration_ms);
        }
    }
}

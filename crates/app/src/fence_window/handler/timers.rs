//! WM_TIMER arms (one per TIMER_* id) and the menu-loop bracket that gates the peek timers.

use crate::commands::FenceCmd;

use super::*;

pub(super) fn on_timer_composition_finish(
    h: &HandlerCtx,
    hwnd: HWND,
    timer: usize,
    _lparam: isize,
) -> Option<isize> {
    let Ok(guard) = h.view.try_borrow() else {
        // An owned popup can re-enter us while the view is borrowed.
        window::set_timer(hwnd, timer, 16);
        return Some(0);
    };
    let Some(v) = guard.as_ref() else {
        return Some(0);
    };
    let (deadline, generation) = if timer == TIMER_VISIBILITY_FINISH {
        (v.animation.vis_deadline, v.animation.vis_gen)
    } else {
        (v.animation.swap_deadline, v.animation.swap_gen)
    };
    let Some(deadline) = deadline else {
        window::kill_timer(hwnd, timer);
        return Some(0);
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    if !remaining.is_zero() {
        // A timer message queued before a reversal must not finish the new transition.
        window::set_timer(hwnd, timer, remaining.as_millis() as u32 + 1);
        return Some(0);
    }
    drop(guard);
    if timer == TIMER_VISIBILITY_FINISH {
        super::misc::on_app_fade_done(h, hwnd, generation, 0)
    } else {
        super::misc::on_app_tab_swap_done(h, hwnd, generation, 0)
    }
}

pub(super) fn on_timer_rename(
    h: &HandlerCtx,
    hwnd: HWND,
    _wparam: usize,
    _lparam: isize,
) -> Option<isize> {
    let HandlerCtx { view, queue, .. } = h;
    window::kill_timer(hwnd, TIMER_RENAME);
    if let Ok(mut guard) = view.try_borrow_mut()
        && let Some(v) = guard.as_mut()
        && let Some(id) = v.file_view.pending_rename.take()
        && let Some(i) = v.file_view.items.iter().position(|it| it.id == id)
        && v.file_view.selected.len() == 1
        && v.file_view.selected.contains(&i)
        && !window::key_down(msg::VK_LBUTTON)
    {
        queue.push(Command::Fence(FenceCmd::RenameItem {
            fence: v.active,
            item: id,
        }));
    }
    Some(0)
}

pub(super) fn on_timer_tip(
    h: &HandlerCtx,
    hwnd: HWND,
    _wparam: usize,
    _lparam: isize,
) -> Option<isize> {
    let HandlerCtx { view, .. } = h;
    let Ok(mut guard) = view.try_borrow_mut() else {
        // An owned native popup may re-enter while the view is borrowed. Retry
        // without discarding the pending deadline or showing from stale state.
        window::set_timer(hwnd, TIMER_TIP, 16);
        return Some(0);
    };
    window::kill_timer(hwnd, TIMER_TIP);
    if let Some(v) = guard.as_mut() {
        let Some(wait) = v.tip_wait else {
            // A cancelled timer may already have a message in the queue.
            return Some(0);
        };
        if let Some(remaining) = wait.remaining_ms(Instant::now()) {
            // This may be the old target's queued message. Keep the new rest period.
            window::set_timer(hwnd, TIMER_TIP, remaining);
            return Some(0);
        }
        v.tip_wait = None;
        if v.tip_target.is_some()
            && v.file_view.drag.is_none()
            && v.file_view.marquee.is_none()
            && v.title_bar.tab_drag.is_none()
            && v.file_view.scroll_drag.is_none()
            && !v.file_view.ole_drag
        {
            v.show_tip(wait.anchor);
        }
    }
    Some(0)
}

pub(super) fn on_timer_tip_hide(
    h: &HandlerCtx,
    hwnd: HWND,
    _wparam: usize,
    _lparam: isize,
) -> Option<isize> {
    let HandlerCtx { view, .. } = h;
    // TTM_TRACKACTIVATE sends TTN_POP synchronously. Do not consume the
    // only auto-pop wake while another operation still owns the view.
    let Ok(mut guard) = view.try_borrow_mut() else {
        window::set_timer(hwnd, TIMER_TIP_HIDE, 16);
        return Some(0);
    };
    window::kill_timer(hwnd, TIMER_TIP_HIDE);
    if let Some(v) = guard.as_mut() {
        if let Some(deadline) = v.tip_expires
            && let Some(remaining) =
                super::super::tooltip::tip_timer_remaining_ms(deadline, Instant::now())
        {
            window::set_timer(hwnd, TIMER_TIP_HIDE, remaining);
            return Some(0);
        }
        v.autopop_tip();
    }
    Some(0)
}

pub(super) fn on_timer_scroll_repeat(
    h: &HandlerCtx,
    hwnd: HWND,
    _wparam: usize,
    _lparam: isize,
) -> Option<isize> {
    let HandlerCtx { view, .. } = h;
    let Ok(mut guard) = view.try_borrow_mut() else {
        return Some(0);
    };
    let Some(v) = guard.as_mut() else {
        return Some(0);
    };
    let Some(side) = v.file_view.page_repeat else {
        window::kill_timer(hwnd, TIMER_SCROLL_REPEAT);
        return Some(0);
    };
    // Hit-test against the thumb at the glide's destination, so paging
    // stops as soon as the thumb is going to sit under the pointer (Win32
    // stops when the hit test changes; the pointer sliding off the bar
    // pauses and resumes). An arrow keeps stepping one row while the
    // pointer stays on it. Repeat steps use the 83 ms cap so the offset
    // reaches each page / row before the next 50 ms tick (Win32 pages at
    // once); a 333 ms glide retargeted every tick would trail the thumb by
    // pages and keep moving after the button is released.
    let pt = window::screen_to_client(hwnd, window::cursor_pos());
    let target = v.scroll_target();
    let row_step = v.scroll_extent().0.row_step();
    match v.scrollbar_hit_for(pt.x, pt.y, target) {
        Some((hit, g)) if hit == side => {
            let step = scroll_step_for(hit, g.view_h, row_step);
            let at_end = (step < 0.0 && target <= 0.0) || (step > 0.0 && target >= g.max_scroll);
            if at_end {
                window::kill_timer(hwnd, TIMER_SCROLL_REPEAT);
            } else {
                v.scroll_to(target + step, motion::FASTER);
                if v.file_view.scroll_anim.is_none() {
                    let _ = v.redraw_content();
                }
                window::set_timer(hwnd, TIMER_SCROLL_REPEAT, SCROLL_PAGE_REPEAT_MS);
            }
        }
        Some(_) => window::kill_timer(hwnd, TIMER_SCROLL_REPEAT),
        None => window::set_timer(hwnd, TIMER_SCROLL_REPEAT, SCROLL_PAGE_REPEAT_MS),
    }
    Some(0)
}

pub(super) fn on_timer_scrollbar_expand(
    h: &HandlerCtx,
    hwnd: HWND,
    _wparam: usize,
    _lparam: isize,
) -> Option<isize> {
    let HandlerCtx { view, .. } = h;
    window::kill_timer(hwnd, TIMER_SCROLLBAR_EXPAND);
    if let Ok(mut guard) = view.try_borrow_mut()
        && let Some(v) = guard.as_mut()
        && v.file_view.scrollbar_hot
        && v.scrollbar_geometry().is_some()
    {
        v.scrollbar_width_to(ScrollbarDraw::EXPANDED_WIDTH);
    }
    Some(0)
}

pub(super) fn on_timer_scrollbar_contract(
    h: &HandlerCtx,
    hwnd: HWND,
    _wparam: usize,
    _lparam: isize,
) -> Option<isize> {
    let HandlerCtx { view, .. } = h;
    window::kill_timer(hwnd, TIMER_SCROLLBAR_CONTRACT);
    if let Ok(mut guard) = view.try_borrow_mut()
        && let Some(v) = guard.as_mut()
        && !v.file_view.scrollbar_hot
        && !v.scrollbar_pressed()
    {
        v.scrollbar_width_to(ScrollbarDraw::REST_WIDTH);
    }
    Some(0)
}

pub(super) fn on_timer_peek_open(
    h: &HandlerCtx,
    hwnd: HWND,
    _wparam: usize,
    _lparam: isize,
) -> Option<isize> {
    let HandlerCtx {
        view,
        peek_armed,
        in_size_move,
        ..
    } = h;
    window::kill_timer(hwnd, TIMER_PEEK_OPEN);
    peek_armed.set(false);
    if in_size_move.get() || window::key_down(msg::VK_LBUTTON) {
        // Modal move/size loop (or a press about to start one) owns the
        // window rect: do not fight it with a peek animation.
        return Some(0);
    }
    {
        let mut guard = view.borrow_mut();
        if let Some(v) = guard.as_mut()
            && !v.animation.retired
            && v.animation.rolled_up
            && !v.animation.peeking
            && v.animation.roll_anim.is_none()
            && v.title_bar.title_hover
        {
            v.animation.peeking = true;
            if v.animation.expanded_h_px <= v.title_h_px() {
                v.animation.expanded_h_px = v.title_h_px() + (200.0 * v.scale()) as i32;
            }
            v.start_roll_anim(false);
        }
    }
    Some(0)
}

// A drag rested on a rolled fence for the hover delay: unroll it (peek) so
// the drop can target its folders / insertion slots. No button / title
// hover guards here — OLE owns the pointer, which is exactly why
// TIMER_PEEK_OPEN cannot be reused.
pub(super) fn on_timer_drag_peek(
    h: &HandlerCtx,
    hwnd: HWND,
    _wparam: usize,
    _lparam: isize,
) -> Option<isize> {
    let HandlerCtx {
        view, in_size_move, ..
    } = h;
    window::kill_timer(hwnd, TIMER_DRAG_PEEK);
    if let Ok(mut guard) = view.try_borrow_mut()
        && let Some(v) = guard.as_mut()
    {
        v.animation.drag_peek_armed = false;
        if !v.animation.retired
            && v.animation.rolled_up
            && !v.animation.peeking
            && v.animation.roll_anim.is_none()
            && (v.file_view.drop_hover || v.title_bar.drop_tab.is_some())
            && !in_size_move.get()
        {
            v.animation.peeking = true;
            if v.animation.expanded_h_px <= v.title_h_px() {
                v.animation.expanded_h_px = v.title_h_px() + (200.0 * v.scale()) as i32;
            }
            v.start_roll_anim(false);
        }
    }
    Some(0)
}

pub(super) fn on_timer_scrollbar(
    h: &HandlerCtx,
    hwnd: HWND,
    _wparam: usize,
    _lparam: isize,
) -> Option<isize> {
    let HandlerCtx { view, .. } = h;
    window::kill_timer(hwnd, TIMER_SCROLLBAR);
    if let Ok(mut guard) = view.try_borrow_mut()
        && let Some(v) = guard.as_mut()
    {
        // Linger over: fade the indicator out (83 ms) unless something
        // still wants it shown.
        v.sync_scrollbar_alpha();
    }
    Some(0)
}

pub(super) fn on_timer_shadow(
    h: &HandlerCtx,
    hwnd: HWND,
    _wparam: usize,
    _lparam: isize,
) -> Option<isize> {
    let HandlerCtx { view, .. } = h;
    window::kill_timer(hwnd, TIMER_SHADOW);
    if let Ok(mut guard) = view.try_borrow_mut()
        && let Some(v) = guard.as_mut()
    {
        v.update_shadow_now();
    }
    Some(0)
}

pub(super) fn on_entermenuloop(
    h: &HandlerCtx,
    _hwnd: HWND,
    _wparam: usize,
    _lparam: isize,
) -> Option<isize> {
    let HandlerCtx { menu_open, .. } = h;
    menu_open.set(true);
    None
}

pub(super) fn on_exitmenuloop(
    h: &HandlerCtx,
    hwnd: HWND,
    _wparam: usize,
    _lparam: isize,
) -> Option<isize> {
    let HandlerCtx {
        view, menu_open, ..
    } = h;
    menu_open.set(false);
    // The popup was part of the peeked fence: its close countdown starts
    // now, not while the menu was up (WM_MOUSEMOVE inside still cancels).
    let peeking = view
        .try_borrow()
        .ok()
        .is_some_and(|g| g.as_ref().is_some_and(|v| v.animation.peeking));
    if peeking {
        window::kill_timer(hwnd, TIMER_PEEK_CLOSE);
        window::set_timer(hwnd, TIMER_PEEK_CLOSE, PEEK_CLOSE_MS);
    }
    None
}

pub(super) fn on_timer_peek_close(
    h: &HandlerCtx,
    hwnd: HWND,
    _wparam: usize,
    _lparam: isize,
) -> Option<isize> {
    let HandlerCtx {
        view,
        menu_open,
        in_size_move,
        ..
    } = h;
    window::kill_timer(hwnd, TIMER_PEEK_CLOSE);
    if in_size_move.get() || window::key_down(msg::VK_LBUTTON) {
        // Never roll up under the user's hands (resize/drag in progress).
        window::set_timer(hwnd, TIMER_PEEK_CLOSE, PEEK_CLOSE_MS);
        return Some(0);
    }
    // A spring-loaded container never closes under its own popup: the
    // context menu and the rename edit count as part of the fence
    // (WinUI keeps the expanded state while a flyout is open).
    let popup = menu_open.get()
        || view.try_borrow().ok().is_some_and(|g| {
            g.as_ref()
                .is_some_and(|v| v.file_view.renaming.is_some() || v.title_bar.title_renaming)
        });
    let pt = window::cursor_pos();
    let r = window::window_rect(hwnd);
    let inside = pt.x >= r.left && pt.x < r.right && pt.y >= r.top && pt.y < r.bottom;
    if inside || popup {
        window::set_timer(hwnd, TIMER_PEEK_CLOSE, PEEK_CLOSE_MS);
        return Some(0);
    }
    {
        let mut guard = view.borrow_mut();
        if let Some(v) = guard.as_mut()
            && !v.animation.retired
            && v.animation.peeking
            && v.animation.roll_anim.is_none()
            && v.file_view.drag.is_none()
            && !v.file_view.ole_drag
            && v.file_view.marquee.is_none()
        {
            v.animation.peeking = false;
            v.start_roll_anim(true);
        }
    }
    Some(0)
}

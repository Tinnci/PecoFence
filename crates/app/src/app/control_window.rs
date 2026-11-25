//! Control window message handler: Win32 timers, tray callbacks, hotkeys, and command pump.

use super::*;

pub(super) fn build_control_handler(
    cell: AppCell,
    queue: CommandQueue,
    exit_ui: impl Fn() -> Result<()> + 'static,
) -> MessageHandler {
    let update_quit_queue = queue.clone();
    let shell_command_queue = queue.clone();
    let timed_commands = queue;
    // Folder-watcher debounce state (see WM_APP_FS_CHANGED / TIMER_FS).
    let fs_armed = Cell::new(false);
    let fs_flushed: Cell<Option<Instant>> = Cell::new(None);
    let peek_focus_pending: Cell<Option<usize>> = Cell::new(None);
    // This lives outside App so nested/modal loops cannot lose a wallpaper signal.
    let wallpaper_schedule = Rc::new(Cell::new(RefreshSchedule::default()));
    let last_desktop_id = Cell::new(wallpaper::desktop_id());
    let notify_wallpaper = {
        let wallpaper_schedule = wallpaper_schedule.clone();
        move |hwnd| {
            let mut schedule = wallpaper_schedule.get();
            if let Some(delay) = schedule.notify(Instant::now()) {
                window::set_timer(hwnd, TIMER_WALLPAPER, delay);
            }
            wallpaper_schedule.set(schedule);
        }
    };
    Box::new(
        move |hwnd: HWND, message: u32, wparam: usize, lparam: isize| -> Option<isize> {
            match message {
                WM_APP_UPDATE_QUIT => {
                    update_quit_queue.push(Command::Desktop(DesktopCmd::Quit));
                    Some(0)
                }
                pecofence_shell_contract::WM_APP_SHELL_COMMAND_AT => {
                    if let Some(command) = ShellCommand::from_wire(wparam, lparam) {
                        shell_command_queue
                            .push(Command::Desktop(DesktopCmd::ShellAction(command)));
                        Some(1)
                    } else {
                        Some(0)
                    }
                }
                pecofence_shell_contract::WM_APP_SHELL_COMMAND => {
                    if let Some(action) = u32::try_from(wparam).ok().and_then(ShellAction::from_id)
                    {
                        let point = window::cursor_pos();
                        shell_command_queue.push(Command::Desktop(DesktopCmd::ShellAction(
                            ShellCommand::new(action, point.x, point.y),
                        )));
                        Some(1)
                    } else {
                        Some(0)
                    }
                }
                WM_APP_COMMAND => {
                    if let Ok(mut guard) = cell.try_borrow_mut() {
                        if let Some(app) = guard.as_mut() {
                            app.process_commands();
                        }
                    } else {
                        // The App is borrowed (modal loop inside a handler: menu, file
                        // dialog): retry shortly. A re-post would spin the CPU for as
                        // long as the modal loop runs.
                        window::set_timer(hwnd, TIMER_CMD_RETRY, 50);
                    }
                    Some(0)
                }
                WM_APP_PLUGIN_EVENT => {
                    if let Ok(mut guard) = cell.try_borrow_mut()
                        && let Some(app) = guard.as_mut()
                    {
                        app.process_panel_events();
                    } else {
                        window::set_timer(hwnd, TIMER_PLUGIN_RETRY, 50);
                    }
                    Some(0)
                }
                WM_APP_PEEK_FOCUSED => {
                    peek_focus_pending.set(Some(wparam));
                    if let Ok(mut guard) = cell.try_borrow_mut()
                        && let Some(app) = guard.as_mut()
                    {
                        app.on_peek_focused(HWND(wparam as *mut core::ffi::c_void));
                        peek_focus_pending.set(None);
                    } else {
                        window::set_timer(hwnd, TIMER_PEEK_FOCUS_RETRY, 50);
                    }
                    Some(0)
                }
                WM_APP_FS_CHANGED | WM_APP_SHELL_CHANGED => {
                    if message == WM_APP_SHELL_CHANGED {
                        // The shell's own notice (Recycle Bin filled or emptied, a
                        // desktop namespace item toggled): release its payload and
                        // treat it like a folder change.
                        ShellChangeWatch::release(wparam, lparam);
                    }
                    // Explorer reacts to a folder change within a frame or two; a file
                    // the shell just moved onto the desktop (drag-out from a portal)
                    // must be filed as fast. Coalesce for one short window, never
                    // postpone while events keep coming, and back off only when a
                    // batch has just run (a big copy in progress).
                    if !fs_armed.replace(true) {
                        let storm = fs_flushed
                            .get()
                            .is_some_and(|t| t.elapsed() < Duration::from_millis(FS_STORM_MS));
                        let delay = if storm { FS_STORM_MS } else { FS_DEBOUNCE_MS };
                        window::set_timer(hwnd, TIMER_FS, delay as u32);
                    }
                    Some(0)
                }
                WM_APP_ICON_READY => {
                    window::set_timer(hwnd, TIMER_ICONS, 40);
                    Some(0)
                }
                msg::WM_HOTKEY if wparam as i32 == ci_entry::HOTKEY_CI => {
                    // Toggle: the hotkey hides the CI/CD fence when it is already in front.
                    if let Ok(mut guard) = cell.try_borrow_mut()
                        && let Some(app) = guard.as_mut()
                    {
                        app.open_ci_watch_now(Default::default(), true);
                    }
                    Some(0)
                }
                msg::WM_HOTKEY if wparam as i32 == HOTKEY_PEEK => {
                    if let Ok(mut guard) = cell.try_borrow_mut()
                        && let Some(app) = guard.as_mut()
                    {
                        app.toggle_peek();
                    }
                    Some(0)
                }
                WM_APP_TRAY => {
                    let ev = tray::decode_tray_message(wparam, lparam);
                    // Right click / Shift+F10 open the menu; a left click or Enter /
                    // Space on the focused icon runs the primary action (Peek).
                    if ev.event == tray::TRAY_EVENT_CONTEXTMENU {
                        if let Ok(mut guard) = cell.try_borrow_mut()
                            && let Some(app) = guard.as_mut()
                        {
                            app.show_tray_menu(ev.x, ev.y);
                        }
                    } else if ev.event == tray::TRAY_EVENT_BALLOON_CLICK
                        && let Ok(mut guard) = cell.try_borrow_mut()
                        && let Some(app) = guard.as_mut()
                    {
                        app.ci_failure_clicked();
                    } else if (ev.event == tray::TRAY_EVENT_SELECT
                        || ev.event == tray::TRAY_EVENT_KEYSELECT)
                        && let Ok(mut guard) = cell.try_borrow_mut()
                        && let Some(app) = guard.as_mut()
                    {
                        app.tray_activate(ev.x, ev.y);
                    }
                    Some(0)
                }
                msg::WM_TIMER => {
                    match wparam {
                        TIMER_EXIT => {
                            window::kill_timer(hwnd, TIMER_EXIT);
                            timed_commands.push(Command::Desktop(DesktopCmd::Quit));
                        }
                        TIMER_REACTOR_EXIT => {
                            window::kill_timer(hwnd, TIMER_REACTOR_EXIT);
                            if let Err(error) = exit_ui() {
                                tracing::error!(%error, "deferred dispatcher exit failed");
                                window::post_quit(0);
                            }
                        }
                        TIMER_EXIT_WATCHDOG => {
                            let committed = cell.try_borrow().ok().map(|guard| {
                                guard
                                    .as_ref()
                                    .is_some_and(|app| app.persistence.exit_is_committed())
                            });
                            match committed {
                                Some(true) => {
                                    window::kill_timer(hwnd, TIMER_EXIT_WATCHDOG);
                                    tracing::warn!(
                                        "exit.native_pump_fallback_after_committed_close"
                                    );
                                    crate::lifecycle::note_exit(
                                        pecofence_core::session::ExitKind::PumpFallback,
                                    );
                                    window::post_quit(0);
                                }
                                Some(false) => window::kill_timer(hwnd, TIMER_EXIT_WATCHDOG),
                                // A nested native dialog still owns AppCell. Let it unwind.
                                None => window::set_timer(hwnd, TIMER_EXIT_WATCHDOG, 50),
                            }
                        }
                        TIMER_PORTALS => {
                            if let Ok(mut guard) = cell.try_borrow_mut()
                                && let Some(app) = guard.as_mut()
                            {
                                app.pump_portal_reads();
                            }
                        }
                        TIMER_PERSISTENCE => {
                            if let Ok(mut guard) = cell.try_borrow_mut()
                                && let Some(app) = guard.as_mut()
                            {
                                app.pump_persistence();
                            }
                        }
                        TIMER_SAVE => {
                            if let Ok(mut guard) = cell.try_borrow_mut()
                                && let Some(app) = guard.as_mut()
                            {
                                window::kill_timer(hwnd, TIMER_SAVE);
                                app.request_save();
                            } else {
                                // Busy (modal loop): retry shortly instead of dropping.
                                window::set_timer(hwnd, TIMER_SAVE, 500);
                            }
                        }
                        TIMER_FS => {
                            if let Ok(mut guard) = cell.try_borrow_mut()
                                && let Some(app) = guard.as_mut()
                            {
                                window::kill_timer(hwnd, TIMER_FS);
                                fs_armed.set(false);
                                app.on_fs_changed();
                                fs_flushed.set(Some(Instant::now()));
                            } else {
                                // Busy (modal loop): retry shortly; stays armed.
                                window::set_timer(hwnd, TIMER_FS, FS_STORM_MS as u32);
                            }
                        }
                        TIMER_HOUSEKEEPING => {
                            if let Ok(mut guard) = cell.try_borrow_mut()
                                && let Some(app) = guard.as_mut()
                            {
                                app.housekeeping();
                            }
                        }
                        TIMER_ICONS => {
                            if let Ok(mut guard) = cell.try_borrow_mut()
                                && let Some(app) = guard.as_mut()
                            {
                                window::kill_timer(hwnd, TIMER_ICONS);
                                app.on_icons_ready();
                            } else {
                                // Native modal dialogs and menus pump messages while
                                // the App is borrowed. Keep the completed icon batch
                                // pending; workers may have sent their final notification.
                                window::set_timer(hwnd, TIMER_ICONS, 100);
                            }
                        }
                        TIMER_WALLPAPER => {
                            window::kill_timer(hwnd, TIMER_WALLPAPER);
                            if let Ok(mut guard) = cell.try_borrow_mut()
                                && let Some(app) = guard.as_mut()
                            {
                                app.check_wallpaper("notification");
                                let mut schedule = wallpaper_schedule.get();
                                if let Some(delay) = schedule.checked(Instant::now()) {
                                    window::set_timer(hwnd, TIMER_WALLPAPER, delay);
                                }
                                wallpaper_schedule.set(schedule);
                            } else {
                                window::set_timer(hwnd, TIMER_WALLPAPER, FIRST_CHECK_MS);
                            }
                        }
                        TIMER_WALLPAPER_POLL => {
                            if let Ok(mut guard) = cell.try_borrow_mut()
                                && let Some(app) = guard.as_mut()
                            {
                                app.check_wallpaper("poll");
                            }
                        }
                        TIMER_DESKTOP_ID => {
                            if let Some(id) = wallpaper::desktop_id()
                                && last_desktop_id.replace(Some(id)) != Some(id)
                            {
                                tracing::debug!("desktop identity changed; checking wallpaper");
                                notify_wallpaper(hwnd);
                            }
                        }
                        TIMER_CMD_RETRY => {
                            window::kill_timer(hwnd, TIMER_CMD_RETRY);
                            window::post_message(hwnd, WM_APP_COMMAND, 0, 0);
                        }
                        TIMER_PLUGIN_RETRY => {
                            window::kill_timer(hwnd, TIMER_PLUGIN_RETRY);
                            window::post_message(hwnd, WM_APP_PLUGIN_EVENT, 0, 0);
                        }
                        TIMER_PANEL_WAKE => {
                            if let Ok(mut guard) = cell.try_borrow_mut()
                                && let Some(app) = guard.as_mut()
                            {
                                app.process_panel_events();
                            } else {
                                window::set_timer(hwnd, TIMER_PANEL_WAKE, 50);
                            }
                        }
                        TIMER_FRAME_RETRY => {
                            window::kill_timer(hwnd, TIMER_FRAME_RETRY);
                            window::post_message(hwnd, WM_APP_FRAME, 0, 0);
                        }
                        TIMER_PEEK_FOCUS_RETRY => {
                            window::kill_timer(hwnd, TIMER_PEEK_FOCUS_RETRY);
                            if let Some(dimmer) = peek_focus_pending.get() {
                                if let Ok(mut guard) = cell.try_borrow_mut()
                                    && let Some(app) = guard.as_mut()
                                {
                                    app.on_peek_focused(HWND(dimmer as *mut core::ffi::c_void));
                                    peek_focus_pending.set(None);
                                } else {
                                    window::set_timer(hwnd, TIMER_PEEK_FOCUS_RETRY, 50);
                                }
                            }
                        }
                        TIMER_TEST => {
                            if let Ok(mut guard) = cell.try_borrow_mut()
                                && let Some(app) = guard.as_mut()
                            {
                                app.test_step();
                                if app.test.is_none() {
                                    window::kill_timer(hwnd, TIMER_TEST);
                                }
                            }
                        }
                        TIMER_WORKAREA => {
                            window::kill_timer(hwnd, TIMER_WORKAREA);
                            if let Ok(mut guard) = cell.try_borrow_mut()
                                && let Some(app) = guard.as_mut()
                            {
                                tracing::info!("work area changed; re-laying out fences");
                                app.on_display_changed();
                            } else {
                                window::set_timer(hwnd, TIMER_WORKAREA, 500);
                            }
                        }
                        _ => {}
                    }
                    Some(0)
                }
                msg::WM_SETTINGCHANGE if wparam == SPI_SETDESKWALLPAPER => {
                    notify_wallpaper(hwnd);
                    None
                }
                WM_APP_FRAME => {
                    if let Ok(mut guard) = cell.try_borrow_mut()
                        && let Some(app) = guard.as_mut()
                    {
                        app.on_frame();
                    } else {
                        window::set_timer(hwnd, TIMER_FRAME_RETRY, 16);
                    }
                    Some(0)
                }
                WM_APP_WALLPAPER => {
                    // Try promptly, then keep checking while Explorer publishes the
                    // new desktop's image. Repeated notifications never delay a check.
                    tracing::debug!(source = wparam, "wallpaper refresh signal");
                    notify_wallpaper(hwnd);
                    Some(0)
                }
                msg::WM_SETTINGCHANGE if wparam == SPI_SETWORKAREA => {
                    // Taskbar moved/resized/auto-hide toggled. The shell may broadcast
                    // several times while the taskbar animates; coalesce.
                    window::set_timer(hwnd, TIMER_WORKAREA, 250);
                    None
                }
                msg::WM_SETTINGCHANGE
                | msg::WM_THEMECHANGED
                | msg::WM_SYSCOLORCHANGE
                | msg::WM_DWMCOLORIZATIONCOLORCHANGED => {
                    if let Ok(mut guard) = cell.try_borrow_mut()
                        && let Some(app) = guard.as_mut()
                    {
                        app.on_system_settings_changed();
                    }
                    None
                }
                msg::WM_TIMECHANGE => {
                    if let Ok(mut guard) = cell.try_borrow_mut()
                        && let Some(app) = guard.as_mut()
                    {
                        app.panel_manager.borrow_mut().time_changed();
                    } else {
                        window::set_timer(hwnd, TIMER_PANEL_WAKE, 50);
                    }
                    Some(0)
                }
                msg::WM_DISPLAYCHANGE => {
                    if let Ok(mut guard) = cell.try_borrow_mut()
                        && let Some(app) = guard.as_mut()
                    {
                        app.on_display_changed();
                    }
                    Some(0)
                }
                msg::WM_QUERYENDSESSION => {
                    if let Ok(mut guard) = cell.try_borrow_mut()
                        && let Some(app) = guard.as_mut()
                    {
                        // OS termination is best effort: enqueue, never block Windows on I/O.
                        app.request_save();
                        if let Some(a) = app.anchor.borrow_mut().as_mut() {
                            a.restore_desktop_icons();
                        }
                    }
                    Some(1)
                }
                msg::WM_ENDSESSION if wparam != 0 => {
                    // Windows session termination cannot wait for I/O.
                    // Still retire the single WinUI lifetime, not a nested
                    // or unrelated Win32 message loop.
                    if let Err(error) = exit_ui() {
                        tracing::error!(%error, "session-end dispatcher exit failed");
                        window::post_quit(0);
                    }
                    Some(0)
                }
                msg::WM_DESTROY => Some(0),
                _ => None,
            }
        },
    )
}

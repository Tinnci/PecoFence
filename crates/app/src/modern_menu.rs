//! WinUI MenuFlyout rendering for application-owned menus.
//! The narrow modal bridge retains existing command scopes; callbacks never borrow App.

use pecofence_platform::{tray, window};
use pecofence_render::ThemeMode;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use windows_core::Result;
use windows_reactor::{
    AppContext, AppMenuOptions, Callback, Key, Menu, MenuItem, ScreenPoint, WindowTheme,
};

fn project(
    entries: &[tray::MenuEntry],
    prefix: &str,
    commands: &mut HashMap<Key, u32>,
) -> Vec<MenuItem> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let key = Key::from(format!("{prefix}/{index}"));
            match entry {
                tray::MenuEntry::Item {
                    id,
                    label,
                    checked,
                    disabled,
                } => {
                    commands.insert(key.clone(), *id);
                    let mut item = if *disabled {
                        MenuItem::disabled(key, label.as_str())
                    } else {
                        MenuItem::item(key, label.as_str())
                    };
                    if let Some(checked) = checked {
                        item = item.checked(*checked);
                    }
                    item
                }
                tray::MenuEntry::Separator => MenuItem::separator(key),
                tray::MenuEntry::Submenu { label, entries } => MenuItem::submenu(
                    key,
                    label.as_str(),
                    project(entries, &format!("{prefix}/{index}"), commands),
                ),
            }
        })
        .collect()
}

fn show(
    context: &AppContext,
    entries: &[tray::MenuEntry],
    x: i32,
    y: i32,
    above: bool,
) -> Result<u32> {
    let mut commands = HashMap::new();
    let items = project(entries, "menu", &mut commands);
    let chosen = Rc::new(Cell::new(0));
    let closed = Rc::new(Cell::new(false));
    let selection = Rc::clone(&chosen);
    let completion = Rc::clone(&closed);
    let menu = Menu::new(items, move |key| {
        if let Some(id) = commands.get(&key) {
            selection.set(*id);
        }
    });
    context.show_menu_at_with_options(
        ScreenPoint { x, y },
        menu,
        AppMenuOptions {
            above_anchor: above,
            theme: if tray::menu_dark() {
                WindowTheme::Dark
            } else {
                WindowTheme::Light
            },
            on_closed: Some(Callback::new(move |()| completion.set(true))),
        },
    )?;
    if !window::pump_until(&closed) {
        return Ok(0);
    }
    tracing::debug!(command = chosen.get(), "WinUI application menu completed");
    Ok(chosen.get())
}

pub(crate) fn install(context: &AppContext, mode: ThemeMode) -> tray::MenuRendererGuard {
    tray::set_menu_dark(mode == ThemeMode::Dark);
    let context = context.clone();
    tray::install_menu_renderer(move |entries, x, y, above| {
        let result = show(&context, entries, x, y, above);
        if let Err(error) = &result {
            tracing::warn!(%error, "WinUI application menu failed");
        }
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_projection_keeps_distinct_keys_and_original_command_ids() {
        let entries = [
            tray::MenuEntry::Item {
                id: 7,
                label: "Open\tEnter".into(),
                checked: None,
                disabled: true,
            },
            tray::MenuEntry::Submenu {
                label: "View".into(),
                entries: vec![tray::MenuEntry::Item {
                    id: 7,
                    label: "Checked".into(),
                    checked: Some(false),
                    disabled: false,
                }],
            },
        ];
        let mut commands = HashMap::new();
        let menu = project(&entries, "root", &mut commands);
        assert_eq!(commands.len(), 2);
        assert!(commands.values().all(|id| *id == 7));
        assert!(matches!(
            &menu[0],
            MenuItem::Item {
                enabled: false,
                checked: None,
                ..
            }
        ));
        let MenuItem::Submenu { items, .. } = &menu[1] else {
            panic!("missing submenu")
        };
        assert!(matches!(
            &items[0],
            MenuItem::Item {
                checked: Some(false),
                ..
            }
        ));
    }

    #[test]
    #[ignore = "opens isolated WinUI menus; run scripts/test-modern-menu.ps1 with staged runtime"]
    fn live_menu_selection_submenu_and_cancellation() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let completed = Arc::new(AtomicBool::new(false));
        let completion = completed.clone();
        windows_reactor::App::run_with(move |context| {
            context.proxy().dispatch(move |context| {
                let _renderer = install(context, ThemeMode::Dark);
                for (phase, expected) in [("Dark", 11), ("Light", 22), ("Cancel", 0)] {
                    tray::set_menu_dark(phase == "Dark");
                    let menu = tray::PopupMenu::new();
                    menu.check_item(11, &format!("Peco {phase} toggle"), true, false)
                        .item(99, "Peco disabled", false, true)
                        .separator();
                    let sub = tray::PopupMenu::new();
                    sub.item(22, "Peco nested command\tCtrl+N", false, false);
                    menu.submenu("Peco submenu", sub);
                    eprintln!("Opening Peco {phase}");
                    let actual = menu.show_context(Default::default(), 500, 350);
                    if actual != expected {
                        return Err(windows_core::Error::new(
                            windows_core::HRESULT(0x80004005u32 as i32),
                            format!("{phase} menu returned {actual}; expected {expected}"),
                        ));
                    }
                }
                completion.store(true, Ordering::SeqCst);
                context.exit()
            })?;
            let timeout = context.clone();
            windows_reactor::subscribe_live_interval(
                std::time::Duration::from_secs(60),
                move || {
                    _ = timeout.exit();
                },
            )
        })
        .unwrap();
        assert!(
            completed.load(Ordering::SeqCst),
            "menu tour did not complete"
        );
    }
}

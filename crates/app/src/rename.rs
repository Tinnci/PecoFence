//! Inline fence rename: a small activatable popup hosting an EDIT control placed over the fence
//! title (plan §5.12 activation model **b** — the fence itself stays `WS_EX_NOACTIVATE`).
//!
//! Enter commits, Esc cancels, losing focus commits.

use crate::commands::{Command, CommandQueue};
use pecofence_core::{FenceId, ItemId};
use pecofence_platform::window::{
    self, ClassOptions, MessageHandler, Window, WindowBuilder, WindowClass, style,
};
use pecofence_platform::{HWND, RECT, edit, monitors, msg};
use pecofence_plugin_api::{InstanceKey, TextInputStyle};
use pecofence_render::ThemeMode;
use std::cell::RefCell;

const RENAME_CLASS: &str = "PecoFence.Rename";

/// Windows rejects reserved punctuation and code points 0 through 31, but permits DEL/C1.
pub fn invalid_file_name(name: &str) -> bool {
    let name = name.trim().trim_end_matches(['.', ' ']);
    name.is_empty()
        || name
            .chars()
            .any(|c| c < ' ' || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

thread_local! {
    static ACTIVE: RefCell<Option<RenameSession>> = const { RefCell::new(None) };
    static CLASS: RefCell<Option<WindowClass>> = const { RefCell::new(None) };
}

/// What the popup renames.
#[derive(Clone, Copy, Debug)]
pub enum RenameTarget {
    Fence(FenceId),
    Item(ItemId),
    PanelInput { key: InstanceKey, action: u64 },
}

/// Reuse the native EDIT popup for text requested by a first-party panel.
pub fn begin_panel_input(
    fence_hwnd: HWND,
    key: InstanceKey,
    action: u64,
    initial: String,
    queue: CommandQueue,
    mode: ThemeMode,
    multiline: bool,
    title_height: f32,
    input_style: Option<TextInputStyle>,
) {
    let r = window::window_rect(fence_hwnd);
    let dpi = monitors::dpi_for_window(fence_hwnd).max(96);
    let scale = dpi as f32 / 96.0;
    let inset = (12.0 * scale).round() as i32;
    let top = r.top + (if multiline { 96.0 } else { 48.0 } * scale).round() as i32;
    let initial = if multiline {
        initial
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\n', "\r\n")
    } else {
        initial
    };
    begin_rename_at(
        input_style.map_or_else(
            || RECT {
                left: r.left + inset,
                top,
                right: (r.right - inset).max(r.left + inset + 120),
                bottom: if multiline {
                    (r.bottom - inset).max(top + (100.0 * scale) as i32)
                } else {
                    top + (28.0 * scale).round() as i32
                },
            },
            |style| panel_input_rect(r, scale, title_height, style),
        ),
        dpi,
        RenameTarget::PanelInput { key, action },
        initial,
        None,
        queue,
        mode,
        multiline,
        input_style,
    );
}

struct RenameSession {
    window: Window,
    edit: edit::EditControl,
    target: RenameTarget,
    queue: CommandQueue,
    committed: bool,
}

/// Opens (or re-targets) the rename popup over `fence_hwnd`'s title bar.
pub fn begin_inline_rename(
    fence_hwnd: HWND,
    fence: FenceId,
    current: String,
    queue: CommandQueue,
    mode: ThemeMode,
) {
    let r = window::window_rect(fence_hwnd);
    let dpi = monitors::dpi_for_window(fence_hwnd).max(96);
    let scale = dpi as f32 / 96.0;
    let x = r.left + (8.0 * scale) as i32;
    let y = r.top + (4.0 * scale) as i32;
    let w = ((r.right - r.left) - (52.0 * scale) as i32).max((80.0 * scale) as i32);
    let h = (24.0 * scale) as i32;
    begin_rename_at(
        RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        },
        dpi,
        RenameTarget::Fence(fence),
        current,
        None,
        queue,
        mode,
        false,
        None,
    );
}

/// How much of an item's editing name the rename box selects (UTF-16 units): the stem only, so
/// typing replaces the name and keeps the extension (Explorer since Vista). `None` = everything:
/// folders, hidden extensions (`.lnk`, Explorer's "hide extensions"), leading-dot names.
pub fn rename_select_len(editing_name: &str, path: &str, is_folder: bool) -> Option<usize> {
    if is_folder {
        return None;
    }
    let ext = std::path::Path::new(path).extension()?.to_string_lossy();
    let suffix = format!(".{ext}");
    let lower = editing_name.to_lowercase();
    if !lower.ends_with(&suffix.to_lowercase()) || editing_name.len() <= suffix.len() {
        return None;
    }
    Some(editing_name.encode_utf16().count() - suffix.encode_utf16().count())
}

/// The extension hidden by the shell's display name must survive rename. Item paths are
/// case-folded, so comparing them to the visible name must ignore case too.
pub fn rename_hidden_suffix(display_name: &str, file_name: &str, is_folder: bool) -> String {
    if is_folder {
        return String::new();
    }
    let file_lower = file_name.to_lowercase();
    let display_lower = display_name.to_lowercase();
    file_lower
        .strip_prefix(&display_lower)
        .map(str::to_string)
        .unwrap_or_else(|| {
            std::path::Path::new(file_name)
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default()
        })
}

/// Opens the rename popup over an item's label rectangle (screen pixels). `select_end` limits
/// the initial selection to the first N UTF-16 units (the stem); `None` selects everything.
pub fn begin_item_rename(
    label_rect: RECT,
    dpi: u32,
    item: ItemId,
    current: String,
    select_end: Option<usize>,
    queue: CommandQueue,
    mode: ThemeMode,
) {
    let scale = dpi.max(96) as f32 / 96.0;
    let min_w = (96.0 * scale) as i32;
    let h = (24.0 * scale) as i32;
    let mut r = label_rect;
    if r.right - r.left < min_w {
        let cx = (r.left + r.right) / 2;
        r.left = cx - min_w / 2;
        r.right = cx + min_w / 2;
    }
    r.bottom = r.top + h;
    begin_rename_at(
        r,
        dpi,
        RenameTarget::Item(item),
        current,
        select_end,
        queue,
        mode,
        false,
        None,
    );
}

fn begin_rename_at(
    r: RECT,
    dpi: u32,
    target: RenameTarget,
    current: String,
    select_end: Option<usize>,
    queue: CommandQueue,
    mode: ThemeMode,
    multiline: bool,
    input_style: Option<TextInputStyle>,
) {
    end_active(false);
    CLASS.with(|c| {
        if c.borrow().is_none() {
            *c.borrow_mut() = WindowClass::register(RENAME_CLASS, ClassOptions::default()).ok();
        }
    });
    let (x, y, w, h) = (r.left, r.top, r.right - r.left, r.bottom - r.top);
    let scale = dpi.max(96) as f32 / 96.0;
    let px = |dip: f32| (dip * scale).round() as i32;
    // Field surface: background and an optional 2 DIP accent underline.
    let field = input_style.map(|skin| {
        (
            colorref(skin.background),
            skin.accent.map(|accent| (colorref(accent), px(2.0).max(1))),
        )
    });

    let handler: MessageHandler = Box::new(
        move |_hwnd: HWND, message: u32, wparam: usize, _lparam: isize| -> Option<isize> {
            match message {
                msg::WM_COMMAND if msg::hi_u16(wparam) == edit::EN_KILLFOCUS => {
                    end_active(true);
                    Some(0)
                }
                msg::WM_COMMAND if msg::hi_u16(wparam) == edit::EN_CHANGE_MSG => {
                    ACTIVE.with(|a| {
                        if let Some(s) = a.borrow().as_ref() {
                            s.edit.refresh_scrollbar();
                        }
                    });
                    Some(0)
                }
                edit::WM_CTLCOLOREDIT_MSG => {
                    ACTIVE.with(|a| a.borrow().as_ref().map(|s| s.edit.ctl_color(wparam)))
                }
                // Styled panel fields paint their own surface and focus underline
                // around the padded text control.
                msg::WM_ERASEBKGND if field.is_some() => {
                    if let Some((background, accent)) = field {
                        edit::paint_field(_hwnd, wparam, background, accent);
                    }
                    Some(1)
                }
                msg::WM_DESTROY => Some(0),
                _ => None,
            }
        },
    );
    let created = CLASS.with(|c| {
        let c = c.borrow();
        let class = c.as_ref()?;
        WindowBuilder::new(class)
            .title("rename")
            .style(style::POPUP)
            .ex_style(style::EX_TOOLWINDOW)
            .bounds(x, y, w, h)
            .create(handler)
            .ok()
    });
    let Some(window) = created else { return };
    let key_handler = if multiline { on_multiline_key } else { on_key };
    let ctl = if let Some(skin) = input_style {
        let [left, top, right, bottom] = skin.padding.map(|p| px(p.max(0.0)));
        let inner_w = (w - left - right).max(1);
        let inner_h = (h - top - bottom).max(1);
        let created = edit::EditControl::create_styled(
            window.hwnd(),
            inner_w,
            inner_h,
            dpi,
            &current,
            key_handler,
            multiline,
            edit::EditAppearance {
                foreground: colorref(skin.foreground),
                background: colorref(skin.background),
                font_size: skin.font_size,
                face: editor_face(skin.font),
            },
        );
        if let Ok(ctl) = &created {
            edit::round_corners(window.hwnd(), w, h, px(skin.radius));
            if multiline {
                ctl.place(left, top, inner_w, inner_h);
            } else {
                // One line, centred in the field like the row title it replaces.
                let line = (ctl.line_height_px() + px(2.0)).min(inner_h);
                ctl.place(left, top + (inner_h - line) / 2, inner_w, line);
                if let Some(text) = skin.placeholder {
                    ctl.set_placeholder(text);
                }
            }
        }
        created
    } else {
        let create = if multiline {
            edit::EditControl::create_multiline
        } else {
            edit::EditControl::create
        };
        create(
            window.hwnd(),
            w,
            h,
            dpi,
            &current,
            mode == ThemeMode::Dark,
            key_handler,
        )
    };
    let Ok(ctl) = ctl else {
        return;
    };
    ctl.refresh_scrollbar();
    let hwnd = window.hwnd();
    let edit_hwnd = ctl.hwnd();
    // Publish before showing/focusing: the first WM_CTLCOLOREDIT must already
    // find the provider's brush. No RefCell borrow is held across native calls.
    ACTIVE.with(|a| {
        *a.borrow_mut() = Some(RenameSession {
            window,
            edit: ctl,
            target,
            queue,
            committed: false,
        });
    });
    window::show_normal(hwnd);
    window::bring_to_front(hwnd);
    let (start, end) = if multiline {
        let end = current.encode_utf16().count() as i32;
        (end, end)
    } else {
        (0, select_end.map_or(-1, |n| n as i32))
    };
    window::set_focus(edit_hwnd);
    window::send_message(edit_hwnd, edit::EM_SETSEL_MSG, start as usize, end as isize);
}

/// GDI face for a styled field: the same bundled Noto face the widget renders with
/// (registered privately for this process), else the system faces the widgets fall
/// back to; code uses a monospace face that ships with every Windows.
fn editor_face(font: pecofence_plugin_api::TextInputFont) -> &'static str {
    use pecofence_plugin_api::TextInputFont;
    let bundled = pecofence_platform::fonts::register_bundled_for_gdi();
    match font {
        TextInputFont::Body if bundled.sans => pecofence_plugin_api::design::FONT_PAPER_BODY,
        TextInputFont::Body => "Segoe UI Variable Text",
        TextInputFont::Serif if bundled.serif => pecofence_plugin_api::design::FONT_PAPER_SERIF,
        TextInputFont::Serif => "Sitka Text",
        TextInputFont::Mono => "Consolas",
    }
}

/// Style coordinates are relative to the panel body, below any real title bar.
fn panel_input_rect(r: RECT, scale: f32, title_height: f32, style: TextInputStyle) -> RECT {
    let w = (r.right - r.left).max(1);
    let h = (r.bottom - r.top).max(1);
    let x = (style.bounds.x * scale).round() as i32;
    let y = ((title_height + style.bounds.y) * scale).round() as i32;
    let x = x.clamp(0, w - 1);
    let y = y.clamp(0, h - 1);
    RECT {
        left: r.left + x,
        top: r.top + y,
        right: r.left + (x + (style.bounds.w * scale).round() as i32).clamp(x + 1, w),
        bottom: r.top + (y + (style.bounds.h * scale).round() as i32).clamp(y + 1, h),
    }
}

fn colorref(rgba: [f32; 4]) -> u32 {
    let [r, g, b] = [rgba[0], rgba[1], rgba[2]].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u32);
    r | (g << 8) | (b << 16)
}

fn on_multiline_key(vk: u32) -> bool {
    if vk == msg::VK_RETURN && !window::key_down(msg::VK_CONTROL) {
        return false;
    }
    on_key(vk)
}

fn on_key(vk: u32) -> bool {
    match vk {
        k if k == msg::VK_RETURN => {
            end_active(true);
            true
        }
        k if k == msg::VK_ESCAPE => {
            end_active(false);
            true
        }
        _ => false,
    }
}

/// What the open rename popup targets, if one is open.
pub fn active_target() -> Option<RenameTarget> {
    ACTIVE.with(|a| a.borrow().as_ref().map(|s| s.target))
}

/// Finishes the active session (if any). `commit` pushes the new title.
pub fn end_active(commit: bool) {
    let session = ACTIVE.with(|a| a.borrow_mut().take());
    if let Some(mut s) = session {
        if commit && !s.committed {
            s.committed = true;
            let text = s.edit.text();
            match s.target {
                RenameTarget::Fence(fence) => {
                    s.queue.push(Command::RenameFence { fence, title: text })
                }
                RenameTarget::Item(item) => {
                    s.queue.push(Command::RenameItemCommit { item, name: text })
                }
                RenameTarget::PanelInput { key, action } => {
                    s.queue.push(Command::PanelTextCommit { key, action, text })
                }
            }
        } else {
            s.queue.push(Command::RedrawAll);
        }
        drop(s.edit);
        drop(s.window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_editor_follows_body_origin_and_stays_inside_the_window() {
        for scale in [1.0, 1.5, 2.0] {
            for title in [0.0, 36.0] {
                let r = RECT {
                    left: -800,
                    top: 100,
                    right: -800 + (420.0 * scale) as i32,
                    bottom: 100 + (500.0 * scale) as i32,
                };
                let style = TextInputStyle::new(
                    pecofence_plugin_api::RectDip {
                        x: 26.0,
                        y: 78.0,
                        w: 368.0,
                        h: 300.0,
                    },
                    [0.0; 4],
                    [1.0; 4],
                    14.0,
                );
                let actual = panel_input_rect(r, scale, title, style);
                assert_eq!(actual.left, r.left + (26.0 * scale).round() as i32);
                assert_eq!(actual.top, r.top + ((title + 78.0) * scale).round() as i32);
                assert!(actual.right <= r.right && actual.bottom <= r.bottom);
                let huge = TextInputStyle {
                    bounds: pecofence_plugin_api::RectDip {
                        x: -99.0,
                        y: -99.0,
                        w: 9999.0,
                        h: 9999.0,
                    },
                    ..style
                };
                assert_eq!(panel_input_rect(r, scale, title, huge), r);
            }
        }
        assert_eq!(
            colorref([
                0xEC as f32 / 255.0,
                0xE4 as f32 / 255.0,
                0xD6 as f32 / 255.0,
                1.0
            ]),
            0x00D6_E4EC
        );
    }

    #[test]
    fn rename_rejects_windows_control_characters_without_rejecting_c1() {
        for value in ["", ". ", "bad:name", "note.txt\0suffix", "a\tb"] {
            assert!(invalid_file_name(value));
        }
        for value in ["报告.txt", "report.txt", "a\u{7f}b", "a\u{85}b"] {
            assert!(!invalid_file_name(value));
        }
    }

    #[test]
    fn rename_keeps_only_hidden_extensions_despite_case_folded_paths() {
        assert_eq!(rename_hidden_suffix("Report.TXT", "report.txt", false), "");
        assert_eq!(rename_hidden_suffix("Report", "report.txt", false), ".txt");
        assert_eq!(
            rename_hidden_suffix("Shortcut", "shortcut.lnk", false),
            ".lnk"
        );
        assert_eq!(rename_hidden_suffix("Docs.Backup", "docs.backup", true), "");
        assert_eq!(rename_hidden_suffix("报告.PDF", "报告.pdf", false), "");
    }

    #[test]
    fn rename_selects_the_stem_only_when_the_extension_is_shown() {
        assert_eq!(
            rename_select_len("report.pdf", r"C:\d\report.pdf", false),
            Some(6)
        );
        assert_eq!(
            rename_select_len("archive.tar.gz", r"C:\d\archive.tar.gz", false),
            Some(11)
        );
        // Explorer hides `.lnk`: the editing name has no extension to keep.
        assert_eq!(
            rename_select_len("Shortcut", r"C:\d\Shortcut.lnk", false),
            None
        );
        // Leading-dot names have no extension (Rust and Explorer agree).
        assert_eq!(rename_select_len(".bashrc", r"C:\d\.bashrc", false), None);
        assert_eq!(rename_select_len("Docs", r"C:\d\Docs", true), None);
        // Non-ASCII stems count UTF-16 units.
        assert_eq!(
            rename_select_len("报告.docx", r"C:\d\报告.docx", false),
            Some(2)
        );
    }
}

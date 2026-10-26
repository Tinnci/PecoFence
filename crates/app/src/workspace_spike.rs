//! Spike: a WinUI 3 workspace window whose middle column is an existing Direct2D
//! board fence, re-parented as a child HWND. Debug builds only, driven by the test
//! script (`workspace-spike`, `spike-*`). It answers one design question: can the
//! SPM workspace host the board this way? See docs/WORKSPACE_HOSTING_SPIKE.md.

use crate::commands::DesktopCmd;

use crate::commands::{Command, CommandQueue};
use pecofence_platform::HWND;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use windows_reactor::{
    AppContext, Border, Button, Component, ComponentContext, ContentDialog, ContentDialogExt, Grid,
    GridLength, LocalSender, StackPanel, TextBlock, TextBox, TitleBar, View, ViewContext,
    WindowConstraints, WindowHandle, WindowTheme, WindowVisuals, keyed,
};

/// Fixed spike layout, in DIPs: title row, navigation column, detail column.
pub const TITLE_H: f64 = 48.0;
pub const NAV_W: f64 = 168.0;
pub const DETAIL_W: f64 = 330.0;

pub struct SpikeShared {
    pub hwnd: Cell<Option<HWND>>,
    sender: RefCell<Option<LocalSender<Message>>>,
    pub typed: RefCell<String>,
}

#[derive(Clone)]
struct Input {
    shared: Rc<SpikeShared>,
    queue: CommandQueue,
    dark: bool,
}

impl PartialEq for Input {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.shared, &other.shared)
    }
}

#[derive(Clone)]
pub enum Message {
    Native(usize),
    Size,
    Typed(Rc<str>),
    Nav(&'static str),
    Dialog(bool),
    DialogClosed,
}

pub fn open(
    context: &AppContext,
    queue: CommandQueue,
    dark: bool,
) -> windows_core::Result<Rc<SpikeShared>> {
    let shared = Rc::new(SpikeShared {
        hwnd: Cell::new(None),
        sender: RefCell::new(None),
        typed: RefCell::new(String::new()),
    });
    context.open_component_window::<Spike>(Input {
        shared: Rc::clone(&shared),
        queue,
        dark,
    })?;
    Ok(shared)
}

impl SpikeShared {
    pub fn show_dialog(&self, open: bool) {
        if let Some(sender) = self.sender.borrow().as_ref() {
            _ = sender.send(Message::Dialog(open));
        }
    }
}

struct Spike {
    input: Input,
    page: &'static str,
    dialog: bool,
}

impl Component for Spike {
    type Input = Input;
    type Message = Message;

    fn create(input: &Input, context: &ComponentContext<Self>) -> Self {
        *input.shared.sender.borrow_mut() = Some(context.sender());
        _ = context
            .run_window(|window: WindowHandle<'_>| Message::Native(window.as_raw() as usize));
        Self {
            input: input.clone(),
            page: "Board",
            dialog: false,
        }
    }

    fn update(&mut self, message: Message, _context: &ComponentContext<Self>) {
        match message {
            Message::Native(raw) => {
                self.input.shared.hwnd.set(Some(HWND(raw as *mut _)));
                self.input
                    .queue
                    .push(Command::Desktop(DesktopCmd::WorkspaceSpikeLayout));
            }
            Message::Size => self
                .input
                .queue
                .push(Command::Desktop(DesktopCmd::WorkspaceSpikeLayout)),
            Message::Typed(text) => *self.input.shared.typed.borrow_mut() = text.to_string(),
            Message::Nav(page) => self.page = page,
            Message::Dialog(open) => self.dialog = open,
            Message::DialogClosed => self.dialog = false,
        }
    }

    fn view(&self, input: &Input, context: &mut ViewContext<Self>) -> View {
        context.window_visuals(
            WindowVisuals::new()
                .client_size(1280.0, 800.0)
                .constraints(WindowConstraints {
                    min_width: Some(720.0),
                    min_height: Some(480.0),
                    max_width: None,
                    max_height: None,
                })
                .theme(if input.dark {
                    WindowTheme::Dark
                } else {
                    WindowTheme::Light
                }),
        );
        context.window_title("SPM Workspace (hosting spike)".to_owned());
        context.on_window_size(context.callback(|_| Message::Size));
        let nav = |label: &'static str| {
            keyed(
                label,
                Button::new()
                    .content(if self.page == label {
                        format!("▸ {label}")
                    } else {
                        label.to_owned()
                    })
                    .on_click(context.message(Message::Nav(label))),
            )
        };
        let mut detail = vec![
            keyed("h", TextBlock::new().text("Detail (WinUI)")),
            keyed(
                "page",
                TextBlock::new().text(format!("Page: {}", self.page)),
            ),
            keyed(
                "commit",
                TextBox::new("")
                    .placeholder_text("Type a commitment")
                    .on_text_changed(context.callback(Message::Typed)),
            ),
            keyed(
                "dialog",
                Button::new()
                    .content("Open dialog")
                    .on_click(context.message(Message::Dialog(true))),
            ),
        ];
        if self.dialog {
            detail.push(keyed(
                "confirm",
                Border::new().content_dialog(
                    ContentDialog::new()
                        .is_open(true)
                        .title("Update Meegle #88213?")
                        .primary_button_text("Send")
                        .close_button_text("Cancel")
                        .content(TextBlock::new().text(
                            "A centred dialog overlaps the board column. Is it drawn above the board?",
                        ))
                        .on_closed(context.callback(|_| Message::DialogClosed)),
                ),
            ));
        }
        Grid::new()
            .rows([GridLength::Pixel(TITLE_H), GridLength::STAR])
            .children((
                TitleBar::new()
                    .title("SPM Workspace (hosting spike)")
                    .is_back_button_visible(false),
                Grid::new()
                    .grid_row(1)
                    .columns([
                        GridLength::Pixel(NAV_W),
                        GridLength::STAR,
                        GridLength::Pixel(DETAIL_W),
                    ])
                    .children((
                        StackPanel::new().spacing(6.0).keyed_children(vec![
                            nav("Board"),
                            nav("Standup"),
                            nav("Sources"),
                        ]),
                        // The board child HWND covers this column.
                        Border::new().grid_column(1),
                        StackPanel::new()
                            .grid_column(2)
                            .spacing(8.0)
                            .keyed_children(detail),
                    )),
            ))
            .into()
    }
}

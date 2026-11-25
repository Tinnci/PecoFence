//! Native WinUI 3 Settings component hosted by the application's Reactor lifetime.
//!
//! The component owns transient editor state only. Application settings, workspace identity,
//! persistence, OS integrations, and use cases remain owned by `App`.

use crate::commands::SettingsCmd;

mod pages;
mod projection;
mod rule_editor;
#[cfg(test)]
mod tests;
mod update;

use projection::{action_confirmation_text, apply_command, confirmation_label, parse_rgb};

use crate::commands::{Command, CommandQueue};
use crate::settings_ui::{ContentOptions, SettingsView};
use pecofence_core::rules::{Rule, RuleSet, Target};
use pecofence_core::settings_protocol::{
    Action, ContainerChange, ContentChange, DocumentStamp, Receipt, Request, RuleChange,
    RuleDraft as RuleCommandDraft, SettingChange, SettingsCommand, VERSION,
};
use pecofence_core::{ContainerId, ContentId, RuleId};
use pecofence_platform::{HWND, tray::OwnedIcon, window};
use pecofence_render::ThemeMode;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use uuid::Uuid;
use windows_core::Result;
use windows_reactor::{
    AppContext, Component, ComponentContext, ContentDialog, ContentDialogExt, ContentDialogResult,
    Grid, GridLength, Icon, LocalSender, NavigationView, NavigationViewBackButtonVisible,
    NavigationViewDisplayMode, NavigationViewItem, NavigationViewPaneDisplayMode, TitleBar, View,
    ViewContext, WindowBackdrop, WindowConstraints, WindowHandle, WindowPlacement, WindowTheme,
    WindowVisuals, keyed,
};

const MAX_REQUESTS: usize = 64;
const MAX_SEQUENCE: u64 = 9_007_199_254_740_991;

fn set_selected<T: PartialEq>(selection: &mut Vec<T>, item: T, checked: bool) {
    if checked && !selection.contains(&item) {
        selection.push(item);
    } else if !checked {
        selection.retain(|value| *value != item);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ContentKey {
    content: ContentId,
    container: ContainerId,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Page {
    #[default]
    General,
    Widgets,
    Content,
    Rules,
    Layout,
    About,
}

impl Page {
    fn all() -> [Self; 6] {
        [
            Self::General,
            Self::Widgets,
            Self::Content,
            Self::Rules,
            Self::Layout,
            Self::About,
        ]
    }

    fn title(self) -> &'static str {
        match self {
            Self::General => "常规",
            Self::Widgets => "小组件外观",
            Self::Content => "内容与窗口",
            Self::Rules => "整理规则",
            Self::Layout => "布局与备份",
            Self::About => "关于与诊断",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Widgets => "widgets",
            Self::Content => "content",
            Self::Rules => "rules",
            Self::Layout => "layout",
            Self::About => "about",
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        Self::all().into_iter().find(|page| page.tag() == tag)
    }

    fn icon(self) -> Icon {
        // Segoe Fluent Icons, supplied by WinUI's native FontIcon source.
        Icon::font(match self {
            Self::General => "\u{E713}",
            Self::Widgets => "\u{E790}",
            Self::Content => "\u{E71D}",
            Self::Rules => "\u{E71C}",
            Self::Layout => "\u{E74E}",
            Self::About => "\u{E946}",
        })
    }
}

struct HostShared {
    alive: Cell<bool>,
    retired: Cell<bool>,
    hwnd: Cell<Option<HWND>>,
    sender: RefCell<Option<LocalSender<Message>>>,
    deferred: RefCell<VecDeque<Message>>,
    #[cfg(test)]
    navigation_mode: Cell<NavigationViewDisplayMode>,
}

struct ComponentInput {
    activation: Uuid,
    shared: Rc<HostShared>,
    mode: ThemeMode,
    liquid_glass: bool,
    queue: CommandQueue,
}

impl Clone for ComponentInput {
    fn clone(&self) -> Self {
        Self {
            activation: self.activation,
            shared: Rc::clone(&self.shared),
            mode: self.mode,
            liquid_glass: self.liquid_glass,
            queue: self.queue.clone(),
        }
    }
}

impl PartialEq for ComponentInput {
    fn eq(&self, other: &Self) -> bool {
        self.activation == other.activation
    }
}

#[derive(Clone)]
enum Message {
    HostUpdate(Uuid, Box<SettingsView>),
    HostDecision(Receipt),
    HostNotice(String, bool),
    HostTheme(ThemeMode, bool),
    LanguageChanged,
    ShowContent(ContentId),
    Summary(DocumentStamp, usize),
    SetIcons((i32, Vec<u8>), (i32, Vec<u8>)),
    NativeHandle(usize),
    WindowPlacement(WindowPlacement),
    Activate,
    Close,
    #[cfg(test)]
    Navigate(Page),
    SelectPage(Option<Rc<str>>),
    TogglePane,
    PaneOpenChanged(bool),
    NavigationModeChanged(NavigationViewDisplayMode),
    Setting(SettingChange),
    IconTint(Option<[u8; 3]>),
    SelectContent(Option<usize>),
    ContentText(String),
    ApplyTitle,
    RetryTitle,
    DiscardTitle,
    Content(ContentKey, ContentChange),
    Container(ContentKey, ContainerChange),
    Opacity(ContentKey, Option<f64>),
    ContainerToggle(ContentKey, ContainerField, bool),
    TitleColorText(ContentKey, String),
    ApplyTitleColor(ContentKey),
    RetryTitleColor(ContentKey),
    DiscardTitleColor(ContentKey),
    KeepUpdated(bool),
    DefaultTarget(Option<usize>),
    RuleEnabled(RuleId, bool),
    RuleMove(RuleId, Option<RuleId>),
    RuleDelete(RuleId),
    RuleEdit(RuleId),
    RuleNew,
    RuleName(String),
    RuleTarget(Option<usize>),
    RuleEnabledDraft(bool),
    ConditionKind(usize),
    ConditionText(String),
    ConditionOperation(usize),
    ConditionType(pecofence_core::rules::TypeCategory, bool),
    ConditionWeekday(u8, bool),
    ConditionOrigin(usize),
    ConditionMinimum(String),
    ConditionMaximum(String),
    ConditionFrom(String),
    ConditionTo(String),
    ConditionIdle(String),
    AddCondition,
    EditCondition(Uuid),
    RemoveCondition(Uuid),
    SaveRule,
    CancelRule,
    RetryRule,
    DiscardRule,
    SnapshotName(String),
    MonitorFirst(Option<usize>),
    MonitorSecond(Option<usize>),
    AskAction(Action),
    DialogClosed(Uuid, ContentDialogResult),
    #[cfg(test)]
    CancelAction,
    RunAction(Action),
}

#[derive(Clone, Copy)]
enum ContainerField {
    AutoHeight,
    Locked,
    ExcludeFromQuickHide,
    PortalNavigate,
    PortalTitleIcon,
}

/// Handle for one Settings activation. The HWND stays unavailable until Reactor reports the
/// component's native window through `ComponentContext::run_window`.
pub struct SettingsHost {
    source: Uuid,
    shared: Rc<HostShared>,
}

impl SettingsHost {
    pub fn open(
        context: &AppContext,
        mode: ThemeMode,
        liquid_glass: bool,
        queue: CommandQueue,
    ) -> Result<Self> {
        let source = Uuid::new_v4();
        let shared = Rc::new(HostShared {
            alive: Cell::new(true),
            retired: Cell::new(false),
            hwnd: Cell::new(None),
            sender: RefCell::new(None),
            deferred: RefCell::new(VecDeque::new()),
            #[cfg(test)]
            navigation_mode: Cell::new(NavigationViewDisplayMode::Expanded),
        });
        context.open_component_window::<SettingsComponent>(ComponentInput {
            activation: source,
            shared: Rc::clone(&shared),
            mode,
            liquid_glass,
            queue,
        })?;
        Ok(Self { source, shared })
    }

    pub fn source(&self) -> Uuid {
        self.source
    }

    pub fn hwnd(&self) -> Option<HWND> {
        self.shared.hwnd.get()
    }

    pub fn is_alive(&self) -> bool {
        self.shared.alive.get() && !self.shared.retired.get()
    }

    pub fn activate(&self) {
        self.send(Message::Activate);
    }

    pub fn set_icons(&mut self, small: (i32, Vec<u8>), big: (i32, Vec<u8>)) {
        self.send(Message::SetIcons(small, big));
    }

    pub fn set_theme(&self, mode: ThemeMode, liquid_glass: bool) {
        self.send(Message::HostTheme(mode, liquid_glass));
    }

    pub fn update_language(&self) {
        self.send(Message::LanguageChanged);
    }

    pub fn update(&self, client: Uuid, view: SettingsView) {
        self.send(Message::HostUpdate(client, Box::new(view)));
    }

    pub fn decision(&self, receipt: Receipt) {
        self.send(Message::HostDecision(receipt));
    }

    pub fn notify(&self, text: &str, error: bool) {
        self.send(Message::HostNotice(text.to_owned(), error));
    }

    pub fn show_content(&self, id: ContentId) {
        self.send(Message::ShowContent(id));
    }

    pub fn summary(&self, stamp: DocumentStamp, item_count: usize) {
        self.send(Message::Summary(stamp, item_count));
    }

    fn send(&self, message: Message) {
        if !self.is_alive() {
            return;
        }
        if let Some(sender) = self.shared.sender.borrow().as_ref() {
            _ = sender.send(message);
        } else {
            let mut deferred = self.shared.deferred.borrow_mut();
            if matches!(&message, Message::HostUpdate(_, _)) {
                deferred.retain(|message| !matches!(message, Message::HostUpdate(_, _)));
            }
            if matches!(&message, Message::HostTheme(_, _)) {
                deferred.retain(|message| !matches!(message, Message::HostTheme(_, _)));
            }
            if deferred.len() < MAX_REQUESTS + 8 {
                deferred.push_back(message);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestKind {
    Immediate,
    RuleDraft(Uuid, Uuid),
    TextDraft(TextField, ContentKey, Uuid),
    Action,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextField {
    Title,
    TitleColor,
}

struct Pending {
    request: Request,
    kind: RequestKind,
}

struct Accepted {
    stamp: DocumentStamp,
    command: SettingsCommand,
}

struct TextDraft {
    value: String,
    base: DocumentStamp,
    generation: Uuid,
    needs_review: bool,
    submitted: Option<DocumentStamp>,
}

struct Notice {
    text: String,
    error: bool,
}

struct Confirmation {
    token: Uuid,
    source: Uuid,
    client: Uuid,
    base: DocumentStamp,
    command: SettingsCommand,
}

struct SettingsComponent {
    source: Uuid,
    shared: Rc<HostShared>,
    queue: CommandQueue,
    mode: ThemeMode,
    liquid_glass: bool,
    page: Page,
    pane_open: bool,
    /// None uses adaptive defaults; a user close persists until the hamburger reopens it.
    /// The user's hamburger choice for the inline (expanded) pane.
    pane_override: Option<bool>,
    /// An overlay pane opened by hand in compact or minimal mode. Kept apart from
    /// `pane_override`: WinUI closes an overlay on its own when the mode changes, so an
    /// expanded-mode "open" must not carry into overlay modes (the next click then closed
    /// a pane that was already shut).
    overlay_open: bool,
    navigation_mode: NavigationViewDisplayMode,
    view: Option<SettingsView>,
    client: Option<Uuid>,
    selected_content: Option<ContentKey>,
    pending_content: Option<ContentId>,
    title_drafts: HashMap<ContentKey, TextDraft>,
    title_color_drafts: HashMap<ContentKey, TextDraft>,
    rule_draft: Option<rule_editor::Draft>,
    inflight: Option<Pending>,
    pending: VecDeque<Pending>,
    next_sequence: u64,
    accepted: Vec<Accepted>,
    notice: Option<Notice>,
    confirm_action: Option<Confirmation>,
    snapshot_name: String,
    monitor_first: Option<String>,
    monitor_second: Option<String>,
    item_summary: Option<(DocumentStamp, usize)>,
    icons: Vec<OwnedIcon>,
    pending_icons: Option<((i32, Vec<u8>), (i32, Vec<u8>))>,
    placement: Option<WindowPlacement>,
}

impl Component for SettingsComponent {
    type Input = ComponentInput;
    type Message = Message;

    fn create(input: &ComponentInput, context: &ComponentContext<Self>) -> Self {
        *input.shared.sender.borrow_mut() = Some(context.sender());
        for message in std::mem::take(&mut *input.shared.deferred.borrow_mut()) {
            _ = context.sender().send(message);
        }
        _ = context
            .run_window(|window: WindowHandle<'_>| Message::NativeHandle(window.as_raw() as usize));
        Self::new(input)
    }

    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {
        self.handle_message(message, context);
    }

    fn view(&self, _input: &ComponentInput, context: &mut ViewContext<Self>) -> View {
        context.window_visuals(
            WindowVisuals::new()
                .client_size(1080.0, 780.0)
                .constraints(WindowConstraints {
                    min_width: Some(560.0),
                    min_height: Some(520.0),
                    max_width: None,
                    max_height: None,
                })
                .backdrop(if self.liquid_glass {
                    WindowBackdrop::Acrylic
                } else {
                    WindowBackdrop::Mica
                })
                .theme(match self.mode {
                    ThemeMode::Light => WindowTheme::Light,
                    ThemeMode::Dark => WindowTheme::Dark,
                }),
        );
        context.on_window_placement(context.callback(Message::WindowPlacement));
        let content = self.render(context);
        let title = tr("PecoFence 设置");
        context.window_title(title.clone());
        Grid::new()
            .rows([GridLength::Auto, GridLength::STAR])
            .children((
                TitleBar::new()
                    .title(title)
                    .icon(Page::General.icon())
                    .is_back_button_visible(false)
                    .is_pane_toggle_button_visible(true)
                    // A capture-free mapping keeps the native listener identity stable so
                    // background renders cannot retire a queued hamburger click.
                    .on_pane_toggle_requested(context.callback(|()| Message::TogglePane)),
                windows_reactor::Border::new().grid_row(1).content(content),
            ))
            .into()
    }
}

impl Drop for SettingsComponent {
    fn drop(&mut self) {
        if self.shared.retired.replace(true) {
            return;
        }
        self.shared.alive.set(false);
        self.shared.hwnd.set(None);
        *self.shared.sender.borrow_mut() = None;
        self.queue
            .push(Command::Settings(SettingsCmd::SettingsClosed {
                source: self.source,
            }));
    }
}

impl SettingsComponent {
    fn new(input: &ComponentInput) -> Self {
        Self {
            source: input.activation,
            shared: Rc::clone(&input.shared),
            queue: input.queue.clone(),
            mode: input.mode,
            liquid_glass: input.liquid_glass,
            page: Page::General,
            pane_open: true,
            pane_override: None,
            overlay_open: false,
            navigation_mode: NavigationViewDisplayMode::Expanded,
            view: None,
            client: None,
            selected_content: None,
            pending_content: None,
            title_drafts: HashMap::new(),
            title_color_drafts: HashMap::new(),
            rule_draft: None,
            inflight: None,
            pending: VecDeque::new(),
            next_sequence: 0,
            accepted: Vec::new(),
            notice: None,
            confirm_action: None,
            snapshot_name: String::new(),
            monitor_first: None,
            monitor_second: None,
            item_summary: None,
            icons: Vec::new(),
            pending_icons: None,
            placement: None,
        }
    }

    fn render(&self, context: &mut ViewContext<Self>) -> View {
        let view = self.projected_view();
        let mut header = vec![
            keyed(
                "heading",
                windows_reactor::TextBlock::new()
                    .text(tr(self.page.title()))
                    .font_size(26.0)
                    .automation_heading_level(windows_reactor::AutomationHeadingLevel::Level1),
            ),
            keyed("status", self.status_view(view.as_ref())),
        ];
        if let Some(notice) = &self.notice {
            header.push(keyed("notice", pages::notice(notice.error, &notice.text)));
        }
        let mut body = vec![keyed(
            format!("page-{:?}", self.page),
            pages::page(self, view.as_ref(), context),
        )];
        if let Some(draft) = &self.rule_draft
            && draft.needs_review
        {
            body.push(keyed(
                "rule-review",
                windows_reactor::StackPanel::new().spacing(8.0).children((
                    pages::notice(
                        true,
                        &tr("规则草稿基于较旧的修订。检查草稿后，明确选择重试或放弃。"),
                    ),
                    windows_reactor::StackPanel::new()
                        .orientation(windows_reactor::Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            pages::button(context.message(Message::RetryRule), "检查后重试"),
                            pages::button(context.message(Message::DiscardRule), "放弃规则草稿"),
                        )),
                )),
            ));
        }
        if let Some(confirmation) = &self.confirm_action {
            let token = confirmation.token;
            let action_label = tr(confirmation_label(&confirmation.command))
                .trim_end_matches('…')
                .to_owned();
            let text = match &confirmation.command {
                SettingsCommand::Action { action } => action_confirmation_text(action),
                SettingsCommand::Rule {
                    change: RuleChange::Delete { .. },
                } => tr("删除此规则后无法从设置中恢复。是否继续？"),
                _ => tr("请确认此操作。"),
            };
            // A distinct owner per token prevents a late native completion from
            // invoking the callback of a newer confirmation in the same XAML root.
            body.push(keyed(
                format!("confirmation-{token}"),
                windows_reactor::Border::new().content_dialog(
                    ContentDialog::new()
                        .is_open(true)
                        .title(&action_label)
                        .primary_button_text(&action_label)
                        .close_button_text(tr("取消"))
                        .content(
                            windows_reactor::TextBlock::new()
                                .text(text)
                                .text_wrapping(windows_reactor::TextWrapping::Wrap),
                        )
                        .on_closed(
                            context.callback(move |result| Message::DialogClosed(token, result)),
                        ),
                ),
            ));
        }
        body.push(keyed(
            "close",
            pages::button(context.message(Message::Close), "关闭设置"),
        ));

        let item = |page: Page| {
            keyed(
                page.tag(),
                NavigationViewItem::new()
                    .tag(page.tag())
                    .icon(page.icon())
                    .is_selected(page == self.page)
                    .content(windows_reactor::TextBlock::new().text(tr(page.title()))),
            )
        };
        NavigationView::new()
            .pane_display_mode(NavigationViewPaneDisplayMode::Auto)
            // The only hamburger lives in the native title bar.
            .is_pane_toggle_button_visible(false)
            .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
            .is_settings_visible(false)
            .is_pane_open(self.pane_open)
            .open_pane_length(280.0)
            .pane_title(
                if self.navigation_mode == NavigationViewDisplayMode::Expanded {
                    "PecoFence"
                } else {
                    ""
                },
            )
            .on_is_pane_open_changed(context.callback(Message::PaneOpenChanged))
            .on_display_mode_changed(context.callback(Message::NavigationModeChanged))
            .on_selected_tag_changed(context.callback(Message::SelectPage))
            .keyed_menu_items(Page::all().map(item))
            .content(
                Grid::new()
                    .rows([GridLength::Auto, GridLength::STAR])
                    .children((
                        windows_reactor::Border::new()
                            .padding(windows_reactor::Thickness {
                                left: 24.0,
                                top: 16.0,
                                right: 24.0,
                                bottom: 12.0,
                            })
                            .content(
                                windows_reactor::StackPanel::new()
                                    .spacing(10.0)
                                    .keyed_children(header),
                            ),
                        windows_reactor::ScrollViewer::new().grid_row(1).content(
                            windows_reactor::Border::new()
                                .padding(windows_reactor::Thickness {
                                    left: 24.0,
                                    top: 4.0,
                                    right: 24.0,
                                    bottom: 24.0,
                                })
                                .content(
                                    windows_reactor::StackPanel::new()
                                        .spacing(14.0)
                                        .keyed_children(body),
                                ),
                        ),
                    )),
            )
            .into()
    }

    fn navigate(&mut self, page: Page) {
        self.page = page;
        self.notice = None;
        if self.navigation_mode != NavigationViewDisplayMode::Expanded {
            self.pane_open = false;
            self.overlay_open = false;
        }
    }

    fn projected_view(&self) -> Option<SettingsView> {
        let mut view = self.view.clone()?;
        for accepted in &self.accepted {
            apply_command(&mut view, &accepted.command);
        }
        for pending in self.inflight.iter().chain(self.pending.iter()) {
            if pending.request.base.workspace == view.stamp.workspace {
                apply_command(&mut view, &pending.request.command);
            }
        }
        for (key, draft) in &self.title_drafts {
            if let Some(content) = view.contents.iter_mut().find(|content| {
                content.content_id == key.content && content.container_id == key.container
            }) {
                content.title.clone_from(&draft.value);
            }
        }
        Some(view)
    }

    fn status_view(&self, view: Option<&SettingsView>) -> View {
        let text = self.status_text(view);
        let issue = view.and_then(|view| view.save_issue.as_ref().or(view.load_issue.as_ref()));
        let busy = issue.is_none()
            && view.is_some_and(|view| {
                view.saving || view.closing || self.inflight.is_some() || !self.pending.is_empty()
            });
        let mut rows = Vec::new();
        if busy {
            rows.push(keyed(
                "progress",
                windows_reactor::ProgressBar::new()
                    .is_indeterminate(true)
                    .height(3.0)
                    .automation_name(&text),
            ));
        }
        if self.page == Page::Layout && view.is_some() {
            // The Layout page's storage card shows the state, the issue and its actions.
        } else if let Some(issue) = issue {
            rows.push(keyed("problem", pages::notice(true, issue)));
        } else {
            rows.push(keyed(
                "summary",
                windows_reactor::TextBlock::new()
                    .text(text)
                    .text_wrapping(windows_reactor::TextWrapping::Wrap),
            ));
        }
        windows_reactor::StackPanel::new()
            .spacing(8.0)
            .keyed_children(rows)
            .into()
    }

    fn status_text(&self, view: Option<&SettingsView>) -> String {
        let Some(view) = view else {
            return tr("正在连接设置工作区…");
        };
        if view.closing {
            return tr("正在等待保存后退出应用；可以取消退出等待。");
        }
        if let Some(issue) = &view.save_issue {
            return format!("{} {}", tr("保存遇到问题："), issue);
        }
        if view.saving {
            return tr("正在保存…");
        }
        if self.inflight.is_some() || !self.pending.is_empty() {
            return tr("正在应用更改…");
        }
        if self.accepted.last().is_some_and(|accepted| {
            accepted.stamp.workspace == view.stamp.workspace
                && view
                    .committed_revision
                    .is_none_or(|revision| revision < accepted.stamp.revision)
        }) {
            return tr("更改已应用，正在等待保存。");
        }
        if self.rule_draft.as_ref().is_some_and(|draft| draft.dirty) {
            return tr("规则草稿尚未提交。");
        }
        if self
            .title_drafts
            .values()
            .chain(self.title_color_drafts.values())
            .any(|draft| draft.submitted.is_none())
        {
            return tr("文本草稿尚未提交。");
        }
        if view.dirty {
            return tr("工作区有尚未保存的修改。");
        }
        if view
            .committed_revision
            .is_some_and(|revision| revision >= view.stamp.revision)
        {
            return tr("所有更改已保存。");
        }
        tr("等待保存状态…")
    }

    fn content(&self, key: ContentKey) -> Option<&ContentOptions> {
        self.view
            .as_ref()?
            .contents
            .iter()
            .find(|content| Self::key(content) == key)
    }

    fn key(content: &ContentOptions) -> ContentKey {
        ContentKey {
            content: content.content_id,
            container: content.container_id,
        }
    }

    fn collection_target(&self, index: usize) -> Option<Target> {
        let content = self
            .view
            .as_ref()?
            .contents
            .iter()
            .filter(|content| content.is_collection)
            .nth(index)?;
        Some(if content.is_inbox {
            Target::Inbox
        } else {
            Target::Collection(content.content_id)
        })
    }

    fn apply_pending_icons(&mut self) {
        let Some(hwnd) = self.shared.hwnd.get() else {
            return;
        };
        let Some((small, big)) = self.pending_icons.take() else {
            return;
        };
        self.icons.clear();
        for (which, (pixels, bgra)) in [(0usize, small), (1usize, big)] {
            if let Ok(icon) = OwnedIcon::from_bgra(pixels, &bgra) {
                window::send_message(hwnd, 0x0080, which, icon.raw());
                self.icons.push(icon);
            }
        }
    }
}

fn tr(source: &'static str) -> String {
    pecofence_core::i18n::text(source).to_owned()
}

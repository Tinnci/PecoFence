//! Native WinUI 3 Settings component hosted by the application's Reactor lifetime.
//!
//! The component owns transient editor state only. Application settings, workspace identity,
//! persistence, OS integrations, and use cases remain owned by `App`.

mod pages;
mod rule_editor;
#[cfg(test)]
mod tests;

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
    Content,
    Rules,
    Layout,
    About,
}

impl Page {
    fn all() -> [Self; 5] {
        [
            Self::General,
            Self::Content,
            Self::Rules,
            Self::Layout,
            Self::About,
        ]
    }

    fn title(self) -> &'static str {
        match self {
            Self::General => "常规",
            Self::Content => "内容与窗口",
            Self::Rules => "整理规则",
            Self::Layout => "布局与备份",
            Self::About => "关于与诊断",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Self::General => "general",
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
        match message {
            Message::HostUpdate(client, view) => self.update_projection(client, *view),
            Message::HostDecision(receipt) => self.on_decision(receipt),
            Message::HostNotice(text, error) => self.notice = Some(Notice { text, error }),
            Message::HostTheme(mode, glass) => {
                self.mode = mode;
                self.liquid_glass = glass;
            }
            Message::LanguageChanged => {}
            Message::ShowContent(id) => {
                self.page = Page::Content;
                if let Some(view) = &self.view {
                    self.selected_content = view
                        .contents
                        .iter()
                        .find(|content| content.content_id == id)
                        .map(Self::key);
                    self.pending_content = None;
                } else {
                    self.pending_content = Some(id);
                }
            }
            Message::Summary(stamp, count) => {
                if self.view.as_ref().is_none_or(|view| {
                    view.stamp.workspace == stamp.workspace && view.stamp.revision <= stamp.revision
                }) {
                    self.item_summary = Some((stamp, count));
                }
            }
            Message::SetIcons(small, big) => {
                self.pending_icons = Some((small, big));
                self.apply_pending_icons();
            }
            Message::NativeHandle(raw) => {
                tracing::info!(source = %self.source, "settings: native controls ready");
                self.shared
                    .hwnd
                    .set(Some(HWND(raw as *mut std::ffi::c_void)));
                self.apply_pending_icons();
            }
            Message::WindowPlacement(placement) => self.placement = Some(placement),
            Message::Activate => {
                _ = context.activate_window();
            }
            Message::Close => {
                _ = context.close_window();
            }
            #[cfg(test)]
            Message::Navigate(page) => {
                self.navigate(page);
            }
            Message::SelectPage(tag) => {
                if let Some(page) = tag.as_deref().and_then(Page::from_tag) {
                    self.navigate(page);
                }
            }
            Message::TogglePane => self.pane_open = !self.pane_open,
            Message::PaneOpenChanged(open) => self.pane_open = open,
            Message::NavigationModeChanged(mode) => {
                #[cfg(test)]
                self.shared.navigation_mode.set(mode);
                if self.navigation_mode != mode {
                    self.navigation_mode = mode;
                    self.pane_open = mode == NavigationViewDisplayMode::Expanded;
                }
            }
            Message::Setting(change) => self.submit(
                SettingsCommand::SetSetting { change },
                RequestKind::Immediate,
                None,
            ),
            Message::IconTint(rgb) => self.submit(
                SettingsCommand::SetSetting {
                    change: SettingChange::IconTint(rgb),
                },
                RequestKind::Immediate,
                None,
            ),
            Message::SelectContent(index) => {
                self.selected_content =
                    index.and_then(|index| self.view.as_ref()?.contents.get(index).map(Self::key));
            }
            Message::ContentText(value) => self.set_title_draft(value),
            Message::ApplyTitle => self.submit_title(false),
            Message::RetryTitle => self.submit_title(true),
            Message::DiscardTitle => {
                if let Some(key) = self.selected_content {
                    self.title_drafts.remove(&key);
                }
            }
            Message::Content(key, change) => self.submit(
                SettingsCommand::SetContent {
                    content_id: key.content,
                    container_id: key.container,
                    change,
                },
                RequestKind::Immediate,
                None,
            ),
            Message::Container(key, change) => {
                if matches!(
                    &change,
                    ContainerChange::TitleColor(
                        pecofence_core::settings_protocol::TitleColor::Tint
                    )
                ) && self
                    .content(key)
                    .is_some_and(|content| content.tint.is_none())
                {
                    self.notice = Some(Notice {
                        text: tr("选择标题色调前，请先为窗口选择一种色调。"),
                        error: true,
                    });
                } else {
                    self.submit(
                        SettingsCommand::SetContainer {
                            content_id: key.content,
                            container_id: key.container,
                            change,
                        },
                        RequestKind::Immediate,
                        None,
                    );
                }
            }
            Message::Opacity(key, value) => {
                if let Some(value) = value.filter(|value| value.is_finite()) {
                    self.submit(
                        SettingsCommand::SetContainer {
                            content_id: key.content,
                            container_id: key.container,
                            change: ContainerChange::Opacity(Some(value as f32)),
                        },
                        RequestKind::Immediate,
                        None,
                    );
                }
            }
            Message::ContainerToggle(key, field, value) => {
                let command = match field {
                    ContainerField::AutoHeight => SettingsCommand::SetContainer {
                        content_id: key.content,
                        container_id: key.container,
                        change: ContainerChange::AutoHeight(value),
                    },
                    ContainerField::Locked => SettingsCommand::SetContainer {
                        content_id: key.content,
                        container_id: key.container,
                        change: ContainerChange::Locked(value),
                    },
                    ContainerField::ExcludeFromQuickHide => SettingsCommand::SetContainer {
                        content_id: key.content,
                        container_id: key.container,
                        change: ContainerChange::ExcludeFromQuickHide(value),
                    },
                    ContainerField::PortalNavigate => SettingsCommand::SetContent {
                        content_id: key.content,
                        container_id: key.container,
                        change: ContentChange::PortalNavigate(value),
                    },
                    ContainerField::PortalTitleIcon => SettingsCommand::SetContent {
                        content_id: key.content,
                        container_id: key.container,
                        change: ContentChange::PortalTitleIcon(value),
                    },
                };
                self.submit(command, RequestKind::Immediate, None);
            }
            Message::TitleColorText(key, value) => {
                let Some(stamp) = self.view.as_ref().map(|view| view.stamp) else {
                    return;
                };
                let draft = self.title_color_drafts.entry(key).or_insert(TextDraft {
                    value: String::new(),
                    base: stamp,
                    generation: Uuid::new_v4(),
                    needs_review: false,
                    submitted: None,
                });
                draft.value = value;
                draft.generation = Uuid::new_v4();
                draft.submitted = None;
            }
            Message::ApplyTitleColor(key) => self.submit_text(TextField::TitleColor, key, false),
            Message::RetryTitleColor(key) => self.submit_text(TextField::TitleColor, key, true),
            Message::DiscardTitleColor(key) => {
                self.title_color_drafts.remove(&key);
            }
            Message::KeepUpdated(value) => self.submit(
                SettingsCommand::Rule {
                    change: RuleChange::KeepUpdated { value },
                },
                RequestKind::Immediate,
                None,
            ),
            Message::DefaultTarget(index) => {
                if let Some(target) = index.and_then(|index| self.collection_target(index)) {
                    self.submit(
                        SettingsCommand::Rule {
                            change: RuleChange::DefaultTarget { target },
                        },
                        RequestKind::Immediate,
                        None,
                    );
                }
            }
            Message::RuleEnabled(id, value) => {
                self.rule_action(RuleChange::SetEnabled { id, value })
            }
            Message::RuleMove(id, before) => self.rule_action(RuleChange::Move { id, before }),
            Message::RuleDelete(id) => self.ask_confirmation(SettingsCommand::Rule {
                change: RuleChange::Delete { id },
            }),
            Message::RuleEdit(id) => self.open_rule_draft(Some(id)),
            Message::RuleNew => self.open_rule_draft(None),
            Message::RuleName(value) => self.update_rule_draft(|draft| draft.name = value),
            Message::RuleTarget(index) => {
                let target = index.and_then(|index| self.collection_target(index));
                if let Some(target) = target {
                    self.update_rule_draft(|draft| draft.target = Some(target));
                }
            }
            Message::RuleEnabledDraft(value) => {
                self.update_rule_draft(|draft| draft.enabled = value)
            }
            Message::ConditionKind(index) => self.update_rule_draft(|draft| {
                draft.form.kind = rule_editor::ConditionKind::from_index(index);
                draft.form.reset_values();
            }),
            Message::ConditionText(value) => {
                self.update_rule_draft(|draft| draft.form.text = value)
            }
            Message::ConditionOperation(index) => {
                self.update_rule_draft(|draft| draft.form.set_operation_index(index))
            }
            Message::ConditionType(category, checked) => self
                .update_rule_draft(|draft| set_selected(&mut draft.form.types, category, checked)),
            Message::ConditionWeekday(day, checked) => {
                self.update_rule_draft(|draft| set_selected(&mut draft.form.weekdays, day, checked))
            }
            Message::ConditionOrigin(index) => {
                self.update_rule_draft(|draft| draft.form.set_origin_index(index))
            }
            Message::ConditionMinimum(value) => {
                self.update_rule_draft(|draft| draft.form.minimum = value)
            }
            Message::ConditionMaximum(value) => {
                self.update_rule_draft(|draft| draft.form.maximum = value)
            }
            Message::ConditionFrom(value) => {
                self.update_rule_draft(|draft| draft.form.time_from = value)
            }
            Message::ConditionTo(value) => {
                self.update_rule_draft(|draft| draft.form.time_to = value)
            }
            Message::ConditionIdle(value) => {
                self.update_rule_draft(|draft| draft.form.idle_days = value)
            }
            Message::AddCondition => self.add_rule_condition(),
            Message::EditCondition(id) => {
                if let Some(draft) = &mut self.rule_draft {
                    draft.edit_condition(id);
                }
            }
            Message::RemoveCondition(id) => {
                if let Some(draft) = &mut self.rule_draft {
                    draft.conditions.retain(|item| item.id != id);
                    if draft.editing_condition == Some(id) {
                        draft.editing_condition = None;
                        draft.form.reset_values();
                    }
                    draft.touch();
                }
            }
            Message::SaveRule => self.save_rule_draft(),
            Message::CancelRule | Message::DiscardRule => self.rule_draft = None,
            Message::RetryRule => self.retry_rule_draft(),
            Message::SnapshotName(value) => self.snapshot_name = value,
            Message::MonitorFirst(index) => {
                self.monitor_first = index.and_then(|index| {
                    self.view
                        .as_ref()?
                        .monitors
                        .get(index)
                        .map(|monitor| monitor.id.clone())
                });
            }
            Message::MonitorSecond(index) => {
                self.monitor_second = index.and_then(|index| {
                    self.view
                        .as_ref()?
                        .monitors
                        .get(index)
                        .map(|monitor| monitor.id.clone())
                });
            }
            Message::AskAction(action) => self.ask_confirmation(SettingsCommand::Action { action }),
            Message::DialogClosed(token, result) => self.finish_confirmation(token, result),
            #[cfg(test)]
            Message::CancelAction => self.confirm_action = None,
            Message::RunAction(action) => self.submit_action(action),
        }
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
                    .on_pane_toggle_requested(context.message(Message::TogglePane)),
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
        self.queue.push(Command::SettingsClosed {
            source: self.source,
        });
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
        let mut body = vec![
            keyed(
                "heading",
                windows_reactor::TextBlock::new()
                    .text(tr(self.page.title()))
                    .font_size(26.0),
            ),
            keyed(
                "status",
                windows_reactor::TextBlock::new()
                    .text(self.status_text(view.as_ref()))
                    .text_wrapping(windows_reactor::TextWrapping::Wrap),
            ),
        ];
        if let Some(notice) = &self.notice {
            body.push(keyed("notice", pages::notice(notice.error, &notice.text)));
        }
        body.push(keyed(
            format!("page-{:?}", self.page),
            pages::page(self, view.as_ref(), context),
        ));
        if let Some(draft) = &self.rule_draft
            && draft.needs_review
        {
            body.push(keyed(
                "rule-review",
                windows_reactor::StackPanel::new().spacing(8.0).children((
                    windows_reactor::TextBlock::new()
                        .text(tr(
                            "规则草稿基于较旧的修订。检查草稿后，明确选择重试或放弃。",
                        ))
                        .text_wrapping(windows_reactor::TextWrapping::Wrap),
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
                        .title(tr("请确认此操作。"))
                        .primary_button_text(tr("确认并继续"))
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
            .pane_title("PecoFence")
            .on_is_pane_open_changed(context.callback(Message::PaneOpenChanged))
            .on_display_mode_changed(context.callback(Message::NavigationModeChanged))
            .on_selected_tag_changed(context.callback(Message::SelectPage))
            .keyed_menu_items(Page::all().map(item))
            .content(
                windows_reactor::ScrollViewer::new().content(
                    windows_reactor::Border::new()
                        .padding(windows_reactor::Thickness {
                            left: 24.0,
                            top: 16.0,
                            right: 24.0,
                            bottom: 24.0,
                        })
                        .content(
                            windows_reactor::StackPanel::new()
                                .spacing(14.0)
                                .keyed_children(body),
                        ),
                ),
            )
            .into()
    }

    fn navigate(&mut self, page: Page) {
        self.page = page;
        self.notice = None;
        if self.navigation_mode != NavigationViewDisplayMode::Expanded {
            self.pane_open = false;
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
            return tr("正在提交；接纳不等于已保存。");
        }
        if self.accepted.last().is_some_and(|accepted| {
            accepted.stamp.workspace == view.stamp.workspace
                && view
                    .committed_revision
                    .is_none_or(|revision| revision < accepted.stamp.revision)
        }) {
            return tr("修改已接纳，尚未确认保存。");
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
            return tr("当前工作区修订已保存。");
        }
        tr("等待保存状态…")
    }

    fn update_projection(&mut self, client: Uuid, mut view: SettingsView) {
        if self.client == Some(client)
            && self.view.as_ref().is_some_and(|current| {
                current.stamp.workspace == view.stamp.workspace
                    && current.stamp.revision > view.stamp.revision
            })
        {
            return;
        }
        let previous = self.view.as_ref().map(|previous| previous.stamp);
        if self.client.is_some_and(|old| old != client) {
            self.pending.clear();
            self.inflight = None;
            self.accepted.clear();
            self.next_sequence = 0;
            if let Some(draft) = &mut self.rule_draft {
                draft.needs_review = true;
            }
            for draft in self
                .title_drafts
                .values_mut()
                .chain(self.title_color_drafts.values_mut())
            {
                draft.needs_review = true;
            }
            self.notice = Some(Notice {
                text: tr("设置会话已更换；草稿仍保留，请先检查。"),
                error: true,
            });
        }
        self.client = Some(client);
        if let Some(previous) = previous {
            if previous.workspace != view.stamp.workspace {
                self.pending.clear();
                self.accepted.clear();
                if let Some(draft) = &mut self.rule_draft {
                    draft.needs_review = true;
                }
                for draft in self
                    .title_drafts
                    .values_mut()
                    .chain(self.title_color_drafts.values_mut())
                {
                    draft.needs_review = true;
                }
                self.notice = Some(Notice {
                    text: tr("工作区已更换；旧草稿没有自动应用到新工作区。"),
                    error: true,
                });
            } else if previous != view.stamp {
                if let Some(draft) = &mut self.rule_draft
                    && draft.submitted.is_none()
                {
                    draft.needs_review = true;
                }
                for draft in self
                    .title_drafts
                    .values_mut()
                    .chain(self.title_color_drafts.values_mut())
                {
                    if draft.submitted.is_none() {
                        draft.needs_review = true;
                    }
                }
            }
        }
        self.accepted.retain(|accepted| {
            accepted.stamp.workspace != view.stamp.workspace
                || accepted.stamp.revision > view.stamp.revision
        });
        self.title_drafts.retain(|_, draft| {
            !draft.submitted.is_some_and(|stamp| {
                stamp.workspace == view.stamp.workspace && view.stamp.revision >= stamp.revision
            })
        });
        self.title_color_drafts.retain(|_, draft| {
            !draft.submitted.is_some_and(|stamp| {
                stamp.workspace == view.stamp.workspace && view.stamp.revision >= stamp.revision
            })
        });
        if self.rule_draft.as_ref().is_some_and(|draft| {
            draft.submitted.is_some_and(|stamp| {
                stamp.workspace == view.stamp.workspace && view.stamp.revision >= stamp.revision
            })
        }) {
            self.rule_draft = None;
        }
        if let Some((stamp, count)) = self.item_summary
            && stamp.workspace == view.stamp.workspace
            && stamp.revision <= view.stamp.revision
        {
            view.item_count = count;
        }
        let explicitly_requested = self.pending_content.is_some();
        if let Some(requested) = self.pending_content.take() {
            self.selected_content = view
                .contents
                .iter()
                .find(|content| content.content_id == requested)
                .map(Self::key);
        }
        if previous.is_none() && !explicitly_requested && self.selected_content.is_none() {
            self.selected_content = view.contents.first().map(Self::key);
        }
        // Missing targets remain selected by identity until the user chooses
        // another one. Background updates must never redirect an edit or swap.
        if previous.is_none() {
            self.monitor_first = view.monitors.first().map(|monitor| monitor.id.clone());
            self.monitor_second = view.monitors.get(1).map(|monitor| monitor.id.clone());
        }
        if self.confirm_action.as_ref().is_some_and(|confirmation| {
            confirmation.client != client
                || confirmation.base != view.stamp
                || (!view.writable && !confirmation.command.permitted_read_only())
        }) {
            self.confirm_action = None;
            self.notice = Some(Notice {
                text: tr("工作区已变化；请重新确认此操作。"),
                error: true,
            });
        }
        self.view = Some(view);
    }

    fn on_decision(&mut self, receipt: Receipt) {
        let Some(inflight) = self.inflight.take() else {
            return;
        };
        if inflight.request.client != receipt.client
            || inflight.request.sequence != receipt.sequence
            || inflight.request.base != receipt.base
        {
            self.inflight = Some(inflight);
            return;
        }
        let workspace = self.view.as_ref().map(|view| view.stamp.workspace);
        if let Some(rejection) = &receipt.rejected {
            self.notice = Some(Notice {
                text: rejection_text(rejection),
                error: true,
            });
            match inflight.kind {
                RequestKind::RuleDraft(token, _) => {
                    if let Some(draft) = &mut self.rule_draft
                        && draft.token == token
                    {
                        draft.needs_review = true;
                    }
                }
                RequestKind::TextDraft(field, key, _) => {
                    if let Some(draft) = self.text_drafts(field).get_mut(&key) {
                        draft.needs_review = true;
                        draft.submitted = None;
                    }
                }
                _ => {}
            }
        } else if receipt.cancelled {
            self.notice = Some(Notice {
                text: tr("操作已取消；没有报告为已保存。"),
                error: false,
            });
        } else {
            self.notice = None;
            if workspace == Some(receipt.current.workspace) {
                self.accepted.push(Accepted {
                    stamp: receipt.current,
                    command: inflight.request.command.clone(),
                });
            }
            match inflight.kind {
                RequestKind::RuleDraft(token, generation) => {
                    if let Some(draft) = &mut self.rule_draft
                        && draft.token == token
                    {
                        if let SettingsCommand::Rule {
                            change: RuleChange::Create { id, draft: created },
                        } = &inflight.request.command
                        {
                            draft.accept_creation(*id, created.priority_class);
                        }
                        draft.mark_submitted(generation, receipt.current);
                    }
                }
                RequestKind::TextDraft(field, key, generation) => {
                    if let Some(draft) = self.text_drafts(field).get_mut(&key) {
                        if draft.generation == generation {
                            draft.submitted = Some(receipt.current);
                            draft.needs_review = false;
                        } else {
                            draft.needs_review = true;
                        }
                    }
                }
                _ => {}
            }
            if inflight.kind == RequestKind::Immediate
                && receipt.current.workspace == receipt.base.workspace
                && workspace == Some(receipt.current.workspace)
            {
                for pending in &mut self.pending {
                    if pending.kind == RequestKind::Immediate
                        && pending.request.base == receipt.base
                        && pending.request.base.workspace == receipt.current.workspace
                    {
                        pending.request.base = receipt.current;
                    }
                }
            }
        }
        self.pump();
    }

    fn submit(&mut self, command: SettingsCommand, kind: RequestKind, base: Option<DocumentStamp>) {
        let (Some(client), Some(view)) = (self.client, self.view.as_ref()) else {
            self.notice = Some(Notice {
                text: tr("设置尚未连接到应用工作区。"),
                error: true,
            });
            return;
        };
        if !view.writable && !command.permitted_read_only() {
            self.notice = Some(Notice {
                text: tr("此工作区为只读；仅恢复、查看和导出操作可用。"),
                error: true,
            });
            return;
        }
        if self.pending.len() + usize::from(self.inflight.is_some()) >= MAX_REQUESTS {
            self.notice = Some(Notice {
                text: tr("设置请求队列已满，请等待当前操作完成。"),
                error: true,
            });
            return;
        }
        if self.next_sequence + self.pending.len() as u64 >= MAX_SEQUENCE {
            self.notice = Some(Notice {
                text: tr("设置会话序号已用尽，请关闭并重新打开设置。"),
                error: true,
            });
            return;
        }
        self.pending.push_back(Pending {
            request: Request {
                protocol: VERSION,
                client,
                // Unsent requests do not consume a protocol sequence. A workspace
                // replacement can discard this queue without creating a gap.
                sequence: 0,
                base: base.unwrap_or(view.stamp),
                command,
            },
            kind,
        });
        self.notice = None;
        self.pump();
    }

    fn pump(&mut self) {
        if self.inflight.is_none() {
            self.inflight = self.pending.pop_front();
            if let Some(pending) = &mut self.inflight {
                self.next_sequence += 1;
                pending.request.sequence = self.next_sequence;
                self.queue.push(Command::SettingsRequest {
                    source: self.source,
                    request: Box::new(pending.request.clone()),
                });
            }
        }
    }

    fn ask_confirmation(&mut self, command: SettingsCommand) {
        if self.confirm_action.is_some() {
            return;
        }
        let (Some(client), Some(view)) = (self.client, self.view.as_ref()) else {
            self.notice = Some(Notice {
                text: tr("设置尚未连接到应用工作区。"),
                error: true,
            });
            return;
        };
        self.confirm_action = Some(Confirmation {
            token: Uuid::new_v4(),
            source: self.source,
            client,
            base: view.stamp,
            command,
        });
    }

    fn finish_confirmation(&mut self, token: Uuid, result: ContentDialogResult) {
        if !self
            .confirm_action
            .as_ref()
            .is_some_and(|confirmation| confirmation.token == token)
        {
            return;
        }
        let confirmation = self.confirm_action.take().unwrap();
        if result != ContentDialogResult::Primary {
            return;
        }
        if !self.shared.alive.get()
            || confirmation.source != self.source
            || self.client != Some(confirmation.client)
            || !self
                .view
                .as_ref()
                .is_some_and(|view| view.stamp == confirmation.base)
        {
            self.notice = Some(Notice {
                text: tr("工作区已变化；请重新确认此操作。"),
                error: true,
            });
            return;
        }
        let command = match confirmation.command {
            SettingsCommand::Action { action } => SettingsCommand::Action {
                action: mark_confirmed(action),
            },
            other => other,
        };
        self.submit(command, RequestKind::Action, Some(confirmation.base));
    }

    fn submit_action(&mut self, action: Action) {
        self.submit(
            SettingsCommand::Action { action },
            RequestKind::Action,
            None,
        );
    }

    fn rule_action(&mut self, change: RuleChange) {
        self.submit(
            SettingsCommand::Rule { change },
            RequestKind::Immediate,
            None,
        );
    }

    fn set_title_draft(&mut self, value: String) {
        let Some(key) = self.selected_content else {
            return;
        };
        let Some(stamp) = self.view.as_ref().map(|view| view.stamp) else {
            return;
        };
        let initial = self
            .content(key)
            .map(|content| content.title.clone())
            .unwrap_or_default();
        let draft = self.title_drafts.entry(key).or_insert(TextDraft {
            value: initial,
            base: stamp,
            generation: Uuid::new_v4(),
            needs_review: false,
            submitted: None,
        });
        draft.value = value;
        draft.generation = Uuid::new_v4();
        draft.submitted = None;
    }

    fn submit_title(&mut self, retry: bool) {
        let Some(key) = self.selected_content else {
            return;
        };
        self.submit_text(TextField::Title, key, retry);
    }

    fn text_drafts(&mut self, field: TextField) -> &mut HashMap<ContentKey, TextDraft> {
        match field {
            TextField::Title => &mut self.title_drafts,
            TextField::TitleColor => &mut self.title_color_drafts,
        }
    }

    fn submit_text(&mut self, field: TextField, key: ContentKey, retry: bool) {
        let Some(latest) = self.view.as_ref().map(|view| view.stamp) else {
            return;
        };
        if !self.view.as_ref().is_some_and(|view| {
            view.contents
                .iter()
                .any(|content| Self::key(content) == key)
        }) {
            self.notice = Some(Notice {
                text: tr("该内容已移动或删除；草稿保留，请重新选择目标。"),
                error: true,
            });
            return;
        }
        let Some(draft) = self.text_drafts(field).get_mut(&key) else {
            return;
        };
        if draft.needs_review && !retry {
            self.notice = Some(Notice {
                text: tr("文本草稿基于较旧的修订；请检查后明确重试。"),
                error: true,
            });
            return;
        }
        if retry && !draft.needs_review {
            return;
        }
        if draft.base.workspace != latest.workspace {
            self.notice = Some(Notice {
                text: tr("文本草稿属于另一工作区；草稿保留，但不能自动重试。"),
                error: true,
            });
            return;
        }
        let title = draft.value.trim().to_owned();
        let command = match field {
            TextField::Title => {
                if title.is_empty() {
                    self.notice = Some(Notice {
                        text: tr("名称不能为空。"),
                        error: true,
                    });
                    return;
                }
                SettingsCommand::SetContent {
                    content_id: key.content,
                    container_id: key.container,
                    change: ContentChange::Title(title),
                }
            }
            TextField::TitleColor => {
                let Some(rgb) = parse_rgb(&title) else {
                    self.notice = Some(Notice {
                        text: tr("标题颜色请使用六位十六进制 RGB 值，例如 #4A90E2。"),
                        error: true,
                    });
                    return;
                };
                SettingsCommand::SetContainer {
                    content_id: key.content,
                    container_id: key.container,
                    change: ContainerChange::TitleColor(
                        pecofence_core::settings_protocol::TitleColor::Custom(rgb),
                    ),
                }
            }
        };
        if retry {
            draft.base = latest;
            draft.needs_review = false;
        }
        let generation = draft.generation;
        let base = draft.base;
        self.submit(
            command,
            RequestKind::TextDraft(field, key, generation),
            Some(base),
        );
    }

    fn update_rule_draft(&mut self, edit: impl FnOnce(&mut rule_editor::Draft)) {
        if let Some(draft) = &mut self.rule_draft {
            edit(draft);
            draft.touch();
        }
    }

    fn add_rule_condition(&mut self) {
        let Some(draft) = &mut self.rule_draft else {
            return;
        };
        match draft.form.to_condition() {
            Ok(condition)
                if draft.conditions.len() < 32
                    || draft
                        .editing_condition
                        .is_some_and(|id| draft.conditions.iter().any(|entry| entry.id == id)) =>
            {
                draft.upsert_condition(condition);
                draft.touch();
                self.notice = None;
            }
            Ok(_) => {
                self.notice = Some(Notice {
                    text: tr("每条规则最多添加 32 个条件。"),
                    error: true,
                })
            }
            Err(error) => {
                self.notice = Some(Notice {
                    text: error,
                    error: true,
                })
            }
        }
    }

    fn open_rule_draft(&mut self, id: Option<RuleId>) {
        if self.rule_draft.is_some() {
            self.notice = Some(Notice {
                text: tr("规则草稿尚未提交。"),
                error: true,
            });
            return;
        }
        self.rule_draft = self.view.as_ref().and_then(|view| match id {
            Some(id) => view
                .rules
                .list
                .iter()
                .find(|rule| rule.id == id)
                .map(|rule| rule_editor::Draft::edit(view.stamp, rule)),
            None => Some(rule_editor::Draft::new(view.stamp)),
        });
    }

    fn save_rule_draft(&mut self) {
        let Some(draft) = &self.rule_draft else {
            return;
        };
        if self.inflight.iter().chain(self.pending.iter()).any(|pending| {
            matches!(pending.kind, RequestKind::RuleDraft(token, _) if token == draft.token)
        }) {
            self.notice = Some(Notice { text: tr("正在提交；接纳不等于已保存。"), error: false });
            return;
        }
        if draft.needs_review {
            self.notice = Some(Notice {
                text: tr("请先检查草稿并明确重试，不能覆盖较新的工作区修订。"),
                error: true,
            });
            return;
        }
        let Some(target) = draft.target else {
            self.notice = Some(Notice {
                text: tr("请选择一个文件集合目标。"),
                error: true,
            });
            return;
        };
        let conditions = draft
            .conditions
            .iter()
            .map(|condition| condition.condition.clone())
            .collect::<Vec<_>>();
        if conditions.is_empty() {
            self.notice = Some(Notice {
                text: tr("请添加至少一个 AND 条件。"),
                error: true,
            });
            return;
        }
        let id = draft.rule_id.unwrap_or(draft.token);
        let rule_draft = RuleCommandDraft {
            name: draft.name.trim().to_owned(),
            enabled: draft.enabled,
            target,
            all_of: conditions,
            priority_class: draft.priority_class(),
        };
        let change = if draft.rule_id.is_some() {
            RuleChange::Edit {
                id,
                draft: rule_draft,
            }
        } else {
            RuleChange::Create {
                id,
                draft: rule_draft,
            }
        };
        let token = draft.token;
        let generation = draft.generation;
        let base = draft.base;
        self.submit(
            SettingsCommand::Rule { change },
            RequestKind::RuleDraft(token, generation),
            Some(base),
        );
    }

    fn retry_rule_draft(&mut self) {
        let Some(stamp) = self.view.as_ref().map(|view| view.stamp) else {
            return;
        };
        let Some(draft) = &mut self.rule_draft else {
            return;
        };
        if !draft.needs_review {
            return;
        }
        if draft.base.workspace != stamp.workspace {
            self.notice = Some(Notice {
                text: tr("规则草稿属于另一工作区；请在当前工作区重新创建。"),
                error: true,
            });
            return;
        }
        draft.base = stamp;
        draft.needs_review = false;
        self.save_rule_draft();
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

fn apply_command(view: &mut SettingsView, command: &SettingsCommand) {
    match command {
        SettingsCommand::SetSetting { change } => {
            _ = change.apply(&mut view.settings);
        }
        SettingsCommand::SetContent {
            content_id,
            container_id,
            change,
        } => {
            if let Some(content) = view.contents.iter_mut().find(|content| {
                content.content_id == *content_id && content.container_id == *container_id
            }) {
                match change {
                    ContentChange::Title(value) => content.title.clone_from(value),
                    ContentChange::IconSize(value) => content.icon_size = *value,
                    ContentChange::Spacing(value) => content.spacing = *value,
                    ContentChange::PortalNavigate(value) => {
                        if let Some(portal) = &mut content.portal {
                            portal.navigate = *value;
                        }
                    }
                    ContentChange::PortalTitleIcon(value) => {
                        if let Some(portal) = &mut content.portal {
                            portal.title_icon = *value;
                        }
                    }
                }
            }
        }
        SettingsCommand::SetContainer {
            content_id,
            container_id,
            change,
        } => {
            let belongs = view.contents.iter().any(|content| {
                content.content_id == *content_id && content.container_id == *container_id
            });
            if belongs {
                for content in view
                    .contents
                    .iter_mut()
                    .filter(|content| content.container_id == *container_id)
                {
                    apply_container_change(content, change);
                }
            }
        }
        SettingsCommand::Rule { change } => apply_rule_change(&mut view.rules, change),
        SettingsCommand::Action { .. } => {}
    }
}

fn apply_container_change(content: &mut ContentOptions, change: &ContainerChange) {
    match change {
        ContainerChange::AutoHeight(value) => content.auto_height = *value,
        ContainerChange::Locked(value) => content.locked = *value,
        ContainerChange::ExcludeFromQuickHide(value) => content.exclude_from_quick_hide = *value,
        ContainerChange::Opacity(value) => content.opacity = *value,
        ContainerChange::Tint(value) => content.tint = *value,
        ContainerChange::TitleColor(value) => content.title_color = value.clone(),
        ContainerChange::TitleSize(value) => content.title_size = *value,
        ContainerChange::DockTop => {}
    }
}

fn apply_rule_change(rules: &mut RuleSet, change: &RuleChange) {
    match change {
        RuleChange::KeepUpdated { value } => rules.keep_updated = *value,
        RuleChange::DefaultTarget { target } => rules.default_target = *target,
        RuleChange::Create { id, draft } => {
            if !rules.list.iter().any(|rule| rule.id == *id) {
                rules.list.push(Rule {
                    id: *id,
                    name: draft.name.clone(),
                    enabled: draft.enabled,
                    target: draft.target,
                    all_of: draft.all_of.clone(),
                    priority_class: draft.priority_class,
                    template: None,
                });
            }
        }
        RuleChange::Edit { id, draft } => {
            if let Some(rule) = rules.list.iter_mut().find(|rule| rule.id == *id) {
                let template = rule.template.clone();
                *rule = Rule {
                    id: *id,
                    name: draft.name.clone(),
                    enabled: draft.enabled,
                    target: draft.target,
                    all_of: draft.all_of.clone(),
                    priority_class: draft.priority_class,
                    template,
                };
            }
        }
        RuleChange::SetEnabled { id, value } => {
            if let Some(rule) = rules.list.iter_mut().find(|rule| rule.id == *id) {
                rule.enabled = *value;
            }
        }
        RuleChange::Move { id, before } => {
            if let Some(index) = rules.list.iter().position(|rule| rule.id == *id) {
                let rule = rules.list.remove(index);
                let index = before
                    .and_then(|target| rules.list.iter().position(|rule| rule.id == target))
                    .unwrap_or(rules.list.len());
                rules.list.insert(index, rule);
            }
        }
        RuleChange::Delete { id } => rules.list.retain(|rule| rule.id != *id),
    }
}

fn mark_confirmed(action: Action) -> Action {
    match action {
        Action::RestoreSnapshot { id } => Action::RestoreSnapshot { id },
        Action::DeleteSnapshot { id } => Action::DeleteSnapshot { id },
        Action::RestoreBackup { path, .. } => Action::RestoreBackup {
            path,
            confirmed: true,
        },
        Action::NewWorkspace { .. } => Action::NewWorkspace { confirmed: true },
        Action::AcceptRecovery { .. } => Action::AcceptRecovery { confirmed: true },
        Action::ImportConfig { .. } => Action::ImportConfig { confirmed: true },
        other => other,
    }
}

fn action_confirmation_text(action: &Action) -> String {
    match action {
        Action::RestoreSnapshot { .. } => tr("恢复快照将替换当前布局，是否继续？"),
        Action::DeleteSnapshot { .. } => tr("删除此快照后无法从设置中恢复。是否继续？"),
        Action::RestoreBackup { .. } => {
            tr("恢复备份会替换当前工作区。现有配置文件会保留。是否继续？")
        }
        Action::ImportConfig { .. } => tr("导入会替换当前工作区。现有配置文件会保留。是否继续？"),
        Action::AcceptRecovery { .. } => tr("接受恢复候选会替换当前工作区。是否继续？"),
        Action::NewWorkspace { .. } => {
            tr("新建工作区会替换当前工作区。现有配置文件会保留。是否继续？")
        }
        _ => tr("请确认此操作。"),
    }
}

fn rejection_text(rejection: &pecofence_core::settings_protocol::Rejection) -> String {
    use pecofence_core::settings_protocol::Rejection;
    match rejection {
        Rejection::Conflict => tr("工作区已变化；草稿已保留，请检查后明确重试。"),
        Rejection::Workspace => tr("工作区已更换；旧请求没有应用。"),
        Rejection::ReadOnly => tr("此工作区为只读，请先解决加载或恢复问题。"),
        Rejection::Invalid(detail) | Rejection::Backend(detail) => detail.clone(),
        Rejection::Expired | Rejection::Sequence | Rejection::ReusedSequence => {
            tr("设置请求序号已失效；请重新打开设置窗口。")
        }
        _ => tr("设置请求未被接受。"),
    }
}

fn tr(source: &'static str) -> String {
    pecofence_core::i18n::text(source).to_owned()
}

fn parse_rgb(value: &str) -> Option<[u8; 3]> {
    let value = value.trim().strip_prefix('#').unwrap_or(value.trim());
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some([
        u8::from_str_radix(&value[0..2], 16).ok()?,
        u8::from_str_radix(&value[2..4], 16).ok()?,
        u8::from_str_radix(&value[4..6], 16).ok()?,
    ])
}

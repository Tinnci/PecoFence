//! Synthetic component tests never create desktop fences, register hotkeys, or change autostart.
use super::*;
use pecofence_core::settings_protocol::{Rejection, TitleColor};
use windows_reactor::{ComponentHost, RecordingAdapter, component};

fn input() -> ComponentInput {
    ComponentInput {
        activation: Uuid::new_v4(),
        shared: Rc::new(HostShared {
            alive: Cell::new(true),
            retired: Cell::new(false),
            hwnd: Cell::new(None),
            sender: RefCell::new(None),
            deferred: RefCell::new(VecDeque::new()),
            navigation_mode: Cell::new(NavigationViewDisplayMode::Expanded),
        }),
        mode: ThemeMode::Light,
        liquid_glass: false,
        queue: CommandQueue::new(),
    }
}

fn projection() -> SettingsView {
    SettingsView {
        stamp: DocumentStamp {
            workspace: Uuid::new_v4(),
            revision: 0,
        },
        settings: Default::default(),
        writable: true,
        saving: false,
        closing: false,
        dirty: false,
        committed_revision: Some(0),
        save_issue: None,
        load_issue: None,
        recovered_from: None,
        desktop_icons_hidden: Some(false),
        rules: Default::default(),
        contents: vec![ContentOptions {
            content_id: ContentId::new(),
            container_id: ContainerId::new(),
            title: "Synthetic inbox".into(),
            window_title: "Synthetic window".into(),
            window_contents: vec!["Synthetic inbox".into()],
            is_file_view: true,
            is_collection: true,
            is_inbox: true,
            portal: None,
            icon_size: 48,
            spacing: Default::default(),
            auto_height: false,
            locked: false,
            exclude_from_quick_hide: false,
            opacity: Some(0.65),
            tint: Some([13, 71, 222]),
            title_color: TitleColor::Custom([9, 8, 7]),
            title_size: Default::default(),
        }],
        snapshots: vec![],
        backups: vec![],
        monitors: vec![],
        version: "synthetic",
        config_path: "synthetic/workspace.v2.json".into(),
        memory_mb: None,
        item_count: 0,
    }
}

fn connected() -> SettingsComponent {
    let mut ui = SettingsComponent::new(&input());
    ui.update_projection(Uuid::new_v4(), projection());
    ui
}

fn setting(value: bool) -> SettingsCommand {
    SettingsCommand::SetSetting {
        change: SettingChange::QuickHideEnabled(value),
    }
}

fn sent(ui: &SettingsComponent) -> Request {
    let commands = ui.queue.drain();
    assert_eq!(commands.len(), 1, "only one request may be in flight");
    match commands.into_iter().next().unwrap() {
        Command::SettingsRequest { source, request } => {
            assert_eq!(source, ui.source);
            *request
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

fn receipt(request: &Request, current: DocumentStamp) -> Receipt {
    Receipt {
        client: request.client,
        sequence: request.sequence,
        base: request.base,
        current,
        rejected: None,
        cancelled: false,
    }
}

#[test]
fn accepted_edit_does_not_overwrite_a_newer_queued_edit() {
    let mut ui = connected();
    ui.submit(setting(true), RequestKind::Immediate, None);
    let first = sent(&ui);
    ui.submit(setting(false), RequestKind::Immediate, None);
    let current = DocumentStamp {
        revision: 1,
        ..first.base
    };
    ui.on_decision(receipt(&first, current));
    let second = sent(&ui);
    assert_eq!(second.sequence, 2);
    assert_eq!(second.base, current);
    assert!(!ui.projected_view().unwrap().settings.quick_hide.enabled);
}

#[test]
fn unsent_requests_do_not_consume_sequences_when_workspace_changes() {
    let mut ui = connected();
    ui.submit(setting(true), RequestKind::Immediate, None);
    let first = sent(&ui);
    ui.submit(setting(false), RequestKind::Immediate, None);
    assert_eq!(ui.next_sequence, 1);
    let mut replacement = projection();
    replacement.settings.quick_hide.enabled = false;
    ui.update_projection(first.client, replacement.clone());
    assert!(ui.pending.is_empty());
    assert!(!ui.projected_view().unwrap().settings.quick_hide.enabled);
    let mut rejected = receipt(&first, replacement.stamp);
    rejected.rejected = Some(Rejection::Workspace);
    ui.on_decision(rejected);
    ui.submit(setting(true), RequestKind::Immediate, None);
    let next = sent(&ui);
    assert_eq!(next.sequence, 2);
    assert_eq!(next.base, replacement.stamp);
}

#[test]
fn request_fifo_is_bounded_and_only_dispatches_one() {
    let mut ui = connected();
    for index in 0..=MAX_REQUESTS {
        ui.submit(setting(index % 2 == 0), RequestKind::Immediate, None);
    }
    assert_eq!(ui.pending.len(), MAX_REQUESTS - 1);
    assert_eq!(ui.next_sequence, 1);
    assert!(ui.notice.as_ref().unwrap().error);
    assert_eq!(sent(&ui).sequence, 1);
}

#[test]
fn mismatched_receipt_cannot_release_inflight_request() {
    let mut ui = connected();
    ui.submit(setting(true), RequestKind::Immediate, None);
    let request = sent(&ui);
    let mut wrong = receipt(&request, request.base);
    wrong.client = Uuid::new_v4();
    ui.on_decision(wrong);
    assert_eq!(ui.inflight.as_ref().unwrap().request, request);
}

#[test]
fn editing_title_during_submission_preserves_the_new_draft() {
    let mut ui = connected();
    let key = ui.selected_content.unwrap();
    ui.set_title_draft("First draft".into());
    ui.submit_title(false);
    let request = sent(&ui);
    ui.set_title_draft("Second draft".into());
    let current = DocumentStamp {
        revision: 1,
        ..request.base
    };
    ui.on_decision(receipt(&request, current));
    let mut view = ui.view.clone().unwrap();
    view.stamp = current;
    view.contents[0].title = "First draft".into();
    ui.update_projection(request.client, view);
    let draft = &ui.title_drafts[&key];
    assert_eq!(draft.value, "Second draft");
    assert!(draft.needs_review);
    assert!(draft.submitted.is_none());
}

#[test]
fn stale_projection_and_cross_workspace_retry_preserve_title() {
    let mut ui = connected();
    let client = ui.client.unwrap();
    let key = ui.selected_content.unwrap();
    ui.set_title_draft("Preserved draft".into());
    let old = ui.view.clone().unwrap();
    let mut latest = old.clone();
    latest.stamp.revision = 5;
    ui.update_projection(client, latest);
    ui.update_projection(client, old);
    assert_eq!(ui.view.as_ref().unwrap().stamp.revision, 5);
    ui.update_projection(client, projection());
    ui.selected_content = Some(key);
    ui.submit_title(true);
    assert!(ui.queue.drain().is_empty());
    assert_eq!(ui.title_drafts[&key].value, "Preserved draft");
}

#[test]
fn checkbox_intents_apply_to_latest_selection_without_stale_vector_replacement() {
    let mut selection = vec![];
    set_selected(&mut selection, 1, true);
    set_selected(&mut selection, 2, true);
    set_selected(&mut selection, 1, true);
    set_selected(&mut selection, 1, false);
    assert_eq!(selection, [2]);
}

#[test]
fn title_color_draft_requires_review_and_cannot_cross_workspaces() {
    let mut ui = connected();
    let key = ui.selected_content.unwrap();
    let base = ui.view.as_ref().unwrap().stamp;
    ui.title_color_drafts.insert(
        key,
        TextDraft {
            value: "#123456".into(),
            base,
            generation: Uuid::new_v4(),
            needs_review: false,
            submitted: None,
        },
    );
    let mut latest = ui.view.clone().unwrap();
    latest.stamp.revision = 3;
    ui.update_projection(ui.client.unwrap(), latest.clone());
    ui.submit_text(TextField::TitleColor, key, false);
    assert!(ui.queue.drain().is_empty());
    assert!(ui.title_color_drafts[&key].needs_review);
    ui.submit_text(TextField::TitleColor, key, true);
    let request = sent(&ui);
    assert_eq!(request.base, latest.stamp);
    latest.stamp.workspace = Uuid::new_v4();
    ui.update_projection(request.client, latest);
    ui.submit_text(TextField::TitleColor, key, true);
    assert!(ui.queue.drain().is_empty());
    assert_eq!(ui.title_color_drafts[&key].value, "#123456");
}

#[test]
fn missing_content_is_not_automatically_retargeted_to_another_container() {
    let mut ui = connected();
    let original = ui.selected_content.unwrap();
    let mut latest = ui.view.clone().unwrap();
    latest.contents[0].container_id = ContainerId::new();
    ui.update_projection(ui.client.unwrap(), latest);
    assert_eq!(ui.selected_content, Some(original));
}

#[test]
fn existing_condition_can_be_updated_at_the_thirty_two_condition_limit() {
    let mut ui = connected();
    let mut draft = rule_editor::Draft::new(ui.view.as_ref().unwrap().stamp);
    for _ in 0..32 {
        draft.upsert_condition(pecofence_core::rules::Cond::Ext(vec!["txt".into()]));
    }
    draft.edit_condition(draft.conditions[0].id);
    draft.form.text = "md".into();
    ui.rule_draft = Some(draft);
    ui.add_rule_condition();
    let draft = ui.rule_draft.as_ref().unwrap();
    assert_eq!(draft.conditions.len(), 32);
    assert_eq!(
        draft.conditions[0].condition,
        pecofence_core::rules::Cond::Ext(vec!["md".into()])
    );
    ui.add_rule_condition();
    assert_eq!(ui.rule_draft.as_ref().unwrap().conditions.len(), 32);
    assert!(ui.notice.as_ref().unwrap().error);
}

#[test]
fn rgb_parser_rejects_unicode_without_panicking() {
    assert_eq!(parse_rgb(" #0aFf19 "), Some([10, 255, 25]));
    for invalid in ["中aaa", "éabcd", "你好", "12345", "1234567", "GG0011"] {
        assert_eq!(parse_rgb(invalid), None);
    }
}

#[test]
fn rule_targets_use_explicit_inbox_role_not_collection_order() {
    let mut ui = connected();
    let mut view = ui.view.clone().unwrap();
    let mut ordinary = view.contents[0].clone();
    ordinary.content_id = ContentId::new();
    ordinary.container_id = ContainerId::new();
    ordinary.is_inbox = false;
    let id = ordinary.content_id;
    view.contents.insert(0, ordinary);
    ui.update_projection(ui.client.unwrap(), view);
    assert_eq!(ui.collection_target(0), Some(Target::Collection(id)));
    assert_eq!(ui.collection_target(1), Some(Target::Inbox));
    ui.open_rule_draft(None);
    ui.rule_draft.as_mut().unwrap().target = ui.collection_target(0);
    assert_eq!(
        ui.rule_draft.as_ref().unwrap().target,
        Some(Target::Collection(id))
    );
}

#[test]
fn accepted_creation_promotes_newer_draft_to_edit_without_duplicate_rule() {
    let mut ui = connected();
    ui.open_rule_draft(None);
    ui.update_rule_draft(|draft| {
        draft.name = "First".into();
        draft.target = Some(Target::Inbox);
        draft.upsert_condition(pecofence_core::rules::Cond::Ext(vec!["txt".into()]));
    });
    ui.save_rule_draft();
    let request = sent(&ui);
    let SettingsCommand::Rule {
        change: RuleChange::Create { id, draft: created },
    } = &request.command
    else {
        panic!("first submission must create");
    };
    ui.update_rule_draft(|draft| draft.name = "Latest".into());
    let mut view = ui.view.clone().unwrap();
    view.stamp.revision += 1;
    view.rules.list.push(Rule {
        id: *id,
        name: created.name.clone(),
        enabled: created.enabled,
        target: created.target,
        all_of: created.all_of.clone(),
        priority_class: created.priority_class,
        template: None,
    });
    ui.on_decision(receipt(&request, view.stamp));
    ui.update_projection(request.client, view);
    assert_eq!(ui.rule_draft.as_ref().unwrap().rule_id, Some(*id));
    assert_eq!(ui.rule_draft.as_ref().unwrap().name, "Latest");
    ui.retry_rule_draft();
    let retry = sent(&ui);
    assert!(matches!(retry.command, SettingsCommand::Rule {
        change: RuleChange::Edit { id: retry_id, ref draft }
    } if retry_id == *id && draft.name == "Latest"));
    assert_eq!(ui.view.as_ref().unwrap().rules.list.len(), 1);
}

#[test]
fn opening_another_rule_editor_cannot_silently_discard_a_draft() {
    let mut ui = connected();
    ui.open_rule_draft(None);
    ui.update_rule_draft(|draft| draft.name = "Keep me".into());
    let token = ui.rule_draft.as_ref().unwrap().token;
    ui.open_rule_draft(None);
    ui.open_rule_draft(Some(Uuid::new_v4()));
    assert_eq!(ui.rule_draft.as_ref().unwrap().token, token);
    assert_eq!(ui.rule_draft.as_ref().unwrap().name, "Keep me");
}

#[test]
fn confirmation_is_bound_to_document_session_and_native_dialog_token() {
    for change in 0..3 {
        let mut ui = connected();
        let command = SettingsCommand::Action {
            action: Action::NewWorkspace { confirmed: false },
        };
        ui.ask_confirmation(command.clone());
        let old = ui.confirm_action.as_ref().unwrap().token;
        let mut view = ui.view.clone().unwrap();
        let client = if change == 0 {
            Uuid::new_v4()
        } else {
            ui.client.unwrap()
        };
        if change == 1 {
            view.stamp.workspace = Uuid::new_v4();
        }
        if change == 2 {
            view.stamp.revision += 1;
        }
        ui.update_projection(client, view);
        assert!(ui.confirm_action.is_none());
        ui.ask_confirmation(command);
        let current = ui.confirm_action.as_ref().unwrap().token;
        let base = ui.confirm_action.as_ref().unwrap().base;
        ui.finish_confirmation(old, ContentDialogResult::Primary);
        assert_eq!(ui.confirm_action.as_ref().unwrap().token, current);
        assert!(ui.queue.drain().is_empty());
        ui.finish_confirmation(current, ContentDialogResult::Primary);
        let request = sent(&ui);
        assert_eq!(request.client, client);
        assert_eq!(request.base, base);
        assert_eq!(
            request.command,
            SettingsCommand::Action {
                action: Action::NewWorkspace { confirmed: true }
            }
        );
    }
}

#[test]
fn native_content_dialog_cancel_and_retirement_never_authorize_an_action() {
    let input = input();
    let mut host = ComponentHost::mount(
        RecordingAdapter::new(),
        [component::<SettingsComponent>("settings", input.clone())],
    )
    .unwrap();
    let sender = input.shared.sender.borrow().as_ref().unwrap().clone();
    let client = Uuid::new_v4();
    let mut view = projection();
    assert!(sender.send(Message::HostUpdate(client, Box::new(view.clone()))));
    assert!(sender.send(Message::AskAction(Action::NewWorkspace {
        confirmed: false
    })));
    host.drain(100).unwrap();
    let dialog = host
        .runtime()
        .graph()
        .objects()
        .find_map(|owner| host.adapter().content_dialog(owner))
        .expect("confirmation must attach a ContentDialog to this window");
    assert!(dialog.1);
    assert!(
        host.test_adapter_mut()
            .complete_content_dialog(dialog.0, ContentDialogResult::None)
    );
    host.drain(100).unwrap();
    assert!(input.queue.drain().is_empty());
    assert!(sender.send(Message::AskAction(Action::NewWorkspace {
        confirmed: false
    })));
    host.drain(100).unwrap();
    view.stamp.workspace = Uuid::new_v4();
    assert!(sender.send(Message::HostUpdate(client, Box::new(view))));
    host.drain(100).unwrap();
    assert!(
        !host
            .runtime()
            .graph()
            .objects()
            .any(|owner| host.adapter().content_dialog(owner).is_some())
    );
    assert!(input.queue.drain().is_empty());
}

#[test]
fn native_titlebar_hamburger_controls_adaptive_navigation_without_resetting_drafts() {
    use windows_reactor::{
        EventDispatch, EventId, EventPayload, ObjectId, ObjectType, PropertyId, PropertyValue,
        SelectionChange,
    };
    let input = input();
    let mut host = ComponentHost::mount(
        RecordingAdapter::new(),
        [component::<SettingsComponent>("settings", input.clone())],
    )
    .unwrap();
    let sender = input.shared.sender.borrow().as_ref().unwrap().clone();
    assert!(sender.send(Message::HostUpdate(Uuid::new_v4(), Box::new(projection()))));
    host.drain(100).unwrap();
    let title = host
        .runtime()
        .graph()
        .window_title_bar()
        .unwrap()
        .unwrap()
        .0;
    let nav = host
        .runtime()
        .graph()
        .objects()
        .find(|object| host.runtime().graph().kind(*object) == Some(ObjectType::NavigationView))
        .unwrap();
    let property = |host: &ComponentHost<RecordingAdapter>, object, id| {
        host.runtime()
            .graph()
            .properties(object)
            .unwrap()
            .iter()
            .find(|property| property.id == id)
            .unwrap()
            .value
            .clone()
    };
    assert_eq!(
        property(&host, title, PropertyId::IsPaneToggleButtonVisible),
        PropertyValue::Bool(true)
    );
    assert_eq!(
        property(&host, nav, PropertyId::IsPaneToggleButtonVisible),
        PropertyValue::Bool(false)
    );
    let emit = |host: &mut ComponentHost<RecordingAdapter>, object: ObjectId, id, payload| {
        let callback = host
            .runtime()
            .graph()
            .events(object)
            .unwrap()
            .iter()
            .find(|event| event.id == id)
            .unwrap()
            .value
            .clone();
        host.test_adapter_mut()
            .queue_event(EventDispatch::new(object, id, callback, payload));
        host.drain(100).unwrap();
    };
    emit(
        &mut host,
        title,
        EventId::PaneToggleRequested,
        EventPayload::Unit,
    );
    assert_eq!(
        property(&host, nav, PropertyId::IsPaneOpen),
        PropertyValue::Bool(false)
    );
    assert!(sender.send(Message::HostNotice("Background notice".into(), false)));
    host.drain(100).unwrap();
    assert_eq!(
        property(&host, nav, PropertyId::IsPaneOpen),
        PropertyValue::Bool(false)
    );
    emit(
        &mut host,
        nav,
        EventId::DisplayModeChanged,
        EventPayload::NavigationViewDisplayMode(NavigationViewDisplayMode::Minimal),
    );
    emit(
        &mut host,
        title,
        EventId::PaneToggleRequested,
        EventPayload::Unit,
    );
    assert_eq!(
        property(&host, nav, PropertyId::IsPaneOpen),
        PropertyValue::Bool(true)
    );
    emit(
        &mut host,
        nav,
        EventId::SelectionChanged,
        EventPayload::Selection(SelectionChange {
            item: None,
            value: Some(Rc::from(Page::Content.tag())),
        }),
    );
    assert_eq!(
        property(&host, nav, PropertyId::IsPaneOpen),
        PropertyValue::Bool(false)
    );
    assert!(sender.send(Message::ContentText("PRESERVED_PANE_DRAFT".into())));
    emit(
        &mut host,
        title,
        EventId::PaneToggleRequested,
        EventPayload::Unit,
    );
    emit(
        &mut host,
        title,
        EventId::PaneToggleRequested,
        EventPayload::Unit,
    );
    assert!(host.runtime().graph().objects().any(|object| {
        host.runtime()
            .graph()
            .properties(object)
            .unwrap()
            .iter()
            .any(|property| {
                property.id == PropertyId::Text
                    && property.value == PropertyValue::String(Rc::from("PRESERVED_PANE_DRAFT"))
            })
    }));
    assert!(
        input.queue.drain().is_empty(),
        "navigation is not a document mutation"
    );
}

#[test]
fn headless_reactor_mounts_all_five_pages_and_full_rule_editor() {
    let input = input();
    let mut host = ComponentHost::mount(
        RecordingAdapter::new(),
        [component::<SettingsComponent>("settings", input.clone())],
    )
    .unwrap();
    let sender = input.shared.sender.borrow().as_ref().unwrap().clone();
    assert!(sender.send(Message::HostUpdate(Uuid::new_v4(), Box::new(projection()))));
    host.drain(100).unwrap();
    for page in Page::all() {
        assert!(sender.send(Message::Navigate(page)));
        host.drain(100).unwrap();
        assert!(host.adapter().object_count() > 5);
    }
    assert!(sender.send(Message::Navigate(Page::Rules)));
    assert!(sender.send(Message::RuleNew));
    host.drain(100).unwrap();
    for index in 0..13 {
        assert!(sender.send(Message::ConditionKind(index)));
        host.drain(100).unwrap();
        assert!(host.adapter().object_count() > 10);
    }
    assert!(
        input.queue.drain().is_empty(),
        "rendering must not execute OS commands"
    );
    drop(host);
    assert!(!input.shared.alive.get());
    assert!(matches!(
        input.queue.drain().as_slice(),
        [Command::SettingsClosed { .. }]
    ));
}

#[test]
fn notices_and_conflicts_do_not_recreate_dirty_native_text_controls() {
    let input = input();
    let mut host = ComponentHost::mount(
        RecordingAdapter::new(),
        [component::<SettingsComponent>("settings", input.clone())],
    )
    .unwrap();
    let sender = input.shared.sender.borrow().as_ref().unwrap().clone();
    let client = Uuid::new_v4();
    let mut view = projection();
    let key = SettingsComponent::key(&view.contents[0]);
    assert!(sender.send(Message::HostUpdate(client, Box::new(view.clone()))));
    assert!(sender.send(Message::Navigate(Page::Content)));
    assert!(sender.send(Message::ContentText("FOCUS_DRAFT_UNIQUE".into())));
    assert!(sender.send(Message::TitleColorText(key, "#AABBCC".into())));
    host.drain(100).unwrap();
    let text_object = |host: &ComponentHost<RecordingAdapter>, value: &str| {
        host.runtime().graph().objects().find(|object| {
            host.runtime().graph().properties(*object).is_some_and(|properties| {
                properties.iter().any(|property| {
                    property.id == windows_reactor::PropertyId::Text
                        && matches!(&property.value, windows_reactor::PropertyValue::String(text) if text.as_ref() == value)
                })
            })
        }).expect("dirty TextBox must remain in the retained graph")
    };
    let title = text_object(&host, "FOCUS_DRAFT_UNIQUE");
    let color = text_object(&host, "#AABBCC");
    view.stamp.revision += 1;
    assert!(sender.send(Message::HostUpdate(client, Box::new(view))));
    assert!(sender.send(Message::HostNotice(
        "Synthetic background update".into(),
        false
    )));
    assert!(sender.send(Message::HostTheme(ThemeMode::Dark, false)));
    host.drain(100).unwrap();
    assert_eq!(text_object(&host, "FOCUS_DRAFT_UNIQUE"), title);
    assert_eq!(text_object(&host, "#AABBCC"), color);
}

#[test]
#[ignore = "opens synthetic WinUI windows; stage the self-contained runtime beside this test EXE"]
fn live_self_contained_pages_close_and_reopen_without_desktop_side_effects() {
    // Never fall back to framework discovery/installation during this test.
    let exe = std::env::current_exe().unwrap();
    _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .try_init();
    pecofence_platform::crashlog::install(
        exe.parent().unwrap().to_path_buf(),
        "synthetic-settings",
    );
    assert!(
        exe.parent()
            .unwrap()
            .join("Microsoft.WindowsAppRuntime.dll")
            .is_file()
    );
    let completed = Rc::new(Cell::new(false));
    let failure = Rc::new(RefCell::new(None::<String>));
    let queue = CommandQueue::new();
    let completed_ui = completed.clone();
    let failure_ui = failure.clone();
    let queue_ui = queue.clone();
    windows_reactor::App::run_with(move |context| {
        let host = SettingsHost::open(context, ThemeMode::Light, false, queue_ui.clone())?;
        host.update(Uuid::new_v4(), projection());
        let host = Rc::new(RefCell::new(host));
        let phase = Cell::new(0);
        let ticks = Cell::new(0);
        let context = context.clone();
        windows_reactor::subscribe_live_interval(std::time::Duration::from_millis(150), move || {
            ticks.set(ticks.get() + 1);
            if ticks.get() > 240 {
                *failure_ui.borrow_mut() = Some("native Settings tour timed out".into());
                _ = context.exit();
                return;
            }
            let current = phase.get();
            eprintln!("native Settings tour phase {current}");
            match current {
                0..=4 => host
                    .borrow()
                    .send(Message::Navigate(Page::all()[current as usize])),
                5 => {
                    host.borrow().send(Message::Navigate(Page::Rules));
                    host.borrow().send(Message::RuleNew);
                }
                6..=18 => host
                    .borrow()
                    .send(Message::ConditionKind((current - 6) as usize)),
                19 => host.borrow().send(Message::AskAction(Action::NewWorkspace {
                    confirmed: false,
                })),
                20 => host.borrow().send(Message::CancelAction),
                21 => host.borrow().send(Message::AskAction(Action::NewWorkspace {
                    confirmed: false,
                })),
                22 => {
                    if host.borrow().hwnd().is_none() {
                        *failure_ui.borrow_mut() =
                            Some("WinUI native HWND was not published".into());
                        _ = context.exit();
                        return;
                    }
                    host.borrow().send(Message::Close);
                }
                23 => {
                    if host.borrow().is_alive() {
                        return;
                    }
                    // The main dispatcher must remain alive with zero Settings windows.
                    match SettingsHost::open(&context, ThemeMode::Dark, false, queue_ui.clone()) {
                        Ok(reopened) => {
                            reopened.update(Uuid::new_v4(), projection());
                            *host.borrow_mut() = reopened;
                        }
                        Err(error) => {
                            *failure_ui.borrow_mut() = Some(format!("reopen failed: {error}"));
                            _ = context.exit();
                            return;
                        }
                    }
                }
                24..=73 => {
                    use pecofence_core::i18n::{Language, set_language};
                    let languages = [
                        Language::SimplifiedChinese,
                        Language::English,
                        Language::Japanese,
                        Language::TraditionalChinese,
                        Language::Korean,
                        Language::German,
                        Language::French,
                        Language::Spanish,
                        Language::Portuguese,
                        Language::Russian,
                    ];
                    let index = (current - 24) as usize;
                    set_language(languages[index / 5]);
                    if index % 5 == 1 {
                        let expected = [
                            NavigationViewDisplayMode::Expanded,
                            NavigationViewDisplayMode::Compact,
                            NavigationViewDisplayMode::Minimal,
                        ][(index / 5) % 3];
                        if host.borrow().shared.navigation_mode.get() != expected {
                            *failure_ui.borrow_mut() =
                                Some(format!("adaptive navigation did not reach {expected:?}"));
                            _ = context.exit();
                            return;
                        }
                    }
                    if index.is_multiple_of(5) {
                        let hwnd = host.borrow().hwnd().unwrap();
                        let scale = pecofence_platform::monitors::dpi_for_window(hwnd).max(96)
                            as f64
                            / 96.0;
                        let rect = window::window_rect(hwnd);
                        let width = [1080.0, 800.0, 580.0][(index / 5) % 3];
                        window::set_window_bounds(
                            hwnd,
                            rect.left,
                            rect.top,
                            (width * scale) as i32,
                            (760.0 * scale) as i32,
                        )
                        .unwrap();
                        host.borrow().send(Message::TogglePane);
                    }
                    host.borrow().send(Message::LanguageChanged);
                    host.borrow()
                        .send(Message::Navigate(Page::all()[index % 5]));
                }
                74 => host.borrow().send(Message::AskAction(Action::NewWorkspace {
                    confirmed: false,
                })),
                75 => {
                    // Exercise the native titlebar close route while a dialog is open.
                    window::post_message(
                        host.borrow().hwnd().unwrap(),
                        pecofence_platform::msg::WM_CLOSE,
                        0,
                        0,
                    );
                }
                76 => {
                    if host.borrow().is_alive() {
                        return;
                    }
                    match SettingsHost::open(&context, ThemeMode::Light, false, queue_ui.clone()) {
                        Ok(reopened) => {
                            reopened.update(Uuid::new_v4(), projection());
                            *host.borrow_mut() = reopened;
                        }
                        Err(error) => {
                            *failure_ui.borrow_mut() =
                                Some(format!("second reopen failed: {error}"));
                            _ = context.exit();
                            return;
                        }
                    }
                }
                77 => host.borrow().send(Message::AskAction(Action::NewWorkspace {
                    confirmed: false,
                })),
                78 => {
                    // Application exit must retire an active modal dialog as well.
                    completed_ui.set(true);
                    _ = context.exit();
                }
                _ => return,
            }
            phase.set(current + 1);
        })
    })
    .unwrap();
    assert!(completed.get(), "{:?}", failure.borrow());
    let commands = queue.drain();
    assert_eq!(commands.len(), 3);
    assert!(
        commands
            .iter()
            .all(|command| matches!(command, Command::SettingsClosed { .. }))
    );
}

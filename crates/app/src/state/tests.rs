use super::*;
use pecofence_core::geometry::WorkArea;

pub(super) fn test_state() -> AppState {
    let work = WorkArea {
        device_path: "test".into(),
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
        dpi: 96,
        mon_left: 0,
        mon_top: 0,
        mon_right: 1920,
        mon_bottom: 1040,
    };
    let dir = std::env::temp_dir().join(format!(
        "pecofence-state-test-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let mut state = AppState {
        config: Config::default(),
        store: ConfigStore::new(dir),
        layout: 0,
        document: DocumentClock::new(true),
        catalog: HashMap::new(),
        portal_items: HashMap::new(),
        portal_members: HashMap::new(),
        portal_cwd: HashMap::new(),
        portals: Default::default(),
        portal_sources: HashMap::new(),
        portal_layout: 0,
        work_areas: vec![work],
        first_run: true,
        recovered_from: None,
        load_issue: None,
        save_allowed: true,
        persistence_issue: None,
        pending_explicit_replacement: None,
    };
    state.ensure_layout();
    state
}

#[test]
fn asynchronous_receipt_covers_only_its_snapshot_and_replacement_barrier() {
    use crate::persistence::{Ticket, WriteResult};
    let mut state = test_state();
    state.pending_explicit_replacement = Some(state.document_stamp());
    let plan = state.prepare_save().unwrap();
    let saved_language = plan.config.settings.language;
    state.config.settings.language = pecofence_core::i18n::Language::English;
    state.mark_dirty();
    assert_eq!(plan.config.settings.language, saved_language);
    assert_eq!(
        state.apply_save_result(WriteResult {
            ticket: Ticket {
                id: 1,
                stamp: plan.stamp,
                replace: plan.replace
            },
            outcome: Ok(pecofence_core::SaveReceipt {
                backup: pecofence_core::BackupStatus::AlreadyExists
            }),
        }),
        Some(true)
    );
    assert_eq!(state.committed_revision(), Some(plan.stamp.revision));
    assert!(state.is_dirty());
    assert!(!state.prepare_save().unwrap().replace);
}

#[test]
fn old_activation_success_or_failure_cannot_acknowledge_or_poison_a_replacement() {
    use crate::persistence::{Ticket, WriteResult};
    let mut state = test_state();
    let previous = state.prepare_save().unwrap().stamp;
    state.persistence_issue = Some("previous workspace failure".into());
    state.replace_config(Config::default()).unwrap();
    assert!(state.persistence_issue.is_none());
    for outcome in [
        Err("old failure".into()),
        Ok(pecofence_core::SaveReceipt {
            backup: pecofence_core::BackupStatus::AlreadyExists,
        }),
    ] {
        assert_eq!(
            state.apply_save_result(WriteResult {
                ticket: Ticket {
                    id: 1,
                    stamp: previous,
                    replace: true
                },
                outcome,
            }),
            None
        );
        assert!(state.persistence_issue.is_none());
        assert!(state.committed_revision().is_none());
        assert!(state.prepare_save().unwrap().replace);
    }
}

#[test]
fn newer_same_activation_replacement_survives_an_older_replace_receipt() {
    use crate::persistence::{Ticket, WriteResult};
    let mut state = test_state();
    state.pending_explicit_replacement = Some(state.document_stamp());
    let older = state.prepare_save().unwrap();
    state.apply_snapshot_layouts(state.config.layouts.clone());
    assert_eq!(state.document_stamp().workspace, older.stamp.workspace);
    assert_eq!(
        state.apply_save_result(WriteResult {
            ticket: Ticket {
                id: 1,
                stamp: older.stamp,
                replace: true
            },
            outcome: Ok(pecofence_core::SaveReceipt {
                backup: pecofence_core::BackupStatus::AlreadyExists
            }),
        }),
        Some(true)
    );
    assert!(state.prepare_save().unwrap().replace);
    assert!(state.is_dirty());
}

#[test]
fn failed_primary_retains_dirty_and_explicit_replacement_until_a_real_receipt() {
    use crate::persistence::{Ticket, WriteResult};
    let mut state = test_state();
    state.replace_config(Config::default()).unwrap();
    let stamp = state.prepare_save().unwrap().stamp;
    let ticket = Ticket {
        id: 1,
        stamp,
        replace: true,
    };
    assert_eq!(
        state.apply_save_result(WriteResult {
            ticket,
            outcome: Err("disk full".into())
        }),
        Some(false)
    );
    assert!(state.is_dirty());
    assert!(state.prepare_save().unwrap().replace);
    assert_eq!(
        state.apply_save_result(WriteResult {
            ticket,
            outcome: Ok(pecofence_core::SaveReceipt {
                backup: pecofence_core::BackupStatus::Degraded("backup offline".into())
            }),
        }),
        Some(true)
    );
    assert!(!state.is_dirty());
    assert_eq!(state.committed_revision(), Some(stamp.revision));
    assert_eq!(state.persistence_issue.as_deref(), Some("backup offline"));
}

#[test]
fn closing_a_fence_removes_its_window_but_keeps_the_fence_until_reopened() {
    let mut state = test_state();
    let id = state
        .new_widget_fence(
            "Board",
            RECT {
                left: 100,
                top: 100,
                right: 700,
                bottom: 500,
            },
            "pecofence.todo",
        )
        .unwrap();
    let host = state.host_of(id).unwrap();
    let hosts = |state: &AppState| state.host_fences().iter().any(|f| f.container_id == host);
    assert!(hosts(&state));
    assert!(state.set_closed(host, true));
    assert!(
        !state.set_closed(host, true),
        "closing twice changes nothing"
    );
    assert!(!hosts(&state));
    assert!(state.fence(id).is_some(), "close never deletes");
    assert_eq!(state.closed_fences(), vec![(host, "Board".to_owned())]);
    assert!(state.set_closed(host, false));
    assert!(hosts(&state));
    assert!(state.closed_fences().is_empty());
}

#[test]
fn panel_config_updates_all_monitor_layouts() {
    let mut state = test_state();
    let id = state
        .new_widget_fence(
            "Todo",
            RECT {
                left: 100,
                top: 100,
                right: 520,
                bottom: 470,
            },
            "pecofence.todo",
        )
        .unwrap();
    let instance_id = match &state.fence(id).unwrap().content {
        pecofence_core::FenceContentSpec::Panel { panel } => panel.instance_id.as_u128(),
        _ => unreachable!(),
    };
    let mut other_layout = state.config.layouts[0].clone();
    other_layout.fingerprint[0].device_path = "other-monitor".into();
    state.config.layouts.push(other_layout);
    let before = state.document_stamp();
    let tasks = serde_json::json!({"tasks": [{"title": "Buy milk", "done": true}]});
    assert!(state.update_panel_config(instance_id, tasks.clone()));
    assert_ne!(state.document_stamp(), before);
    for layout in &state.config.layouts {
        let panel = layout
            .contents
            .iter()
            .find_map(|fence| match &fence.content {
                pecofence_core::ContentSpec::Panel { panel }
                    if panel.instance_id.as_u128() == instance_id =>
                {
                    Some(panel)
                }
                _ => None,
            });
        assert_eq!(panel.unwrap().config, tasks);
    }
}

fn add_item(state: &mut AppState, fence: FenceId, name: &str) -> ItemId {
    let item = Item {
        id: uuid::Uuid::new_v4(),
        key: ItemKey::from_path(&format!("C:/Users/Me/Desktop/{name}.txt")),
        origin: Origin::UserDesktop,
        display_name: name.to_string(),
        file_id: None,
        mtime: 1,
        is_folder: false,
        attrs: 0,
        icon_key: IconKey::ByExt(".txt".into()),
        orphaned_since: None,
        size: 0,
        open_count: 0,
        last_opened: None,
    };
    let id = item.id;
    state.catalog.insert(item.key.clone(), id);
    state.config.items.insert(id, item);
    state.config.assign(0, id, fence, AssignedBy::User);
    id
}

fn names(state: &AppState, fence: FenceId) -> Vec<String> {
    let f = state.fence(fence).unwrap();
    state
        .items_of(&f)
        .iter()
        .map(|i| i.display_name.clone())
        .collect()
}

#[test]
fn host_detach_through_app_state_keeps_content_and_can_restore() {
    let mut state = test_state();
    let (a, b, c) = (
        state.fences()[0].id,
        state.fences()[1].id,
        state.fences()[2].id,
    );
    add_item(&mut state, a, "first");
    add_item(&mut state, b, "second");
    add_item(&mut state, c, "third");
    let host = state.host_of(a).unwrap();
    state.attach_tab(b, host).unwrap();
    state.attach_tab(c, host).unwrap();
    let before = state.tabs_of(host);
    state.document.commit(state.document_stamp());
    let geometry = state.container(host).unwrap().geometry.clone();
    let (transition, change) = state
        .detach_tab_with_plan(a, geometry)
        .expect("the host must be detachable too");
    assert!(state.is_dirty());
    assert_eq!(state.host_of(a), Some(transition.created_containers[0]));
    assert_eq!(state.host_of(c), Some(host));
    assert_eq!(state.active_tab_of(host), Some(c));
    assert_eq!(names(&state, a), vec!["first"]);
    assert_eq!(names(&state, b), vec!["second"]);
    assert_eq!(names(&state, c), vec!["third"]);
    state.rename_fence(a, "edited during drag");
    state.document.commit(state.document_stamp());
    state.cancel_tab_detach(&change).unwrap();
    assert!(state.is_dirty());
    assert_eq!(state.tabs_of(host), before);
    assert_eq!(state.fence(a).unwrap().title, "edited during drag");
}

#[test]
fn rejected_and_recovered_loads_do_not_create_or_autosave_defaults() {
    let baseline = test_state();
    let areas = baseline.work_areas.clone();
    for reason in [
        FreshReason::CorruptPrimary {
            reason: "bad graph".into(),
            quarantined: None,
        },
        FreshReason::UnreadablePrimary {
            reason: "denied".into(),
        },
        FreshReason::UnsupportedFormat {
            path: "workspace.v3.json".into(),
            schema: Some(3),
        },
    ] {
        let dir = std::env::temp_dir().join(format!("pecofence-gated-{}", uuid::Uuid::new_v4()));
        let mut state = AppState::from_load(
            areas.clone(),
            ConfigStore::new(&dir),
            LoadOutcome::Fresh(Config::default(), reason),
        );
        assert!(!state.first_run);
        assert!(!state.save_allowed);
        assert!(state.load_issue.is_some());
        state.ensure_layout();
        assert!(state.config.layouts.is_empty());
        state.mark_dirty();
        assert!(!state.save_if_dirty());
        assert!(!dir.exists());
        state.reset_workspace().unwrap();
        assert!(state.save_allowed);
        assert_eq!(state.fences().len(), 8);
        assert!(state.config.validate().is_ok());
    }
    let dir = std::env::temp_dir().join(format!("pecofence-recovered-{}", uuid::Uuid::new_v4()));
    let mut state = AppState::from_load(
        areas,
        ConfigStore::new(&dir),
        LoadOutcome::Recovered(baseline.config.clone(), "backup.json".into()),
    );
    assert!(!state.save_allowed);
    assert!(state.recovered_from.is_some());
    assert!(!state.is_dirty());
    state.mark_dirty();
    assert!(!state.save_if_dirty());
    assert!(!dir.exists());
    state.accept_recovery().unwrap();
    assert!(state.save_allowed);
    assert!(state.recovered_from.is_none());
}

#[test]
fn rejected_import_preserves_document_and_gate() {
    let mut state = test_state();
    state.save_allowed = false;
    let before = state.config.clone();
    let mut invalid = before.clone();
    invalid.layouts[0].containers[0].active_tab = FenceId::new_v4();
    assert!(state.replace_config(invalid).is_err());
    assert_eq!(
        serde_json::to_value(&state.config).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
    assert!(!state.save_allowed);
}

#[test]
fn tab_errors_and_noops_do_not_dirty_and_stale_cancel_is_rejected() {
    let mut state = test_state();
    let a = state.fences()[0].id;
    let b = state.fences()[1].id;
    let c = state.fences()[2].id;
    let host = state.host_of(a).unwrap();
    state.document.commit(state.document_stamp());
    state.set_active_tab(host, a).unwrap();
    state.reorder_tab(host, a, 0).unwrap();
    assert!(!state.is_dirty());
    assert!(state.set_active_tab(host, b).is_err());
    assert!(!state.is_dirty());
    state.attach_tab(b, host).unwrap();
    state.attach_tab(c, host).unwrap();
    let geometry = state.container(host).unwrap().geometry.clone();
    let (_, plan) = state.detach_tab_with_plan(b, geometry).unwrap();
    state.reorder_tab(host, c, 0).unwrap();
    state.document.commit(state.document_stamp());
    assert!(state.cancel_tab_detach(&plan).is_err());
    assert!(!state.is_dirty());
    assert_ne!(state.host_of(b), Some(host));
}

#[test]
fn late_container_events_after_delete_or_merge_are_not_panics_or_edits() {
    for deleted in [true, false] {
        let mut state = test_state();
        let victim = state.fences()[0].id;
        let retired = state.host_of(victim).unwrap();
        if deleted {
            assert!(state.delete_fence(victim));
            assert_eq!(state.host_of(victim), None);
        } else {
            let target = state.fences()[1].container_id;
            state.attach_tab(victim, target).unwrap();
            assert_eq!(state.host_of(victim), Some(target));
        }
        assert_eq!(state.active_tab_of(retired), None);
        state.document.commit(state.document_stamp());
        let before = serde_json::to_value(&state.config).unwrap();
        state.set_fence_bounds(retired, RECT::default(), false, 100);
        assert_eq!(state.toggle_rolled(retired), None);
        state.set_auto_height(retired, true);
        assert!(!state.is_dirty());
        assert_eq!(serde_json::to_value(&state.config).unwrap(), before);
    }
    let mut state = test_state();
    state.config.layouts.clear();
    assert_eq!(state.host_of(FenceId::new_v4()), None);
    assert_eq!(state.active_tab_of(ContainerId::new()), None);
}

#[test]
fn portal_read_and_identity_survive_container_remount() {
    let mut state = test_state();
    let portal = state
        .new_portal_fence(Path::new(r"C:\Portal"), RECT::default())
        .unwrap();
    let request = state.next_portal_read().unwrap();
    let host = state.host_of(state.fences()[0].id).unwrap();
    state.attach_tab(portal, host).unwrap();
    state.set_active_tab(host, portal).unwrap();
    let geometry = state.container(host).unwrap().geometry.clone();
    let (_, plan) = state.detach_tab_with_plan(portal, geometry).unwrap();
    state.cancel_tab_detach(&plan).unwrap();
    state.document.commit(state.document_stamp());
    let path = PathBuf::from(r"C:\Portal\kept");
    assert_eq!(
        state.accept_portal_result(PortalResult {
            request,
            outcome: PortalOutcome::Complete(vec![portal_entry(path.clone())]),
        }),
        Some(portal)
    );
    let item = state.items_of(&state.fence(portal).unwrap())[0].id;
    assert_eq!(item, portal_item_id(portal, &path.to_string_lossy()));
    assert!(!state.is_dirty());
    assert!(state.next_portal_read().is_none());
}

#[test]
fn wrong_role_targets_never_receive_desktop_memberships() {
    let mut state = test_state();
    let source = state.fences()[0].id;
    let item = add_item(&mut state, source, "desktop");
    let portal = state
        .new_portal_fence(Path::new(r"C:\Portal"), RECT::default())
        .unwrap();
    assert_eq!(state.routable_fence(portal), None);
    assert_eq!(state.move_items(&[item], portal), 0);
    assert!(!state.reorder_items(portal, &[item], 0));
    assert_eq!(state.config.content_of_item(0, item), Some(source));
    assert_eq!(
        state
            .target_fence(Decision::Default(Target::Collection(portal)))
            .unwrap()
            .0,
        state.inbox_id().unwrap()
    );
    state.content_mut(portal).unwrap().content = ContentSpec::Panel {
        panel: pecofence_core::PanelSpec {
            provider: "test".into(),
            instance_id: uuid::Uuid::new_v4(),
            config_version: 1,
            config: serde_json::Value::Null,
        },
    };
    assert_eq!(state.routable_fence(portal), None);
    assert_eq!(state.move_items(&[item], portal), 0);
}

#[test]
fn workspace_count_includes_portals_deduplicates_paths_and_excludes_orphans() {
    let mut state = test_state();
    let fence = state.fences()[0].id;
    let desktop = add_item(&mut state, fence, "desktop");
    let orphan = add_item(&mut state, fence, "gone");
    state.config.items.get_mut(&orphan).unwrap().orphaned_since = Some(1);
    let mut mirrored = state.config.items[&desktop].clone();
    mirrored.id = uuid::Uuid::new_v4();
    state.portal_items.insert(mirrored.id, mirrored.clone());
    let mut portal_only = mirrored;
    portal_only.id = uuid::Uuid::new_v4();
    portal_only.key = ItemKey::from_path("C:/Projects/portal-only.txt");
    let portal_id = portal_only.id;
    state.portal_items.insert(portal_id, portal_only);
    assert_eq!(state.workspace_item_count(), 2);
    state.portal_items.remove(&portal_id);
    assert_eq!(state.workspace_item_count(), 1);
}

#[test]
fn portal_navigation_preserves_folder_case_from_folded_item_keys() {
    let root = PathBuf::from(r"C:\Portal");
    let nested = root.join("Nested Target").join("日本語 Folder");
    let mut state = test_state();
    let fence = state.new_portal_fence(&root, RECT::default()).unwrap();
    let root_title = state.fence(fence).unwrap().title.clone();
    finish_portal(
        &mut state,
        PortalOutcome::Complete(vec![portal_entry(nested.clone())]),
    );
    let folded = PathBuf::from(nested.to_string_lossy().to_lowercase());
    assert!(state.portal_enter(fence, &folded));
    assert_eq!(state.portal_path(fence), Some(nested));
    assert!(
        state
            .display_title(&state.fence(fence).unwrap())
            .starts_with("日本語 Folder")
    );
    assert!(state.portal_up(fence));
    assert!(
        state
            .display_title(&state.fence(fence).unwrap())
            .starts_with("Nested Target")
    );
    assert!(state.portal_up(fence));
    assert!(
        state
            .display_title(&state.fence(fence).unwrap())
            .starts_with(&root_title)
    );
    assert!(!state.portal_up(fence));
}

fn portal_entry(path: PathBuf) -> pecofence_core::portal::PortalEntry {
    pecofence_core::portal::PortalEntry {
        display_name: path.file_name().unwrap().to_string_lossy().into_owned(),
        path,
        is_folder: true,
        attributes: 0,
        mtime: 1,
        size: 0,
    }
}

fn finish_portal(state: &mut AppState, outcome: PortalOutcome) -> Option<FenceId> {
    let request = state.next_portal_read().unwrap();
    state.accept_portal_result(PortalResult { request, outcome })
}

#[test]
fn portal_snapshot_health_projection_and_dirty_boundary() {
    use pecofence_core::portal::ReadFailure;
    let mut state = test_state();
    let root = PathBuf::from(r"C:\Portal");
    let id = state.new_portal_fence(&root, RECT::default()).unwrap();
    state.document.commit(state.document_stamp());
    finish_portal(
        &mut state,
        PortalOutcome::Complete(vec![portal_entry(root.join("kept"))]),
    );
    assert_eq!(names(&state, id), ["kept"]);
    for outcome in [
        PortalOutcome::Unavailable(ReadFailure::Open("offline".into())),
        PortalOutcome::Partial {
            entries: vec![portal_entry(root.join("partial"))],
            failure: ReadFailure::Traversal("failed".into()),
        },
    ] {
        state.request_portal_read(id);
        finish_portal(&mut state, outcome);
        assert_eq!(names(&state, id), ["kept"]);
        assert!(matches!(
            state.portals.snapshot(id).unwrap().health,
            PortalHealth::Stale(_)
        ));
    }
    state.request_portal_read(id);
    finish_portal(&mut state, PortalOutcome::Complete(vec![]));
    assert!(names(&state, id).is_empty());
    assert!(!state.is_dirty());
    state.request_portal_read(id);
    finish_portal(
        &mut state,
        PortalOutcome::Complete(vec![portal_entry(root.join("old"))]),
    );
    let old = state.items_of(&state.fence(id).unwrap())[0].id;
    state.request_portal_read(id);
    let late = state.next_portal_read().unwrap();
    assert!(state.portal_enter(id, &root.join("missing")));
    assert!(names(&state, id).is_empty());
    assert!(state.item(old).is_none());
    assert_eq!(
        state.accept_portal_result(PortalResult {
            request: late,
            outcome: PortalOutcome::Complete(vec![portal_entry(root.join("late"))])
        }),
        None
    );
    finish_portal(
        &mut state,
        PortalOutcome::Unavailable(ReadFailure::Open("missing".into())),
    );
    assert_eq!(state.portal_path(id), Some(root.join("missing")));
    assert!(!state.is_dirty());
}

#[test]
fn portal_lifecycle_reconfiguration_import_layout_and_reused_id() {
    for mode in 0..6 {
        let mut state = test_state();
        let id = state
            .new_portal_fence(Path::new(r"C:\Portal"), RECT::default())
            .unwrap();
        let request = state.next_portal_read().unwrap();
        match mode {
            0 => {
                state.content_mut(id).unwrap().content = ContentSpec::FolderPortal {
                    root: r"C:\Other".into(),
                    recursive: false,
                    filter: None,
                    navigate: true,
                    hide_title_icon: false,
                };
            }
            1 => state.replace_config(state.config.clone()).unwrap(),
            2 => state.apply_snapshot_layouts(state.config.layouts.clone()),
            3 => {
                let content = state.config.layouts[0].content(id).unwrap().clone();
                let geometry = state.fence(id).unwrap().geometry;
                assert!(state.delete_fence(id));
                Workspace::new(&mut state.config.layouts[0])
                    .unwrap()
                    .create(content, geometry)
                    .unwrap();
            }
            4 => {
                // A source option change must invalidate even with the same id/path.
                state.content_mut(id).unwrap().content = ContentSpec::FolderPortal {
                    root: r"C:\Portal".into(),
                    recursive: true,
                    filter: None,
                    navigate: true,
                    hide_title_icon: false,
                };
            }
            _ => {
                let mut other = state.config.layouts[0].clone();
                other.fingerprint[0].device_path = "other-monitor".into();
                state.config.layouts.push(other);
                state.work_areas[0].device_path = "other-monitor".into();
                state.ensure_layout();
                // Switching back before the completion still cannot revive the old read.
                state.work_areas[0].device_path =
                    state.config.layouts[0].fingerprint[0].device_path.clone();
                state.ensure_layout();
            }
        }
        assert_eq!(
            state.accept_portal_result(PortalResult {
                request,
                outcome: PortalOutcome::Complete(vec![portal_entry(r"C:\Portal\late".into())])
            }),
            None,
            "mode {mode}"
        );
        assert!(names(&state, id).is_empty());
        finish_portal(
            &mut state,
            PortalOutcome::Complete(vec![portal_entry(r"C:\Portal\current".into())]),
        );
        assert_eq!(names(&state, id), ["current"]);
    }
}

#[test]
fn portals_on_same_path_have_independent_actionable_items() {
    let mut state = test_state();
    let root = Path::new(r"C:\Portal");
    let a = state.new_portal_fence(root, RECT::default()).unwrap();
    finish_portal(
        &mut state,
        PortalOutcome::Complete(vec![portal_entry(root.join("kept"))]),
    );
    let b = state.new_portal_fence(root, RECT::default()).unwrap();
    finish_portal(
        &mut state,
        PortalOutcome::Complete(vec![portal_entry(root.join("kept"))]),
    );
    let a_item = state.items_of(&state.fence(a).unwrap())[0].id;
    let b_item = state.items_of(&state.fence(b).unwrap())[0].id;
    assert_ne!(a_item, b_item);
    assert_eq!(state.portal_of_item(b_item), Some(b));
    state.request_portal_read(a);
    finish_portal(&mut state, PortalOutcome::Complete(vec![]));
    assert!(state.item(a_item).is_none());
    assert!(state.item(b_item).is_some());
    assert_eq!(names(&state, b), ["kept"]);
    assert!(state.delete_fence(a));
    assert_eq!(names(&state, b), ["kept"]);
}

#[test]
fn portal_navigation_rejects_dot_components_without_reading() {
    let mut state = test_state();
    let root = Path::new(r"C:\Portal");
    let id = state.new_portal_fence(root, RECT::default()).unwrap();
    finish_portal(
        &mut state,
        PortalOutcome::Complete(vec![portal_entry(root.join("kept"))]),
    );
    state.document.commit(state.document_stamp());
    for path in [
        r"C:\Portal\..\Outside",
        r"C:\Portal\.\Child",
        "C:/Portal/Child/../../Outside",
        r"C:\PortalElsewhere",
    ] {
        assert!(!state.portal_enter(id, Path::new(path)));
        assert_eq!(state.portal_path(id), Some(root.to_path_buf()));
        assert_eq!(names(&state, id), ["kept"]);
    }
    assert!(state.next_portal_read().is_none());
    assert!(!state.is_dirty());
}

#[test]
fn reorder_items_places_before_display_index() {
    let mut state = test_state();
    let fence = state.fences()[0].id;
    assert_eq!(state.fence(fence).unwrap().view.sort, SortMode::Manual);
    let a = add_item(&mut state, fence, "a");
    let b = add_item(&mut state, fence, "b");
    let c = add_item(&mut state, fence, "c");
    assert_eq!(names(&state, fence), ["a", "b", "c"]);
    // c before a.
    assert!(state.reorder_items(fence, &[c], 0));
    assert_eq!(names(&state, fence), ["c", "a", "b"]);
    // a,b to the end: already there -> no change.
    assert!(!state.reorder_items(fence, &[a, b], 3));
    assert_eq!(names(&state, fence), ["c", "a", "b"]);
    // Dropping onto its own slot (index right after itself) is a no-op.
    assert!(!state.reorder_items(fence, &[c], 1));
    // b before c.
    assert!(state.reorder_items(fence, &[b], 0));
    assert_eq!(names(&state, fence), ["b", "c", "a"]);
    // Reversed view: display order is reversed, drop index is a display index.
    state.set_reverse(fence, true);
    assert_eq!(names(&state, fence), ["a", "c", "b"]);
    assert!(state.reorder_items(fence, &[b], 0));
    assert_eq!(names(&state, fence), ["b", "a", "c"]);
    // A sorted fence refuses.
    state.set_sort(fence, SortMode::Name);
    assert!(!state.reorder_items(fence, &[a], 0));
}

#[test]
fn dir_prefix_tolerates_drive_roots() {
    assert_eq!(dir_prefix("d:\\"), "d:\\");
    assert_eq!(dir_prefix("d:\\users"), "d:\\users\\");
    assert!("d:\\users".starts_with(&dir_prefix("d:\\")));
    assert!(!"d:\\usersx".starts_with(&dir_prefix("d:\\users")));
}

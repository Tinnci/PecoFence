use pecofence_core::*;
use uuid::Uuid;

fn geometry(x: f32) -> NormGeometry {
    NormGeometry {
        monitor: "monitor".into(),
        x,
        y: 20.0,
        w: 300.0,
        h: 240.0,
        work_w: 1920.0,
        work_h: 1080.0,
        anchor: Anchor::LeftTop,
    }
}
fn empty() -> Layout {
    Layout {
        fingerprint: vec![],
        containers: vec![],
        contents: vec![],
    }
}
fn group() -> (Layout, ContainerId, [ContentId; 3]) {
    let mut layout = empty();
    let a = ContentInstance::collection("Inbox", true);
    let b = ContentInstance::portal("Portal", r"C:\files");
    let c = ContentInstance::panel(
        "Panel",
        PanelSpec {
            provider: "test.provider".into(),
            instance_id: Uuid::new_v4(),
            config_version: 1,
            config: serde_json::json!({"value": 17}),
        },
    );
    let ids = [a.id, b.id, c.id];
    let mut workspace = Workspace::new(&mut layout).unwrap();
    let host = workspace
        .create(a, geometry(10.0))
        .unwrap()
        .created_containers[0];
    workspace.add(host, b).unwrap();
    workspace.add(host, c).unwrap();
    (layout, host, ids)
}
fn config(layout: Layout) -> Config {
    Config {
        layouts: vec![layout],
        ..Config::default()
    }
}

#[test]
fn schema2_roundtrips_normalized_ownership_without_obsolete_fields() {
    let (layout, host, ids) = group();
    let cfg = config(layout);
    cfg.validate().unwrap();
    assert!(ids.iter().all(|id| id.0 != host.0));
    let json = serde_json::to_value(&cfg).unwrap();
    let layout_json = &json["layouts"][0];
    assert!(layout_json.get("fences").is_none());
    assert!(layout_json["containers"][0].get("title").is_none());
    assert!(layout_json["containers"][0].get("content").is_none());
    assert!(layout_json["contents"][0].get("geometry").is_none());
    assert!(layout_json["contents"][0].get("tabHost").is_none());
    assert!(
        layout_json["contents"][0]["view"]
            .get("autoHeight")
            .is_none()
    );
    assert_eq!(layout_json["contents"][1]["content"]["root"], r"C:\files");
    assert!(
        layout_json["contents"][1]["content"]
            .get("hideTitleIcon")
            .is_some()
    );
    let roundtrip: Config = serde_json::from_value(json).unwrap();
    roundtrip.validate().unwrap();
    assert_eq!(roundtrip.layouts, cfg.layouts);
}

#[test]
fn select_and_reorder_never_rebuild_content_or_replace_window_identity() {
    let (mut layout, host, ids) = group();
    let before = layout.contents.clone();
    {
        let mut workspace = Workspace::new(&mut layout).unwrap();
        workspace.select(host, ids[1]).unwrap();
        workspace.reorder(host, ids[0], 2).unwrap();
        assert_eq!(
            workspace.layout().container(host).unwrap().tabs,
            [ids[1], ids[2], ids[0]]
        );
        assert_eq!(
            workspace.layout().container(host).unwrap().active_tab,
            ids[1]
        );
    }
    assert_eq!(layout.contents, before);
    assert_eq!(layout.containers[0].id, host);
    layout.validate().unwrap();
}

#[test]
fn first_middle_and_last_tab_detach_preserve_content_and_surviving_container() {
    for index in 0..3 {
        let (mut layout, host, ids) = group();
        let before = layout.contents.clone();
        let source_geometry = layout.container(host).unwrap().geometry.clone();
        Workspace::new(&mut layout)
            .unwrap()
            .select(host, ids[index])
            .unwrap();
        let transition = Workspace::new(&mut layout)
            .unwrap()
            .detach(ids[index], geometry(700.0))
            .unwrap();
        let detached = transition.created_containers[0];
        assert_ne!(host, detached);
        assert_ne!(detached.0, ids[index].0);
        assert_eq!(layout.owner_of(ids[index]), Some(detached));
        assert_eq!(layout.container(detached).unwrap().geometry.x, 700.0);
        assert_eq!(layout.container(host).unwrap().geometry, source_geometry);
        assert_eq!(
            layout.container(host).unwrap().tabs,
            ids.into_iter()
                .filter(|id| *id != ids[index])
                .collect::<Vec<_>>()
        );
        assert_eq!(layout.contents, before);
        assert!(transition.removed_containers.is_empty());
        layout.validate().unwrap();
    }
}

#[test]
fn attach_last_tab_removes_only_empty_container_and_preserves_business_instances() {
    let (mut layout, host, ids) = group();
    let before = layout.contents.clone();
    let detached = Workspace::new(&mut layout)
        .unwrap()
        .detach(ids[1], geometry(700.0))
        .unwrap()
        .created_containers[0];
    let transition = Workspace::new(&mut layout)
        .unwrap()
        .attach(ids[1], host, 0)
        .unwrap();
    assert_eq!(transition.removed_containers, [detached]);
    assert_eq!(transition.deleted_content, None);
    assert_eq!(layout.containers.len(), 1);
    assert_eq!(layout.containers[0].id, host);
    assert_eq!(
        layout.container(host).unwrap().tabs,
        [ids[1], ids[0], ids[2]]
    );
    assert_eq!(layout.contents, before);
    layout.validate().unwrap();
}

#[test]
fn detach_cancellation_restores_only_ownership_preserving_intervening_edits() {
    for index in 0..3 {
        let (mut layout, host, ids) = group();
        let (_, plan) = Workspace::new(&mut layout)
            .unwrap()
            .detach_with_plan(ids[index], geometry(600.0))
            .unwrap();
        layout.content_mut(ids[index]).unwrap().title = "edited while dragging".into();
        layout.content_mut(ids[1]).unwrap().view.sort = SortMode::Date;
        if let ContentSpec::Panel { panel } = &mut layout.content_mut(ids[2]).unwrap().content {
            panel.config = serde_json::json!({"value": 99});
        }
        layout.container_mut(host).unwrap().geometry.x = 77.0;
        layout.container_mut(host).unwrap().locked = true;
        let edited = layout.contents.clone();
        let result = Workspace::new(&mut layout)
            .unwrap()
            .cancel_detach(&plan)
            .unwrap();
        assert_eq!(result.removed_containers, [plan.detached]);
        assert_eq!(layout.container(host).unwrap().tabs, ids);
        assert_eq!(layout.contents, edited);
        assert_eq!(layout.container(host).unwrap().geometry.x, 77.0);
        assert!(layout.container(host).unwrap().locked);
        layout.validate().unwrap();
    }
}

#[test]
fn stale_detach_inverse_cannot_clobber_subsequent_ownership_or_selection_changes() {
    for change in 0..4 {
        let (mut layout, host, ids) = group();
        let (_, plan) = Workspace::new(&mut layout)
            .unwrap()
            .detach_with_plan(ids[1], geometry(600.0))
            .unwrap();
        match change {
            0 => {
                Workspace::new(&mut layout)
                    .unwrap()
                    .attach(ids[1], host, 0)
                    .unwrap();
            }
            1 => {
                Workspace::new(&mut layout)
                    .unwrap()
                    .add(host, ContentInstance::collection("new", false))
                    .unwrap();
            }
            2 => {
                Workspace::new(&mut layout)
                    .unwrap()
                    .select(host, ids[0])
                    .unwrap();
            }
            _ => {
                Workspace::new(&mut layout)
                    .unwrap()
                    .add(plan.detached, ContentInstance::collection("new", false))
                    .unwrap();
            }
        }
        let before = layout.clone();
        assert_eq!(
            Workspace::new(&mut layout).unwrap().cancel_detach(&plan),
            Err(WorkspaceError::StaleDetach)
        );
        assert_eq!(layout, before);
    }
}

#[test]
fn singleton_detach_recreates_container_policy_not_hidden_content_geometry() {
    let mut layout = empty();
    let inbox = ContentInstance::collection("Inbox", true);
    let id = inbox.id;
    let host = Workspace::new(&mut layout)
        .unwrap()
        .create(inbox, geometry(12.0))
        .unwrap()
        .created_containers[0];
    layout.container_mut(host).unwrap().appearance = Some(AppearanceOverride {
        opacity: Some(0.7),
        ..AppearanceOverride::default()
    });
    let old_container = layout.container(host).unwrap().clone();
    let (transition, plan) = Workspace::new(&mut layout)
        .unwrap()
        .detach_with_plan(id, geometry(900.0))
        .unwrap();
    assert_eq!(transition.removed_containers, [host]);
    assert!(layout.container(host).is_none());
    assert!(
        layout
            .container(plan.detached)
            .unwrap()
            .appearance
            .is_none()
    );
    layout.content_mut(id).unwrap().title = "edited".into();
    Workspace::new(&mut layout)
        .unwrap()
        .cancel_detach(&plan)
        .unwrap();
    assert_eq!(layout.container(host).unwrap(), &old_container);
    assert_eq!(layout.content(id).unwrap().title, "edited");
}

#[test]
fn invalid_transition_is_atomic_and_does_not_overwrite_content_edits() {
    let (mut layout, host, ids) = group();
    layout.content_mut(ids[1]).unwrap().title = "intervening edit".into();
    let before = layout.clone();
    let mut workspace = Workspace::new(&mut layout).unwrap();
    assert!(matches!(
        workspace.detach(ids[1], geometry(f32::NAN)),
        Err(WorkspaceError::InvalidLayout(_))
    ));
    assert!(matches!(
        workspace.attach(ids[1], ContainerId::new(), 0),
        Err(WorkspaceError::MissingContainer(_))
    ));
    assert!(matches!(
        workspace.reorder(host, ids[1], 5),
        Err(WorkspaceError::InvalidIndex { .. })
    ));
    assert_eq!(workspace.delete(ids[0]), Err(WorkspaceError::InboxRequired));
    assert_eq!(workspace.layout(), &before);
}

#[test]
fn deletion_revokes_only_deleted_content_identity() {
    let (mut layout, host, ids) = group();
    let panel_before = layout.content(ids[2]).unwrap().clone();
    let result = Workspace::new(&mut layout).unwrap().delete(ids[1]).unwrap();
    assert_eq!(result.deleted_content, Some(ids[1]));
    assert_eq!(result.moved_content, None);
    assert!(result.removed_containers.is_empty());
    assert!(layout.content(ids[1]).is_none());
    assert_eq!(layout.content(ids[2]), Some(&panel_before));
    assert_eq!(layout.container(host).unwrap().tabs, [ids[0], ids[2]]);
    layout.validate().unwrap();
}

#[test]
fn settings_projection_resolves_container_appearance_and_content_view_separately() {
    let (mut layout, host, ids) = group();
    layout.container_mut(host).unwrap().appearance = Some(AppearanceOverride {
        opacity: Some(0.55),
        ..AppearanceOverride::default()
    });
    layout.container_mut(host).unwrap().auto_height = true;
    layout.content_mut(ids[1]).unwrap().view.sort = SortMode::Size;
    layout.content_mut(ids[1]).unwrap().view.icon_size = 64;
    let a = layout.project(ids[0]).unwrap();
    let b = layout.project(ids[1]).unwrap();
    assert_eq!(a.container_id, b.container_id);
    assert_eq!(a.appearance, b.appearance);
    assert!(a.auto_height && b.auto_height);
    assert_eq!(a.view.sort, SortMode::Manual);
    assert_eq!(b.view.sort, SortMode::Size);
    assert_eq!(b.view.icon_size, 64);
    assert_eq!(a.view.icon_size, 48);
    Workspace::new(&mut layout)
        .unwrap()
        .detach(ids[1], geometry(700.0))
        .unwrap();
    let moved = layout.project(ids[1]).unwrap();
    assert_eq!(moved.id, b.id);
    assert_eq!(moved.view, b.view);
    assert_ne!(moved.container_id, b.container_id);
    assert!(moved.appearance.is_none());
}

#[test]
fn validation_rejects_schema_ownership_identity_inbox_geometry_and_rule_target_errors() {
    let (layout, host, ids) = group();
    let valid = config(layout);
    valid.validate().unwrap();
    let mut cases = Vec::new();
    for schema in [0, 1, 3] {
        let mut bad = valid.clone();
        bad.schema_version = schema;
        cases.push(bad);
    }
    let mut bad = valid.clone();
    bad.layouts[0].containers[0].active_tab = ContentId::new();
    cases.push(bad);
    let mut bad = valid.clone();
    bad.layouts[0].containers[0].tabs.push(ids[0]);
    cases.push(bad);
    let mut bad = valid.clone();
    bad.layouts[0].containers[0].tabs.push(ContentId::new());
    cases.push(bad);
    let mut bad = valid.clone();
    bad.layouts[0].containers[0].tabs.remove(0);
    cases.push(bad);
    let mut bad = valid.clone();
    bad.layouts[0].contents[1].id = ids[0];
    cases.push(bad);
    let mut bad = valid.clone();
    bad.layouts[0].containers[0].id = ContainerId(ids[0].0);
    cases.push(bad);
    let mut bad = valid.clone();
    let duplicate_container = bad.layouts[0].container(host).unwrap().clone();
    bad.layouts[0].containers.push(duplicate_container);
    cases.push(bad);
    let mut bad = valid.clone();
    bad.layouts[0].contents[0].content = ContentSpec::FileCollection {
        inbox: false,
        items: vec![],
    };
    cases.push(bad);
    let mut bad = valid.clone();
    bad.layouts[0].contents[1].content = ContentSpec::FileCollection {
        inbox: true,
        items: vec![],
    };
    cases.push(bad);
    let mut bad = valid.clone();
    bad.layouts[0].containers[0].geometry.work_w = f32::INFINITY;
    cases.push(bad);
    let mut bad = valid.clone();
    bad.layouts[0].containers[0].expanded_h = f32::NAN;
    cases.push(bad);
    let mut bad = valid.clone();
    bad.layouts[0].containers[0].appearance = Some(AppearanceOverride {
        opacity: Some(f32::NAN),
        ..AppearanceOverride::default()
    });
    cases.push(bad);
    let mut bad = valid.clone();
    bad.rules.default_target = Target::Collection(ids[1]);
    cases.push(bad);
    let mut bad = valid.clone();
    bad.rules.default_target = Target::Collection(ContentId::new());
    cases.push(bad);
    let mut bad = valid.clone();
    bad.layouts[0].contents[0]
        .items_mut()
        .unwrap()
        .push(ItemRef {
            item_id: Uuid::new_v4(),
            manual_index: None,
            assigned_by: AssignedBy::User,
        });
    cases.push(bad);
    let mut bad = valid.clone();
    bad.layouts[0].contents[1].view.column_widths = Some([1.0, f32::NAN, 3.0]);
    cases.push(bad);
    for (index, bad) in cases.into_iter().enumerate() {
        assert!(bad.validate().is_err(), "invalid case {index} accepted");
    }
}

#[test]
fn legacy_layout_and_material_aliases_are_not_parsed() {
    assert!(serde_json::from_str::<Layout>(r#"{"fingerprint":[],"fences":[]}"#).is_err());
    assert!(serde_json::from_str::<Backdrop>(r#""micaLike""#).is_err());
    assert!(serde_json::from_str::<Backdrop>(r#""solid""#).is_err());
    let (layout, _, _) = group();
    let mut json = serde_json::to_value(layout).unwrap();
    json["contents"][0]["tabHost"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<Layout>(json).is_err());
}

#[test]
fn assignment_targets_collection_never_portal_or_container() {
    let (layout, _, ids) = group();
    let mut cfg = config(layout);
    let item_id = Uuid::new_v4();
    cfg.items.insert(
        item_id,
        Item {
            id: item_id,
            key: ItemKey::from_path(r"C:\Desktop\file.txt"),
            origin: Origin::UserDesktop,
            display_name: "file.txt".into(),
            file_id: None,
            mtime: 0,
            is_folder: false,
            attrs: 0,
            icon_key: IconKey::ByExt("txt".into()),
            orphaned_since: None,
            size: 1,
            open_count: 0,
            last_opened: None,
        },
    );
    assert!(cfg.assign(0, item_id, ids[1], AssignedBy::User).is_none());
    assert!(cfg.assign(0, item_id, ids[0], AssignedBy::User).is_some());
    let collection = ContentInstance::collection("Documents", false);
    let collection_id = collection.id;
    Workspace::new(&mut cfg.layouts[0])
        .unwrap()
        .create(collection, geometry(500.0))
        .unwrap();
    let rule_id = Uuid::new_v4();
    let assignment = cfg
        .assign(0, item_id, collection_id, AssignedBy::Rule(rule_id))
        .unwrap();
    assert_eq!(assignment.from, Some(ids[0]));
    assert_eq!(assignment.to, collection_id);
    assert_eq!(cfg.undo_log, [assignment]);
    assert!(cfg.layouts[0].content(ids[0]).unwrap().items().is_empty());
    cfg.rules.default_target = Target::Collection(collection_id);
    cfg.validate().unwrap();
}

#[test]
fn namespace_key_normalization_remains_source_independent() {
    assert!(ItemKey::from_path("::{ABCD}").is_namespace());
    assert!(!ItemKey::from_path(r"C:\files").is_namespace());
}

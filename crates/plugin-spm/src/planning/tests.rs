use super::*;
use crate::board::text::Heuristic;
fn view() -> DeliveryPlanView {
    serde_json::from_str(include_str!("../../fixtures/planning-demo.json")).unwrap()
}
fn q(rev: Option<u64>, offset: u32) -> GetDeliveryPlanRequest {
    request(
        &Selection {
            project_id: ProjectId::new("DEMO").unwrap(),
            delivery_scope_id: DeliveryScopeId::new("release-a").unwrap(),
            plan_id: "plan-a".into(),
        },
        rev,
        offset,
    )
}
#[test]
fn pages_publish_atomically_and_reject_changed_revision_scope_or_summary() {
    let mut first = view();
    first.tasks.truncate(1);
    first.resource_waits.clear();
    first.total_tasks = 2;
    first.next_offset = Some(1);
    let mut pages = Pages::default();
    assert!(!pages.accept(first.clone(), &q(None, 0)).unwrap());
    let mut second = first.clone();
    second.tasks[0].id = "test".into();
    second.offset = 1;
    second.next_offset = None;
    let mut wrong = second.clone();
    wrong.plan_revision = 2;
    assert!(pages.accept(wrong, &q(Some(1), 1)).is_err());
    let mut wrong = second.clone();
    wrong.finish = Some(25);
    assert!(pages.accept(wrong, &q(Some(1), 1)).is_err());
    let mut wrong = second.clone();
    wrong.delivery_scope_id = DeliveryScopeId::new("other").unwrap();
    assert!(pages.accept(wrong, &q(Some(1), 1)).is_err());
    assert!(pages.accept(second, &q(Some(1), 1)).unwrap());
    assert_eq!(pages.view.as_ref().unwrap().tasks.len(), 2);
}
#[test]
fn layout_retains_full_semantics_unknowns_and_scroll_bounds() {
    let mut v = view();
    v.tasks[0].start = None;
    v.tasks[0].finish = None;
    v.tasks[0].unscheduled_reason = Some("unknown_duration".into());
    let p = Palette::from_theme(&ThemeSnapshot::dark());
    for width in [320.0, 720.0] {
        for scroll in [0.0, 80.0] {
            let viewport = r(0.0, 0.0, width, 600.0);
            let snapshot = PlanSnapshot::from_page(v.clone());
            let (nodes, _) = layout(Some(&snapshot), viewport, scroll, &p, &Heuristic);
            assert!(nodes.iter().all(|n| n.rect.x >= 0.0
                && n.rect.x + n.rect.w <= width + 0.01
                && n.rect.y >= 0.0
                && n.rect.y + n.rect.h <= 600.01));
            assert!(nodes.iter().any(|n| n.name.contains("工期待确认")));
            assert!(nodes.iter().all(|n| !n.name.contains("已满足")));
        }
    }
}
#[test]
fn render_planning_in_light_dark_and_high_contrast() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    for (name, theme) in [
        ("planning-light", {
            let mut t = ThemeSnapshot::dark();
            t.paper.style = PaperStyle::Light;
            t
        }),
        ("planning-dark", ThemeSnapshot::dark()),
        ("planning-hc", {
            let mut t = ThemeSnapshot::dark();
            t.high_contrast = true;
            t
        }),
    ] {
        let p = Palette::from_theme(&theme);
        let (nodes, _) = layout(
            Some(&PlanSnapshot::from_page(view())),
            r(0.0, 0.0, 720.0, 680.0),
            0.0,
            &p,
            &Heuristic,
        );
        let scene = Scene::new(
            FrameIdentity {
                mount_key: MountKey {
                    instance: InstanceKey {
                        id: 1,
                        activation: 1,
                    },
                    generation: 1,
                },
                frame_revision: 1,
                layout_revision: 1,
                theme_epoch: 1,
                device_epoch: 1,
            },
            r(0.0, 0.0, 720.0, 680.0),
            nodes,
        )
        .unwrap();
        let mut canvas = pecofence_render::HeadlessCanvas::default();
        scene.paint(&mut canvas).unwrap();
        let pixmap = pecofence_render::raster::rasterize(canvas.commands(), 720, 680, p.base);
        pecofence_render::raster::assert_snapshot(&pixmap, &dir.join(format!("{name}.png")));
    }
}

#[test]
fn aggregate_over_two_hundred_tasks_keeps_wire_page_valid() {
    let mut first = view();
    let template = first.tasks[0].clone();
    first.resource_waits.clear();
    first.tasks = (0..200)
        .map(|i| {
            let mut t = template.clone();
            t.id = format!("task-{i}");
            t
        })
        .collect();
    first.total_tasks = 201;
    first.next_offset = Some(200);
    let mut pages = Pages::default();
    assert!(!pages.accept(first.clone(), &q(None, 0)).unwrap());
    let mut last = first;
    last.tasks = vec![template];
    last.offset = 200;
    last.next_offset = None;
    assert!(pages.accept(last, &q(Some(1), 200)).unwrap());
    let result = pages.view.unwrap();
    assert_eq!(result.tasks.len(), 201);
    result.page.validate().unwrap();
}

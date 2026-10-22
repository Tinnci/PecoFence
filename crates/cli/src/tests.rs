use super::*;
use pecofence_core::{
    Anchor, AssignedBy, Container, ContentId, IconKey, ItemKey, Layout, NormGeometry, Origin,
    SortMode,
};

const DAY: i64 = 86_400;
const NOW: i64 = 1_000 * DAY;

fn item(name: &str, opens: u32) -> Item {
    let path = format!(r"c:\users\a\desktop\{name}");
    Item {
        id: uuid_like(name),
        key: ItemKey::from_path(&path),
        origin: Origin::UserDesktop,
        display_name: name.rsplit_once('.').map_or(name, |(stem, _)| stem).into(),
        file_id: None,
        mtime: NOW - 2 * DAY,
        is_folder: false,
        attrs: 0,
        icon_key: IconKey::ByContent { path, mtime: 0 },
        orphaned_since: None,
        size: 0,
        open_count: opens,
        last_opened: (opens > 0).then_some(NOW - DAY),
    }
}

/// Deterministic, non-nil ids so the fixture is the same on every run.
fn uuid_like(seed: &str) -> pecofence_core::ItemId {
    let mut v: u128 = 0xcbf2_9ce4_8422_2325;
    for b in seed.bytes() {
        v = (v ^ u128::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    pecofence_core::ItemId::from_u128(v | 1)
}

struct Fixture {
    dir: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn fixture(name: &str) -> (Fixture, Config) {
    let dir = std::env::temp_dir().join(format!("pecofence-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = Config::default();
    let mut layout = Layout {
        fingerprint: Vec::new(),
        containers: Vec::new(),
        contents: Vec::new(),
    };
    let mut add = |title: &str, inbox: bool, files: Vec<(&str, u32, AssignedBy)>| {
        let mut content = ContentInstance::collection(title, inbox);
        content.id = ContentId::from_u128(
            0xabcd_0000_0000_0000_0000_0000_0000_0000 + layout.contents.len() as u128 + 1,
        );
        if let ContentSpec::FileCollection { items, .. } = &mut content.content {
            for (file, opens, by) in &files {
                let it = item(file, *opens);
                items.push(ItemRef {
                    item_id: it.id,
                    manual_index: None,
                    assigned_by: by.clone(),
                });
                config.items.insert(it.id, it);
            }
        }
        layout.containers.push(Container::new(
            content.id,
            NormGeometry {
                monitor: String::new(),
                x: 0.0,
                y: 0.0,
                w: 300.0,
                h: 200.0,
                work_w: 1920.0,
                work_h: 1040.0,
                anchor: Anchor::LeftTop,
            },
        ));
        layout.contents.push(content);
    };
    let user = || AssignedBy::User;
    let default = || AssignedBy::Default;
    add(
        "Desktop",
        true,
        vec![("draft.pdf", 0, default()), ("~$draft.docx", 0, default())],
    );
    add(
        "Work",
        false,
        vec![
            ("a.pdf", 0, user()),
            ("b.pdf", 0, user()),
            ("c.pdf", 0, user()),
        ],
    );
    add(
        "Apps",
        false,
        vec![
            ("Code.lnk", 50, default()),
            ("Chrome.lnk", 30, default()),
            ("Paint.lnk", 1, default()),
            ("Calc.lnk", 1, default()),
            ("Maps.lnk", 0, default()),
            ("Mail.lnk", 0, default()),
            ("Notes.lnk", 0, default()),
        ],
    );
    config.layouts.push(layout);
    config.validate().expect("fixture is a valid workspace");
    ConfigStore::export_to(&config, &ConfigStore::new(&dir).primary_path()).unwrap();
    (Fixture { dir }, config)
}

/// No processes: nothing alive, nothing to quit.
fn quiet(var: Box<dyn Fn(&str) -> Option<String>>) -> Env {
    Env {
        var,
        exe_dir: None,
        alive: Box::new(|_, _| Some(false)),
        request_quit: Box::new(|_| None),
        wait_exit: Box::new(|_, _| Some(true)),
    }
}

fn env_for(dir: Option<&Path>) -> Env {
    let workspace = dir.map(|d| d.display().to_string());
    quiet(Box::new(move |name| {
        (name == "PECOFENCE_WORKSPACE")
            .then(|| workspace.clone())
            .flatten()
    }))
}

fn cli(args: &str, env: &Env) -> (u8, String, String) {
    let args: Vec<String> = args.split_whitespace().map(String::from).collect();
    main_with(&args, env, NOW)
}

fn envelope(text: &str) -> Value {
    serde_json::from_str(text.trim()).expect("one JSON envelope")
}

#[test]
fn suggest_ranks_and_explains_learned_rules_frequent_order_and_skips_temp_files() {
    let (fx, _) = fixture("suggest");
    let env = env_for(Some(&fx.dir));
    let (code, out, err) = cli("suggest --json", &env);
    assert_eq!((code, err.as_str()), (0, ""));
    let v = envelope(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["meta"]["readOnly"], true);
    assert_eq!(v["meta"]["workspaceSource"], "PECOFENCE_WORKSPACE");
    let ids: Vec<&str> = v["data"]["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert!(ids.iter().any(|id| id.starts_with("rule:pdf:")), "{ids:?}");
    assert!(ids.iter().any(|id| id.ends_with(":frequent")), "{ids:?}");
    // draft.pdf is covered by the rule; the Office lock file is never moved.
    assert!(!ids.iter().any(|id| id.starts_with("move:")), "{ids:?}");
    let rule = &v["data"]["suggestions"][ids.iter().position(|i| i.starts_with("rule:")).unwrap()];
    assert_eq!(rule["affects"], 1);
    assert_eq!(rule["examples"][0], "draft");
    assert_eq!(rule["action"]["type"], "addRule");

    let (_, human, _) = cli("suggest", &env);
    assert!(
        human.contains("why: 3 of 3 .pdf items were placed there by hand"),
        "{human}"
    );
    assert!(human.contains("right-click the fence → Suggestions"));
}

#[test]
fn suggest_filters_by_fence_and_kind_and_reports_truncation() {
    let (fx, _) = fixture("filter");
    let env = env_for(Some(&fx.dir));
    let (_, out, _) = cli("suggest --fence apps --kind sort --json", &env);
    let v = envelope(&out);
    let list = v["data"]["suggestions"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["action"]["sort"], "openCount");
    let (_, out, _) = cli("suggest --limit 1 --json", &env);
    let v = envelope(&out);
    assert_eq!(v["data"]["suggestions"].as_array().unwrap().len(), 1);
    assert_eq!(v["meta"]["truncated"], true);
    // A fence with nothing to suggest is an empty, successful result with an explanation.
    let (code, human, _) = cli("suggest --fence Work --kind move", &env);
    assert_eq!(code, 0);
    assert!(human.starts_with("No suggestions"), "{human}");
}

#[test]
fn order_previews_without_changing_the_fence() {
    let (fx, config) = fixture("order");
    let env = env_for(Some(&fx.dir));
    let (code, out, _) = cli("order --fence Apps --by frequent --limit 3 --json", &env);
    assert_eq!(code, 0);
    let v = envelope(&out);
    let names: Vec<&str> = v["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Code", "Chrome", "Calc"]);
    assert_eq!(v["data"]["preview"], true);
    assert_eq!(v["meta"]["total"], 7);
    assert_eq!(v["data"]["items"][0]["detail"]["opens"], 50);
    let saved = ConfigStore::parse_file(&ConfigStore::new(&fx.dir).primary_path()).unwrap();
    assert_eq!(saved.layouts[0].contents[2].view.sort, SortMode::Manual);
    assert_eq!(saved.items.len(), config.items.len());
}

#[test]
fn fences_lists_kinds_counts_and_sort_keys() {
    let (fx, _) = fixture("fences");
    let env = env_for(Some(&fx.dir));
    let (_, out, _) = cli("fences --json", &env);
    let v = envelope(&out);
    let fences = v["data"]["fences"].as_array().unwrap();
    assert_eq!(fences.len(), 3);
    assert_eq!(fences[0]["kind"], "inbox");
    assert_eq!(fences[2]["items"], 7);
    assert_eq!(fences[2]["sort"], "manual");
    let (_, human, _) = cli(
        &format!("fences --workspace {}", fx.dir.display()),
        &env_for(None),
    );
    assert!(human.lines().next().unwrap().starts_with("ID"));
    assert!(human.contains("Apps"));
}

#[test]
fn argument_errors_exit_2_with_a_next_step() {
    let env = env_for(None);
    for (args, needle) in [
        ("", "missing command"),
        ("sort", "unknown command"),
        ("suggest --limit 0", "--limit must be"),
        ("suggest --kind everything", "unknown --kind"),
        ("order", "order needs --fence"),
        ("fences --by name", "does not apply"),
        ("order --fence x --by opened", "unknown --by"),
        ("suggest --fence", "needs a value"),
    ] {
        let (code, out, err) = cli(args, &env);
        assert_eq!(code, 2, "{args}");
        assert!(out.is_empty());
        assert!(
            err.contains(needle) && err.contains("Next: "),
            "{args}: {err}"
        );
    }
    let (code, _, err) = cli("suggest --limit 0 --json", &env);
    assert_eq!(code, 2);
    let v = envelope(&err);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "INVALID_ARGUMENT");
    assert_eq!(v["error"]["retryable"], false);
}

#[test]
fn unknown_or_ambiguous_fences_name_the_candidates() {
    let (fx, _) = fixture("fence-errors");
    let env = env_for(Some(&fx.dir));
    let (code, _, err) = cli("order --fence Games --json", &env);
    assert_eq!(code, 2);
    assert_eq!(envelope(&err)["error"]["code"], "FENCE_NOT_FOUND");
    // Same-titled fences are told apart by id.
    let path = ConfigStore::new(&fx.dir).primary_path();
    let mut config = ConfigStore::parse_file(&path).unwrap();
    config.layouts[0].contents[1].title = "Apps".into();
    ConfigStore::export_to(&config, &path).unwrap();
    let (code, _, err) = cli("order --fence Apps", &env);
    assert_eq!(code, 2);
    assert!(
        err.contains("matches 2 fences") && err.contains("fence id"),
        "{err}"
    );
}

#[test]
fn missing_or_invalid_workspaces_exit_1_and_say_what_to_do() {
    let dir = std::env::temp_dir().join(format!("pecofence-cli-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (code, _, err) = cli(
        &format!("fences --json --workspace {}", dir.display()),
        &env_for(None),
    );
    assert_eq!(code, 1);
    let v = envelope(&err);
    assert_eq!(v["error"]["code"], "WORKSPACE_UNAVAILABLE");
    assert!(
        v["error"]["suggested_action"]
            .as_str()
            .unwrap()
            .contains("--workspace")
    );

    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("workspace.v2.json"), "{\"schemaVersion\": 1}").unwrap();
    let (code, _, err) = cli(
        &format!("fences --workspace {}", dir.display()),
        &env_for(None),
    );
    assert_eq!(code, 1);
    assert!(
        err.contains("unsupported workspace schema") && err.contains("backups"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);

    let none = quiet(Box::new(|_| None));
    let (code, _, err) = cli("fences", &none);
    assert_eq!(code, 1);
    assert!(err.contains("APPDATA"));
}

#[test]
fn layout_out_of_range_is_an_argument_error() {
    let (fx, _) = fixture("layout");
    let (code, _, err) = cli("fences --layout 3", &env_for(Some(&fx.dir)));
    assert_eq!(code, 2);
    assert!(err.contains("1 layout(s)"), "{err}");
}

#[test]
fn help_and_version_need_no_workspace() {
    let env = env_for(None);
    let (code, out, _) = cli("--help", &env);
    assert_eq!(code, 0);
    assert!(out.contains("EXAMPLES") && out.contains("Exit codes"));
    let (code, out, _) = cli("-V", &env);
    assert_eq!(code, 0);
    assert!(out.starts_with("pecofence-cli "));
}

mod lifetime {
    use super::*;
    use pecofence_core::session::{self, ExitKind, Session, State};

    struct Local {
        root: PathBuf,
    }

    impl Drop for Local {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// A fake %LOCALAPPDATA% with `record` stored as the session.
    fn appdata(name: &str, record: Option<&Session>) -> Local {
        let root =
            std::env::temp_dir().join(format!("pecofence-cli-life-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("PecoFence")).unwrap();
        if let Some(record) = record {
            session::store(&session::path(&root.join("PecoFence"), None), record).unwrap();
        }
        Local { root }
    }

    fn record(state: State) -> Session {
        let mut record = Session::running(
            None,
            4242,
            Path::new(r"C:\PecoFence\pecofence.exe"),
            Path::new(r"C:\missing\config"),
            NOW - 3_700,
        );
        record.state = state;
        record.watchdog_pid = Some(77);
        record
    }

    fn env(local: &Local, alive: Option<bool>) -> Env {
        let root = local.root.display().to_string();
        let mut env = quiet(Box::new(move |name| {
            (name == "LOCALAPPDATA").then(|| root.clone())
        }));
        env.alive = Box::new(move |_, _| alive);
        env
    }

    fn status(env: &Env) -> Value {
        let (code, out, err) = cli("status --json", env);
        assert_eq!((code, err.as_str()), (0, ""));
        envelope(&out)["data"].clone()
    }

    #[test]
    fn status_reports_a_live_session_with_its_watchdog() {
        let local = appdata("live", Some(&record(State::Running)));
        let data = status(&env(&local, Some(true)));
        assert_eq!(data["state"], "running");
        assert_eq!(data["uptimeSeconds"], 3_700);
        assert_eq!(data["watchdog"]["alive"], true);
        assert!(data["exit"].is_null());
        let (_, text, _) = cli("status", &env(&local, Some(true)));
        assert!(text.starts_with("PecoFence is running (pid 4242"), "{text}");
        assert!(
            text.contains("up 1 h 1 min") && text.contains("watching"),
            "{text}"
        );
    }

    #[test]
    fn an_open_record_without_its_process_ended_unexpectedly() {
        let local = appdata("gone", Some(&record(State::Closing)));
        let data = status(&env(&local, Some(false)));
        assert_eq!(data["state"], "endedUnexpectedly");
        assert!(data["uptimeSeconds"].is_null());
        // Where processes cannot be checked, say so rather than guess.
        assert_eq!(status(&env(&local, None))["state"], "unverified");
    }

    #[test]
    fn closed_records_explain_how_the_app_ended() {
        let mut exited = record(State::Exited);
        exited.exit = Some(ExitKind::DiscardedChanges);
        exited.unsaved = true;
        exited.ended_at = Some(NOW - 120);
        let local = appdata("exited", Some(&exited));
        let data = status(&env(&local, Some(true)));
        assert_eq!(
            data["state"], "exited",
            "a closed record never consults the process"
        );
        assert_eq!(data["exit"], "discardedChanges");
        assert_eq!(data["unsaved"], true);
        let (_, text, _) = cli("status", &env(&local, None));
        assert!(
            text.contains("exited 2 min ago") && text.contains("not saved"),
            "{text}"
        );

        let mut crashed = record(State::Crashed);
        crashed.exit_code = Some(0xC000_0005);
        crashed.ended_at = Some(NOW - 5);
        let local = appdata("crashed", Some(&crashed));
        let data = status(&env(&local, None));
        assert_eq!(data["state"], "crashed");
        assert!(data["exit"].is_null(), "a crash has no exit kind");
        assert_eq!(data["exitCodeHex"], "0xC0000005");
    }

    #[test]
    fn missing_and_unreadable_records() {
        let local = appdata("none", None);
        assert_eq!(status(&env(&local, None))["state"], "noRecord");
        std::fs::write(local.root.join("PecoFence").join("session.json"), "{").unwrap();
        let (code, _, err) = cli("status --json", &env(&local, None));
        assert_eq!(code, 1);
        assert_eq!(envelope(&err)["error"]["code"], "SESSION_INVALID");
        let (code, _, err) = cli("status", &quiet(Box::new(|_| None)));
        assert_eq!(code, 1);
        assert!(err.contains("LOCALAPPDATA"), "{err}");
    }

    #[test]
    fn quit_is_idempotent_and_reports_how_the_app_exited() {
        let local = appdata("quit", Some(&record(State::Running)));
        let (code, out, _) = cli("quit --json", &env(&local, Some(true)));
        assert_eq!(code, 0, "nothing running is already the goal");
        assert_eq!(envelope(&out)["data"]["state"], "notRunning");

        // The app answers the request by saving, recording its exit and ending.
        let record_path = session::path(&local.root.join("PecoFence"), None);
        let mut env = env(&local, Some(true));
        env.request_quit = Box::new(move |title| {
            assert_eq!(title, "PecoFence.Control.main");
            let mut done = session::load(&record_path).unwrap();
            done.state = State::Exited;
            done.exit = Some(ExitKind::Normal);
            session::store(&record_path, &done).unwrap();
            Some(4242)
        });
        let (code, out, _) = cli("quit --json", &env);
        assert_eq!(code, 0);
        let v = envelope(&out);
        assert_eq!(v["data"]["exit"], "normal");
        assert_eq!(v["meta"]["forceKill"], false);

        // Still closing (a save-failure dialog open): report, never force.
        env.wait_exit = Box::new(|pid, millis| {
            assert_eq!((pid, millis), (4242, 5_000));
            Some(false)
        });
        let (code, _, err) = cli("quit --wait 5 --json", &env);
        assert_eq!(code, 1);
        let error = &envelope(&err)["error"];
        assert_eq!(error["code"], "QUIT_TIMEOUT");
        assert!(
            error["suggested_action"]
                .as_str()
                .unwrap()
                .contains("never force")
        );
    }

    #[test]
    fn lifetime_options_are_checked() {
        let env = quiet(Box::new(|_| None));
        for (args, needle) in [
            ("quit --wait 999", "--wait must be"),
            ("status --wait 3", "does not apply"),
            ("fences --instance lab", "does not apply"),
            ("status --workspace x", "does not apply"),
        ] {
            let (code, _, err) = cli(args, &env);
            assert_eq!(code, 2, "{args}");
            assert!(err.contains(needle), "{args}: {err}");
        }
    }
}

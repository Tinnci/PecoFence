use pecofence_plugin_api::{PanelConfig, PanelProvider};
use pecofence_plugin_spm::{board, ci_watch, issues, planning};
fn config(value: serde_json::Value) -> PanelConfig {
    PanelConfig {
        version: 1,
        bytes: serde_json::to_vec(&value).unwrap().into(),
    }
}
#[test]
fn release_providers_accept_projectless_live_configs() {
    for (provider, value) in [
        (
            &board::BoardPlugin as &dyn PanelProvider,
            board::live_config().1,
        ),
        (
            &issues::IssuesPlugin as &dyn PanelProvider,
            issues::live_config().1,
        ),
    ] {
        provider.validate(&config(value)).unwrap();
    }
    assert!(
        issues::IssuesPlugin
            .validate(&config(serde_json::json!({"demo": false})))
            .is_err()
    );
}
#[cfg(not(feature = "demo-data"))]
#[test]
fn normal_builds_require_developer_opt_in_for_demo_data() {
    for (provider, value) in [
        (
            &board::BoardPlugin as &dyn PanelProvider,
            board::demo_config().1,
        ),
        (
            &issues::IssuesPlugin as &dyn PanelProvider,
            issues::demo_config().1,
        ),
        (
            &ci_watch::CiWatchPlugin as &dyn PanelProvider,
            ci_watch::demo_config().1,
        ),
        (
            &planning::PlanningPlugin as &dyn PanelProvider,
            planning::demo_config().1,
        ),
    ] {
        assert!(provider.validate(&config(value)).is_err());
    }
}

//! Pure schema verification for lab seeds and interaction fixtures; never starts a native host.

use pecofence_core::{
    Anchor, Config, ConfigStore, Container, ContentInstance, Layout, NormGeometry, PeekSettings,
    QuickHideSettings, Settings,
};
use std::process::ExitCode;

fn sample() -> Config {
    let content = ContentInstance::collection("Desktop", true);
    let container = Container::new(
        content.id,
        NormGeometry {
            monitor: String::new(),
            x: 16.0,
            y: 16.0,
            w: 400.0,
            h: 300.0,
            work_w: 1920.0,
            work_h: 1040.0,
            anchor: Anchor::LeftTop,
        },
    );
    Config {
        settings: Settings {
            autostart: false,
            hide_real_icons: false,
            peek: PeekSettings {
                enabled: false,
                ..PeekSettings::default()
            },
            quick_hide: QuickHideSettings {
                enabled: false,
                ..QuickHideSettings::default()
            },
            ..Settings::default()
        },
        layouts: vec![Layout {
            fingerprint: Vec::new(),
            containers: vec![container],
            contents: vec![content],
        }],
        ..Config::default()
    }
}

fn main() -> ExitCode {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [argument] = arguments.as_slice() else {
        eprintln!("usage: validate_workspace <workspace.v2.json> | --sample");
        return ExitCode::from(2);
    };
    if argument == "--sample" {
        let document = sample();
        if let Err(error) = document.validate() {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
        println!("{}", serde_json::to_string_pretty(&document).unwrap());
        return ExitCode::SUCCESS;
    }
    match ConfigStore::parse_file(std::path::Path::new(argument)) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_has_valid_independent_roles_and_no_desktop_side_effects() {
        let config = sample();
        config.validate().unwrap();
        let layout = &config.layouts[0];
        assert_ne!(
            layout.containers[0].id.as_uuid(),
            layout.contents[0].id.as_uuid()
        );
        assert!(!config.settings.autostart);
        assert!(!config.settings.hide_real_icons);
        assert!(!config.settings.peek.enabled);
        assert!(!config.settings.quick_hide.enabled);
    }
}

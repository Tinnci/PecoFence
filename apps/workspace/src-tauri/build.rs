fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "load_hub",
            "launch_target",
            "first_frame",
            "template_plan",
            "template_apply",
            "project_status",
        ]),
    ))
    .expect("Tauri build configuration");
}

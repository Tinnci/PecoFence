fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // The Windows resource embeds icons/icon.ico; rebuild it when the icons change.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "load_hub",
            "launch_target",
            "first_frame",
            "template_plan",
            "template_apply",
            "project_status",
            "connections",
            "connection_sign_in",
            "discover",
            "draft",
            "activate",
            "service_status",
            "remap",
            "focus",
            "save_png",
        ]),
    ))
    .expect("Tauri build configuration");
}

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../../third_party/winappsdk/app.manifest");
    let common_controls = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("common-controls.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed={}", common_controls.display());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    assert!(
        manifest.is_file(),
        "missing audited self-contained WinUI activation manifest"
    );
    // Embed in the binary AND app test executables: the self-contained runtime
    // must never fall back to registering/installing a framework package.
    match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
        Ok("msvc") => {
            println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
            println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
            // The native close observer requires the v6 common-controls
            // subclass helper, not Windows' legacy v5 compatibility export.
            println!(
                "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
                common_controls.display()
            );
        }
        _ => panic!("PecoFence WinUI deployment currently supports the MSVC toolchain"),
    }
}

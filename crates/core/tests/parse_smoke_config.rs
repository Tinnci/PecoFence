use pecofence_core::Config;

#[test]
fn parse_smoke_config() {
    // Portable smoke check: only validates when the local smoke config exists
    // (the hardcoded maintainer path is absent on CI and other machines).
    let Ok(data) = std::fs::read("C:/Users/Administrator/PecoFence-smoke/config/config.json")
    else {
        return;
    };
    match serde_json::from_slice::<Config>(&data) {
        Ok(config) => {
            println!("PARSE OK: fences={}", config.layouts[0].fences.len());
            for fence in &config.layouts[0].fences {
                println!(
                    "  fence {} kind={:?} content={:?}",
                    fence.id, fence.kind, fence.content
                );
            }
        }
        Err(error) => panic!("config parse failed: {error}"),
    }
}

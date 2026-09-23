use pecofence_core::Config;

#[test]
fn parse_smoke_config() {
    let data = std::fs::read("C:/Users/Administrator/PecoFence-smoke/config/config.json")
        .expect("config readable");
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

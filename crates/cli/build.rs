use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--git-path", "HEAD"])
        .output()
    {
        if output.status.success() {
            let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !head.is_empty() {
                println!("cargo:rerun-if-changed={head}");
                if let Ok(contents) = std::fs::read_to_string(&head) {
                    if let Some(reference) = contents.trim().strip_prefix("ref: ") {
                        if let Ok(ref_out) = Command::new("git")
                            .args(["rev-parse", "--git-path", reference])
                            .output()
                        {
                            if ref_out.status.success() {
                                let ref_path =
                                    String::from_utf8_lossy(&ref_out.stdout).trim().to_string();
                                if !ref_path.is_empty() {
                                    println!("cargo:rerun-if-changed={ref_path}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let run_git = |args: &[&str]| -> Option<String> {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };

    let sha = run_git(&["rev-parse", "--short=7", "HEAD"]).or_else(|| {
        std::env::var("GITHUB_SHA")
            .ok()
            .map(|s| s.chars().take(7).collect::<String>())
            .filter(|s| !s.is_empty())
    });
    let rev = run_git(&["rev-list", "--count", "HEAD"]);
    let pkg = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".into());

    let display = match (&rev, &sha) {
        (Some(r), Some(s)) => format!("{pkg} (r{r} · {s})"),
        (None, Some(s)) => format!("{pkg} ({s})"),
        _ => pkg,
    };

    println!(
        "cargo:rustc-env=PECOFENCE_GIT_SHA={}",
        sha.unwrap_or_default()
    );
    println!(
        "cargo:rustc-env=PECOFENCE_GIT_REV={}",
        rev.unwrap_or_default()
    );
    println!("cargo:rustc-env=PECOFENCE_DISPLAY_VERSION={display}");
}

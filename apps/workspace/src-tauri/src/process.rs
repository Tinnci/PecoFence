//! Fixed owner command, bounded pipes and one deadline; no shell or frontend argv.
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

const LIMIT: usize = 4 * 1024 * 1024;
async fn read(mut stream: impl AsyncRead + Unpin) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    (&mut stream)
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut output)
        .await
        .map_err(|_| "hub_read_failed")?;
    if output.len() > LIMIT {
        return Err("hub_output_limit".into());
    }
    Ok(output)
}

pub async fn run(program: &str, args: &[&str], deadline: Duration) -> Result<Vec<u8>, String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().map_err(|_| "hub_unavailable")?;
    #[cfg(unix)]
    let _group = ProcessGroup(child.id().ok_or("hub_unavailable")?);
    let stdout = child.stdout.take().ok_or("hub_read_failed")?;
    let stderr = child.stderr.take().ok_or("hub_read_failed")?;
    let outcome = tokio::time::timeout(deadline, async {
        let (out, _, status) = tokio::try_join!(read(stdout), read(stderr), async {
            child
                .wait()
                .await
                .map_err(|_| "hub_wait_failed".to_string())
        })?;
        // Exit 1 can contain a complete hub report describing invalid manifests.
        if status.code().is_some_and(|code| code == 0 || code == 1) {
            Ok(out)
        } else {
            Err("hub_unavailable".into())
        }
    })
    .await;
    match outcome {
        Ok(Ok(bytes)) => Ok(bytes),
        failure => {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
            match failure {
                Ok(Err(error)) => Err(error),
                _ => Err("hub_timeout".into()),
            }
        }
    }
}
#[cfg(unix)]
struct ProcessGroup(u32);
#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        // Every spawned process gets its own group; reap descendants on all exits.
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
    }
}

pub async fn hub(check: bool) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        let binary = match std::env::var("TINNCI_DEV_CLI_WSL") {
            Ok(path) if path.starts_with('/') && !path.chars().any(char::is_control) => path,
            Ok(_) => return Err("hub_invalid_install_path".into()),
            Err(_) => {
                let bytes = run(
                    "wsl.exe",
                    &["--exec", "/usr/bin/printenv", "HOME"],
                    Duration::from_secs(5),
                )
                .await?;
                let home = String::from_utf8(bytes).map_err(|_| "hub_invalid_install_path")?;
                let home = home.trim_end_matches(['\r', '\n']);
                if !home.starts_with('/') || home.chars().any(char::is_control) {
                    return Err("hub_invalid_install_path".into());
                }
                format!("{home}/.local/bin/dev-cli")
            }
        };
        let mut args = vec!["--exec", binary.as_str(), "hub", "--json"];
        if check {
            args.push("--check");
        }
        run("wsl.exe", &args, Duration::from_secs(65)).await
    }
    #[cfg(not(windows))]
    {
        let args: &[&str] = if check {
            &["hub", "--json", "--check"]
        } else {
            &["hub", "--json"]
        };
        run("dev-cli", args, Duration::from_secs(65)).await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[tokio::test]
    async fn bounds_output_and_deadline_and_preserves_partial_report() {
        assert_eq!(
            run(
                "/usr/bin/printf",
                &["literal;$HOME"],
                Duration::from_secs(2)
            )
            .await
            .unwrap(),
            b"literal;$HOME"
        );
        assert_eq!(
            run("/usr/bin/sleep", &["2"], Duration::from_millis(20))
                .await
                .unwrap_err(),
            "hub_timeout"
        );
        assert_eq!(
            run(
                "/usr/bin/head",
                &["-c", "4194305", "/dev/zero"],
                Duration::from_secs(2)
            )
            .await
            .unwrap_err(),
            "hub_output_limit"
        );
    }
}

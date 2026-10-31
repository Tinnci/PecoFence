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

#[cfg(any(windows, test))]
fn wsl_environment(bytes: &[u8]) -> Result<(String, String), String> {
    if bytes.len() > 65536 {
        return Err("hub_invalid_install_path".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "hub_invalid_install_path")?;
    let mut lines = text.lines();
    let home = lines.next().ok_or("hub_invalid_install_path")?;
    let path = lines.next().ok_or("hub_invalid_install_path")?;
    if lines.next().is_some()
        || !home.starts_with('/')
        || home.chars().any(char::is_control)
        || path.chars().any(char::is_control)
    {
        return Err("hub_invalid_install_path".into());
    }
    Ok((home.to_owned(), path.to_owned()))
}

pub async fn hub(check: bool, component: Option<&str>) -> Result<Vec<u8>, String> {
    if component.is_some_and(|id| !crate::hub::valid_component(id)) {
        return Err("hub_invalid_component".into());
    }
    #[cfg(windows)]
    {
        let bytes = run(
            "wsl.exe",
            &["--exec", "/usr/bin/printenv", "HOME", "PATH"],
            Duration::from_secs(5),
        )
        .await?;
        let (home, inherited_path) = wsl_environment(&bytes)?;
        let binary = match std::env::var("TINNCI_DEV_CLI_WSL") {
            Ok(path) if path.starts_with('/') && !path.chars().any(char::is_control) => path,
            Ok(_) => return Err("hub_invalid_install_path".into()),
            Err(_) => format!("{home}/.local/bin/dev-cli"),
        };
        let bin_dir = binary
            .rsplit_once('/')
            .map(|(dir, _)| dir)
            .filter(|dir| !dir.is_empty())
            .unwrap_or("/");
        // Non-interactive WSL omits ~/.local/bin. Supply a process-local PATH
        // so discovery and the owner's subprocesses resolve the installed tools.
        let path = format!("PATH={bin_dir}:{home}/.local/bin:{inherited_path}");
        let mut args = vec![
            "--exec",
            "/usr/bin/env",
            path.as_str(),
            binary.as_str(),
            "hub",
            "--json",
        ];
        if check {
            args.push("--check");
        }
        if let Some(id) = component {
            args.extend(["--component", id]);
        }
        run("wsl.exe", &args, Duration::from_secs(65)).await
    }
    #[cfg(not(windows))]
    {
        let mut args = vec!["hub", "--json"];
        if check {
            args.push("--check");
        }
        if let Some(id) = component {
            args.extend(["--component", id]);
        }
        run("dev-cli", &args, Duration::from_secs(65)).await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wsl_environment_preserves_spaces_and_rejects_extra_records() {
        assert_eq!(
            wsl_environment("/home/用户 name\n/usr/bin:/mnt/c/Program Files/Tools\n".as_bytes())
                .unwrap(),
            (
                "/home/用户 name".into(),
                "/usr/bin:/mnt/c/Program Files/Tools".into()
            )
        );
        for bad in [
            b"relative\n/bin\n".as_slice(),
            b"/home/user\n/bin\nextra\n",
            b"/home/user\n/bin\0secret\n",
            b"/home/user\n",
        ] {
            assert!(wsl_environment(bad).is_err());
        }
    }
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

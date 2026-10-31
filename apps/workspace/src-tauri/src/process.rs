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

pub struct Output {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
}

async fn capture(
    program: impl AsRef<std::ffi::OsStr>,
    args: &[&str],
    deadline: Duration,
) -> Result<Output, String> {
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
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "executable_missing"
        } else {
            "hub_unavailable"
        }
    })?;
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
        Ok(Output {
            code: status.code(),
            stdout: out,
        })
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
/// Runs the located SPM owner CLI with literal arguments.
#[cfg(windows)]
pub async fn spm(
    program: &std::path::Path,
    args: &[&str],
    deadline: Duration,
) -> Result<Output, crate::setup::SetupFailure> {
    capture(program, args, deadline).await.map_err(|code| {
        crate::setup::SetupFailure::new(match code.as_str() {
            "executable_missing" => "spm_missing",
            "hub_timeout" => "spm_timeout",
            _ => "spm_unavailable",
        })
    })
}
#[cfg(not(windows))]
pub async fn run(program: &str, args: &[&str], deadline: Duration) -> Result<Vec<u8>, String> {
    let output = capture(program, args, deadline).await?;
    report_output(output)
}
fn report_output(output: Output) -> Result<Vec<u8>, String> {
    // Exit 1 may carry a complete report describing invalid manifests.
    if matches!(output.code, Some(0 | 1)) {
        Ok(output.stdout)
    } else {
        Err(if output.code == Some(2) {
            "hub_update_required"
        } else {
            "hub_unavailable"
        }
        .into())
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
        tokio::time::timeout(
            Duration::from_secs(65),
            windows_hub(
                check,
                component,
                std::env::var("TINNCI_DEV_CLI_WSL").ok(),
                |args, deadline| async move {
                    let args: Vec<_> = args.iter().map(String::as_str).collect();
                    capture("wsl.exe", &args, deadline).await
                },
            ),
        )
        .await
        .map_err(|_| "hub_timeout".to_string())?
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
        let help = capture("dev-cli", &["hub", "--help"], Duration::from_secs(5))
            .await
            .map_err(|code| {
                if code == "executable_missing" {
                    "cli_missing".into()
                } else {
                    code
                }
            })?;
        compatible_cli(&help)?;
        run("dev-cli", &args, Duration::from_secs(65)).await
    }
}
#[cfg(any(windows, test))]
async fn windows_hub<F, Fut>(
    check: bool,
    component: Option<&str>,
    override_path: Option<String>,
    mut execute: F,
) -> Result<Vec<u8>, String>
where
    F: FnMut(Vec<String>, Duration) -> Fut,
    Fut: std::future::Future<Output = Result<Output, String>>,
{
    let listing = execute(
        vec!["--list".into(), "--all".into(), "--quiet".into()],
        Duration::from_secs(5),
    )
    .await
    .map_err(|code| -> String {
        match code.as_str() {
            "executable_missing" => "wsl_missing".into(),
            "hub_timeout" => "wsl_timeout".to_owned(),
            _ => "wsl_unavailable".into(),
        }
    })?;
    if listing.code != Some(0) {
        return Err("wsl_unavailable".into());
    }
    if listing.stdout.len() > 65536 {
        return Err("wsl_unavailable".into());
    }
    // WSL's distribution listing is UTF-16LE on Windows; some versions emit UTF-8.
    // Some WSL versions return an empty listing without a console even when
    // distributions exist. Successful launch takes precedence over that listing.
    let has_distribution = listing
        .stdout
        .iter()
        .any(|c| !matches!(c, 0 | b' ' | b'\r' | b'\n' | b'\t' | 0xff | 0xfe));
    let environment = execute(
        vec![
            "--exec".into(),
            "/usr/bin/printenv".into(),
            "HOME".into(),
            "PATH".into(),
        ],
        Duration::from_secs(10),
    )
    .await
    .map_err(|code| {
        if code == "hub_timeout" {
            "wsl_timeout".to_owned()
        } else {
            "wsl_distribution_unavailable".to_owned()
        }
    })?;
    if environment.code != Some(0) {
        return Err(if has_distribution {
            "wsl_distribution_unavailable"
        } else {
            "wsl_distribution_missing"
        }
        .into());
    }
    let (home, inherited_path) = wsl_environment(&environment.stdout)?;
    let binary = match override_path {
        Some(path) if path.starts_with('/') && !path.chars().any(char::is_control) => path,
        Some(_) => return Err("hub_invalid_install_path".into()),
        None => format!("{home}/.local/bin/dev-cli"),
    };
    let executable = execute(
        vec![
            "--exec".into(),
            "/usr/bin/test".into(),
            "-x".into(),
            binary.clone(),
        ],
        Duration::from_secs(5),
    )
    .await?;
    if executable.code != Some(0) {
        return Err("cli_missing".into());
    }
    let bin_dir = binary
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .filter(|dir| !dir.is_empty())
        .unwrap_or("/");
    let path = format!("PATH={bin_dir}:{home}/.local/bin:{inherited_path}");
    let mut args = vec![
        "--exec".into(),
        "/usr/bin/env".into(),
        path,
        binary,
        "hub".into(),
    ];
    let mut help = args.clone();
    help.push("--help".into());
    compatible_cli(&execute(help, Duration::from_secs(5)).await?)?;
    args.push("--json".into());
    if check {
        args.push("--check".into());
    }
    if let Some(id) = component {
        args.extend(["--component".into(), id.into()]);
    }
    report_output(execute(args, Duration::from_secs(65)).await?)
}

fn compatible_cli(output: &Output) -> Result<(), String> {
    match output.code {
        Some(0)
            if String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .any(|word| word == "--component") =>
        {
            Ok(())
        }
        Some(0 | 2) => Err("hub_update_required".into()),
        _ => Err("cli_unavailable".into()),
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
    fn output(code: i32, text: &[u8]) -> Result<Output, String> {
        Ok(Output {
            code: Some(code),
            stdout: text.to_vec(),
        })
    }
    async fn simulate(
        outputs: Vec<Result<Output, String>>,
        check: bool,
        component: Option<&str>,
    ) -> (Result<Vec<u8>, String>, Vec<Vec<String>>) {
        let mut outputs: std::collections::VecDeque<_> = outputs.into();
        let mut calls = Vec::new();
        let result = windows_hub(check, component, None, |args, _| {
            calls.push(args);
            std::future::ready(outputs.pop_front().expect("unexpected process invocation"))
        })
        .await;
        assert!(outputs.is_empty());
        (result, calls)
    }
    #[tokio::test]
    async fn prerequisite_failures_stop_before_owner_checks() {
        for (responses, expected, count) in [
            (vec![Err("executable_missing".into())], "wsl_missing", 1),
            (vec![Err("hub_timeout".into())], "wsl_timeout", 1),
            (
                vec![output(1, b"private localized error")],
                "wsl_unavailable",
                1,
            ),
            (
                vec![output(0, b"\xff\xfe\r\0\n\0"), output(1, b"")],
                "wsl_distribution_missing",
                2,
            ),
            (
                vec![output(0, b"Debian"), output(1, b"private error")],
                "wsl_distribution_unavailable",
                2,
            ),
            (
                vec![output(0, b"Debian"), Err("hub_timeout".into())],
                "wsl_timeout",
                2,
            ),
            (
                vec![
                    output(0, b"Debian"),
                    output(0, b"/home/user\n/usr/bin\n"),
                    output(1, b""),
                ],
                "cli_missing",
                3,
            ),
            (
                vec![
                    output(0, b"Debian"),
                    output(0, b"/home/user\n/usr/bin\n"),
                    output(0, b""),
                    output(2, b"old command"),
                ],
                "hub_update_required",
                4,
            ),
            (
                vec![
                    output(0, b"Debian"),
                    output(0, b"/home/user\n/usr/bin\n"),
                    output(0, b""),
                    output(0, b"hub --json --check"),
                ],
                "hub_update_required",
                4,
            ),
        ] {
            let (result, calls) = simulate(responses, true, Some("wecom-bridge")).await;
            assert_eq!(result.unwrap_err(), expected);
            assert_eq!(calls.len(), count);
            assert!(
                !calls
                    .iter()
                    .any(|args| args.iter().any(|arg| arg == "--check"))
            );
        }
    }
    #[tokio::test]
    async fn recovery_keeps_literal_paths_and_requested_scope() {
        let listing: Vec<u8> = Vec::new();
        let (result, calls) = simulate(
            vec![
                output(0, &listing),
                output(
                    0,
                    "/home/用户 name\n/usr/bin:/mnt/c/Program Files\n".as_bytes(),
                ),
                output(0, b""),
                output(0, b"usage: hub --component COMPONENT"),
                output(1, b"complete report"),
            ],
            true,
            Some("wecom-bridge"),
        )
        .await;
        assert_eq!(result.unwrap(), b"complete report");
        assert_eq!(
            calls[2],
            [
                "--exec",
                "/usr/bin/test",
                "-x",
                "/home/用户 name/.local/bin/dev-cli"
            ]
        );
        assert_eq!(
            &calls[4][4..],
            ["hub", "--json", "--check", "--component", "wecom-bridge"]
        );
        assert!(calls[4][2].contains("/home/用户 name/.local/bin:/usr/bin:/mnt/c/Program Files"));
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

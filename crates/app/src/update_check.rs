//! Background release update check and portable self-update handoff.

use pecofence_platform::http;
use std::path::{Path, PathBuf};
use std::process::Command;

const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const MAX_HTML_BYTES: usize = 128 * 1024;
const MAX_SCRIPT_BYTES: usize = 2 * 1024 * 1024;
const MAX_CHUNKS: usize = 16;
const MAX_NODES: usize = 10;
const GITHUB_OWNER_REPO: &str = "Tinnci/PecoFence";
const AKAMS_HOST: &str = "github.akams.cn";
const DEFAULT_MIRRORS: &[&str] = &["ghproxy.net", "gh-proxy.com"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub version: String,
    pub download_url: String,
    pub sha256: Option<String>,
    pub notes: Option<String>,
}

pub fn host_platform_key() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "windows-aarch64"
    } else {
        "windows-x86_64"
    }
}

pub fn is_trusted_release_url(url: &str) -> bool {
    let prefix = format!("https://github.com/{GITHUB_OWNER_REPO}/releases/download/");
    url.starts_with(&prefix) && !url.contains("..") && !url.chars().any(char::is_control)
}

fn parse_semver(raw: &str) -> Option<(u64, u64, u64, String)> {
    let trimmed = raw.trim().strip_prefix('v').unwrap_or(raw.trim());
    let (core, pre) = match trimmed.split_once('-') {
        Some((core, pre)) => (core, pre.to_owned()),
        None => (trimmed, String::new()),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch, pre))
}

pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    let (Some(cur), Some(cand)) = (parse_semver(current), parse_semver(candidate)) else {
        return false;
    };
    if (cand.0, cand.1, cand.2) != (cur.0, cur.1, cur.2) {
        return (cand.0, cand.1, cand.2) > (cur.0, cur.1, cur.2);
    }
    match (cur.3.is_empty(), cand.3.is_empty()) {
        (false, true) => true,
        (true, false) => false,
        _ => cand.3 > cur.3,
    }
}

pub fn parse_latest_manifest(bytes: &[u8], platform_key: &str) -> Option<ReleaseInfo> {
    let manifest: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let version = manifest.get("version")?.as_str()?.trim();
    let platforms = manifest.get("platforms")?.as_object()?;
    let asset = platforms
        .get(platform_key)
        .or_else(|| platforms.get("windows-x86_64"))?;
    let url = asset.get("url")?.as_str()?;
    if !is_trusted_release_url(url) {
        return None;
    }
    Some(ReleaseInfo {
        version: version.trim_start_matches('v').to_owned(),
        download_url: url.to_owned(),
        sha256: asset
            .get("sha256")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        notes: manifest
            .get("notes")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
    })
}

/// Validate a public HTTPS mirror hostname (blocks localhost and IP literals).
pub fn sanitize_mirror_host(raw: &str) -> Option<String> {
    let clean = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    if clean.is_empty()
        || clean == "localhost"
        || !clean.contains('.')
        || !clean
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic())
        || clean.parse::<std::net::IpAddr>().is_ok()
        || !clean
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        return None;
    }
    Some(clean)
}

/// Extract unique `/_next/static/chunks/*.js` script paths from `github.akams.cn` HTML.
pub fn extract_akams_chunks(html: &str) -> Vec<String> {
    let prefix = "/_next/static/chunks/";
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(pos) = rest.find(prefix) {
        rest = &rest[pos..];
        let end = rest
            .find(|c: char| {
                !(c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '.' | '~' | '-'))
            })
            .unwrap_or(rest.len());
        let cand = &rest[..end];
        if cand.ends_with(".js") && !cand.contains("..") && !out.iter().any(|s| s == cand) {
            out.push(cand.to_owned());
            if out.len() >= MAX_CHUNKS {
                break;
            }
        }
        rest = &rest[end.max(1)..];
    }
    out
}

/// Extract unique sanitized mirror hosts from `label: "contribute"|"search", value: "<host>"`.
pub fn extract_akams_nodes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for marker in [
        r#"label: "contribute""#,
        r#"label: "search""#,
        r#"label:"contribute""#,
        r#"label:"search""#,
    ] {
        let mut rest = text;
        while let Some(pos) = rest.find(marker) {
            rest = &rest[pos + marker.len()..];
            let window = &rest[..rest.len().min(64)];
            if let Some(vpos) = window.find("value:") {
                let after = window[vpos + 6..].trim_start();
                if let Some(quoted) = after.strip_prefix('"')
                    && let Some(qend) = quoted.find('"')
                    && let Some(host) = sanitize_mirror_host(&quoted[..qend])
                    && !out.contains(&host)
                {
                    out.push(host);
                    if out.len() >= MAX_NODES {
                        return out;
                    }
                }
            }
        }
    }
    out
}

fn discover_akams_mirrors() -> Vec<String> {
    let mut out = Vec::new();
    let Ok(html_bytes) = http::get_https(AKAMS_HOST, "/", MAX_HTML_BYTES) else {
        return out;
    };
    let html = String::from_utf8_lossy(&html_bytes);
    for host in extract_akams_nodes(&html) {
        if !out.contains(&host) {
            out.push(host);
        }
    }
    for chunk in extract_akams_chunks(&html) {
        if out.len() >= MAX_NODES {
            break;
        }
        if let Ok(bytes) = http::get_https(AKAMS_HOST, &chunk, MAX_SCRIPT_BYTES) {
            let text = String::from_utf8_lossy(&bytes);
            for host in extract_akams_nodes(&text) {
                if !out.contains(&host) {
                    out.push(host);
                    if out.len() >= MAX_NODES {
                        break;
                    }
                }
            }
        }
    }
    out
}

pub fn check_latest_release(current_version: &str) -> Result<Option<ReleaseInfo>, String> {
    let path = format!("/{GITHUB_OWNER_REPO}/releases/latest/download/latest.json");
    let mirror_path = format!("/https://github.com{path}");
    let mut last_err = match http::get_https("github.com", &path, MAX_MANIFEST_BYTES) {
        Ok(bytes) => {
            let info = parse_latest_manifest(&bytes, host_platform_key())
                .ok_or_else(|| "invalid or untrusted release manifest".to_owned())?;
            return Ok(is_newer_version(current_version, &info.version).then_some(info));
        }
        Err(error) => format!("release check failed: {error}"),
    };

    for host in DEFAULT_MIRRORS {
        if let Ok(bytes) = http::get_https(host, &mirror_path, MAX_MANIFEST_BYTES)
            && let Some(info) = parse_latest_manifest(&bytes, host_platform_key())
        {
            return Ok(is_newer_version(current_version, &info.version).then_some(info));
        }
    }

    for host in discover_akams_mirrors() {
        if let Ok(bytes) = http::get_https(&host, &mirror_path, MAX_MANIFEST_BYTES)
            && let Some(info) = parse_latest_manifest(&bytes, host_platform_key())
        {
            return Ok(is_newer_version(current_version, &info.version).then_some(info));
        }
        last_err = format!("release check failed via {host}");
    }

    Err(last_err)
}

fn find_updater_script(exe_dir: &Path) -> Option<PathBuf> {
    let sibling = exe_dir.join("update-portable-pair.ps1");
    if sibling.is_file() {
        return Some(sibling);
    }
    let repo_script = exe_dir.join("scripts").join("update-portable-pair.ps1");
    repo_script.is_file().then_some(repo_script)
}

/// Launches the existing `update-portable-pair.ps1` script detached against a verified ZIP.
pub fn trigger_portable_self_update(
    pecofence_zip: &Path,
    spm_zip: Option<&Path>,
) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| std::io::Error::other("missing executable directory"))?;
    let script = find_updater_script(exe_dir).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "updater script missing")
    })?;
    let mut cmd = Command::new("pwsh");
    cmd.arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script)
        .arg("-PecoFenceZip")
        .arg(pecofence_zip);
    if let Some(spm) = spm_zip {
        cmd.arg("-SpmZip").arg(spm);
    }
    cmd.spawn()?;
    Ok(())
}

/// Format a concise title and body for a PecoFence tray/toast update notification.
pub fn format_update_notification(info: &ReleaseInfo) -> (String, String) {
    let title = format!("PecoFence v{} available", info.version);
    let body = info
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.lines().next().unwrap_or(s).to_owned())
        .unwrap_or_else(|| "A newer release package is ready to install.".to_owned());
    (title, body)
}

/// Check for a newer release and return a ready-to-display notification tuple when available.
pub fn check_and_format_notification(
    current_version: &str,
) -> Option<(String, String, ReleaseInfo)> {
    let info = check_latest_release(current_version).ok().flatten()?;
    let (title, body) = format_update_notification(&info);
    Some((title, body, info))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_ordering_handles_prereleases() {
        assert!(is_newer_version("0.0.6", "0.0.7"));
        assert!(is_newer_version("0.1.0-rc.1", "0.1.0"));
        assert!(!is_newer_version("0.1.0", "0.1.0-rc.1"));
        assert!(!is_newer_version("0.2.0", "0.1.9"));
    }

    #[test]
    fn manifest_parser_enforces_trusted_github_release_urls() {
        let valid = br#"{
            "version": "v0.0.7",
            "notes": "Release notes",
            "platforms": {
                "windows-aarch64": {
                    "url": "https://github.com/Tinnci/PecoFence/releases/download/v0.0.7/pecofence-0.0.7-arm64.zip",
                    "sha256": "deadbeef"
                }
            }
        }"#;
        let parsed = parse_latest_manifest(valid, "windows-aarch64").unwrap();
        assert_eq!(parsed.version, "0.0.7");
        assert_eq!(parsed.sha256.as_deref(), Some("deadbeef"));

        let untrusted = br#"{
            "version": "0.0.8",
            "platforms": {
                "windows-x86_64": {
                    "url": "https://evil.example.com/pecofence.zip"
                }
            }
        }"#;
        assert_eq!(parse_latest_manifest(untrusted, "windows-x86_64"), None);
    }

    #[test]
    fn formats_update_notification_with_first_notes_line() {
        let info = ReleaseInfo {
            version: "0.0.7".into(),
            download_url: "https://github.com/Tinnci/PecoFence/releases/download/v0.0.7/p.zip"
                .into(),
            sha256: None,
            notes: Some("Faster startup\nMore details".into()),
        };
        let (title, body) = format_update_notification(&info);
        assert_eq!(title, "PecoFence v0.0.7 available");
        assert_eq!(body, "Faster startup");
    }

    #[test]
    fn akams_extracts_chunks_and_filters_unsafe_mirror_hosts() {
        assert_eq!(sanitize_mirror_host("localhost"), None);
        assert_eq!(sanitize_mirror_host("127.0.0.1"), None);
        assert_eq!(
            sanitize_mirror_host("Mirror-A.Example.com").as_deref(),
            Some("mirror-a.example.com")
        );

        let html = r#"<script src="/_next/static/chunks/main-123.js"></script><script src="/_next/static/chunks/main-123.js"></script>"#;
        assert_eq!(
            extract_akams_chunks(html),
            vec!["/_next/static/chunks/main-123.js".to_owned()]
        );

        let js = r#"{label: "contribute", value: "mirror-a.example.com"}, {label: "search", value: "127.0.0.1"}, {label: "search", value: "mirror-b.example.com"}"#;
        assert_eq!(
            extract_akams_nodes(js),
            vec![
                "mirror-a.example.com".to_owned(),
                "mirror-b.example.com".to_owned(),
            ]
        );
    }
}

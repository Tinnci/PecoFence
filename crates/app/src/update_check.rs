//! Background release update check and portable self-update handoff.

use pecofence_platform::http;
use std::path::{Path, PathBuf};
use std::process::Command;

const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const GITHUB_OWNER_REPO: &str = "Tinnci/PecoFence";

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

pub fn check_latest_release(current_version: &str) -> Result<Option<ReleaseInfo>, String> {
    let path = format!("/{GITHUB_OWNER_REPO}/releases/latest/download/latest.json");
    let bytes = http::get_https("github.com", &path, MAX_MANIFEST_BYTES)
        .map_err(|error| format!("release check failed: {error}"))?;
    let info = parse_latest_manifest(&bytes, host_platform_key())
        .ok_or_else(|| "invalid or untrusted release manifest".to_owned())?;
    Ok(is_newer_version(current_version, &info.version).then_some(info))
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
}

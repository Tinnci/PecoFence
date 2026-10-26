//! Turns a daemon `NavigationTarget` into an HTTPS link.
//!
//! The daemon names a configured origin and a record path; it never sends a
//! full URL. This computer's panel config maps each origin id to an HTTPS base,
//! so a daemon can only point at sites the user configured.

use pecofence_plugin_api::{Error, ExternalTarget, PluginContext, Result, check_https_uri};
use spm_contracts::{NavigationTarget, SourceSystem};
use std::collections::BTreeMap;

/// Origin id (for example `meegle-acme`) → HTTPS base (`https://meegle.example.com/acme`).
pub type NavigationOrigins = BTreeMap<String, String>;

/// Why a resolved target could not become a link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// The daemon returned no target for this record.
    NoTarget,
    /// No base is configured for this origin id.
    UnknownOrigin(String),
    /// The record path is empty or contains `.`/`..` or empty segments.
    BadPath,
}

/// Rejects bases that are not plain HTTPS or that carry a query or fragment.
pub fn validate_origins(origins: &NavigationOrigins) -> Result<()> {
    for (id, base) in origins {
        if id.is_empty() {
            return Err(Error::Invalid("navigation origin id is empty".into()));
        }
        check_https_uri(base)
            .map_err(|error| Error::Invalid(format!("navigation origin {id}: {error}")))?;
        if base.contains(['?', '#']) {
            return Err(Error::Invalid(format!(
                "navigation origin {id}: the base cannot have a query or fragment"
            )));
        }
    }
    Ok(())
}

/// A checked HTTPS link and the system it leads to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub system: SourceSystem,
    pub uri: String,
}

/// Builds the link for the first target whose origin is configured.
pub fn target_link(
    origins: &NavigationOrigins,
    targets: &[NavigationTarget],
) -> std::result::Result<Link, LinkError> {
    let first = targets.first().ok_or(LinkError::NoTarget)?;
    let (target, base) = targets
        .iter()
        .find_map(|target| {
            origins
                .get(&target.configured_origin_id)
                .map(|base| (target, base))
        })
        .ok_or_else(|| LinkError::UnknownOrigin(first.configured_origin_id.clone()))?;
    let path = target
        .relative_record_path
        .strip_prefix('/')
        .ok_or(LinkError::BadPath)?;
    let mut uri = base.trim_end_matches('/').to_owned();
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(LinkError::BadPath);
        }
        uri.push('/');
        encode_segment(segment, &mut uri);
    }
    check_https_uri(&uri).map_err(|_| LinkError::BadPath)?;
    Ok(Link {
        system: target.system,
        uri,
    })
}

/// Why "Open source" did not open anything.
#[derive(Debug)]
pub enum OpenError {
    /// No safe link could be built from the daemon's targets.
    Link(LinkError),
    /// The host refused or could not open the link.
    Host(Error),
}

/// Resolve `targets` against the configured origins and ask the host to open the
/// link. Both SPM panels go through here, so the origin rule lives in one place.
pub fn open(
    ctx: &PluginContext,
    origins: &NavigationOrigins,
    targets: &[NavigationTarget],
) -> std::result::Result<Link, OpenError> {
    let link = target_link(origins, targets).map_err(OpenError::Link)?;
    let navigation = ctx
        .navigation
        .as_ref()
        .ok_or_else(|| OpenError::Host(Error::Backend("link opening unavailable".into())))?;
    navigation
        .with(|n| {
            n.open(
                &ctx.scope,
                ExternalTarget {
                    system: format!("{:?}", link.system),
                    https_uri: link.uri.clone(),
                },
            )
        })
        .map_err(OpenError::Host)?;
    Ok(link)
}

/// Percent-encodes everything except RFC 3986 unreserved characters, so a
/// segment cannot add path levels, a query or a fragment.
fn encode_segment(segment: &str, out: &mut String) {
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origins() -> NavigationOrigins {
        [(
            "meegle-acme".to_owned(),
            "https://meegle.example.com/acme/".to_owned(),
        )]
        .into()
    }

    fn target_uri(
        origins: &NavigationOrigins,
        targets: &[NavigationTarget],
    ) -> std::result::Result<String, LinkError> {
        target_link(origins, targets).map(|link| link.uri)
    }

    fn target(origin: &str, path: &str) -> NavigationTarget {
        NavigationTarget {
            system: SourceSystem::Meegle,
            configured_origin_id: origin.into(),
            relative_record_path: path.into(),
        }
    }

    #[test]
    fn joins_the_configured_base_and_encodes_each_segment() {
        assert_eq!(
            target_uri(&origins(), &[target("meegle-acme", "/orion/story/88232")]),
            Ok("https://meegle.example.com/acme/orion/story/88232".into())
        );
        assert_eq!(
            target_uri(&origins(), &[target("meegle-acme", "/首页 设置/a?b#c/x%2F")]),
            Ok("https://meegle.example.com/acme/%E9%A6%96%E9%A1%B5%20%E8%AE%BE%E7%BD%AE/a%3Fb%23c/x%252F".into())
        );
    }

    #[test]
    fn uses_the_first_target_with_a_configured_origin() {
        let mut gerrit = target("gerrit-acme", "/c/1");
        gerrit.system = SourceSystem::Gerrit;
        let targets = [gerrit, target("meegle-acme", "/s/2")];
        assert_eq!(
            target_link(&origins(), &targets),
            Ok(Link {
                system: SourceSystem::Meegle,
                uri: "https://meegle.example.com/acme/s/2".into()
            })
        );
    }

    #[test]
    fn refuses_unknown_origins_missing_targets_and_path_tricks() {
        assert_eq!(target_uri(&origins(), &[]), Err(LinkError::NoTarget));
        assert_eq!(
            target_uri(&origins(), &[target("jira-acme", "/a")]),
            Err(LinkError::UnknownOrigin("jira-acme".into()))
        );
        for path in [
            "",
            "a/b",
            "/",
            "//evil.example/x",
            "/a/../b",
            "/./a",
            "/a//b",
        ] {
            assert_eq!(
                target_uri(&origins(), &[target("meegle-acme", path)]),
                Err(LinkError::BadPath),
                "{path:?}"
            );
        }
    }

    #[test]
    fn validates_configured_bases() {
        assert!(validate_origins(&origins()).is_ok());
        for base in [
            "http://meegle.example.com",
            "https://user:pw@meegle.example.com",
            "https://meegle.example.com/?x=1",
            "https://meegle.example.com/#top",
            "javascript:alert(1)",
            "https://",
        ] {
            let bad: NavigationOrigins = [("o".to_owned(), base.to_owned())].into();
            assert!(validate_origins(&bad).is_err(), "{base}");
        }
        let empty_id: NavigationOrigins = [(String::new(), "https://a.example".to_owned())].into();
        assert!(validate_origins(&empty_id).is_err());
    }
}

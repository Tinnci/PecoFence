//! Validated argument lists for every `spm` call; nothing else reaches the process.
use super::*;

/// `spm focus flow`: one to ten 飞书项目 items' 流转记录, read in parallel.
pub fn flow_args(code: &str, items: &[String]) -> Option<Vec<String>> {
    if !valid_code(code) || items.is_empty() || items.len() > 10 {
        return None;
    }
    let mut args: Vec<String> = vec!["focus".into(), "flow".into(), "--code".into(), code.into()];
    for item in items {
        if !valid_item("meegle", item) {
            return None;
        }
        args.extend(["--item".into(), item.clone()]);
    }
    Some(args)
}
/// `spm focus diff`: a change's files, or one file's diff when `path` is given.
pub fn diff_args(code: &str, change: u32, path: Option<&str>, all: bool) -> Option<Vec<String>> {
    if !valid_code(code) || change == 0 {
        return None;
    }
    let mut args: Vec<String> = vec![
        "focus".into(),
        "diff".into(),
        "--code".into(),
        code.into(),
        "--change".into(),
        change.to_string(),
    ];
    if let Some(path) = path {
        if path.is_empty()
            || path.len() > 1024
            || path.starts_with(['-', '/'])
            || path.contains("..")
            || path.chars().any(char::is_control)
        {
            return None;
        }
        args.extend(["--path".into(), path.into()]);
    } else if all {
        args.push("--all".into());
    }
    Some(args)
}
pub fn build_evidence_args(code: &str, job: &str, build: u32) -> Option<Vec<String>> {
    if !valid_code(code)
        || build == 0
        || job.len() > 300
        || job.split('/').any(|p| {
            p.is_empty()
                || p == "."
                || p == ".."
                || !p.starts_with(|c: char| c.is_ascii_alphanumeric())
                || !p
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        })
    {
        return None;
    }
    Some(vec![
        "focus".into(),
        "build-evidence".into(),
        "--code".into(),
        code.into(),
        "--job".into(),
        job.into(),
        "--build".into(),
        build.to_string(),
    ])
}

/// A work item the local layer may address: its source and the source's ID.
pub fn valid_item(source: &str, item: &str) -> bool {
    matches!(source, "jira" | "meegle")
        && (1..=64).contains(&item.len())
        && item.starts_with(|c: char| c.is_ascii_alphanumeric())
        && item
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
/// A Gerrit branch name as the frontend may pass it.
pub fn valid_branch(branch: &str) -> bool {
    (1..=200).contains(&branch.len())
        && branch.starts_with(|c: char| c.is_ascii_alphanumeric())
        && branch
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./+".contains(&b))
}

/// A model code: letters, digits, `-` and `_`.
pub fn valid_code(code: &str) -> bool {
    (2..=32).contains(&code.len())
        && code.starts_with(|c: char| c.is_ascii_alphanumeric())
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
/// Arguments for `spm focus`; `None` when a value is malformed.
/// Everything the frontend may pass to the `focus` command, as one request.
#[derive(Debug, Default, Clone)]
pub struct FocusRequest {
    pub action: String,
    pub code: Option<String>,
    pub job: Option<String>,
    pub build: Option<u32>,
    pub change: Option<u32>,
    pub review: Option<String>,
    pub branch: Option<String>,
    pub source: Option<String>,
    pub item: Option<String>,
    pub path: Option<String>,
    pub items: Option<Vec<String>>,
    pub addresses: Option<Vec<String>>,
    /// The source server an item belongs to, e.g. `192.168.11.130:8080`.
    pub instance: Option<String>,
    /// The chip a model is built on, as the user names it.
    pub chip: Option<String>,
    pub baseband: Option<String>,
    /// Words to find in what is cached.
    pub query: Option<String>,
}

fn valid_address(url: &str) -> bool {
    url.len() <= 2048
        && !url.chars().any(char::is_control)
        && (url.starts_with("https://") || url.starts_with("http://"))
}
/// A source server as SPM names it: host, port and path, no scheme, spaces or options.
pub fn valid_instance(instance: &str) -> bool {
    (1..=200).contains(&instance.len())
        && !instance.starts_with('-')
        && instance
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".:/_-".contains(&b))
}

/// The `spm` arguments for a focus request, or None when the request is not one the
/// frontend may make. Every action the frontend sends is routed here (tested against
/// `src/api.ts`).
pub fn route_args(r: &FocusRequest) -> Option<Vec<String>> {
    let action = r.action.as_str();
    let code = r.code.as_deref();
    match action {
        "source_fields" | "source_fields_refresh" => {
            let [address] = r.addresses.as_deref()? else {
                return None;
            };
            source_fields_args(
                code?,
                r.item.as_deref()?,
                address,
                action == "source_fields_refresh",
            )
        }
        "flow" | "flows" | "flows_refresh" => {
            let list = match (&r.items, &r.item) {
                (Some(list), _) if action != "flow" => list.clone(),
                (_, Some(one)) => vec![one.clone()],
                _ => vec![],
            };
            if let Some(urls) = &r.addresses
                && (urls.len() != list.len() || !urls.iter().all(|u| valid_address(u)))
            {
                return None;
            }
            let mut args = flow_args(code?, &list)?;
            for url in r.addresses.iter().flatten() {
                args.extend(["--address".into(), url.clone()]);
            }
            if action == "flows_refresh" {
                args.push("--refresh".into());
            }
            Some(args)
        }
        "change_files" | "change_diff" | "change_diffs" => {
            let path = if action == "change_diff" {
                Some(r.path.as_deref()?)
            } else {
                None
            };
            diff_args(code?, r.change?, path, action == "change_diffs")
        }
        "build_evidence" => build_evidence_args(code?, r.job.as_deref()?, r.build?),
        "platforms" => Some(vec!["focus".into(), "platforms".into()]),
        "needs" => Some(vec!["focus".into(), "needs".into()]),
        "needs_cached" => Some(vec!["focus".into(), "needs".into(), "--cached".into()]),
        "forecast" => Some(vec!["focus".into(), "forecast".into()]),
        "calendar" => Some(vec!["focus".into(), "calendar".into()]),
        "calendar_cached" => Some(vec!["focus".into(), "calendar".into(), "--cached".into()]),
        "mine" => Some(vec!["focus".into(), "mine".into()]),
        "mine_cached" => Some(vec!["focus".into(), "mine".into(), "--cached".into()]),
        "search" => {
            let query = r.query.as_deref().map(str::trim).filter(|q| {
                (1..=200).contains(&q.chars().count())
                    && !q.starts_with('-')
                    && !q.chars().any(char::is_control)
            })?;
            Some(
                [
                    "focus", "search", query, "--limit", "30", "--format", "json",
                ]
                .map(String::from)
                .to_vec(),
            )
        }
        "logs" => Some(
            [
                "focus", "logs", "--since", "7d", "--limit", "1000", "--format", "json",
            ]
            .map(String::from)
            .to_vec(),
        ),
        "traces" => Some(
            [
                "focus", "traces", "--since", "24h", "--limit", "100", "--format", "json",
            ]
            .map(String::from)
            .to_vec(),
        ),
        "chip" => {
            let code = code.filter(|c| valid_code(c))?;
            if r.chip.is_none() && r.baseband.is_none() {
                return None;
            }
            let mut args = vec!["focus".into(), "chip".into(), "--code".into(), code.into()];
            for (flag, value) in [("chip", &r.chip), ("baseband", &r.baseband)] {
                if let Some(value) = value {
                    let value = value.trim();
                    if value.chars().count() > 80 || value.chars().any(char::is_control) {
                        return None;
                    }
                    // Equals keeps names starting with a dash as values.
                    args.push(format!("--{flag}={value}"));
                }
            }
            Some(args)
        }
        "related" => {
            let (source, item) = (r.source.as_deref()?, r.item.as_deref()?);
            let code = code.filter(|c| valid_code(c))?;
            if !valid_item(source, item) {
                return None;
            }
            let mut args: Vec<String> = vec![
                "focus".into(),
                "related".into(),
                "--code".into(),
                code.into(),
                "--source".into(),
                source.into(),
                "--item".into(),
                item.into(),
            ];
            if let Some(instance) = r.instance.as_deref().filter(|i| !i.is_empty()) {
                if !valid_instance(instance) {
                    return None;
                }
                args.extend(["--instance".into(), instance.into()]);
            }
            Some(args)
        }
        _ => {
            let mut args = focus_args(
                action,
                code,
                r.change,
                r.review.as_deref(),
                r.branch.as_deref(),
                r.source.as_deref().zip(r.item.as_deref()),
            )?;
            if matches!(action, "annotate" | "history") {
                if let Some(instance) = r.instance.as_deref() {
                    if !valid_instance(instance) {
                        return None;
                    }
                    args.extend(["--instance".into(), instance.into()]);
                }
                if action == "history"
                    && let Some(code) = code.filter(|c| valid_code(c))
                {
                    args.extend(["--code".into(), code.into()]);
                }
            }
            Some(args)
        }
    }
}

pub fn focus_args(
    action: &str,
    code: Option<&str>,
    change: Option<u32>,
    review: Option<&str>,
    branch: Option<&str>,
    annotation: Option<(&str, &str)>,
) -> Option<Vec<String>> {
    let code = || code.filter(|c| valid_code(c)).map(str::to_owned);
    let mut args: Vec<String> = vec!["focus".into()];
    match action {
        "annotate" | "history" => {
            let (source, item) = annotation.filter(|(s, i)| valid_item(s, i))?;
            args.extend([
                action.into(),
                "--source".into(),
                source.into(),
                "--item".into(),
                item.into(),
            ]);
            if action == "annotate"
                && let Some(code) = code()
            {
                args.extend(["--code".into(), code]);
            }
        }
        "issues"
        | "issues_cached"
        | "issues_full"
        | "requirements"
        | "requirements_cached"
        | "requirements_full" => {
            let (kind, mode) = action.split_once('_').unwrap_or((action, ""));
            args.extend([
                "work-items".into(),
                "--code".into(),
                code()?,
                "--kind".into(),
                kind.into(),
            ]);
            match mode {
                "cached" => args.push("--cached".into()),
                "full" => args.push("--full".into()),
                _ => {}
            }
        }
        "list" => args.push("list".into()),
        "save" => args.push("save".into()),
        "summary" => args.push("summary".into()),
        "cached" => args.extend(["summary".into(), "--cached".into()]),
        "suggest" => args.extend(["suggest".into(), "--code".into(), code()?]),
        "remove" => args.extend(["remove".into(), "--code".into(), code()?]),
        "restore" => args.extend(["restore".into(), "--code".into(), code()?]),
        "refresh" => args.extend(["summary".into(), "--code".into(), code()?]),
        "preview" => args.push("preview".into()),
        "changes" => args.extend([
            "changes".into(),
            "--code".into(),
            code()?,
            "--branch".into(),
            branch.filter(|b| valid_branch(b))?.into(),
        ]),
        "builds" => args.extend(["builds".into(), "--code".into(), code()?]),
        "repos" => args.extend(["repos".into(), "--code".into(), code()?]),
        "builds_cached" => {
            args.extend(["builds".into(), "--code".into(), code()?, "--cached".into()])
        }
        "review" => {
            let review = review.filter(|r| ["accept", "track", "untrack"].contains(r))?;
            args.extend([
                "review".into(),
                "--code".into(),
                code()?,
                "--change".into(),
                change?.to_string(),
                "--action".into(),
                review.into(),
            ]);
        }
        _ => return None,
    }
    Some(args)
}
pub fn decode_focus(code: Option<i32>, stdout: &[u8]) -> Result<FocusData, SetupFailure> {
    owner_data(code, stdout)
}

/// Keys the frontend may pass to `spm discover`: Meegle space, type and field keys.
pub fn valid_key(key: &str) -> bool {
    (1..=64).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
/// What a discovery list may name: Meegle keys and a Jira project.
#[derive(Debug, Default, Clone, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DiscoverArgs {
    #[serde(default)]
    #[ts(optional)]
    pub space: Option<String>,
    /// A Meegle type key: the issue type for `groups`, the group's target type for `items`.
    #[serde(default)]
    #[ts(optional)]
    pub kind: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub search: Option<String>,
    /// `items`: count issues of this type key per group, through `field`.
    #[serde(default)]
    #[ts(optional)]
    pub count_type: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub field: Option<String>,
    /// `jira_versions` and `jira_filters`.
    #[serde(default)]
    #[ts(optional)]
    pub project: Option<String>,
    /// `gerrit_activity`: a repository path and/or a branch.
    #[serde(default)]
    #[ts(optional)]
    pub repo: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub branch: Option<String>,
}

/// A Gerrit repository path or branch name: letters, digits and /._-+@.
pub fn valid_gerrit_name(name: &str) -> bool {
    (1..=200).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || "/._-+@".contains(c))
}

/// A Jira project key such as PLV.
pub fn valid_project(key: &str) -> bool {
    (1..=20).contains(&key.len())
        && key.starts_with(|c: char| c.is_ascii_uppercase())
        && key
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// Arguments for one discovery list; `None` when an argument is malformed.
pub fn discover_args<'a>(list: &str, a: &'a DiscoverArgs) -> Option<Vec<&'a str>> {
    let key = |k: &'a Option<String>| k.as_deref().filter(|k| valid_key(k));
    let project = || a.project.as_deref().filter(|p| valid_project(p));
    let space = key(&a.space);
    let kind = key(&a.kind);
    let search = a.search.as_deref();
    let mut args = vec!["discover"];
    match list {
        "spaces" => args.push("spaces"),
        "jira_projects" => args.push("jira-projects"),
        "jira_versions" => args.extend(["jira-versions", "--project", project()?]),
        "jira_filters" => args.extend(["jira-filters", "--project", project()?]),
        "gerrit_projects" => args.push("gerrit-projects"),
        "gerrit_activity" => {
            args.push("gerrit-activity");
            let name = |n: &'a Option<String>| n.as_deref().filter(|n| valid_gerrit_name(n));
            match (&a.repo, &a.branch) {
                (None, None) => return None,
                (repo, branch) => {
                    if repo.is_some() {
                        args.extend(["--project", name(repo)?]);
                    }
                    if branch.is_some() {
                        args.extend(["--branch", name(branch)?]);
                    }
                }
            }
        }
        "types" => args.extend(["types", "--space", space?]),
        "groups" => args.extend(["groups", "--space", space?, "--type", kind?]),
        "items" => {
            args.extend(["items", "--space", space?, "--type", kind?]);
            match (&a.count_type, &a.field) {
                (Some(_), Some(_)) => args.extend([
                    "--count-type",
                    key(&a.count_type)?,
                    "--field",
                    key(&a.field)?,
                ]),
                (None, None) => {}
                _ => return None,
            }
            if let Some(search) = search.map(str::trim).filter(|s| !s.is_empty()) {
                if search.chars().count() > 60 || search.chars().any(char::is_control) {
                    return None;
                }
                args.extend(["--search", search]);
            }
        }
        _ => return None,
    }
    Some(args)
}
pub fn draft_args(apply: bool, expect: Option<&str>) -> Option<Vec<&str>> {
    let mut args = vec!["setup", "draft"];
    if apply {
        let sha = expect.filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))?;
        args.extend(["--apply", "--replace", "--expect", sha]);
    }
    Some(args)
}

pub(super) fn owner_data<T: serde::de::DeserializeOwned>(
    code: Option<i32>,
    stdout: &[u8],
) -> Result<T, SetupFailure> {
    let envelope = envelope(code, stdout)?;
    if !envelope.ok {
        let error = envelope
            .error
            .ok_or(SetupFailure::new("spm_invalid_response"))?;
        return Err(SetupFailure::owner(&error.code, error.message));
    }
    serde_json::from_value(envelope.data).map_err(|_| SetupFailure::new("spm_invalid_response"))
}
pub fn decode_discovery(code: Option<i32>, stdout: &[u8]) -> Result<Discovery, SetupFailure> {
    owner_data(code, stdout)
}
pub fn decode_draft(code: Option<i32>, stdout: &[u8]) -> Result<Draft, SetupFailure> {
    owner_data(code, stdout)
}

//! `pecofence-cli`: read-only inspection of a saved PecoFence workspace — its fences, the order
//! a sort mode produces, and organizing suggestions. It never writes the workspace and never
//! talks to the running app; suggestions are applied in PecoFence (fence menu → 建议).
//! Design notes and use cases: `docs/SUGGESTIONS.md`.

use pecofence_core::suggest::{self, Suggestion, parse_sort, sort_key};
use pecofence_core::{Config, ConfigStore, ContentInstance, ContentSpec, Item, ItemRef};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const HELP: &str = "\
pecofence-cli — inspect a saved PecoFence workspace (read-only)

USAGE
  pecofence-cli fences  [OPTIONS]
  pecofence-cli suggest [--fence ID|TITLE] [--kind sort|rule|move|cleanup]... [--limit N] [OPTIONS]
  pecofence-cli order   --fence ID|TITLE [--by MODE] [--limit N] [OPTIONS]

COMMANDS
  fences    List fences with their kind, item count and sort mode.
  suggest   Ranked organizing suggestions, each with its reason (default limit 10).
  order     Preview a fence's item order under its current or another sort mode
            (MODE: manual, name, type, date, size, frequent; default limit 50).

OPTIONS
  --workspace PATH  workspace.v2.json, or the folder holding it. Default: $PECOFENCE_WORKSPACE,
                    then a portable config/ folder next to this program, then %APPDATA%\\PecoFence.
  --layout N        Monitor layout index (default 0; `fences` reports how many exist).
  --json            One JSON envelope on stdout; errors as an envelope on stderr.
  -h, --help        This text.   -V, --version   Version.

EXAMPLES
  pecofence-cli suggest --json
  pecofence-cli suggest --fence Desktop --kind move
  pecofence-cli order --fence Apps --by frequent --limit 10

Nothing here changes the workspace. Results reflect the last save; PecoFence saves shortly
after each change. Apply a suggestion in PecoFence: right-click the fence → Suggestions.
Exit codes: 0 success (also when empty), 1 workspace unreadable, 2 invalid arguments.";

const SCHEMA: &str = "1.0";
const MAX_LIMIT: usize = 1000;

#[derive(Debug)]
struct Failure {
    code: &'static str,
    message: String,
    action: String,
    exit: u8,
}

impl Failure {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            code: "INVALID_ARGUMENT",
            message: message.into(),
            action: "Run pecofence-cli --help for commands, options and examples.".into(),
            exit: 2,
        }
    }
}

#[derive(Debug, Default)]
struct Options {
    command: String,
    json: bool,
    workspace: Option<PathBuf>,
    layout: usize,
    fence: Option<String>,
    kinds: Vec<String>,
    limit: Option<usize>,
    by: Option<String>,
}

fn parse(args: &[String]) -> Result<Options, Failure> {
    let mut options = Options::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = |name: &str| {
            rest.next()
                .filter(|v| !v.starts_with("--"))
                .cloned()
                .ok_or_else(|| Failure::usage(format!("{name} needs a value")))
        };
        match arg.as_str() {
            "--json" => options.json = true,
            "-h" | "--help" | "help" => options.command = "help".into(),
            "-V" | "--version" => options.command = "version".into(),
            "--workspace" => options.workspace = Some(PathBuf::from(value(arg)?)),
            "--layout" => {
                let raw = value(arg)?;
                options.layout = raw.parse().map_err(|_| {
                    Failure::usage(format!("--layout must be a number, got {raw:?}"))
                })?;
            }
            "--fence" => options.fence = Some(value(arg)?),
            "--kind" => {
                let kind = value(arg)?;
                if !["sort", "rule", "move", "cleanup"].contains(&kind.as_str()) {
                    return Err(Failure::usage(format!(
                        "unknown --kind {kind:?}; use sort, rule, move or cleanup"
                    )));
                }
                options.kinds.push(kind);
            }
            "--limit" => {
                let raw = value(arg)?;
                match raw.parse::<usize>() {
                    Ok(n) if (1..=MAX_LIMIT).contains(&n) => options.limit = Some(n),
                    _ => {
                        return Err(Failure::usage(format!(
                            "--limit must be 1–{MAX_LIMIT}, got {raw:?}"
                        )));
                    }
                }
            }
            "--by" => {
                let by = value(arg)?;
                if parse_sort(&by).is_none() {
                    return Err(Failure::usage(format!(
                        "unknown --by {by:?}; use manual, name, type, date, size or frequent"
                    )));
                }
                options.by = Some(by);
            }
            command if !command.starts_with('-') && options.command.is_empty() => {
                if !["fences", "suggest", "order"].contains(&command) {
                    return Err(Failure::usage(format!(
                        "unknown command {command:?}; use fences, suggest or order"
                    )));
                }
                options.command = command.into();
            }
            other => return Err(Failure::usage(format!("unexpected argument {other:?}"))),
        }
    }
    if options.command.is_empty() {
        return Err(Failure::usage("missing command: fences, suggest or order"));
    }
    let only = |flag: &str, allowed: &[&str], set: bool| {
        if set && !allowed.contains(&options.command.as_str()) {
            Err(Failure::usage(format!(
                "{flag} does not apply to `{}`",
                options.command
            )))
        } else {
            Ok(())
        }
    };
    only("--fence", &["suggest", "order"], options.fence.is_some())?;
    only("--kind", &["suggest"], !options.kinds.is_empty())?;
    only("--limit", &["suggest", "order"], options.limit.is_some())?;
    only("--by", &["order"], options.by.is_some())?;
    if options.command == "order" && options.fence.is_none() {
        return Err(Failure::usage(
            "order needs --fence ID|TITLE (see `pecofence-cli fences`)",
        ));
    }
    Ok(options)
}

/// Where the workspace came from, for the report.
struct Located {
    path: PathBuf,
    source: &'static str,
}

/// Environment lookups, injectable for tests.
struct Env {
    var: Box<dyn Fn(&str) -> Option<String>>,
    exe_dir: Option<PathBuf>,
}

fn locate(explicit: Option<&Path>, env: &Env) -> Result<Located, Failure> {
    let file = |p: PathBuf| {
        if p.is_dir() {
            ConfigStore::new(&p).primary_path()
        } else {
            p
        }
    };
    if let Some(path) = explicit {
        return Ok(Located {
            path: file(path.to_path_buf()),
            source: "argument",
        });
    }
    if let Some(path) = (env.var)("PECOFENCE_WORKSPACE").filter(|v| !v.trim().is_empty()) {
        return Ok(Located {
            path: file(PathBuf::from(path)),
            source: "PECOFENCE_WORKSPACE",
        });
    }
    if let Some(dir) = &env.exe_dir {
        let portable = ConfigStore::new(dir.join("config")).primary_path();
        if portable.is_file() {
            return Ok(Located {
                path: portable,
                source: "portable",
            });
        }
    }
    match (env.var)("APPDATA") {
        Some(appdata) => Ok(Located {
            path: ConfigStore::new(Path::new(&appdata).join("PecoFence")).primary_path(),
            source: "appdata",
        }),
        None => Err(Failure {
            code: "WORKSPACE_UNAVAILABLE",
            message: "no workspace location: APPDATA is not set".into(),
            action: "Pass --workspace with the folder or workspace.v2.json file.".into(),
            exit: 1,
        }),
    }
}

fn load(located: &Located) -> Result<Config, Failure> {
    ConfigStore::parse_file(&located.path).map_err(|error| {
        let missing = !located.path.exists();
        Failure {
            code: if missing {
                "WORKSPACE_UNAVAILABLE"
            } else {
                "WORKSPACE_INVALID"
            },
            message: format!("{}: {error}", located.path.display()),
            action: if missing {
                "Start PecoFence once to create the workspace, or pass --workspace (portable \
                 editions keep it in config\\ next to pecofence.exe)."
                    .into()
            } else {
                "The file is not a workspace this version reads. PecoFence keeps backups in \
                 workspace.v2.backups; pass one with --workspace to inspect it."
                    .into()
            },
            exit: 1,
        }
    })
}

fn kind_of(content: &ContentInstance) -> &'static str {
    match &content.content {
        ContentSpec::FileCollection { inbox: true, .. } => "inbox",
        ContentSpec::FileCollection { .. } => "collection",
        ContentSpec::FolderPortal { .. } => "portal",
        ContentSpec::Panel { .. } => "widget",
    }
}

fn members<'a>(config: &'a Config, content: &'a ContentInstance) -> Vec<(&'a ItemRef, &'a Item)> {
    content
        .items()
        .iter()
        .filter_map(|r| config.items.get(&r.item_id).map(|it| (r, it)))
        .filter(|(_, it)| it.orphaned_since.is_none())
        .collect()
}

fn find_fence<'a>(
    contents: &'a [ContentInstance],
    query: &str,
) -> Result<&'a ContentInstance, Failure> {
    let q = query.trim().to_lowercase();
    let by_id: Vec<_> = contents
        .iter()
        .filter(|c| q.len() >= 4 && c.id.to_string().starts_with(&q))
        .collect();
    let matches = if by_id.is_empty() {
        contents
            .iter()
            .filter(|c| c.title.trim().to_lowercase() == q)
            .collect()
    } else {
        by_id
    };
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(Failure {
            code: "FENCE_NOT_FOUND",
            message: format!("no fence titled or with an id starting {query:?}"),
            action: "Run pecofence-cli fences to list titles and ids.".into(),
            exit: 2,
        }),
        many => Err(Failure {
            code: "FENCE_AMBIGUOUS",
            message: format!(
                "{query:?} matches {} fences: {}",
                many.len(),
                many.iter()
                    .map(|c| format!("{} ({})", short(c), c.title))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            action: "Pass the fence id (or its first 8 characters) instead of the title.".into(),
            exit: 2,
        }),
    }
}

fn short(content: &ContentInstance) -> String {
    content.id.to_string().chars().take(8).collect()
}

struct Output {
    data: Value,
    text: String,
    meta: Value,
}

fn run(options: &Options, env: &Env, now: i64) -> Result<Output, Failure> {
    let located = locate(options.workspace.as_deref(), env)?;
    let config = load(&located)?;
    let layouts = config.layouts.len();
    let Some(layout) = config.layouts.get(options.layout) else {
        return Err(Failure::usage(format!(
            "--layout {} is out of range; this workspace has {layouts} layout(s)",
            options.layout
        )));
    };
    let saved_at = std::fs::metadata(&located.path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let mut meta = json!({
        "workspace": located.path.display().to_string(),
        "workspaceSource": located.source,
        "savedAt": saved_at,
        "layout": options.layout,
        "layouts": layouts,
        "readOnly": true,
    });
    let contents = &layout.contents;
    match options.command.as_str() {
        "fences" => {
            let rows: Vec<Value> = contents
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id.to_string(),
                        "title": c.title,
                        "kind": kind_of(c),
                        "items": members(&config, c).len(),
                        "sort": sort_key(c.view.sort),
                        "reverse": c.view.reverse,
                    })
                })
                .collect();
            let mut text = format!(
                "{:<8}  {:<10}  {:>5}  {:<9}  TITLE\n",
                "ID", "KIND", "ITEMS", "SORT"
            );
            for c in contents {
                let items = match &c.content {
                    ContentSpec::FileCollection { .. } => members(&config, c).len().to_string(),
                    _ => "-".into(),
                };
                text.push_str(&format!(
                    "{:<8}  {:<10}  {:>5}  {:<9}  {}\n",
                    short(c),
                    kind_of(c),
                    items,
                    sort_key(c.view.sort),
                    c.title
                ));
            }
            if contents.is_empty() {
                text = "No fences in this layout.\n".into();
            }
            if layouts > 1 {
                text.push_str(&format!(
                    "Layout {} of {layouts}; pass --layout N for the others.\n",
                    options.layout
                ));
            }
            Ok(Output {
                data: json!({ "fences": rows }),
                text,
                meta,
            })
        }
        "suggest" => {
            let fence = options
                .fence
                .as_deref()
                .map(|q| find_fence(contents, q))
                .transpose()?;
            let all = suggest::suggest(&config, options.layout, now);
            let kind = |s: &Suggestion| s.id.split(':').next().unwrap_or("").to_string();
            let wanted: Vec<&Suggestion> = all
                .iter()
                .filter(|s| fence.is_none_or(|f| s.action.concerns(f.id)))
                .filter(|s| options.kinds.is_empty() || options.kinds.contains(&kind(s)))
                .collect();
            let limit = options.limit.unwrap_or(10);
            let total = wanted.len();
            let shown = &wanted[..total.min(limit)];
            meta["total"] = json!(total);
            meta["truncated"] = json!(total > shown.len());
            let mut text = String::new();
            for (i, s) in shown.iter().enumerate() {
                text.push_str(&format!(
                    "{}. {}  (confidence {:.2})\n   why: {}\n",
                    i + 1,
                    s.summary,
                    s.confidence,
                    s.reason
                ));
                if !s.examples.is_empty() {
                    text.push_str(&format!("   e.g. {}\n", s.examples.join(", ")));
                }
                text.push_str(&format!("   id: {}\n", s.id));
            }
            if shown.is_empty() {
                let files = contents
                    .iter()
                    .filter(|c| matches!(c.content, ContentSpec::FileCollection { .. }))
                    .count();
                text.push_str(&format!(
                    "No suggestions ({files} file fences checked). They appear after consistent \
                     manual placements, frequently opened items, unsorted inbox items that match \
                     a fence, or idle installers.\n"
                ));
            } else {
                if total > shown.len() {
                    text.push_str(&format!(
                        "Showing {} of {total}; raise --limit to see more.\n",
                        shown.len()
                    ));
                }
                text.push_str("Apply in PecoFence: right-click the fence → Suggestions.\n");
            }
            Ok(Output {
                data: json!({ "suggestions": shown }),
                text,
                meta,
            })
        }
        "order" => {
            let query = options.fence.as_deref().unwrap_or_default();
            let content = find_fence(contents, query)?;
            if !matches!(content.content, ContentSpec::FileCollection { .. }) {
                return Err(Failure {
                    code: "NOT_A_FILE_FENCE",
                    message: format!("“{}” is a {} fence", content.title, kind_of(content)),
                    action: match &content.content {
                        ContentSpec::FolderPortal { .. } => {
                            "Folder portals list their folder live; their items are not saved \
                             in the workspace. Pick a collection or the inbox."
                        }
                        _ => "Widgets hold no items. Pick a collection or the inbox.",
                    }
                    .into(),
                    exit: 2,
                });
            }
            let mut view = content.view.clone();
            if let Some(by) = options.by.as_deref().and_then(parse_sort) {
                view.sort = by;
            }
            let mut items: Vec<(Option<&ItemRef>, &Item)> = members(&config, content)
                .into_iter()
                .map(|(r, it)| (Some(r), it))
                .collect();
            pecofence_core::order::order_items(&mut items, &view, now);
            let limit = options.limit.unwrap_or(50);
            let total = items.len();
            meta["total"] = json!(total);
            meta["truncated"] = json!(total > limit);
            let detail = |it: &Item| match view.sort {
                pecofence_core::SortMode::OpenCount => json!({
                    "opens": it.open_count,
                    "lastOpened": it.last_opened,
                    "score": (pecofence_core::order::frequent_score(it, now) * 100.0).round() / 100.0,
                }),
                pecofence_core::SortMode::Date => json!({ "modified": it.mtime }),
                pecofence_core::SortMode::Size => json!({ "bytes": it.size }),
                pecofence_core::SortMode::Type => json!({
                    "group": pecofence_core::order::type_rank(it),
                    "extension": pecofence_core::order::extension(it),
                }),
                _ => json!({}),
            };
            let rows: Vec<Value> = items
                .iter()
                .take(limit)
                .enumerate()
                .map(|(i, (_, it))| {
                    json!({
                        "position": i + 1,
                        "name": it.display_name,
                        "file": pecofence_core::order::file_name(it),
                        "id": it.id.to_string(),
                        "detail": detail(it),
                    })
                })
                .collect();
            let changed = view.sort != content.view.sort;
            let mut text = format!(
                "“{}” by {}{}{}:\n",
                content.title,
                sort_key(view.sort),
                if view.reverse { ", reversed" } else { "" },
                if changed {
                    " (preview; the fence is unchanged)"
                } else {
                    ""
                }
            );
            for (i, (_, it)) in items.iter().take(limit).enumerate() {
                text.push_str(&format!("{:>4}. {}\n", i + 1, it.display_name));
            }
            if total == 0 {
                text.push_str("   (empty)\n");
            } else if total > limit {
                text.push_str(&format!(
                    "Showing {limit} of {total}; raise --limit to see more.\n"
                ));
            }
            Ok(Output {
                data: json!({
                    "fence": { "id": content.id.to_string(), "title": content.title },
                    "by": sort_key(view.sort),
                    "reverse": view.reverse,
                    "preview": changed,
                    "items": rows,
                }),
                text,
                meta,
            })
        }
        _ => Err(Failure::usage("missing command")),
    }
}

/// Runs one invocation; returns (exit code, stdout, stderr).
fn main_with(args: &[String], env: &Env, now: i64) -> (u8, String, String) {
    let json = args.iter().any(|a| a == "--json");
    let result = parse(args).and_then(|options| match options.command.as_str() {
        "help" => Ok(format!("{HELP}\n")),
        "version" => Ok(format!("pecofence-cli {}\n", env!("CARGO_PKG_VERSION"))),
        _ => run(&options, env, now).map(|output| {
            if json {
                let envelope = json!({
                    "schema_version": SCHEMA,
                    "ok": true,
                    "data": output.data,
                    "meta": output.meta,
                });
                format!("{envelope}\n")
            } else {
                output.text
            }
        }),
    });
    match result {
        Ok(stdout) => (0, stdout, String::new()),
        Err(failure) if json => {
            let envelope = json!({
                "schema_version": SCHEMA,
                "ok": false,
                "error": {
                    "code": failure.code,
                    "message": failure.message,
                    "retryable": false,
                    "suggested_action": failure.action,
                },
            });
            (failure.exit, String::new(), format!("{envelope}\n"))
        }
        Err(failure) => (
            failure.exit,
            String::new(),
            format!(
                "pecofence-cli: {}\nNext: {}\n",
                failure.message, failure.action
            ),
        ),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = Env {
        var: Box::new(|name| std::env::var(name).ok()),
        exe_dir: std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf)),
    };
    let (code, stdout, stderr) = main_with(&args, &env, pecofence_core::now_unix());
    print!("{stdout}");
    eprint!("{stderr}");
    ExitCode::from(code)
}

#[cfg(test)]
mod tests;

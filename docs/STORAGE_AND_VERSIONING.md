# Local storage, versions and status

Architecture review: 2026-10-21. This document describes the existing storage
contracts and the proposed shared inspection model for native Settings and CLI
clients. The proposed catalog and commands are not implemented yet.

## Ownership

| Store | Owner | Contents |
|---|---|---|
| `%APPDATA%\PecoFence\workspace.v2.json` | Application document and save coordinator | Settings, item catalog, layouts, rules, widget configuration, assignment undo records and layout snapshots |
| `workspace.v2.bak` | ConfigStore | Previous primary document |
| `workspace.v2.backups/` | ConfigStore | Whole-document daily backups |
| `workspace.v2.replaced-<UUID>.json` | Explicit document replacement | Raw primary bytes preserved before import, reset or recovery |
| `holiday-cache/` | Holiday worker | Refreshable calendar schedules |
| `%LOCALAPPDATA%\PecoFence` | Application diagnostics and watchdog | Logs and desktop-icon recovery marker |
| Installed `versions/` | Installer | Immutable executable, runtime, font and license payloads |
| Deployment backups | Installer/update workflow | Upgrade-specific configuration, deployment evidence and recovery files |
| SPM SQLite database | Separate SPM service | Backend business data, with its own schema and lifecycle |
| Repository `target/`, `.cache/`, `dist/` | Development and packaging tools | Build output and local verification artifacts |

Portable mode uses the executable's `config/` directory; `--config-dir` can select
another workspace location. File collections and portals contain references to
external files. A workspace backup does not copy those files.

Built-in widgets submit `HostCommand::PersistConfig`, which updates
`PanelSpec.config` inside the workspace. Saving a widget therefore writes the
whole document, including embedded snapshots. The registered `MemoryStorage`
service is a separate, unfinished abstraction: values and revisions are in
memory, its read method only returns a token, and keys are plain strings. New
providers need a defined durable storage, namespace and completion contract
before depending on that service.

## Saving and recovery

The host owns the current document and revision. One background writer accepts
one write at a time and coalesces intervening changes. Completion carries a ticket
with workspace activation and revision; an old result cannot acknowledge a newer
document. ConfigStore validates the document, writes and syncs a same-directory
staging file, rotates the prior primary and replaces the primary. Primary-save
failure and optional backup degradation have different outcomes. See
[persistence](PERSISTENCE.md) for exit and failure handling.

This coordinates writes within the application. Workspace-level cooperative
writer locking and external-file-change CAS remain future work. A CLI that edits
the JSON independently would bypass the host's revision protocol.

Recovery entries have different scopes:

- `.bak` advances with primary saves and holds the previous primary.
- Daily backups capture the first successful saved document on a UTC date. The
  retention limit is seven dated files, not seven revisions or necessarily seven
  consecutive calendar days.
- Layout snapshots are embedded in the primary, with a twenty-entry limit. They
  capture layouts and optional appearance, rather than the entire settings,
  rules and item catalog. Restore preserves current panel configuration for
  matching provider/instance IDs and removes references to missing catalog
  records. Thus restoring a layout does not rewind the current text of surviving
  matching note/todo instances. An absent current instance can be restored from
  the saved layout configuration.
- Replacement archives preserve original bytes before explicit replacement.
  ConfigStore currently has no retention policy for these archives.
- Deployment backups and executable rollback belong to the installer. Their
  contents vary by workflow and need an inventory before offering restoration.

Assignment undo records are limited to fifty rule-driven assignment records;
they do not describe every workspace change or every Shell file operation.
There is no complete durable file-transfer journal.

The schema-2 loader accepts the exact supported document format. Retained
`config.json`, `config.bak` and legacy backup directories are not automatically
migrated. Executable rollback needs workspace and provider-format compatibility
checks as well as a retained binary.

## Version identities

| Identity | Meaning |
|---|---|
| Git commit | Source implementation |
| Product version | Declared release family |
| Version plus archive hash | Exact immutable installation payload |
| Build receipt and package manifest | Build inputs, runtime provenance and file hashes |
| Workspace schema version | Document format compatibility |
| Provider configuration version | Widget format compatibility |
| Workspace activation plus revision | Current in-memory save coordination |
| Snapshot/backup identity | Recovery point and restoration scope |
| Deployment selection record | Installer's selected version and last observation |

Several payloads can share a product version. About and diagnostic output should
include the commit and package identity. Runtime document revisions restart with
a new activation and are not persistent historical revision numbers.

`deployment.json` is a deployment observation, not live application state.
The paired updater uses `active.json`; startup entries, shortcuts and running
process paths are additional references that can drift. A workspace hash changed
since installation should be reported as data changed since that observation.
Installed payload integrity is checked independently against package hashes.

The current executable CI budget is **7 MiB (7,340,032 bytes)**. Runtime and total
package sizes are measured separately. Package validation and release size-gate
results should remain separate status fields.

## Existing inspection and gaps

Settings already projects dirty/saving state, committed revision, recovery mode,
load/save issues and retry, plus snapshot, backup and import/export actions.
About shows the product version, configuration path, memory and item count.
`--validate-workspace` checks a stored document without starting the desktop.

The missing pieces are a unified inventory, recovery previews, version provenance,
retention planning and a machine-readable live status endpoint. Offline file
timestamps cannot establish whether the running document has unsaved changes.
`ConfigStore::list_backups()` currently turns enumeration errors into an empty
list; discovery failure needs its own result. The main log is truncated on startup
and cannot provide a durable event history.

## Proposed shared model

Introduce a read-only storage catalog whose entries carry stable identity, kind,
owner, location, size, capture time, provenance, format, compatibility, validation,
retention class and dependency links. Distinguish user data, recovery copies,
installed payloads, rebuildable caches and runtime observations. Return discovery
errors alongside available entries.

A live status projection should include API version, observation time and source
(`liveHost` or `diskInspection`), active build, workspace activation/revision,
committed revision, dirty/saving/read-only state, primary-save result, backup
health and external-change status. Unavailable offline runtime fields remain
unknown. Native Settings and agent clients consume the same model.

The native storage view should provide a category breakdown with drill-down, build
and save status, and a recovery timeline. Selecting a history entry shows its
restoration scope, compatibility, item/container changes and widget-data behavior.
Legacy files and rebuildable caches belong in separate categories from current
data and validated recovery points.

Proposed CLI surface:

```text
peco status --json
peco storage list --json
peco history list --json
peco history inspect <id> --json
peco restore plan <id> --json
```

Mutation should later consume a restore plan and expected document stamp through
the host, preserving one writer. Implement the catalog, error reporting, build
identity and status projection first; add recovery previews and retention plans
next. Independent widget storage and durable widget history can follow when their
lifecycle and query requirements are established.

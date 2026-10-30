---
name: pecofence
description: "Inspect and operate the PecoFence Windows desktop organizer with pecofence-cli: whether it is running, why it closed or crashed, closing it gracefully for a restart or upgrade, fence listings, sort previews and organizing suggestions. Also build, verify and install its packages with scripts/package_desktop.py. Start read-only; quit is the one command with a side effect."
---

# PecoFence

PecoFence is a Windows GUI. Agents use `pecofence-cli` for the app and its workspace, and
`scripts/package_desktop.py` in the checkout for packages. Use neither tool to change notes,
TODOs or fences.

**Find the CLI.** It sits beside the running app's executable
(`(Get-Process pecofence).Path`), or in the installation folder the user names. In a checkout,
use `cargo run -p pecofence-cli --`. From WSL, call the `.exe` directly. `deployment.json` only
records what was installed; the running process path is the truth.

```sh
pecofence-cli --help
pecofence-cli status --json
pecofence-cli fences --json
pecofence-cli suggest --limit 5 --json
```

| Command | Effect |
| --- | --- |
| `status`, `fences`, `suggest`, `order` | Read-only: the session record and the last saved workspace |
| `quit [--wait S]` | Asks the app to save and exit, like the tray's Exit. Never forces. |
| `package_desktop.py verify`, `install --dry-run` | Read-only archive checks |
| `package_desktop.py capture`, `record-build`, `portable`, `install` | Write builds, packages or versions; check `--help` first |

## Tasks

- **"Is it running?" or "why did it close?"** Run `status --json`.
  - `state` is one of `running`, `closing`, `exited`, `crashed`, `endedUnexpectedly` or
    `noRecord` (an app older than session records).
  - For `exited`, read `exit`:
    - `normal`: fine.
    - `pumpFallback`: saved, then a late WinUI exit.
    - `discardedChanges`: the user chose to drop unsaved changes.
    - `unclosed`: Windows sign-out or a fatal UI error.
  - For `crashed`, report `exitCodeHex` and point to `logs.previous`. Quote only the few log
    lines that matter.
  - A running app with `watchdog.alive: false` cannot record a crash. Say so.
- **"Tidy my desktop."** Run `suggest --json` and present each `summary` with its `reason`.
  The user applies a suggestion with a right-click on the fence → Suggestions (建议).
  `order --fence <id> --by <mode>` previews an order. Never edit `workspace.v2.json`.
- **Restart or upgrade.** Only when the user asked for it:
  1. Run `quit --wait 60 --json` and require `exit: normal`, or `state: notRunning`.
  2. Back up the configuration, keep the old version for rollback, then replace the binaries.
  3. Start the app and confirm with `status` that it is `running` with the watchdog alive.
  4. Verify hashes and preserve layouts and TODO contents.
- **Packaging.** Run `package_desktop.py verify --archive <zip> --sha256 <trusted> --summary --json`,
  then `install ... --dry-run`. Use Python 3.14 or newer, from the checkout.
  - A checksum computed from an untrusted download authenticates nothing.
  - Fix verification failures; don't retry with checks disabled.
  - Release packaging requires clean source.

## Output and errors

`--json` prints one envelope: `{"schema_version":"1.0","ok":…,"data"|"error":…,"meta":…}`.
Errors go to stderr with `code`, `message`, `retryable` and `suggested_action`. Exit codes are
0 for success (empty results included), 1 for an operational failure and 2 for invalid
arguments.

- `FENCE_AMBIGUOUS`: retry with the id prefix from `fences`.
- `WORKSPACE_UNAVAILABLE` / `WORKSPACE_INVALID`: pass `--workspace`. Backups are in
  `workspace.v2.backups`.
- `QUIT_TIMEOUT`: the app is still closing, usually because a save-failure dialog is waiting.
  Tell the user to answer it. Do not kill the process; that loses unsaved changes.
- Bounded lists report `meta.total` and `meta.truncated`. Raise `--limit` instead of assuming
  the list is complete.

## Boundaries

- `--shell-command new-note|new-todo` creates a component. Use it only when asked, and verify
  the result: dispatch is not proof that it saved.
- `--validate-workspace` and `--probe-spm` are diagnostics, not data APIs. Do not invent
  TODO or note commands.
- Workspace data, session records and logs are private. Do not upload them, and do not alter
  the SPM database to inspect packaging.
- Details: `docs/LIFECYCLE.md`, `docs/SUGGESTIONS.md`, `docs/AGENT_CLI_GUIDANCE.md`.

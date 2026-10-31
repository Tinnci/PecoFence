<img src="icons/workspace.svg" alt="" width="64" height="64">

# Tinnci Workspace · 工作台

An independent Windows window for discovering the local tool ecosystem and
understanding first-time setup. Version 0.3 adds first-launch prerequisite diagnosis and recovery. PecoFence and spmd keep their own lifetimes.
Closing Workspace leaves both running.

This first increment provides Home, a per-component setup checklist and Health.
All cards and steps come from `dev-cli hub --json`; no project or account fixture
is bundled. Opening Home reads inventory without running checks. Each checkable component can also be checked on its own; other observations and their timestamps are preserved. “检查状态” runs
`hub --check`, with the CLI's read-only check policy. Unimplemented setup steps
remain “待支持”; this increment does not execute setup commands or accept tokens.
Board, timeline, issues, CI and accounts pages follow in later increments.

## Run and build

Requirements: Windows 11 x64, WebView2, Rust MSVC, C++ Build Tools, Node >=24,
and the default WSL distribution with `dev-cli` installed. The hub command first
shipped in tooling commit `4884bc0`. Component-scoped checks require
tooling commit `670b4f6` or newer on `main`. The desktop needs the
installed CLI, not a tooling checkout at a hardcoded path.

The backend asks WSL for `HOME` and `PATH`, then resolves `.local/bin/dev-cli`.
It prepends the CLI directory and the user tool directory to the child
process PATH, so non-interactive WSL can discover and run installed tools. A nonstandard
installation can set the Windows user environment variable `TINNCI_DEV_CLI_WSL`
to its absolute Linux executable path before starting Workspace. It never runs
a login shell or accepts arbitrary commands from the frontend.

From this directory on Windows:

```powershell
npm ci --ignore-scripts
npm run build
cargo build --release --locked --manifest-path src-tauri/Cargo.toml
./src-tauri/target/release/tinnci-workspace.exe
```

Cargo defaults to the `custom-protocol` feature, embedding the built frontend.
The resulting executable starts by double-click, without Vite or PecoFence.
For desktop UI development, use `npm run tauri -- dev --no-default-features`.
For WSL cross-builds, run `./apps/workspace/scripts/build-windows.sh` from the
repository root. It uses the existing MSVC and SDK overrides documented by
PecoFence's cross-build script.

For browser development on WSL:

```sh
npm ci --ignore-scripts
npm run bridge:build
npm run dev
```

The development-only localhost adapter invokes the same compiled Rust hub
projection. Rebuild the bridge after changing Rust. Production has no HTTP
listener. The Rust crate has its own workspace and lockfile so the organizer's
ordinary build does not acquire a web runtime dependency.

## Verification

```sh
npm test
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Hub DTOs are generated from Rust (`cargo run --manifest-path src-tauri/Cargo.toml
-- --export-types`) and checked for drift by a test. SPM wire DTOs and the daemon
transport will be added with the first data page. The current Home does not
open the SPM database or depend on a running daemon.

The native boundary exposes only inventory/check, validated launch-target and
first-frame commands. A capability file grants them to the local main window;
CSP blocks remote content. Owner commands use bounded stdout/stderr, an overall
65-second timeout, direct argv and null stdin. Concurrent hub calls are refused.
Windows cleanup terminates the direct child; owner checks retain their own
bounded subprocess deadlines. The UI retains previous observations on refresh
failure and labels unchecked and planned states explicitly.

## Diagnosis and prerequisites

Health and the setup checklist display per-component check times, controlled
failure explanations and expandable error-code/exit-code details. A component
without declared automatic checks has no retry button. Planned steps stay
unavailable. Dependency buttons open the prerequisite component's checklist;
local prerequisite-step buttons focus the referenced step.

Scoped results carry `scope` and exactly one component. Rust validates the
response scope; React merges that component into the last full inventory.
A failed attempt retains previous observations, and the error-panel retry
repeats the same request. `checked_at` means a check ran, including an unknown
or failed outcome; it is separate from successful setup. Source diagnostic
messages, command arguments and executable paths stay outside the UI DTO.

The generated DTO contract remains additive. Older full-inventory responses
are accepted; scoped responses require a current dev-cli. An unexpected scope
or malformed scoped response leaves the previous inventory intact.

## Native measurement

Run with no other Workspace instance open, from Windows or WSL:

```sh
python scripts/measure.py src-tauri/target/release/tinnci-workspace.exe
```

From WSL use `src-tauri/target/x86_64-pc-windows-msvc/release/…` instead. The script
launches an explicitly timed instance, checks second-launch handoff, observes
process metadata including WebView2 children, and waits for normal timed exit.
The explicit `--smoke-test-ms 1000..60000` option is available in both profiles.

Local x64 release observation, 2026-10-31, executable on WSL storage:

| Measurement | First observation | Repeat observation |
|---|---:|---:|
| Launch to first-frame callback | 1,322 ms | 984 ms |
| Backend initialization to callback | 1,105 ms | 770 ms |
| App process working set | 46.97 MiB | 46.89 MiB |
| App + WebView2 summed working set | 479.20 MiB | 471.75 MiB |
| App + WebView2 private working set | 135.49 MiB | 145.00 MiB |
| Second launch / timed exit | exit 0 / exit 0 | exit 0 / exit 0 |
| Workspace instances after second launch | 1 | 1 |

The callback runs after two animation frames of the initial shell. It measures
shell presentation; inventory loading finishes separately. Summed working sets
include shared pages; private working set is reported separately. ADR-0003's
1-second first-paint goal is met only in the repeat observation; its 150-MB
working-set goal remains unmet by the summed working set. Profile loader/WebView initialization and memory
before adding full data pages; retain both memory metrics for future comparisons.

Single-instance activation uses Tauri's official plugin, with validated targets
`--open home|setup|health`. It stores the latest target before emitting navigation
so a second launch during initial rendering is not lost. Item-level links and
installer/shortcut distribution are future work.

References: [Tauri capabilities](https://v2.tauri.app/security/capabilities/),
[single-instance plugin](https://v2.tauri.app/plugin/single-instance/),
[application ADR](https://github.com/Tinnci/dev-cli-tools/blob/main/docs/ADR-0003-spm-workspace-app.md).

## First-launch recovery (0.3)

A failed inventory load now returns a generated `LoadFailure` DTO with a controlled
code and observation time. Home explains missing/unavailable WSL, a default
Linux distribution that cannot start, a missing or non-executable dev-cli,
an incompatible hub capability, invalid JSON, timeout and output limits. It
shows owner installation instructions and a retry; no installation runs on open.
WSL installation guidance follows [Microsoft's installation guide](https://learn.microsoft.com/en-us/windows/wsl/install)
and [command reference](https://learn.microsoft.com/en-us/windows/wsl/basic-commands).

Windows probes the distribution listing, launches the default distribution to
read its environment, verifies the CLI executable and checks `hub --help` for
`--component`. A successful launch takes precedence over an empty distribution
listing: this host's WSL can return an empty listing when launched without a
console. These bounded probes and the hub call share an overall 65-second
budget. The frontend retains the last diagnostic while retrying, preserves the
previous inventory on failure, and repeats the original check/scope. Successful
recovery clears the diagnostic and renders inventory without restarting the app.
The development bridge relays only allow-listed diagnostic codes, never raw
process stderr. Diagnostic timestamps record attempts rather than successful setup.

Automated cases cover prerequisite failures stopping before component checks,
literal paths with spaces/Unicode, empty-listing recovery, capability mismatch,
first-launch recovery and scoped retry preservation. Native Windows and browser
verification results are recorded with the corresponding release checkpoint.

0.3 local verification, 2026-10-31: frontend 19 tests and native 11 tests passed;
strict Clippy passed for Linux and Windows targets. The Windows release
`--hub-json` returned `cli_missing` for a process-local missing CLI override and
`hub_invalid_response` for a temporary malformed-output CLI, then recovered to
all 13 real components with the override removed. The browser exercised an
incompatible CLI through the real Rust bridge, displayed installation guidance,
and recovered to inventory using Retry. Missing WSL/distribution cases use the
controlled process-runner tests. No WSL installation or source configuration was
changed by verification.

## Icon

`icons/workspace.svg` is the Windows app icon: an Art Deco 工, drawn as a
stepped ziggurat workbench with pinstripes and an inlaid diamond, rising from a
sunburst inside a double gold frame on deep emerald. Frames and rays blur below
48 px, so `icons/workspace-small.svg` (tile and enlarged silhouette) supplies the
16, 24 and 32 px sizes used by the title bar, tray and small taskbar.
`scripts/icons.sh` regenerates `src-tauri/icons` from both.

Following Windows guidance, the plated icon stays on Windows surfaces. Inside the
app, the sidebar `BrandMark` draws the same stepped 工 as a single-colour glyph in
the interface's text colour; the browser favicon uses the small variant.


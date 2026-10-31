# Tinnci Workspace · 工作台

An independent Windows window for discovering the local tool ecosystem and
understanding first-time setup. PecoFence and spmd keep their own lifetimes.
Closing Workspace leaves both running.

This first increment provides Home, a per-component setup checklist and Health.
All cards and steps come from `dev-cli hub --json`; no project or account fixture
is bundled. Opening Home reads inventory without running checks. “检查状态” runs
`hub --check`, with the CLI's read-only check policy. Unimplemented setup steps
remain “待支持”; this increment does not execute setup commands or accept tokens.
Board, timeline, issues, CI and accounts pages follow in later increments.

## Run and build

Requirements: Windows 11 x64, WebView2, Rust MSVC, C++ Build Tools, Node >=24,
and the default WSL distribution with `dev-cli` installed. The hub command first
shipped in tooling commit `4884bc0` (task/system-lifecycle). The desktop needs the
installed CLI, not a tooling checkout at a hardcoded path.

The backend asks WSL for `HOME`, then resolves `.local/bin/dev-cli`. A nonstandard
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
[application ADR](https://github.com/Tinnci/dev-cli-tools/blob/task/system-lifecycle/docs/ADR-0003-spm-workspace-app.md).

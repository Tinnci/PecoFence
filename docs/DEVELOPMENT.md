# Development

## Prerequisites

- Windows 11 x64.
- Rust stable (MSVC toolchain); the workspace's minimum Rust version is in `Cargo.toml`.
- Visual Studio Build Tools with the Desktop development with C++ workload and Windows SDK.
- Microsoft Edge WebView2 Runtime to use Settings.
- Python >=3.11 for catalog checks/source packaging.
- Node.js >=22 (CI uses 24), PowerShell 7 and Chrome or Edge for complete Settings verification.

Public builds use only this repository and public registries. The local
`crates/spm-contracts` crate is version 0.1.0; no SPM checkout or private token
is required. Ordinary desktop fences work without the optional private backend;
live SPM data needs an authorized backend installation, not distributed here.
See [SPM_BOUNDARY.md](SPM_BOUNDARY.md) for the contract and verification boundary.

Python is not pinned to a minor version. The validation, site-build and
source-packaging scripts use the standard library, including `tomllib` (available
since 3.11), and support newer Python releases. CI uses the latest stable Python 3.
If the system `python` is older, use `uv` without replacing it or adding a version pin:

```powershell
uv run --no-project --python ">=3.11" python scripts/check-locales.py
```

The same prefix works for other Python scripts. `setup-toolchain.ps1` resolves
a compatible Python through `uv` and updates Rust stable, rustfmt and Clippy.

## Build and run

```powershell
cargo build --locked
Copy-Item third_party/webview2/WebView2Loader.x64.dll target/debug/WebView2Loader.dll
./target/debug/pecofence.exe
```

The WebView2 loader is imported at process startup and must be next to the executable,
even if Settings is not opened. The watchdog should also be packaged beside the app.

For an isolated test instance, use a separate directory, `--portable`,
`--no-hide-icons` and a unique `PECOFENCE_INSTANCE`. Keep autostart disabled in its
configuration. Portable startup leaves the Windows autostart entry alone.
`--exit-after <milliseconds>` closes a smoke-test instance automatically.

### Build disk usage

Development and test builds disable debug symbols and incremental compilation
to keep `target/` smaller. Debug assertions and overflow checks remain enabled;
the tradeoffs are less detailed debugger/backtrace information and slower rebuilds.
Release builds retain line-table symbols for crash diagnosis. Only `pecofence-core`
uses size optimization for model/protocol code; renderer/platform/application keep
performance optimization. The 4.5 MiB executable gate is unchanged.

For a debugger session, temporarily set `CARGO_PROFILE_DEV_DEBUG=2` (or
`CARGO_PROFILE_TEST_DEBUG=2` for tests). Incremental compilation can likewise be
enabled with `CARGO_PROFILE_DEV_INCREMENTAL=true` or
`CARGO_PROFILE_TEST_INCREMENTAL=true`. Unset the overrides afterward to return
to the low-disk defaults.

Changing profiles does not remove old symbols or incremental caches. To reclaim
existing development/test artifacts, run `cargo clean --profile dev`; the next
build/test will rebuild them. For cross-target builds, add
`--target x86_64-pc-windows-msvc`.

Runtime logging is separate from build artifacts. The app defaults to `info`;
set `RUST_LOG=warn` to reduce log output, or `RUST_LOG=debug` when diagnosing issues.

## Verification

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python scripts/check-locales.py
python scripts/check-readme-translations.py
node --test scripts/test-settings-client.cjs
pwsh -NoProfile -File scripts/test-settings-browser.ps1
```

For the complete native CI/release gates and a portable ZIP, run
`./scripts/build-and-verify.ps1` on Windows. It shares the build contract used by
both workflows, prints per-command timings and compiles release binaries only
once. See [RELEASING.md](RELEASING.md#ci-and-deployment-flow) for authentication,
cache boundaries and deployment triggers.

`-SourceOnly` runs source checks without native compilation. CI runs that phase,
then `-SkipSourceChecks` for the native phase; neither phase reads private SPM
source or authenticates to it. The default local command runs both.
Required source and headless/synthetic protocol tests cannot silently skip
failures behind feature gates. Live private-daemon integration is a separate
authorized private workflow; public test success does not prove it was run.
Packaging/immutable installation and build receipts are
described in [PACKAGING.md](PACKAGING.md). Native UI replacement is the chosen
direction in [ADR-007](decisions/ADR-007-native-settings.md), not an implemented host.

The browser command launches headless Chrome/Edge against the production Settings
page with a local mock bridge; it does not start PecoFence or change desktop settings.
Use `-Browser <path>` if needed. It covers rule editing/AND conditions, conflicts,
language switching, draft/user data preservation and narrow-window layout.
To inspect it interactively instead, run `node scripts/test-settings-ui.mjs` and
open the printed loopback URL. See [SETTINGS_PROTOCOL.md](SETTINGS_PROTOCOL.md) for
the current protocol and remaining OS/persistence boundaries.

Some platform tests use real Windows APIs and a desktop session. They are not a
substitute for testing native menus, multiple monitors and supported Windows builds.

After building the package binaries, `python scripts/test-language-smoke.py`
opens an isolated portable Settings window, changes all ten languages through the
typed Settings application handler, and checks saved preferences and preservation
of names/autostart. This native test intent bypasses the page/session handshake;
the protocol and browser suites above verify that boundary separately.
It keeps its generated configuration and report under `.cache/`.

## Architecture

| Directory | Responsibility |
|---|---|
| `crates/core` | Platform-independent models, configuration, rules, geometry and translation |
| `crates/platform` | Win32, shell, desktop integration and display-language detection |
| `crates/render` | Direct2D/Composition drawing, motion and glass |
| `crates/app` | Native windows, input, application state and Settings IPC |
| `crates/watchdog` | Restores desktop icons after an abnormal app exit |
| `crates/spm-contracts` | Public shared SPM wire contracts (0.1.0), not the private daemon |
| `ui` | Offline settings HTML and localization helper embedded into the executable |
| `locales` | Shared native and settings messages |

Regenerate Win32 bindings with `cargo run -p tool_bindgen`. The definitions in
`tools/bindgen` generate platform, Composition and GPU-glass bindings.

`vendor/windows-composition` carries the small upstream wrapper patch needed by
the renderer. Preserve its license files when changing or distributing it.

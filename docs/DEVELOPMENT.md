# Development

## Prerequisites

- Windows 11 x64.
- Rust stable (MSVC toolchain); the workspace's minimum Rust version is in `Cargo.toml`.
- Visual Studio Build Tools with the Desktop development with C++ workload and Windows SDK.
- Python >=3.11 for catalog checks/source packaging.

Public builds use only this repository and public registries. The local
`crates/spm-contracts` crate is version 0.1.0; no SPM checkout or private token
is required. Ordinary desktop fences work without the optional private backend;
live SPM data needs an authorized backend installation, not distributed here.
See [SPM_BOUNDARY.md](SPM_BOUNDARY.md) for the contract and verification boundary.

Python is not pinned to a minor version. The validation and source-packaging
scripts use the standard library, including `tomllib` (available
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
uv run --no-project --python ">=3.11" python scripts/stage-winappsdk-runtime.py `
  stage --target-dir target --profile debug
./target/debug/pecofence.exe
```

The staging command downloads the pinned public Windows App SDK NuGet once,
verifies its hashes and places the audited runtime beside the executable.
Settings use native Win32 controls; no WebView2 loader or Edge WebView2 Runtime is
required. The watchdog should also be packaged beside the app.

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
performance optimization. The `pecofence.exe` gate is 6.5 MiB (6,815,744 bytes);
the 59,158,503-byte SDK runtime inventory is measured and bounded separately.

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
```

For the complete native CI/release gates and a portable ZIP, run
`./scripts/build-and-verify.ps1` on Windows. It shares the build contract used by
both workflows, prints per-command timings and compiles release binaries only
once. See [RELEASING.md](RELEASING.md#ci-and-deployment-flow) for authentication,
cache boundaries and deployment triggers.

`-SourceOnly` runs Python/source checks only: it does not run Cargo, download the
SDK runtime, launch a GUI or read/authenticate to private SPM. CI runs that phase,
then `-SkipSourceChecks` for the native phase; the full phase runs format checks,
stages the runtime for debug tests and builds/packages release. The default local
command runs both.
Required source and headless/synthetic protocol tests cannot silently skip
failures behind feature gates. Live private-daemon integration is a separate
authorized private workflow; public test success does not prove it was run.
Packaging/immutable installation and build receipts are
described in [PACKAGING.md](PACKAGING.md). [ADR-007](decisions/ADR-007-native-settings.md)
records the decision and migration history for native Settings.

Some platform tests use real Windows APIs and a desktop session. They are not a
substitute for testing native menus, multiple monitors and supported Windows builds.

After building the package binaries, `python scripts/test-language-smoke.py`
opens an isolated portable Settings window, changes the configured languages
through the typed Settings application handler, and checks saved preferences and
preservation of names/autostart. It requires an interactive Windows desktop and
may briefly open a window; it keeps generated configuration and reports under
`.cache/`. The script checks the current user's autostart entries before and after
the run and expects them to be unchanged.

## Architecture

| Directory | Responsibility |
|---|---|
| `crates/core` | Platform-independent models, configuration, rules, geometry and translation |
| `crates/platform` | Win32, shell, desktop integration and display-language detection |
| `crates/render` | Direct2D/Composition drawing, motion and glass |
| `crates/app` | Native windows, input, application state and Settings UI |
| `crates/watchdog` | Restores desktop icons after an abnormal app exit |
| `crates/spm-contracts` | Public shared SPM wire contracts (0.1.0), not the private daemon |
| `locales` | Shared native and settings messages |

Regenerate Win32 bindings with `cargo run -p tool_bindgen`. The definitions in
`tools/bindgen` generate platform, Composition and GPU-glass bindings.

`vendor/windows-composition` carries the small upstream wrapper patch needed by
the renderer. Preserve its license files when changing or distributing it.

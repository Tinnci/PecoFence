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

Local validation and source-packaging scripts use the standard library,
including `tomllib` (available since 3.11), and support Python 3.11 or newer.
Windows CI separately pins Python 3.13 for repeatable worker setup. If the system
`python` is older, use `uv` without replacing it:

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
./scripts/build-and-verify.ps1 -Mode SourceOnly
./scripts/build-and-verify.ps1 -Mode Quick
./scripts/build-and-verify.ps1 -Mode Quick -TestPackage "PACKAGE_NAME"
./scripts/build-and-verify.ps1 -Mode Full
```

Replace `PACKAGE_NAME` with an exact Cargo workspace package name. It narrows
Quick's ordinary workspace tests only; the Reactor shutdown and workspace
validator suites remain mandatory. SourceOnly performs Python source-policy
checks only, with no dependency/runtime downloads, Rust/Cargo or GUI. Quick adds
formatting, resolved public-dependency policy, strict Clippy, debug runtime
staging and executed tests. Full runs all existing gates, including generated
binding consistency, release build and size checks, and real portable-ZIP
validation. Full is the default when `-Mode` is omitted; Quick or a narrowed
Quick test selection never reduces Full or release acceptance.

The entry point requires PowerShell 7+, Python 3.11+ and Git; SourceOnly needs
no Rust or Windows SDK. Add `-ReportPath target/verification-report.json` for a schema-version-1 aggregate JSON
report, or `-TimeoutSeconds 0` for an unlimited global time budget. A nonzero
timeout applies across the entire invocation. The console prints each check with
`[PASS]`, `[FAIL]`, `[ERROR]`, `[TIMEOUT]` or `[NOT_RUN]`, followed by `RESULT`,
`MODE`, any failed check and a next action. Exit 0 means the requested scope
passed; 1 is a gate failure; 2 indicates invalid arguments, a missing tool or
configuration, or a report error; 124 indicates timeout. Partial reports are
written on failure when possible. A report records only checks actually run in
that invocation as passed. Raw stdout remains in terminal logs; report JSON
contains no secret environment variables or credentials.

The legacy `-SourceOnly` and `-SkipSourceChecks` switches remain accepted for
older callers; prefer `-Mode` for new commands. See
[VERIFICATION_GATES.md](VERIFICATION_GATES.md) for the complete check matrix,
report fields and scope boundaries. See [RELEASING.md](RELEASING.md#ci-and-deployment-flow)
for authentication, cache boundaries and deployment triggers.

Quick and Full run as independent CI jobs. Full remains the build job and
produces portable artifacts. SourceOnly still takes about five seconds; the
previously measured 11m6s CI run is a historical reference. Do not claim an
optimization benefit until the updated tests and CI have rerun; separate runners
can increase total compute even when Quick provides earlier feedback.
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

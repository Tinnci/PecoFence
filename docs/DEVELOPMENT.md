# Development

## Prerequisites

- Windows 11 x64.
- Rust stable (MSVC toolchain); the workspace's minimum Rust version is in `Cargo.toml`.
- Visual Studio Build Tools with the Desktop development with C++ workload and Windows SDK.
- Microsoft Edge WebView2 Runtime to use Settings.
- Python >=3.11 for catalog checks/source packaging; Node.js for the settings browser tests.

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
Release builds retain line-table symbols for crash diagnosis.

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
node scripts/test-settings-ui.mjs
```

For the complete native CI/release gates and a portable ZIP, run
`./scripts/build-and-verify.ps1` on Windows. It shares the build contract used by
both workflows, prints per-command timings and compiles release binaries only
once. See [RELEASING.md](RELEASING.md#ci-and-deployment-flow) for authentication,
cache boundaries and deployment triggers.

Open the address printed by the last command. The browser suite uses a mock host
bridge and does not change the running desktop application's settings. It covers
language switching, draft/user data preservation and narrow-window layout.

Some platform tests use real Windows APIs and a desktop session. They are not a
substitute for testing native menus, multiple monitors and supported Windows builds.

After building the package binaries, `python scripts/test-language-smoke.py`
opens an isolated portable Settings window, changes all ten languages through the
real IPC handler, and checks saved preferences and preservation of names/autostart.
It keeps its generated configuration and report under `.cache/`.

## Architecture

| Directory | Responsibility |
|---|---|
| `crates/core` | Platform-independent models, configuration, rules, geometry and translation |
| `crates/platform` | Win32, shell, desktop integration and display-language detection |
| `crates/render` | Direct2D/Composition drawing, motion and glass |
| `crates/app` | Native windows, input, application state and Settings IPC |
| `crates/watchdog` | Restores desktop icons after an abnormal app exit |
| `ui` | Offline settings HTML and localization helper embedded into the executable |
| `locales` | Shared native and settings messages |
| `site` | Static product website, built by `scripts/build-site.py` (see [WEBSITE.md](WEBSITE.md)) |

Regenerate Win32 bindings with `cargo run -p tool_bindgen`. The definitions in
`tools/bindgen` generate platform, Composition and GPU-glass bindings.

`vendor/windows-composition` carries the small upstream wrapper patch needed by
the renderer. Preserve its license files when changing or distributing it.

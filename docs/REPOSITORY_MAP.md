# Repository map

This repository builds PecoFence, a native Windows 11 desktop organizer. The
Rust workspace, packaging, translations and verification scripts all live here.

Maon, the Tauri workspace app, was split out and is maintained as a separate
repository. It is not a member of this Cargo workspace and is not built by these
scripts.

## Product

| | PecoFence |
| --- | --- |
| What it is | Native Windows 11 desktop organizer |
| Source root | repository root |
| Rust workspace | root `Cargo.toml` |
| UI | Native WinUI 3 / Win32 |
| Main binary | `pecofence.exe` |
| Satellite binaries | `pecofence-watchdog.exe`, `pecofence-cli.exe`, shell-extension DLL |
| Build entry point | `scripts/build-desktop.ps1`, `scripts/build-windows.sh` |
| Package | portable ZIP from `scripts/make-portable.ps1` |
| Install | `scripts/install-user.ps1 -Archive <zip>` |
| Install target | `%LOCALAPPDATA%\Tinnci\PecoFence\versions` |

## Directory map

```text
/
├── Cargo.toml                  PecoFence Rust workspace
├── crates/
│   ├── app/                    pecofence.exe (native shell and wiring)
│   ├── core/                   model, workspace and protocol logic
│   ├── platform/ render/       Windows platform and rendering adapters
│   ├── plugin-api/             public plugin boundary
│   ├── plugin-kernel/          plugin loading and lifecycle
│   ├── plugin-spm/             live SPM integration
│   ├── plugin-widgets/         note, TODO, calendar and diagram widgets
│   ├── cli/                    pecofence-cli.exe
│   ├── watchdog/               pecofence-watchdog.exe
│   ├── shell-extension/        Explorer context-menu COM DLL
│   ├── shell-contract/         shared shell types
│   ├── spm-contracts/          public SPM wire contract (shared with other tools)
│   └── work-calendar/          regional work-day data (shared with other tools)
├── docs/                       current PecoFence and repo-level docs
│   ├── archive/                retired designs and historical reviews
│   ├── decisions/              ADRs
│   └── readme/                 root README translations
├── locales/                    PecoFence translations
├── scripts/                    build, verification, packaging and lab scripts
├── packaging/msix/             MSIX identity and manifest
├── vendor/                     patched third-party Rust crates
├── third_party/                fonts, holiday data, Windows App SDK notices
├── tools/bindgen/              workspace member used to generate bindings
├── extras/pecofence-promo/     independent promo-video project
└── skills/pecofence/           local agent skill definition
```

Generated trees are not part of this map: `target/`, `dist/`, `node_modules/`,
`.venv/`, `.cache/`, `.claude/`, and `docs/history/`.

## Artifact flow

```text
crates/* + locales/ + third_party/
  → cargo build --release
  → target/release/pecofence.exe (+ watchdog, CLI, shell extension)
  → scripts/make-portable.ps1
  → dist/pecofence-<version>-x64.zip
```

The WSL cross-build uses Linux `clang-cl-19`, `lld-link` and `llvm-rc` against
the MSVC and Windows SDK libraries mounted under `/mnt/c`. No Windows compiler
process is involved. Details are in [ENVIRONMENT.md](ENVIRONMENT.md).

## Documentation map

- Root [README.md](../README.md): product, install and build entry point.
- [docs/README.md](README.md): current documentation index.
- [docs/archive/](archive/): historical proposals and reviews. Not current behavior.

Use `docs/` for PecoFence or cross-product material, and `docs/archive/` for
anything superseded. Build scripts stay with the product they build; shared
verification and packaging policy stays under root `scripts/`.

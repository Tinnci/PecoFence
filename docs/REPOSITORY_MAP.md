# Repository map

This repository builds two independent Windows products and publishes two shared
Rust crates. The root is PecoFence. Maon lives under `apps/workspace` and is not
a module of the PecoFence Cargo workspace.

## Products

| | PecoFence | Maon |
| --- | --- | --- |
| What it is | Native Windows 11 desktop organizer | Tauri window for SPM models, changes and builds |
| Source root | repository root | `apps/workspace` |
| Rust workspace | root `Cargo.toml` | `apps/workspace/src-tauri/Cargo.toml` (independent) |
| UI | Native WinUI 3 / Win32 | React 19 + Vite in `apps/workspace/src` |
| Main binary | `pecofence.exe` | `lune.exe` |
| Satellite binaries | `pecofence-watchdog.exe`, `pecofence-cli.exe`, shell-extension DLL | none |
| Build entry point | `scripts/build-desktop.ps1`, `scripts/build-windows.sh` | `apps/workspace/scripts/build-windows.sh` |
| Package | portable ZIP from `scripts/make-portable.ps1` | single executable; no installer or bundle |
| Install | `scripts/install-user.ps1 -Archive <zip>` | `apps/workspace/scripts/install-user.ps1 -Executable <exe>` |
| Install target | `%LOCALAPPDATA%\Tinnci\PecoFence\versions` | `%LOCALAPPDATA%\Tinnci\Workspace\versions\<version>-<sha256>` |

Maon's product and binary names differ from its directory. These are intentional
compatibility names, not three projects:

| Context | Name |
| --- | --- |
| Product and window title | Maon |
| Directory | `apps/workspace` |
| npm package | `lune` |
| Rust package | `lune` |
| Executable | `lune.exe` |
| Tauri identifier | `dev.tinnci.workspace` |
| Install directory | `%LOCALAPPDATA%\Tinnci\Workspace` |
| Older package name | `tinnci-workspace` (retired) |

Keep the executable name and Tauri identifier stable unless the installer,
shortcuts, WebView data directory and upgrade path move together.

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
├── apps/
│   └── workspace/              Maon: React frontend, Tauri Rust crate, its docs and scripts
├── docs/                       PecoFence and repo-level current docs
│   ├── archive/                retired designs and historical reviews
│   ├── decisions/              ADRs
│   └── readme/                 root README translations
├── locales/                    PecoFence translations
├── scripts/                    PecoFence build, verification, packaging and lab scripts
├── packaging/msix/             PecoFence MSIX identity and manifest
├── vendor/                     patched third-party Rust crates
├── third_party/                fonts, holiday data, Windows App SDK notices
├── tools/bindgen/              workspace member used to generate bindings
├── extras/pecofence-promo/     independent promo-video project
└── skills/pecofence/           local agent skill definition
```

Generated trees are not part of this map: `target/`, `dist/`,
`apps/workspace/dist/`, `apps/workspace/src-tauri/target/`, `node_modules/`,
`.venv/`, `.cache/`, `.claude/`, and `docs/history/`.

## Artifact flow

```text
PecoFence
  crates/* + locales/ + third_party/
    → cargo build --release
    → target/release/pecofence.exe (+ watchdog, CLI, shell extension)
    → scripts/make-portable.ps1
    → dist/pecofence-<version>-x64.zip

Maon
  apps/workspace/src + public/ + index.html
    → npm run build
    → apps/workspace/dist/
  apps/workspace/src-tauri/ + embedded dist/
    → cargo build --release --target x86_64-pc-windows-msvc
    → apps/workspace/src-tauri/target/x86_64-pc-windows-msvc/release/lune.exe
    → install-user.ps1 or install-windows.sh
    → %LOCALAPPDATA%/Tinnci/Workspace/versions/<version>-<sha256>/lune.exe
```

The WSL cross-build uses Linux `clang-cl-19`, `lld-link` and `llvm-rc` against
the MSVC and Windows SDK libraries mounted under `/mnt/c`. No Windows compiler
process is involved. Details are in [ENVIRONMENT.md](ENVIRONMENT.md) and the
[Maon build guide](../apps/workspace/docs/BUILD.md).

## Documentation map

- Root [README.md](../README.md): PecoFence product and install.
- [docs/README.md](README.md): current PecoFence and repo-level documentation index.
- [apps/workspace/README.md](../apps/workspace/README.md): Maon behavior, runtime
  boundary and feature history.
- [apps/workspace/docs/BUILD.md](../apps/workspace/docs/BUILD.md): Maon toolchain,
  artifact and install.
- [docs/archive/](archive/): historical proposals and reviews. Not current behavior.

Use `docs/` for PecoFence or cross-product material, `apps/workspace/docs/` for
Maon-only material, and `docs/archive/` for anything superseded. Build scripts
stay with the product they build; shared verification and packaging policy stays
under root `scripts/`.

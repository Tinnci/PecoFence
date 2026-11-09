# Build, artifact and install

Maon is a Tauri 2 desktop app. `npm run build` produces the frontend, and one
`cargo build` embeds it into a single Windows executable. There is no installer,
sidecar, or bundled WebView2 runtime: `bundle.active` is `false`.

## Artifact identity

- Cargo package and command: `lune` → `lune.exe`
- Product name: `Maon`
- Source crate: `apps/workspace/src-tauri`
- WSL build output: `apps/workspace/src-tauri/target/x86_64-pc-windows-msvc/release/lune.exe`
- Windows build output: `apps/workspace/src-tauri/target/release/lune.exe`
- Install root: `%LOCALAPPDATA%\Tinnci\Workspace\versions\<version>-<sha256-prefix>\lune.exe`

The directory suffix is the first 12 hex characters of the executable's SHA256,
not a Git commit. Use this to identify the installed binary:

```sh
sha256sum lune.exe
git rev-parse HEAD
```

Observed on 2026-11-09: the installed `0.10.0-9e25dbaeb7b7` directory contained
a 15,253,504-byte `lune.exe` with SHA256
`9e25dbaeb7b747672d4c8ea6d64a5e34418f109a37084f421ee62b3fcb3a1622`, byte-for-byte
identical to the WSL release artifact. The Git HEAD at that time was
`149b62e36dd4d71e2bb72af71b50fe9b36e81a1b`; the hash suffix does not encode it.

## Toolchain

The WSL build is a cross-compile to `x86_64-pc-windows-msvc`. No Windows compiler
runs. Linux `clang-cl-19` and `lld-link` read the MSVC headers/libraries and
Windows SDK libraries through `/mnt/c`; `llvm-rc` compiles Windows resources.
Only the final installation helper invokes `powershell.exe`.

What the scripts expect, with their defaults:

| Component | Default |
| --- | --- |
| Rust | `cargo` / `rustc` on `PATH` (1.99 observed) |
| C compiler | `clang-cl-19` |
| Linker | `lld-link` |
| Resource compiler | `llvm-rc` |
| Windows SDK | `/mnt/c/Program Files (x86)/Windows Kits/10`, version `10.0.26100.0` |
| MSVC | `/mnt/c/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.44.35207` |
| Node / npm | Node >= 24 (`npm ci` runs with `--ignore-scripts`) |

Override the two Windows locations with `PFW_SDK_ROOT`, `PFW_WINDOWS_SDK_VER`,
and `PFW_MSVC_ROOT`. Build output stays in the repository's ext4 `target/`
tree; the scripts unset `CARGO_TARGET_DIR` so an ambient Windows variable cannot
redirect it.

## Build from WSL

From the repository root:

```sh
./apps/workspace/scripts/build-windows.sh
```

The script performs the whole pipeline:

1. `npm ci --prefix apps/workspace --ignore-scripts`
2. `npm run build --prefix apps/workspace` → `apps/workspace/dist`
3. `cargo build --locked --release --manifest-path apps/workspace/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc`

It exports `LIB` with Windows-style semicolons, sets
`CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=lld-link` and `RC=llvm-rc`, then
writes `src-tauri/target/x86_64-pc-windows-msvc/release/lune.exe`.

The default Cargo feature `custom-protocol` embeds `apps/workspace/dist` into
the executable. A release executable starts by double-click without Vite,
Node, or a PecoFence process.

For a debug cross-build, use the Windows-style profile through Cargo:

```sh
cargo build --locked --manifest-path apps/workspace/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
```

## Build from Windows

From `apps/workspace`:

```powershell
npm ci --ignore-scripts
npm run build
cargo build --release --locked --manifest-path src-tauri/Cargo.toml
./src-tauri/target/release/lune.exe
```

For desktop UI development with a Vite dev server:

```powershell
npm run tauri -- dev --no-default-features
```

The browser development path needs the Rust bridge:

```sh
npm ci --ignore-scripts
npm run bridge:build
npm run dev
```

That path is development-only. Production has no HTTP listener.

## Install

`install-user.ps1` is the clean per-user install. It copies the executable
into a new hash-named version directory, verifies the copy, and creates the
Maon Start menu and desktop shortcuts:

```powershell
.\scripts\install-user.ps1 `
  -Executable .\src-tauri\target\x86_64-pc-windows-msvc\release\lune.exe `
  -Version 0.10.0
```

Earlier version directories stay in place. `-NoDesktop` skips the desktop
shortcut; `-DryRun` prints the target and shortcut paths.

`install-windows.sh` is the WSL-side replacement path. It defaults to the
newest directory under `/mnt/c/Users/Administrator/AppData/Local/Tinnci/Workspace/versions`,
refuses a binary older than its source, moves a running executable aside as
`.old`, copies the new one, compares it with `cmp`, then asks Windows
PowerShell to prune older versions (keeping two). Use `install-user.ps1` for a
first or clean install; use `install-windows.sh` to update that install.

The desktop shortcut `%USERPROFILE%\Desktop\Maon.lnk` points at the installed
`lune.exe` and passes no arguments. Reinstall or rerun the helper after moving
the version directory.

## Verify

From `apps/workspace`:

```sh
npm test
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

The Windows cross-build variant is:

```sh
cargo test --locked --target x86_64-pc-windows-msvc --manifest-path src-tauri/Cargo.toml --no-run
```

Native measurement and the first-frame check are documented in the
[app README](../README.md#native-measurement).

## Common failures

- **Missing MSVC or SDK directory**: check the three paths in the table above,
  or set the `PFW_*` overrides. Do not copy the libraries into the repository.
- **An old binary is installed**: the install script compares timestamps; rebuild
  before reinstalling.
- **`CARGO_TARGET_DIR` is set globally**: the build script unsets it. Check it
  yourself if you invoke `cargo` directly.
- **Frontend changes do not appear**: a plain Cargo rebuild does not rerun Vite.
  Run `npm run build` first; the script does this automatically.

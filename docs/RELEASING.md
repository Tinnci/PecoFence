# Releases

## CI and deployment flow

PRs, pushes to `main` and manual CI runs execute the Windows `build` job. A
matching `v<version>` tag executes the release job against that tag's own source.
Both use [.github/actions/build-desktop/action.yml](../.github/actions/build-desktop/action.yml)
and [scripts/build-and-verify.ps1](../scripts/build-and-verify.ps1):

1. Install Rust stable with rustfmt/Clippy, latest stable Python 3 and Node 24.
2. Run public source/Actions policy, Settings browser and desktop packaging tests.
   Dependency resolution uses this repository plus public registries; there is
   no private SPM authentication or checkout.
3. Restore **public registry archives only**, never private Git checkouts or
   compiled intermediates. Crate downloads are reusable across CI/releases and
   Rust upgrades; native dependency compilation happens on each runner.
4. Run formatting, translation and Settings client checks before native compilation. Run Clippy,
   regenerate/compare bindings, compile and run workspace tests with the
   WebView2 loader staged beside their executables.
5. Compile `pecofence` and `pecofence-watchdog` once with `build-desktop.ps1`, record
   source/toolchain/binary fingerprints, enforce the
   existing 4.5 MiB application budget, then package those exact binaries with
   `make-portable.ps1 -SkipBuild`. All Cargo dependency resolution uses `--locked`.

CI uploads the ZIP/checksum as `pecofence-portable`, retained for seven days. It
does **not** publish a release or deploy the app. New commits cancel superseded
CI runs; release jobs are not cancelled this way. Both jobs have a 45-minute limit.

The tag workflow checks that the tag matches `Cargo.toml`, runs the same gates,
then creates a **draft** GitHub Release. Publishing that draft is a separate
manual step. It does not imply Store or winget publication.

Tag packaging requires clean source and selects exactly the current version's
ZIP/checksum, not a wildcard collection of possibly stale versions.
See [PACKAGING.md](PACKAGING.md) for receipt validation, deterministic ZIPs,
per-user installation and shared MSIX payloads.

The product website, its builder and its hosting workflows have been removed.
Desktop CI and tag releases do not build or deploy a website. Other distribution steps remain separate:

- Microsoft Store/MSIX packaging and Partner Center upload remain local/manual,
  disabled until an independently assigned product identity is supplied.
- Source ZIP export remains a separate local step; GitHub tag archives are not
  a replacement for the curated exporter.

### Credentials and boundaries

Public CI, tag builds, fork PRs and Dependabot PRs require no private token or SPM
checkout. Actions no longer reads private SPM source. Do not introduce privileged
`pull_request_target` or a private-source cache. Checkout uses
`persist-credentials: false`; caches contain public registry archives only.

The local public `crates/spm-contracts` crate (0.1.0) is not a daemon distribution.
Source checks and headless/synthetic protocol tests remain required; feature gates
must not silently skip failures. Live private-daemon integration belongs to a
separate authorized private workflow and must not be claimed from public CI
results. See [SPM_BOUNDARY.md](SPM_BOUNDARY.md).

Dependabot needs no `PRIVATE_REPO_TOKEN` secret. Major upgrades remain allowed
but require review, not auto-merge. Configuration covers both workflows and the
local composite action. See [DEPENDENCIES.md](DEPENDENCIES.md).

Other credentials are independent: `GH_TOKEN`/`contents: write` creates the
draft release; `WINGET_TOKEN` opens the winget PR. No website hosting credentials
are required by the remaining workflows.

The independent edition leaves unknown publishing identities unset rather than
reusing upstream ones. See [INDEPENDENCE.md](INDEPENDENCE.md) for the configuration
checklist and the separate product-name/data-migration decision.

winget updates require `ENABLE_WINGET_PUBLISH=true`, your own registered
`WINGET_PACKAGE_IDENTIFIER`, and `WINGET_TOKEN`. The first package version must
already exist in winget-pkgs. Both automatic and manual runs reject draft or
prerelease releases, require a stable `v<major>.<minor>.<patch>` tag and check for
exactly one x64 ZIP. Review the action and its transitive tools before enabling it.
There is no configured Store or winget channel by default.

## Local preparation

```powershell
./scripts/build-and-verify.ps1
python scripts/package-source.py
```

The shared script does not install tools or configure credentials on the local
machine. Public builds do not require private Git credentials.
If the system Python is too old, pass a compatible interpreter path as `-Python`;
for example `-Python (uv python find 3.15)`. This selects a local interpreter,
not a repository version pin. Use `-TargetDir target/package` to isolate builds
from a running release executable.

The portable ZIP and SHA-256 file appear in `dist/`. The source exporter creates
`dist/github-source/PecoFence/` and a separate source ZIP. It includes existing
tracked and untracked project source while excluding ignored files, internal
engineering archives, local media and Git history.
Curated images and GIFs in `docs/assets/` are included so the root README and the
translations in `docs/readme/` render after upload.

For a first public repository, use the exported source folder if the development
checkout has private screenshots or other artifacts in its historical commits.
Removing a file from the current tree does not remove it from old Git history.

## First GitHub upload

Create an empty GitHub repository under your own account. From the exported
source directory, replace the example remote with its actual URL:

```powershell
git init -b main
git add .
git commit -m "Initial PecoFence release"
git remote add origin https://github.com/YOUR-ACCOUNT/YOUR-REPOSITORY.git
git push -u origin main
```

Use your configured Git identity. Review the exported files before committing.
The project does not assume or claim a particular GitHub organization. Keep
repository and download links in the README accurate for the actual destination.

## Versioned release

Update the workspace version in `Cargo.toml`, regenerate `Cargo.lock` if needed,
and update `CHANGELOG.md`. Push a matching `v<version>` tag:

```powershell
git tag v0.0.1
git push origin v0.0.1
```

The release workflow checks the tag against `Cargo.toml`, verifies the project,
builds the x64 portable archive and creates a **draft** GitHub Release with its
ZIP and checksum. Review the release notes and publish the draft in GitHub.
For the first published release, also update the root README and its translations
that currently say no Releases have been published; keep download availability
claims accurate.

Current portable packaging is x64 and unsigned. ARM64 is not configured.
The ZIP includes `LICENSE`, `NOTICE` and third-party notices. Optional winget
updates run from `.github/workflows/winget.yml` only after configuration and opt-in.
After obtaining your own Partner Center identity, the Microsoft Store package can
be built locally with `./scripts/make-msix.ps1` and uploaded there; see [STORE.md](STORE.md).

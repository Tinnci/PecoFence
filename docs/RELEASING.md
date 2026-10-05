# Releases

## CI and deployment flow

PRs, pushes to `main` and manual CI runs execute the Windows `build` job. A
matching `v<version>` tag executes the release job against that tag's own source.
Both use [.github/actions/build-desktop/action.yml](../.github/actions/build-desktop/action.yml)
and [scripts/build-and-verify.ps1](../scripts/build-and-verify.ps1):

1. Authenticate the private `Tinnci/spm` Git dependency.
2. Install Rust stable with rustfmt/Clippy and the latest stable Python 3.
3. Restore **public registry archives only**, never private Git checkouts or
   compiled intermediates. Crate downloads are reusable across CI/releases and
   Rust upgrades; native dependency compilation happens on each runner.
4. Run formatting, translation checks and a strict website build before native
   compilation. Run Clippy, regenerate/compare bindings, compile and run workspace
   tests with the WebView2 loader staged beside their executables.
5. Compile `pecofence` and `pecofence-watchdog` once in release mode, enforce the
   existing 4.5 MiB application budget, then package those exact binaries with
   `make-portable.ps1 -SkipBuild`. All Cargo dependency resolution uses `--locked`.

CI uploads the ZIP/checksum as `pecofence-portable`, retained for seven days. It
does **not** publish a release or deploy the app. New commits cancel superseded
CI runs; release jobs are not cancelled this way. Both jobs have a 45-minute limit.

The tag workflow checks that the tag matches `Cargo.toml`, runs the same gates,
then creates a **draft** GitHub Release. Publishing that draft is a separate
manual step, after which the winget workflow submits a package update.

Website deployment is independent of native builds:

- `.github/workflows/website.yml` strictly builds the static site on relevant
  `main` changes or manual dispatch, then uploads it to Cloudflare Pages. Forks
  must explicitly set the repository variable `ENABLE_WEBSITE_DEPLOY=true`
  after reviewing their publishing identity and configuring the site credentials.
- `.github/workflows/pages.yml` is a manually triggered GitHub Pages fallback.
- Microsoft Store/MSIX packaging and Partner Center upload remain local/manual.
- Source ZIP export remains a separate local step; GitHub tag archives are not
  a replacement for the curated exporter.

### Credentials and boundaries

`PRIVATE_REPO_TOKEN` must have read access to `Tinnci/spm` in **both CI and
release jobs**. The current repository's `GITHUB_TOKEN` cannot read another
private repository. The shared action passes the token only through step
environment variables; Git configuration contains a variable reference, not
the credential. Checkout uses `persist-credentials: false`.

Fork pull requests do not receive this secret, so they cannot complete the
private-dependent native build. Do not switch to `pull_request_target` to run
untrusted fork code with the secret. Public contribution builds require a
separate decision about publishing or safely distributing the contracts crate.

Other credentials are independent: `GH_TOKEN`/`contents: write` creates the
draft release; `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` deploy the site;
`WINGET_TOKEN` opens the winget PR. GitHub Pages uses `pages: write` and OIDC.

Before publishing a fork, review `site/site.json`, the Cloudflare project,
winget identifier and `packaging/msix/identity.json`: upstream identities must
not silently become the fork's publishing targets. Manual Website dispatch
currently deploys its selected ref to the Cloudflare `main` production branch;
manual winget dispatch can bypass the automatic non-prerelease condition.
The fork opt-in prevents accidental Website deployment; it does not change
those manual ref/prerelease policies once publishing is enabled.

## Local preparation

```powershell
./scripts/build-and-verify.ps1
python scripts/package-source.py
```

The shared script does not install tools or configure credentials on the local
machine. Use your existing Git credential helper for the private dependency.
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
The project does not assume or claim a particular GitHub organization. After the
first push, set the repository URL in `site/site.json` and follow
[WEBSITE.md](WEBSITE.md) to publish the product page.

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

Current packaging is x64 and unsigned. ARM64 is not configured. winget publishing runs
from `.github/workflows/winget.yml` on published releases; the Microsoft Store package is
built locally with `./scripts/make-msix.ps1` and uploaded in Partner Center, see
[STORE.md](STORE.md).

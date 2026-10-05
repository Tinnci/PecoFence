# Independent Tinnci/PecoFence edition

This repository is developed independently at
<https://github.com/Tinnci/PecoFence>. Source ownership, publishing authority and
runtime identity are separate concerns. Independence does not remove upstream or
third-party credit, and it does not imply ownership of upstream publishing accounts.

## Status and configuration

At the time of this setup, this repository has no published GitHub Releases;
draft-release automation is configured. No independent Store listing, winget
identity or public site is configured. This is a configuration snapshot, not a
claim that no release has ever existed anywhere.
Check current repository settings and published artifacts before announcing a
release; disabled deployment defaults are not evidence about external accounts.

| Area | Current status | Action before use |
|---|---|---|
| Source repository | `Tinnci/PecoFence` | Check access, branch protections and repository settings separately. |
| GitHub hosting relationship | Still a GitHub fork of `DayuanJiang/PecoFence`, as checked during this setup | Treat repository metadata separately from publishing identities; preserve commit history and credit. |
| GitHub Releases | Tag workflow builds draft releases; no releases published at setup time | Review artifacts, release notes and signing policy before publishing a draft. |
| Website | `baseUrl=null`, `customDomain=null`; local default `http://localhost:8000`; no analytics beacon or Store/winget buttons | Choose your own HTTPS URL and host; configure `site/site.json`; validate with `--strict --deploy`. See [WEBSITE.md](WEBSITE.md). |
| Cloudflare Pages | Opt-in; no assumed project, domain or account | Supply `CLOUDFLARE_PAGES_PROJECT`, secrets `CLOUDFLARE_API_TOKEN` / `CLOUDFLARE_ACCOUNT_ID`, then `ENABLE_WEBSITE_DEPLOY=true`; deployment is `main` only. |
| GitHub Pages | Manual, opt-in alternative, not a live fallback | Configure your own URL, select GitHub Actions in Pages settings, enable `ENABLE_GITHUB_PAGES=true`, and run on `main` using OIDC. |
| Microsoft Store/MSIX | Publishing identity fields are `null`; no CI Store upload | Obtain your own Partner Center identity; fill required fields or pass a separate JSON with `-Identity`; build/upload locally. See [STORE.md](STORE.md). |
| winget | No independently confirmed package identity; automation updates existing packages only | Establish an accepted first version in `winget-pkgs`, confirm `WINGET_PACKAGE_IDENTIFIER`, supply secret `WINGET_TOKEN`, review transitive tooling, then enable `ENABLE_WINGET_PUBLISH=true`. |
| Code signing | Production signing decision outstanding | Choose certificate ownership, key storage and signing procedure. MSIX test certificates are local-only. |
| Private dependency | `Tinnci/spm` is our dependency; Cargo pins its contracts revision | Give authorized contributors authenticated access, or decide how to distribute the contracts for public reproducible builds. Never place credentials in files. |
| Runtime/product identity | Product and binaries remain PecoFence | Decide whether to retain the name; require a naming, data migration and coexistence plan before changing identifiers. |
| Media | Existing screenshots and demonstrations remain | Replace with this edition's own demos when ready; retain licensing and attribution for any retained material. |

The winget gate accepts stable `vX.Y.Z` tags only, verifies the GitHub release is
neither draft nor prerelease, and requires exactly one x64 ZIP. These checks apply
to manual and automatic submissions. The pinned wrapper still invokes
`cargo-bins/cargo-binstall@main` and installs the latest `komac`; review that
transitive supply chain before enabling publication.

## Attribution and history are retained

- Keep `LICENSE`, `NOTICE`, contributor credit and third-party notices. Both
  portable and MSIX distributions must carry the applicable notices.
- Keep the inherited `CHANGELOG.md` as historical context. Past upstream changes
  or certifications are not evidence of this edition's releases or approval.
- A GitHub fork relationship is repository metadata, distinct from source content,
  commit history and Store/winget/site publishing identities. Check GitHub repository
  settings separately; do not rewrite history to erase credit.
- Replacing screenshots or marketing media does not erase licensing obligations
  for source, dependencies or retained assets.

## Runtime naming and coexistence need a separate plan

The independent repository does **not** yet rename the PecoFence product or its
`pecofence.exe` / `pecofence-watchdog.exe` binaries. Normal configuration under
`%APPDATA%\PecoFence` and logging under `%LOCALAPPDATA%\PecoFence`, the startup
registry identity, single-instance identity and legacy OpenFence compatibility
remain shared. Repository independence alone does not isolate installations.

Before changing these identifiers, explicitly inventory and decide:

1. The product, binary, package and runtime names to keep or replace.
2. Whether and how to import existing PecoFence/OpenFence settings, including
   backups, rollback and avoiding conflicting writes.
3. How old and new installations coexist: startup entries, instance locks,
   desktop-icon ownership, logging, packaged data and uninstall behavior.
4. How to test migrations and communicate the change to existing users.

Do not casually rename runtime IDs as a publishing setup step. Until a plan is
implemented, avoid running installations against shared settings concurrently.
Use isolated portable test instances as described in [DEVELOPMENT.md](DEVELOPMENT.md).

## Decisions still required

- Keep PecoFence as the product name or perform a planned rename?
- Which website host and public HTTPS URL should this edition use?
- Should it publish to the Store, winget, both, or neither?
- What is the production code-signing policy?
- How will public contributors obtain the pinned private `spm` contracts:
  authenticated access or an explicitly licensed distribution strategy?

Keep secrets in approved credential stores or repository Actions secrets, never
in checked-in configuration. Enable publishing only after the relevant identity,
account access and deployment settings have been independently verified.

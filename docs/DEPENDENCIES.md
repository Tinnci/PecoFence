# Dependency automation and Actions runtimes

Dependabot can open upgrade PRs when an action publishes a newer version with a
supported runtime. It cannot change the runtime of old upstream code, repair
breaking action inputs, configure credentials, or guarantee an upgrade passes CI.
The desktop workflow does not install a project Node.js runtime. Its scripts use
Rust and Python; GitHub Actions that declare a Node runtime still execute under
their own declared `runs.using` version.

## Current policy

- [.github/dependabot.yml](../.github/dependabot.yml) checks root workflows **and**
  `/.github/actions/build-desktop` weekly. Root scanning alone can miss the local
  composite action.
- Minor/patch updates are grouped; major updates remain allowed as separate PRs.
  There is no automatic merge.
- Direct remote actions use full commit SHA pins with version comments. Updated
  official actions declare Node 24; Rust/winget wrappers
  are composite actions.
- `scripts/check-actions.py` checks direct pins, local-action coverage and rejects
  privileged `pull_request_target`. Its `--online` mode fetches pinned public
  action metadata and rejects declared Node runtimes below 24. This is not a full
  YAML validator or a recursive supply-chain audit.
- The winget wrapper still invokes mutable/downloaded upstream tools. It remains
  disabled by default; see [INDEPENDENCE.md](INDEPENDENCE.md).
- Cargo automation is deliberately not enabled yet. `spm-contracts` 0.1.0 is a
  public workspace crate, not a private Git dependency; Windows Composition is
  vendored. Review contracts, Rust/Windows toolchain and binding changes together.
- The Windows App SDK runtime comes from the public NuGet flat-container endpoint
  at one exact version and SHA-256/SHA-512. CI caches only that pinned source
  archive under a hash-specific key; staging verifies the archive before extraction,
  and runtime files are regenerated and independently checked on each runner.

## Required repository configuration

Adding the file to the default branch enables version-update scheduling, subject
to repository/organization Dependabot policies. This change does not alter those
remote settings or supply a credential.

Public CI, releases and Dependabot PR builds use this repository plus public
registries. They do not read `Tinnci/spm` and need no `PRIVATE_REPO_TOKEN`,
private checkout or Dependabot private-repository secret. The private backend
remains separate; see [SPM_BOUNDARY.md](SPM_BOUNDARY.md).

Do not use privileged `pull_request_target`, workflow privilege escalation or a
private-source cache. Required source checks and headless/synthetic protocol
tests must fail on errors, not silently skip behind feature gates. A green public
build is not evidence of live private-daemon integration.

## Verification

```powershell
uv run --no-project --python ">=3.11" python scripts/test-actions-policy.py
uv run --no-project --python ">=3.11" python scripts/check-actions.py --online
```

GitHub-hosted runners are the supported CI target. A self-hosted runner must meet
the selected actions' current runner/Node requirements. Enabling deployment,
signing, Store identities and winget submission remains separate from automation.

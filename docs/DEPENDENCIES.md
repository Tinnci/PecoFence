# Dependency automation and Actions runtimes

GitHub Actions are updated by hand, committed directly to `main`: there is no
Dependabot configuration and no upgrade PR. An update replaces the full commit SHA
after checking it against the publisher's release tag, reads the release notes of
any major version crossed for removed or changed inputs, and fixes the version
comment. `scripts/check-actions.py --online` then confirms the declared runtime.
The desktop workflow does not install a project Node.js runtime. Its scripts use
Rust and Python; GitHub Actions that declare a Node runtime still execute under
their own declared `runs.using` version.

## Current policy

- Pins in root workflows **and** the local composite action
  `/.github/actions/build-desktop` are reviewed together; root workflows alone miss
  the composite action's own `uses:` lines.
- Direct remote actions use full commit SHA pins with version comments. Updated
  official actions declare Node 24; Rust/winget wrappers
  are composite actions.
- `scripts/check-actions.py` checks direct pins, that local actions exist, and rejects
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

## Windows CI feedback and cache boundary

Windows CI pins Python 3.14 for worker setup; local validation scripts continue
to support Python 3.14 or newer. The Quick and Full verification jobs run
independently. Quick is intended to return native feedback sooner; Full remains
the build job and produces the portable artifacts with every existing release
compile, size, binding-consistency and real-ZIP gate intact.

Each run uses a fresh public `CARGO_HOME` and retains the existing public Cargo
registry and NuGet caches. CI also has a narrowly validated Git object-database
cache for the public `windows-rs` source at its single pinned URL and commit.
It must not cache a Cargo checkout/worktree, compiled outputs, private SPM
content, credentials or secret environment. Cache reuse does not replace or
bypass source, import or artifact-hash validation; the pinned dependency and
runtime checks still run on each job.

Git cache validation checks history connectivity and recomputes the commit,
tree and blob content hashes for the entire pinned source snapshot. It does
not rehash every unused historical SDK blob on every restore: that exceeded
120 seconds on the local object database, while snapshot hashing plus
connectivity took 5.37 seconds. This is a local validation measurement, not a
claim about end-to-end CI speed. Nested repositories, active Git hooks,
unrelated root payloads and object substitutions are rejected.

Quick may accept `-TestPackage` to narrow its ordinary workspace test packages.
The aggregate report identifies that requested scope, and the Reactor shutdown
and workspace-validator suites remain mandatory. A narrowed Quick run is not a
Full run and cannot change Full or release acceptance.

The earlier 11m6s CI duration is a historical measurement, not a baseline for
the new jobs. Do not claim a time-to-feedback improvement until the updated
tests and CI have rerun. Running Quick and Full on separate workers can increase
total CI compute even if Quick completes earlier.

## Required repository configuration

No dependency-automation file or secret is configured.

Public CI and releases use this repository plus public registries. They do not
read `Tinnci/spm` and need no `PRIVATE_REPO_TOKEN` or private checkout. The private backend
remains separate; see [SPM_BOUNDARY.md](SPM_BOUNDARY.md).

Do not use privileged `pull_request_target`, workflow privilege escalation or a
private-source cache. Required source checks and headless/synthetic protocol
tests must fail on errors, not silently skip behind feature gates. A green public
build is not evidence of live private-daemon integration.

## Verification

```powershell
uv run --no-project --python ">=3.14" python scripts/test-actions-policy.py
uv run --no-project --python ">=3.14" python scripts/check-actions.py --online
```

GitHub-hosted runners are the supported CI target. A self-hosted runner must meet
the selected actions' current runner/Node requirements. Enabling deployment,
signing, Store identities and winget submission remains separate from automation.

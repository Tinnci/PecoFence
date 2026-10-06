# Dependency automation and Actions runtimes

Dependabot can open upgrade PRs when an action publishes a newer version with a
supported runtime. It cannot change the runtime of old upstream code, repair
breaking action inputs, configure credentials, or guarantee an upgrade passes CI.
Setting `node-version: 24` installs Node for our scripts; it does **not** change an
action's own `runs.using`.

## Current policy

- [.github/dependabot.yml](../.github/dependabot.yml) checks root workflows **and**
  `/.github/actions/build-desktop` weekly. Root scanning alone can miss the local
  composite action.
- Minor/patch updates are grouped; major updates remain allowed as separate PRs.
  There is no automatic merge.
- Direct remote actions use full commit SHA pins with version comments. Updated
  official actions and Wrangler declare Node 24; Rust/Pages upload/winget wrappers
  are composite actions.
- `scripts/check-actions.py` checks direct pins, local-action coverage and rejects
  privileged `pull_request_target`. Its `--online` mode fetches pinned public
  action metadata and rejects declared Node runtimes below 24. This is not a full
  YAML validator or a recursive supply-chain audit.
- The winget wrapper still invokes mutable/downloaded upstream tools. It remains
  disabled by default; see [INDEPENDENCE.md](INDEPENDENCE.md).
- Cargo automation is deliberately not enabled yet: `Tinnci/spm` is private and
  revision-pinned for paired contract changes, and Windows Composition is vendored.
  Review Rust/Windows toolchain and binding changes together rather than pretending
  an unauthenticated updater covers these dependencies.

## Required repository configuration

Adding the file to the default branch enables version-update scheduling, subject
to repository/organization Dependabot policies. This change does not alter those
remote settings or supply a credential.

For native builds of Dependabot PRs, add a **Dependabot secret** named
`PRIVATE_REPO_TOKEN` under Settings → Secrets and variables → Dependabot. An Actions
secret with the same name is not automatically available to Dependabot-triggered
PR workflows. Use narrowly scoped read-only access to `Tinnci/spm`; do not paste
the token into source, logs or documentation.

Main/tag jobs use the Actions secret. `github.token` cannot read the separate
private repository and is no longer an ineffective fallback. Public source checks
run before private authentication; missing access still fails the required native
build instead of silently marking an untested package green.

Do not use `pull_request_target` or workflow privilege escalation to bypass these
restrictions. Ordinary fork PRs require a separate contracts-distribution decision.

## Verification

```powershell
uv run --no-project --python ">=3.11" python scripts/test-actions-policy.py
uv run --no-project --python ">=3.11" python scripts/check-actions.py --online
```

GitHub-hosted runners are the supported CI target. A self-hosted runner must meet
the selected actions' current runner/Node requirements. Enabling deployment,
signing, Store identities and winget submission remains separate from automation.

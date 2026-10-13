# Public contracts and private SPM backend boundary

## Chosen architecture

PecoFence builds from this repository and public sources. The Rust dependency
boundary is this checkout, crates.io, and exactly one immutable Microsoft Windows
Rust SDK Git source: `https://github.com/microsoft/windows-rs` at revision
`2672e615d9cc0771448a781f3b2fe34e7fd08c6a`. The only allowed package names
from that source, all at version `0.100.0`, are `windows-collections`,
`windows-core`, `windows-future`, `windows-implement`, `windows-interface`,
`windows-link`, `windows-reference`, `windows-result`, `windows-strings`,
`windows-threading` and `windows-time`. The lockfile must record the same
commit for those 11 packages.

`windows-reactor` remains crate version `0.100.0` and is the sole vendored SDK
exception. `vendor/windows-reactor` is imported from the same immutable commit
(crate tree `0f27b6f95eb2177098543dd9bad6b6e84d9ad997`) with one focused
content-dialog shutdown fix. Its manifest keeps the exact crate identity and
pins its direct SDK dependencies to the same Git revision. The upstream source,
tests, public API snapshot, MIT and Apache licenses, and provenance are retained.
The crate is excluded from workspace membership so dependency-license
generation continues to include it.

The shared wire contracts live in the public
workspace crate `crates/spm-contracts`, version
**0.1.0**, extracted from the exact previously pinned SPM revision
`e0dd3e058d40ef1c4623c12dba2f0715d2011a1b`. This is a contracts extraction,
not a publication or migration of the SPM backend.

`Tinnci/spm` remains private. No private token or SPM checkout is required to
compile PecoFence or use ordinary desktop fences. The public Windows build needs
unauthenticated access to GitHub for that pinned SDK revision, crates.io, and the
separately pinned public NuGet runtime artifact. It does not need private source
credentials or a private backend. Live SPM data requires an authorized private
backend installation; the daemon is not distributed here.
Private data, credentials, configuration and operational artifacts stay private.
Retain all applicable licenses, `NOTICE` and upstream/third-party attribution.

The crate and [provenance review](SPM_CONTRACT_PROVENANCE.md) are included here.
`scripts/check-public-dependencies.py` checks manifests, the lockfile, workflow
credential references and, with `--resolved`, Cargo's actual dependency graph.
It rejects other Git sources, mutable refs, alternate URLs, package names or
versions, same-version Git/registry Windows type duplicates, and Cargo source
replacement configuration. SourceOnly checks source-level dependency policy
without invoking Cargo. Quick also checks Cargo's resolved dependency graph
against this policy; Full retains its existing source and dependency checks.
CI uses a fresh public Cargo home and no Git credential helper. Existing public
Cargo-registry and NuGet caches are retained, with only a narrowly validated
Git object-database cache for the pinned public `windows-rs` source and commit.
No checkout tree, compiled output, private SPM data or credentials are cached.
Cache reuse does not bypass hash, import or source-policy checks. Backend
migration, public commit availability and live integration remain separate
verification results.

## One source of truth

The public contracts crate is the single mutable source of wire types and
serialization rules. PecoFence uses its workspace path dependency. The private
backend must consume the same public implementation at an immutable Git commit
containing crate version 0.1.0 (or an explicitly reviewed successor).
Do not leave a duplicate independently mutable contracts implementation in SPM.

Cargo crate version and **wire protocol major** are different identifiers.
Version 0.1.0 does not mean protocol major 0 or 1. Preserve the existing protocol
major and encoding behavior from the extraction; a protocol-major change needs
an explicit compatibility/migration decision and paired validation.

## Authorized backend migration

When `Tinnci/spm` is attached and the maintainer authorizes backend edits:

1. Review the exact public extraction and provenance against the old pin above.
   Record the actual immutable **public PecoFence commit SHA** containing the
   contracts crate. The old private extraction pin is not that public commit.
   Do not substitute a branch, floating tag or invented/pending SHA.
2. In the backend workspace dependency source of truth, replace the local
   contracts dependency with `git = "https://github.com/Tinnci/PecoFence.git"`,
   `rev = "<verified public commit SHA>"`, and `version = "=0.1.0"`.
   Backend member crates should inherit that workspace dependency rather than
   define different pins. Update/review its lockfile and dependency graph.
   Do not claim remote availability until the commit is actually published.
3. Remove the duplicate local contracts implementation from the active backend
   workspace as a reviewed change. If compatibility re-exports are necessary,
   make them thin re-exports of the public crate, not a second copy of the types.
   Keep private business logic, data, configuration and credentials untouched.
4. Run paired public/private protocol and golden-fixture tests against the same
   contract revision: framing, serialization, handshake/version handling,
   reconnect/error behavior and representative synthetic payloads. Use only
   approved synthetic fixtures in public source; no customer data.
5. Separately run authorized live private-daemon integration in an isolated
   private workflow. Record backend/PecoFence SHAs, contracts pin, commands and
   results, including any unrun checks. Synthetic success is not live evidence.

Attaching a repository is not permission to publish code, change visibility,
delete remote content or automatically edit remote repositories. No automatic
remote edits, deletion or visibility changes are part of this migration.

## Public CI and evidence

Public CI, releases and Dependabot build from public source without
`PRIVATE_REPO_TOKEN` or a private source checkout/cache. Do not use privileged
`pull_request_target` to bypass that boundary. Dependency caches are limited to
public registry/NuGet archives and the validated object database for the one
pinned public Windows SDK Git source; never cache checkout trees, compiled
outputs, private SPM content or credentials. Deployment credentials are
unrelated to contracts consumption.

Source checks and headless/synthetic protocol tests remain required. Quick runs
the Reactor shutdown and workspace-validator suites even when `-TestPackage`
narrows its other workspace tests; Full continues to run every existing gate.
CI feature gates must not silently skip failures or turn missing verification
into a green result. Record which tests ran; native Windows, browser, protocol
and live integration results are distinct evidence.

A public build/test result never establishes that the private backend was
migrated, published or tested live. See [DEVELOPMENT.md](DEVELOPMENT.md),
[DEPENDENCIES.md](DEPENDENCIES.md) and [RELEASING.md](RELEASING.md) for the
public workflow and release gates.

# Public SPM contract provenance

## Origin and license

The public `crates/spm-contracts` package derives solely from the protocol crate
in [Tinnci/spm](https://github.com/Tinnci/spm), at the exact revision
`e0dd3e058d40ef1c4623c12dba2f0715d2011a1b`. Source was obtained read-only through
authenticated GitHub API requests with that revision, not from the latest branch.
The private backend remains private; public PecoFence builds use this local
protocol package and do not require access to the private source repository.

The pinned crate inherits version `0.1.0`, edition `2021`, license `Apache-2.0`
and authors `SPM Engineering` from its root workspace manifest. These declarations
are retained explicitly in the public package. The pinned repository root has
no `LICENSE`, `NOTICE` or `COPYING` entry, and reviewed crate files have no
additional copyright or attribution notices. This records declared metadata,
not an independent determination of ownership or licensing authority.

The full Apache License 2.0 text is copied from PecoFence's root `LICENSE` into
`crates/spm-contracts/LICENSE`, so the package includes its license independently
of the repository root. `crates/spm-contracts/NOTICE` records source attribution
and changes without inventing a copyright holder. Redistributors must retain
the license and relevant attribution; packaged binaries should include these
notices along with PecoFence's other third-party notices.

## Exact import allowlist

All following paths are relative to the pinned source repository. No wildcard
import of other crates or repository contents is authorized.

| Imported paths | Treatment |
| --- | --- |
| `crates/spm-contracts/src/canonical.rs` | Unchanged |
| `crates/spm-contracts/src/endpoint.rs` | Unchanged |
| `crates/spm-contracts/src/envelope.rs` | Unchanged |
| `crates/spm-contracts/src/error.rs` | Unchanged |
| `crates/spm-contracts/src/framing.rs` | Unchanged |
| `crates/spm-contracts/src/ids.rs` | Unchanged |
| `crates/spm-contracts/src/lib.rs` | Unchanged |
| `crates/spm-contracts/src/query.rs` | Unchanged |
| `crates/spm-contracts/src/rpc.rs` | Unchanged |
| `crates/spm-contracts/src/snapshot.rs` | Unchanged |
| `crates/spm-contracts/src/version.rs` | Unchanged |
| `crates/spm-contracts/Cargo.toml` | Modified: explicit metadata, public workspace dependencies and explicit compatible `chrono`/`thiserror` versions |
| `crates/spm-contracts/tests/framing.rs` | Unchanged generic synthetic transport tests |
| `crates/spm-contracts/tests/golden.rs` | Unchanged synthetic hello serialization test |
| `crates/spm-contracts/tests/lifecycle_rules.rs` | Unchanged generic protocol tests |
| `crates/spm-contracts/tests/schema_negative.rs` | Unchanged Serde/semantic rejection tests |
| `crates/spm-contracts/fixtures/rpc/hello-request.json` | Unchanged fictional build label and repeated-digit UUID |
| `crates/spm-contracts/fixtures/invalid/page-size-501.json` | Unchanged synthetic page boundary case |
| `crates/spm-contracts/fixtures/invalid/unknown-gate-state.json` | Unchanged generic invalid enum case |

The manifest does not inherit PecoFence's version or edition. Workspace `serde`
requires `derive`; workspace `uuid` requires `v4` and `serde`; workspace
`serde_json` is used directly. `chrono` remains `0.4` with `std`, `clock`, `serde`
and no default features; `thiserror` remains `2.0`. The runtime-independent
implementation preserves all public types, wire tags, field names, validation
rules, frame boundaries, endpoint derivation and protocol constants exactly.

## Deliberate exclusions and synthetic replacements

- The source fixture catalog, all original snapshot fixtures and their
  `tests/snapshots.rs` consumer are excluded. They contain project/tenant names,
  regional scope labels, baseline examples and milestone terminology that are
  unnecessary to verify the public protocol. Their real-world provenance is not
  asserted or published here.
- The three source `schema/*.json` files are excluded. They are partial
  descriptive schemas, are not consumed by the Rust tests, and carry a local
  schema address. Serde decoding and explicit contract validation remain the
  executable verification source of truth; this import does not claim full JSON
  Schema conformance or redesign those schemas.
- Every other crate, backend implementation, business/domain model, adapter,
  database, real configuration/data, repository history, skill and UI is excluded.
  Root workspace metadata was read only to resolve inherited package attributes
  and dependency features; it was not imported.

The new `fixtures/snapshots/fictional-example.json` and
`tests/synthetic_protocol.rs` are deliberately authored public verification
cases, not exports or renamed copies of backend data. Their names explicitly
say fictional, their timestamp is a fixed year-2000 example, their UUID uses
repeated digits, and their counts/threshold are small made-up numbers. They
contain no tenants, owners, addresses, source records or real milestones.
Tests cover snapshot serialization, all four gate states, event identity and
direction, IDs/cursors, page boundaries, retryability and truncated frames.
The retained SID and hello UUID examples are synthetic transport identifiers,
not an observed account or session.

## Privacy review and its limits

Before importing, all pinned crate source, comments, manifest, tests, schema and
fixtures were inspected for enterprise identifiers, addresses, data and secrets.
No credentials, private service addresses or real account data were found in the
allowlisted protocol implementation and generic verification inputs. Source-system
enum names and generic DTO fields are protocol API, not private backend records;
they remain unchanged for compatibility. Potentially business-specific snapshot
examples and the local schema address were kept out of the import.

This is a bounded manual review of this pinned protocol tree and inherited
metadata, not a secret scan of the private repository, a review of private
history, or proof that arbitrary strings accepted by the protocol are safe to
publish. Future fixture additions require a fresh review. Never copy production
payloads, logs, source references or configuration into public tests.

## Future paired changes

1. Agree on a contract change with the private backend maintainers and record an
   exact new source revision; never silently track a branch or latest main.
2. Review the proposed protocol diff, license/attribution and every candidate
   test, fixture and schema before copying. Use the allowlist above as the
   baseline; explicitly justify additions and keep backend/business data out.
3. Apply the protocol change to this public crate and the private provider as a
   coordinated pair. Preserve compatibility where intended; document any wire
   version/feature change rather than performing an unrelated redesign.
4. Run public contract tests and PecoFence consumer tests from a clean checkout
   without private-repository credentials. Backend maintainers separately run
   provider-side tests against the same synthetic wire cases and new revision.
5. Update this provenance record, package version/dependency requirements where
   appropriate, and redistribution notices in the same reviewed change.
   Publication/landing is a separate explicit action, not part of source import.

# Verification gates

The gates answer different questions. Cheap source checks protect pull requests
before native compilation; Windows CI verifies the executable and portable
package; release acceptance verifies the real desktop experience. Manual hardware
checks are release criteria, not a requirement to repeat on every commit.

## Three tiers

| Tier | Required checks | What a pass means | What it does not mean |
| --- | --- | --- | --- |
| **SourceOnly** | Python tests and source-policy checks, including public-dependency and action-pin policy, locale/README integrity, and package-safety tests against fixtures. It must not invoke Rust/Cargo, download the Windows App SDK runtime, launch a GUI, check out private SPM code, or request private credentials. | Source-level policy and fixture checks passed without secrets or a native build. | Rust compiles, native tests pass, a package is built, or the application works on Windows. |
| **Full Windows CI** | Formatting; strict Clippy; locked workspace tests that execute (including the workspace validator); generated-binding consistency; one locked release build; staging the public Windows App SDK Runtime 2.5.1 package after SHA-512/SHA-256 verification and checking its exact file/PE inventory; safe portable-package creation and verification using those same release binaries; and the executable-size budget of **6.5 MiB (6,815,744 bytes)**. | The checked source builds and tests on the CI Windows image, the pinned local runtime and safe package pass automated checks, and the EXE fits the agreed CI budget. | Manual desktop behavior, feature parity, accessibility, every display/language combination, or clean-machine acceptance. |
| **Release acceptance** | A clean source checkout at the intended stable version tag, with build provenance tying the artifact to that tag/commit, plus actual desktop workflows on Windows. Exercise Settings and representative desktop operations; verify keyboard and focus order, UI Automation, Narrator, high contrast, text scaling, all ten locales at 100%/150%/200% DPI, and the self-contained package on a clean supported Windows machine. Confirm the pinned runtime loads from the application package and no WebView2 runtime, loader, helper, UI assets, browser modules or child processes are present in the inspected release paths. Record the Windows version, package identity/hash, exact steps, results and exceptions. | The named release candidate has passed the recorded real-machine acceptance cases. | Untested Windows versions, hardware, workflows or paths; do not generalize beyond the recorded matrix. |

Run SourceOnly for fast, credential-free source feedback. Full Windows CI is the
automated commit/PR gate. Release acceptance is required before claiming the
release candidate is fully accepted; it must not be silently treated as a check
that ran on every commit.

## Keep, adjust and remove

| Existing or proposed gate | Decision | Reason |
| --- | --- | --- |
| Python/source-only checks before private credentials or expensive native work | **Keep; define as SourceOnly** | Fast policy checks should remain usable on public pull requests with no private access. SourceOnly must stay genuinely source-only: no Cargo invocation, runtime download or GUI launch. |
| `cargo test --no-run` followed by the same tests executing | **Remove the duplicate compile gate** | The locked executed test run compiles its test targets. Keep the one executed test run and its results rather than compiling the same suite twice. |
| 4.5 MiB executable limit | **Adjust to 6.5 MiB / 6,815,744 bytes in Full Windows CI** | This is the agreed ceiling for the selected native Settings and self-contained runtime architecture. Do not infer a startup, memory or total-package improvement from the EXE limit. |
| Flat package limited to 16 files / 64 MiB | **Remove those legacy product constraints; retain bounded safety validation** | Verify archive integrity and safe paths, cap the archive at 128 MiB, total unpacked content at 160 MiB and entries at 512, and require the exact pinned runtime inventory. These are parser/resource safety limits, not a promise that the package must be flat or contain a fixed file count. |
| Pinned runtime identity, hashes and complete inventory | **Keep in Full Windows CI and release-package checks** | The package must contain the reviewed self-contained Windows App SDK Runtime 2.5.1 payload, not an arbitrary runtime or WebView2 substitute. |
| Identical README heading/table/image counts, locale-specific hero image and language-bar punctuation count | **Remove editorial shape gates; retain integrity** | Translations may use their own layout. All ten files, UTF-8 content, existing relative link targets and valid heading anchors remain checked. Locale keys and placeholders are not relaxed. |
| Keyboard, accessibility, localization, DPI, clean-machine and hardware checks on every commit | **Move to Release acceptance; keep as requirements** | Their real-machine value is not replaced by headless tests, but requiring the manual matrix for each commit needlessly blocks ordinary iteration. Record and complete it for release acceptance. |
| Clean tag/source/artifact provenance | **Keep for Release acceptance** | A passing test suite does not prove that a published artifact came from the intended clean, reviewed source. |

## Current evidence and limits

The native Settings implementation now has five task pages and thirteen rule
condition forms, a native title-bar hamburger and adaptive left navigation.
Reported headless tests and a synthetic WinUI tour, including
closing and reopening Settings, passed; the latest reported validation run also
passed workspace tests and strict Clippy. A local probe loaded 19 Windows App SDK
DLLs from the staged runtime; no browser modules or child processes were observed
in the sampled tour. That observation is limited to the exercised paths.

The local full Windows verification pipeline and real portable-package verification
passed. The normal release executable measured 6,225,408 bytes (5.94 MiB), below
the 6.5 MiB cap; the ZIP measured 25,158,775 bytes (23.99 MiB). Native dialog
cancel/retirement, window close/reopen and application exit with an active modal
passed after the narrow vendored Reactor fix. Detailed measurements and artifact
scope are in [SETTINGS_NATIVE_VALIDATION.md](SETTINGS_NATIVE_VALIDATION.md).

These results are implementation and automated-workflow evidence, not proof of
full feature parity or release acceptance. In particular, real keyboard/UIA and
Narrator use, high-contrast and text-scaling behavior, the complete ten-locale by
three-DPI matrix, clean-machine acceptance, and clean-tag release-package inspection
remain separate evidence items. Local checks are not a claim that remote CI or
the eventual release candidate has already been accepted;
do not invent values or claim startup, memory or overall performance improvement.

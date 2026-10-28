# Upstream compatibility review — 2026-10-18

## Scope and history

Reviewed all 49 fetched upstream branch heads. Forty-eight are ancestors of upstream/main at `12799fd`. The Task Scheduler branch has one additional commit and remains a reference.

The upstream repository rewrote its history. Local main and the newly fetched upstream/main have no common ancestor. The tree of the previously fetched upstream commit `906911a` is identical to rewritten commit `cb3b04e`, confirming a content correspondence despite different ancestry. There are 221 non-merge commits between that reference and the new upstream main.

A complete tree replacement would change 643 files, remove the plugin/native-Settings architecture and reintroduce the upstream WebView/configuration/publishing paths. This review does not force an unrelated-history merge or mark unported features as merged.

## Integrated compatible changes

| Upstream references | Integration |
|---|---|
| `2bf6713`, `4e79507` | Classify TSV documents; match compound extensions and dotfiles in existing rules. |
| `e8fdb0e`, `c205aa3`, `728bede` | Reject Windows control characters in file names, preserve permitted DEL/C1 characters, normalize trailing dots/spaces. |
| `3696bf7`, `ca74c10` | Rename without replacing a concurrently created destination; use the 248-UTF-16-unit long-path threshold. |
| `d77abbc` and upstream shared URL decoder | Decode UTF-16 Internet shortcuts and system-ANSI text consistently for URL targets and icon paths. |
| `ea12fc5` | Fall back to QueryWorkingSet when older Windows versions report zero private working-set bytes. |
| `5b9cf69` | Keep the default icon-worker stack for shell and third-party handlers. |
| `4fb7be0` | Release the Composition batch revoker reference while retaining its completion subscription. |
| `b10d46b` | Ignore transient Windows 11 shell popups in the foreground-change handler. |
| `3ffbf1a` | Include language-specific text_in calls in the native locale scanner, retaining the fork's Rust literal decoding. |

The existing folder enumerator already skips unreadable entries, matching the intent of `61e09b4`; no duplicate change is needed.

## Retained differences and deferred ports

These are reviewed differences, not features claimed to be integrated:

- Keep schema-2 containers/content identities, plugin scenes, SPM contracts, native WinUI Settings and existing persistence protection. Upstream flat-fence config, IPC/CLI and WebView Settings require a deliberate port rather than replacing these modules.
- Keep the fork's owned per-user startup entry and opt-in handling. Do not replace it with the independent Task Scheduler branch during this upgrade.
- Do not import the upstream Store identity, rating/feedback endpoints, website, analytics, signing/publishing accounts, or remove the fork's executable-size/package verification gates.
- Upstream scrolling/left-column tab strips, new tab/title options, folder previews and shortcut overlays, revised multi-monitor layouts and arrangement are not merged into the custom container/chrome model in this upgrade.
- Upstream compositor resize/memory-rebuild machinery needs a separate integration with the fork's renderer. Only the independently applicable completion-subscription leak is included here.
- Upstream internal UI strings, generated flat-config schemas and version numbers are not overlaid onto the fork's native catalogs or public wire contract.

All upstream branch references remain available. Local and origin feature branches can be deleted only after confirming ancestry in local main and successfully pushing main.

## Verification and installation boundary

The compatible changes add portable rule/scanner regressions and Windows regressions for rename collisions, case-only rename, long-path prefixes, UTF-8/UTF-16 shortcuts and memory counters. Windows-target strict Clippy, generated-binding consistency and the appropriate tests gate packaging.

Build inputs must be recaptured after committing this review and its code changes. The earlier b2b2991 release is not the final upgraded payload. Installation must preserve the live schema-2 workspace and existing SPM daemon/database.

## Branch inventory

| Reference | Reviewed head | Contained in upstream/main |
|---|---|---|
| `upstream/anchor-elevated` | `8899f17` | yes |
| `upstream/bugfix-review-round` | `2bcaf24` | yes |
| `upstream/drag-alignment-aids` | `9972917` | yes |
| `upstream/drop-language-section` | `f918279` | yes |
| `upstream/feat/left-tab-column` | `71752ab` | yes |
| `upstream/feat/tab-strip-scroll` | `e3aae04` | yes |
| `upstream/feat/tab-switch-on-hover` | `1289003` | yes |
| `upstream/feedback-diagnostics` | `8399f0d` | yes |
| `upstream/feedback-scroll` | `17c255b` | yes |
| `upstream/fence-clearance` | `c132665` | yes |
| `upstream/fence-group-select` | `fd651d4` | yes |
| `upstream/fence-text-locale` | `ae3e80a` | yes |
| `upstream/fix-icon-edges` | `53982c6` | yes |
| `upstream/fix/delete-off-ui-thread` | `324bfc5` | yes |
| `upstream/fix/desktop-in-front-at-start` | `7d107ad` | yes |
| `upstream/fix/feedback-49-50` | `9ea2fc7` | yes |
| `upstream/fix/issue24-monitors` | `7d4d0c4` | yes |
| `upstream/fix/issue33-add-rule` | `746cbc7` | yes |
| `upstream/fix/layouts-per-resolution` | `a1c7c26` | yes |
| `upstream/fix/marquee-offer-dismiss` | `97d146d` | yes |
| `upstream/fix/marquee-offer-selection` | `5aa55d3` | yes |
| `upstream/fix/rail-collapse-tabs` | `2b0918e` | yes |
| `upstream/fix/rows-stay-put` | `aca2a8d` | yes |
| `upstream/inapp-feedback` | `125c8d5` | yes |
| `upstream/issue-follow-up` | `514a8ad` | yes |
| `upstream/issue74-show-desktop` | `a176ff8` | yes |
| `upstream/l10n-polish` | `f234c72` | yes |
| `upstream/liquid-glass-kube` | `5a7bc89` | yes |
| `upstream/main` | `12799fd` | yes |
| `upstream/memory-reduction` | `6bccd69` | yes |
| `upstream/menu-icons` | `112b63c` | yes |
| `upstream/per-fence-title-on-hover` | `9ac7650` | yes |
| `upstream/perf/memory-growth` | `f192b8f` | yes |
| `upstream/readme-linux-do` | `ceec640` | yes |
| `upstream/readme-memory` | `b29d7c8` | yes |
| `upstream/readme/manual-gifs` | `1d32ab0` | yes |
| `upstream/readme/trim` | `c120d84` | yes |
| `upstream/release/0.2.1` | `39ae424` | yes |
| `upstream/resize-alignment` | `345f860` | yes |
| `upstream/settings-win11` | `631497f` | yes |
| `upstream/shell-drop-target` | `3798251` | yes |
| `upstream/store-notes-0.1.3` | `faeff13` | yes |
| `upstream/task-scheduler-startup` | `52fd93b` | no; startup alternative retained |
| `upstream/ux-night` | `f04b124` | yes |
| `upstream/vd-switch` | `3357947` | yes |
| `upstream/version-0.1.1` | `0548c3f` | yes |
| `upstream/version-0.1.3` | `d5247e9` | yes |
| `upstream/version-0.1.5` | `bbc6f7d` | yes |
| `upstream/win10-support` | `0c1d662` | yes |

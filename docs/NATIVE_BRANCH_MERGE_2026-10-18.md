# Native branch integration — 2026-10-18

## Result and scope

This integration joins the upstream histories to the native fork. All 49 fetched upstream branch heads are covered by upstream/main and task-scheduler-startup. This is a history merge with explicit conflict resolutions. It does not promise that every upstream feature is enabled.

The upstream history was rewritten. The earlier reference `906911a` has the same tree as rewritten `cb3b04e`, but the histories have no common ancestor. The unrelated-history merge produced 180 conflicting paths. The native fork is the resolution baseline. The JSON report records every conflicting path and its disposition.

## Native architecture decisions

- Keep native WinUI Settings, Win32/DirectWrite/Composition widgets, native Mermaid rendering and bundled fonts.
- Keep schema-2 containers, plugin scenes, SPM protocol, background portal reads and existing workspace persistence.
- Exclude WebView Settings, browser payloads, upstream flat-model IPC/CLI and website/service publishing identities.
- Retain the fork's owned HKCU Run startup with explicit config-directory arguments. Resolve the independent Task Scheduler branch in favor of this implementation; do not register a second startup task.
- Keep executable size, trusted build receipts and package checks that reject WebView payloads and imports.

These are deliberate final conflict resolutions, not a claim that the upstream tab-scroll/left-column UI, folder-preview/shortcut-overlay options, flat-model monitor layout, or new compositor reconstruction is ported. Their current native-fork equivalents remain. Reintroducing them requires a new implementation against schema-2.

## Additional native changes

- Adopt upstream's directory-watcher cancellation retry and handle cleanup. Exited portal watchers can be recreated. The rename-identity probe is available in the platform adapter; UI code does not perform synchronous identity checks on remote folders.
- Adopt native desktop z-order fixes for a topmost neighbor and placement refused by an elevated window.
- Adopt the watchdog's bounded retry and running-instance check before restoring desktop icons. A replacement instance owns its icons and marker.
- Add upstream's optional QuickLook integration through its local named pipe. Space toggles preview; arrow navigation updates it; Esc closes it. Ctrl+Space and ongoing type-ahead retain their existing behavior. QuickLook must already be running; no browser/runtime dependency is added.
- Release the native integration as 0.0.4. Keep all earlier compatible backports listed in UPSTREAM_REVIEW_2026-10-18.md.

## Verification

Run Windows-target strict Clippy, portable model/plugin tests, native mutex/watcher regressions and font tests. Exercise the native Settings test window and isolated TODO/note restart test. Build and verify the clean-source package before switching the production installation. Preserve the exact workspace, update owned launch entries and retain a verified rollback archive before deleting the old version directory.

The branch inventory and per-path conflict decisions are in [the machine-readable report](NATIVE_BRANCH_MERGE_2026-10-18.json). The earlier upstream review describes the pre-merge selective-backport state.

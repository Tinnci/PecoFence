# CI watch

The first-party provider `pecofence.spm.ci-watch` presents SPM CI observations in
480 × 780 paper fences. The views are **构建 · 关注列表**. The tray and fence menus
create the live panel; the board is available alongside it. The debug script
`new-ci-watch x y w h` creates deterministic demo data without spmd; add `live`
for the SPM source. `scripts/demo-ci-watch.ps1 -RepoRoot <checkout> -Theme light`
(or `dark`) runs an isolated portable instance. Copy the PowerShell driver to a
local Windows folder before running it with `powershell.exe -NoProfile -File`.

Builds have glyphs and words, measured condensed display titles, progress bars,
queue reasons and backend-projected ETA/remaining time. Overdue bars are striped.
Watched jobs show the last observation and a ten-build strip with a complete
accessible summary. Gerrit rows show current patch sets, label pills, merge
conflicts and CI matched by SPM to the current revision. Source timestamps are
presented as ages, including permission gaps. The title strip uses PanelChrome:
user/freshness, failure status, refresh and watchlist commands. Initial chrome is
republished during startup because a host wake can precede the actual fence HWND.

The green success ink is `paper::GOOD`: Light #3D6B4B, Dark #9CCB9F, adjusted by
`Paper::tone` to maintain 4.5:1 on paper ground and card. High contrast uses the
host's system palette and outlined controls. Text measurement uses the same font
role and size as painting. Text, geometry, hit testing and accessible names share
one scene description; stale frame actions are rejected.

The watchlist editor uses the host's native text entry and a bounded suggestion
list. Suggestions arrive after text submission, rather than on every keystroke,
because the existing editor reports submitted text only. Arrows move through
visible controls; Enter invokes, Esc returns to builds, Left/Right change views,
and Page Up/Down or Home/End scroll longer lists. Toggle names include their
current state. The demo's local edits are in memory; live edits go only to SPM's
local watchlist. The view and focus belong to the panel instance, not remote data.
Failure-alert preferences are persisted; this iteration does not emit desktop
notifications.

No provider code executes Jenkins or Gerrit commands. Refresh uses the existing
IPC send/completion and host wake path, every 30 seconds while a build is running,
otherwise two minutes. SPM owns collection, attribution, ETA, history and matching.
Navigation uses the existing configured-origin table and encoded path segments;
raw remote URLs never reach the host. The host currently requires HTTPS origins,
so an HTTP-only Jenkins deployment needs an approved HTTPS navigation origin.

The single wire contract is `crates/spm-contracts/src/ci_watch.rs`. Its additive
fields have serde defaults; watchlists, history, changes, suggestions, trigger IDs,
paths and current-patchset CI links are validated. `remaining_ms` is an additive
backend projection so the UI never needs ETA arithmetic. A CI build reference in
a change embeds its bounded identity/status observation rather than requiring a
second lookup. Regenerate fixtures with `scripts/generate_ci_watch_fixtures.py`;
SPM's checked-in demo fixture is a byte-identical copy of that output.

The two mockups differ in watchlist membership. This implementation uses one
consistent snapshot in both views: four jobs and three project/branch pairs.
Gerrit branch suggestions are limited to branches with observed changes because
the installed grr CLI has no branch-list command. SPM reports bounded results and
retains prior observations on failures.

Raster goldens: `crates/plugin-spm/tests/snapshots/ci-{builds-light,builds-dark,watchlist,hc}.png`.
Real Windows captures: `crates/plugin-spm/tests/snapshots/native/ci-watch/` (Light
and Dark, builds and watchlist; 536 × 836 including the native shadow around the
480 × 780 fence). These are actual demo windows; the high-contrast regression is
a headless scene/raster test and does not change the Windows contrast setting.

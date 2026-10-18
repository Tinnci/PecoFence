# Changelog

## 0.0.5 — Native widgets and bounded exit

- Give header summaries the full row width and taller title line boxes; use native single-line ellipsis for centered labels while preserving multiline note bodies.
- Size widgets in whole icon cells and show single widgets without a title bar, with a fading grab handle and move cursor.
- Initialize live resize rules and presentation on load and tab switches; preserve saved geometry and clear widget rules on file tabs.
- Verify frameless widget grids after restart alongside TODO and note persistence.
- Bound normal exit after the save transaction completes: fall back to the native message pump when WinUI exit does not finish. Log each exit stage; keep save failures and cancellation outside the fallback.

## 0.0.4 — Native branch integration

- Merge all fetched upstream branch histories with explicit native architecture resolutions. Keep native WinUI Settings, schema-2 workspaces, SPM and bundled fonts; exclude WebView UI/runtime and flat-model replacements.
- Fix watcher cancellation/handle cleanup and restart exited portal watchers. Improve desktop z-order anchoring and watchdog recovery when another instance has started.
- Add optional QuickLook preview using Space, arrow navigation and Esc, when QuickLook is already running.
- See `docs/NATIVE_BRANCH_MERGE_2026-10-18.md` for retained differences and exact merge scope.

## Unreleased — Tinnci edition

- Backport compatible upstream fixes for document rules, safe file renaming and
  long paths, Internet-shortcut encoding, memory counters, icon-worker stacks,
  Composition batch lifetime, Windows 11 shell popups and locale scanning.
  Preserve the native/plugin/schema-2 architecture; see the
  [upstream compatibility review](docs/UPSTREAM_REVIEW_2026-10-18.md).

- Unify local, CI and tag-release verification; reduce development build disk usage.
- Separate repository/download links from upstream publishing accounts. Store
  and winget publishing require independently configured identities.
- Remove inherited website analytics and include `NOTICE` in binary packages.
- Remove the product website, website-only assets, builder/tests, hosting workflows
  and tracked deployment caches; retain desktop verification and packaging.
- Replace the Settings web UI with Reactor-hosted native WinUI 3 controls. The
  five task pages and thirteen rule-condition forms are implemented; reported
  headless tests and a synthetic WinUI tour including close/reopen pass. This is
  not full feature-parity or release acceptance. Typed `SettingsView`,
  `Request`/`Receipt` and `DocumentStamp` remain the application boundary; no
  generic service framework or second mutable model is added.
- Select a self-contained Windows App SDK Runtime 2.5.1 distribution with no
  WebView2 payload. Users should not need to install the Windows App SDK Runtime
  separately for a correctly packaged build. Payload/import/process checks,
  keyboard/UIA/screen-reader, high-contrast, all-ten-locale, DPI and clean-machine
  acceptance remain pending. The runtime adds deployment/resource cost; no size
  or startup improvement is claimed. Full Windows CI uses the agreed 6.5 MiB
  (6,815,744-byte) EXE cap. Final release measurements and acceptance evidence
  remain pending. See [verification gates](docs/VERIFICATION_GATES.md).
- Redesign the note, TODO and calendar widgets. Shared header with a one-line
  summary and right-aligned pill/icon actions; labels, checkboxes and dates are
  centered through a new `TextAlign` on plugin text. Notes get a full-height body
  and a centered empty state; TODO lists add a progress bar, Fluent checkbox
  glyphs and pending-first ordering; the calendar shows a fixed six-week grid
  with dimmed adjacent-month dates and a solid accent for today. Narrow panels
  and large text scales collapse text actions to icons. Verified with headless
  layout tests only; on-device visual, high-contrast and screen-reader checks
  remain pending.
- Borrow widget concepts from DeskBox (concepts only; no code): Chinese-locale
  calendars show lunar dates, the 24 solar terms (2001–2099) and common
  festivals; notes get paper tints; short panels draw one-line capsule summaries.
  Add Fluent micro-animations: TODO rows glide when completed (FLIP) with a
  checkbox pop and eased progress bar, months fade in with a sliding title, and
  notes cross-fade tints and briefly confirm edits. Plugins receive the shared
  Fluent motion tokens and the Windows "Animation effects" switch; the host now
  re-reads a panel's wake interval after input. New dependency:
  `chinese-lunisolar-calendar` 0.2.5 (MIT). Headless tests only; EXE size gate
  and on-device motion not yet measured.
- Restyle the note, TODO and calendar widgets in a warm paper palette: cream
  ground (#F6F1E7), warm ink, terracotta accent, solid paper tints for notes,
  and serif titles (built-in Sitka). The widgets paint their own ground; the
  clock still follows the fence theme, and high contrast keeps system colors.
  Every text pair is tested at WCAG 4.5:1 or better. Not yet checked on device;
  Chinese titles fall back to the system CJK font.
- Add a Mermaid diagram widget (`pecofence.diagram`), inspired by Pele but fully
  native: a dependency-free parser and layered layout for flowcharts and Gantt
  charts, drawn as themed scene nodes with a new polyline primitive. New
  diagrams open the editor with an example; parse errors keep the last good
  diagram and show the line. Headless tests only. See
  [diagram widget](docs/DIAGRAM_WIDGET.md).
- Render the warm paper widgets in the approved faces: Noto Serif SC SemiBold
  titles and Noto Sans SC body text (new `FontRole::PaperBody`; the rest of the
  app keeps Segoe UI Variable). Subset OFL fonts (Latin, GB2312 symbols and
  3,755 level-1 hanzi; 1.1 + 1.6 MiB) ship in `fonts/` beside the EXE and load
  into a private DirectWrite collection without system installation. A missing
  folder falls back to Sitka Subheading / Segoe; characters outside the subset
  use DirectWrite system fallback. Font-branch release EXE 6,609,920 bytes (+9,216 over
  its parent; WSL lld-link cross-build). The combined build with the diagram
  widget must be measured against the 6,815,744-byte gate separately.
  Headless tests and Windows Clippy only; not yet checked on device.
- Size widgets on a grid of fence icon cells, as Android and iOS size widgets in
  home-screen icon slots ([widget grid](docs/WIDGET_GRID.md)). Each widget has a
  default span and a content minimum that rounds up with text scaling; one row is
  the capsule layout. Resizing snaps to whole cells, spans are saved with the
  window (older files migrate to the nearest span), and new windows are placed
  with the same 8 DIP gutter as drag-snapping. Live resizing not yet checked on
  device.
- Make single-widget windows frameless, like iOS/Android/macOS/Plasma widgets: no
  fence title bar above the widget's own header, so a span is all content. Drag
  empty widget areas to move (with the usual snapping and drop-to-merge),
  right-click or Shift+F10 for the fence menu; roll-up is disabled. A grab handle
  fades in at the top while hovering, and draggable areas show the move cursor. Tabbed groups
  keep their title bar. Not yet checked on device.
- Style every widget text field the same way: an in-place field over the element
  being edited (text exactly where the widget draws it), with the field's own
  surface, rounded corners and a 2 DIP accent underline. Editors now use the
  bundled Noto faces (registered privately for GDI) instead of Segoe UI. Todo edit
  keeps the checkbox visible; Todo add opens a new row with a "新任务…" hint; the
  Mermaid editor is a monospace page over the stage. Not yet checked on device.
- Redesign the clock as a paper widget: large serif numerals sized from its cells,
  a short terracotta rule and the date (with the lunar date in Chinese); one-row
  clocks show time and date side by side. Frameless windows no longer draw a
  half-cut fence title at the top edge.

The versioned entries below are retained from the upstream baseline. References
to Store submissions or a published website describe that history, not an active
distribution channel for this independent edition.

## 0.0.3

- Fence context menu → Sort → "Group by date": items are shown under Today / Yesterday / This week / This month / Earlier headers in the icon, list and details layouts.
- Settings → Organizing rules → Quick add: one click creates a fence and its rule for Images, Music, Videos, Archives, Installers, or "To clean up" (installers and archives unused for 30 days; gathered, never deleted).
- New rule condition "idle days": the item was neither modified nor opened from a fence for N days. Rules using it are re-run hourly.
- New file type category "Installers": .msi/.msix/.appx packages and setup/install-named .exe files.

## 0.0.2

- While Windows desktop icons are hidden, the special desktop items the user has enabled in Windows (Recycle Bin, This PC, User's Files, Network, Control Panel) appear in the Desktop fence: double-click opens them, the shell context menu works (Empty Recycle Bin, Properties), files dropped on the Recycle Bin are recycled, and the Recycle Bin icon follows its contents (also when files are recycled from elsewhere).
- Version bump for the Microsoft Store resubmission; package identity requires a new version per upload.
- Executables link the C runtime statically; the Visual C++ Redistributable is no longer required (Store policy 10.2.4.1).
- License changed from MIT to the Apache License 2.0; a NOTICE file accompanies the LICENSE.

## 0.0.1

- Renamed the product, executables and packages to PecoFence; existing configuration
  directories, environment overrides and startup entries are handled compatibly.
- Desktop fences, folder portals, tab groups, Explorer file operations and sorting.
- Quick Hide, Peek, Fluent and Liquid Glass themes.
- Automatic organizing rules, layout snapshots, configuration import/export and backups.
- Ten offline interface languages with live switching and Windows display-language detection.
- README and hero artwork in all ten languages under `docs/readme/`.
- Static product website in `site/`, built by `scripts/build-site.py` and published with GitHub Pages.
- Shared native/settings translation catalogs and translation validation.
- Portable x64 packaging, clean source export and draft GitHub Release workflow.
- Portable startup preserves the installed copy's Windows autostart registration.
- Independent instances use separate WebView2 profiles.

This is an initial release. Full compatibility testing across older Windows 11
versions and different graphics/display configurations is still in progress.

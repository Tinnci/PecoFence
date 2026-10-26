# PecoFence: SOLID review (2026-10-26)

Scope: the whole workspace as it stands, uncommitted work included. Measured
with line and field counts; generated bindings (`render/src/comp.rs`,
`platform/src/bindings.rs`, `render/src/gpu_bindings.rs`) are excluded from
judgements. Companion to [SPM_ARCHITECTURE_REVIEW.md](SPM_ARCHITECTURE_REVIEW.md),
which covers `plugin-spm` in detail.

Implementation audit: 2026-10-27. Findings below describe the original review
snapshot; recommendation statuses identify the resulting commits.

## What is already sound

- **Plugin seam (DIP, ISP).** `plugin-api` defines `PanelProvider`, `PanelInstance`
  and narrow service traits (`RenderService`, `IpcService`, `NavigationService`,
  `ClipboardService`, …) handed out as revocable capabilities. Panels never see
  `App` or a window. Tests substitute every service with mocks.
- **Pure core.** `core` (model, geometry, rules) has no Win32 dependency and is
  tested on Linux.
- **Contracts as a crate.** `spm-contracts` is the single wire definition for both
  sides, with golden fixtures.
- **Headless rendering.** `render::HeadlessCanvas` and `raster` let layouts be
  snapshot-tested without a GPU.
- **Substitutability (LSP).** No implementation found that breaks its trait's
  contract; the paper and high-contrast fallbacks are documented rather than
  surprising.

## Findings

| # | Principle | Finding | Evidence |
|---|---|---|---|
| 1 | SRP | `FenceViewState` holds title-bar state, file-grid state, drag and drop, scrolling, tooltips, accessibility, roll and peek animation, theme and the plugin-panel host in one struct. A panel fence carries every file-grid field unused. | 161 fields; `impl FenceViewState` spread over 16 files |
| 2 | SRP | `App` is a coordinator, a command interpreter, a persistence driver and a feature host at once; its methods are split across files by `impl` blocks rather than by owner. | 50 fields; 188 methods in 18 `impl App` files |
| 3 | OCP | Every feature adds a variant to one `Command` enum and an arm to one match. Adding caption buttons meant editing both. | 61 variants; `App::handle` is about 510 lines |
| 4 | OCP | Title-row controls (up, chevron, panel command, caption) are each wired by hand through scene nodes, hit-testing, press, release, hover, NC hit-test, accessibility ids, tooltips and drawing. | 4 control kinds × ~9 sites; panel commands and captions were added this way in the last two changes |
| 5 | OCP | Fence materials (Fluent acrylic, Liquid Glass, Paper) are boolean flags on `Theme`, so each new material or tweak edits every branch. | 39 `if theme.liquid_glass / paper / high_contrast` branches in `render` and `app` |
| 6 | ISP | `FenceWindow` exposes one API for file fences and panel fences; callers must know which methods apply (`set_items`, `set_layout`, `set_columns_visible` are called for panel fences too). `FenceWindow::create` takes 12 parameters. | 61 `pub fn` in `fence_window/api.rs`; `create` is 288 lines |
| 7 | SRP | `AppState` mixes document editing, layout geometry, work areas, portal sources and window bookkeeping. | 86 methods in one `impl` |
| 8 | DIP | The app crate contains SPM's pipe transport (`spm_transport.rs`, which speaks `spm-contracts`), while the architecture document allows SPM names only in the composition root. | `spm_transport.rs`, `panel_manager.rs` and `app/mod.rs` import SPM types |
| 9 | DIP | Application logic calls Win32 wrappers directly (`pecofence_platform::shell`, `window`, `accessibility`, …), so `App` itself cannot be unit-tested; only `AppState` and the window internals have tests. | `shell` 17 call sites, `window` 16, `accessibility` 9 in `app/` |

## Recommended changes, by value against effort

1. **Title controls as data (fixes 4, eases 1). Done — `ff79ef5`.** One `TitleControl { id, kind,
   rect, glyph, label, enabled, state }` list built per frame from the deco, the
   tabs, the panel chrome and the caption flag. Scene nodes, hit-testing,
   press/release, hover, NC hit-test, accessibility ids and tooltips iterate
   that list; drawing dispatches on `kind`. Adding a control becomes one entry.
   Contained in `fence_window` and `render::fence_chrome`; the existing
   chrome-scene and caption tests guard it.
2. **Split `FenceViewState` (fixes 1, 6). Done — `5c69840`; file/panel facets in `4e32656`.** Keep it as the composite and move
   state into owned parts: `TitleBar` (tabs, hover, press, captions, panel
   chrome), `FileView` (items, selection, scroll, drag and drop, columns),
   `PanelHost` (plugin panel, mounts, invalidation) and `Motion` (roll, peek,
   fades). Then give `FenceWindow` two facets, `files()` and `panel()`, so a
   caller cannot call file methods on a panel fence.
3. **Route commands by feature (fixes 3, eases 2). Done — `097acf9`.** `Command::Fence(FenceCmd)`,
   `Command::Panel(PanelCmd)`, `Command::Settings(SettingsCmd)`,
   `Command::Desktop(DesktopCmd)`, each handled by its own module's function.
   `App::handle` becomes a short dispatcher; features stop sharing one match.
4. **Material as a strategy (fixes 5). Done — `df29e7d`.** A `Material` trait (title fill, hover and
   pressed fills, backdrop source, corner radius, ink over the material) with
   `Fluent`, `LiquidGlass`, `Paper` and `HighContrast` implementations chosen
   once. `Theme` keeps tokens; drawing asks the material.
5. **Move SPM transport out of `app` (fixes 8). Done — `f0d9f2d`.** Put `spm_transport.rs` behind
   `IpcService` in its own crate (or in `plugin-spm`'s host adapter) and register
   it from the composition root, as the board and summary providers already are.
6. **Ports for the shell (fixes 9, later). Done for the scoped capabilities — `a222933`.** Thin traits for the few platform
   capabilities `App` decides on (open, file operations, clipboard,
   accessibility notifications) would let command handlers be tested on Linux.
   Worth it only after step 3 makes handlers small enough to test.

The implemented parts are `TitleBar`, `FileView`, `PanelHost` and `WindowMotion`.
`FenceWindowSpec` groups creation arguments. Shell, clipboard and accessibility
notification traits now have native adapters. The Win32 coordinator itself still
requires Windows; extracting pure command use cases and adding Linux handler
tests remains follow-up work. Finding 7 (`AppState` responsibilities) is also
open; this pass did not split the document model.

## Verification audit (2026-10-27)

- Linux: 300 tests pass across core, plugin-api, plugin-kernel, plugin-spm,
  plugin-widgets, spm-contracts and spm-transport. The five board pixel snapshots
  match without regeneration.
- Windows: 185 app tests pass, two existing live WinUI tests remain ignored;
  all 55 render tests pass. Strict Windows workspace Clippy and strict Linux
  Clippy for supported libraries, binaries and tests pass; formatting passes.
- Native caption regression: five consecutive isolated runs pass, each with
  three maximize/restore cycles, edge hits, outside release, capture cancellation
  and saved close state. Input uses synthetic native messages at 96 DPI.
- Test-count reconciliation: 16 actor tests moved from `app::spm_transport` to
  `spm-transport/src/actor.rs`; four host IPC tests moved from
  `app::panel_manager::pipe_tests` to `spm-transport/src/host.rs`. No test was
  deleted. Thus the prior 203 passing tests became 183 app + 20 transport;
  two subsequent caption edge/disabled-command tests bring this to 185 + 20.

Title controls, caption lifecycle and the native title sheet land together in
`ff79ef5` because they share the committed chrome frame. Shared paper and
navigation prerequisites precede the board. The remaining refactors each have
a separate commit.

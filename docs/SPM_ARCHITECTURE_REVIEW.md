# SPM panels: architecture review (2026-10-26)

Scope: `crates/plugin-spm` (summary panel and board), its use of `plugin-api`, and
the shared paper style. Measured on the working tree, uncommitted work included.

Implementation audit: 2026-10-27. Findings retain the original review snapshot;
statuses below distinguish completed changes from production follow-up work.

## What the code is for (first principles)

One job: show one SPM read model (a project and delivery scope at a revision)
and let a person act on it. Everything else is a means:

- **Data**: subscribe, accept revisions in order, page evidence.
- **Actions**: refresh, copy briefing, open a source record.
- **View state**: selection, filters, scroll, which presentation.
- **Drawing**: turn the above into scene nodes for a fence or a window.

Each should change for one reason. Today they are not separated that way.

## Findings

| # | Finding | Principle | Evidence |
|---|---|---|---|
| 1 | Two panels consume the same read model with separate cursors, action queues, status text and navigation. The board already has compact and capsule presentations that cover the summary panel's job. | First principles, DRY | `lib.rs` (`SpmPanel`, 1,378 non-test lines, 18 fields) and `board/mod.rs` (`BoardPanel`, 39 fields) |
| 2 | `BoardPanel` holds transport, revision cursor, evidence cache, pending requests, view state, palette, text cache and layout ids in one struct. | SRP | `board/mod.rs:156` |
| 3 | The demo source is a variant inside the panel (`Backend::Demo`, `deliver_demo`), so the panel knows about fixtures and timing. | DIP | 11 `Backend::Demo`/`deliver_demo` references in `board/mod.rs` |
| 4 | `layout.rs` is about 2,650 lines: drawing primitives, header, filters, cards, lanes, detail, timeline, compact and capsule in one file. | SRP | `board/layout.rs` |
| 5 | The panel draws its own title row, and the fence draws another title ("Board") above it. In a standalone window the title would appear a third time. | ISP, DIP | `layout.rs` `fn header`; host fence chrome |
| 6 | The summary panel's strings are English-only; the board has a zh/en table. | Consistency | `lib.rs` status strings vs `board/text.rs` |
| 7 | Paper colors and contrast math were copied between widgets and SPM. | DRY | fixed below |
| 8 | "Open source" logic was written twice, once per panel. | DRY | fixed below |

## Done in this pass

- **Done — `dcfb9c8`, `a3c6566`.** `plugin-api/src/paper.rs`: one paper resolver and contrast helpers. Widgets map
  it to their `Colors`, the board to its `Palette`, the summary panel to its
  tokens. Widget colors are unchanged: all 88 widget tests pass, including the
  contrast sweep over every paper style, brightness and wallpaper.
- **Done — `a3c6566`.** `board/palette.rs`: color roles moved out of the layout file. High contrast is
  its own constructor instead of a branch per field.
- **Done — `16cd9b0`.** `navigation::open`: one function resolves a target and asks the host to open
  it; both panels only map the outcome to their own message.
- **Done — `a3c6566`.** Timeline entries link to their source: the source name ("Gerrit ↗") opens that
  record directly, without expanding the entry first.

## Recommended next steps, in order

1. **Header as data (fixes 5). Done — `a3c6566` (contract), `ff79ef5` (native hosts).** A panel publishes its chrome instead of
   drawing it: `HostCommand::SetChrome { title, subtitle, status, commands }`,
   where each command has an id, label, glyph and enabled state. The fence
   renders it in its title strip; a standalone window renders it in the app
   title bar (see the canvas artboard "Title bar"). Command clicks come back as
   `PanelEvent::Command { id }`. The board now draws only filters and content;
   the fence shows title, subtitle, status and icon commands (with tooltips and
   UIA names) in its single title strip. A standalone window can render the same
   `PanelChrome` in its title bar.
   **Title bar, WinUI-aligned.** A fence whose panel publishes chrome draws as
   one matte paper sheet in the panel's own ground (`PanelChrome.surface`): flat
   title, no glass, no divider, like a Windows Settings window. Status severities
   are WinUI's `InfoBarSeverity` levels, with inks shared in `paper::status_ink`.
   `chrome_title_bar.rs` maps the same chrome onto a Reactor `TitleBar`
   (title, subtitle, `AppBarButton`s with `FontIcon` and tooltips) for a
   standalone window; Reactor's `TitleBar` has no background, so that window
   paints `sheet_color` on its root grid.
2. **Split `BoardPanel` (fixes 2). Not done; no implementation commit.** `BoardSession` (subscription, cursor,
   pending requests, evidence cache; no drawing), `BoardView` (selection,
   filters, scroll, tab, collapsed lanes) and the renderer (palette, text cache,
   layout ids). Each is testable alone; the session tests need no layout.
3. **Demo as a service (fixes 3). Not done; no implementation commit.** Implement the demo as an `IpcService` that
   the host or the tests hand to the panel. The panel then has one data path,
   and `BoardSource::Demo` becomes a host configuration, not panel code.
4. **Split `layout.rs` by surface (fixes 4). Not done; no implementation commit.** `builder` (primitives),
   `header`, `card`, `workspace`, `detail` (with `timeline`), `compact`,
   `capsule`. No behaviour change; the pixel snapshots guard it.
5. **Retire the summary panel (fixes 1, 6). Not done; no implementation commit.** Add the gate summary to the
   board's compact and capsule presentations, migrate saved summary fences to
   the board provider, then delete `SpmPanel`. Until then it shares the
   palette and navigation code.

Each step is behaviour-preserving and covered by the existing tests and the
board's pixel snapshots, so they can land one at a time.

## Verification and limits (2026-10-27)

All 44 plugin-spm tests and all five board pixel snapshots pass without
regeneration. Widgets retain all 88 passing tests. The native app passes 185
tests (two live WinUI tests ignored), render passes 55, and the caption driver
passes five consecutive runs. See [SOLID_REVIEW.md](SOLID_REVIEW.md) for transport
test moves and the full verification scope.

Recommendations 2–5 remain a deliberate backlog. The daemon still has no board
producer; this pass verifies the deterministic demo and host, not production
board delivery. The debug-only hosting experiment (`7864268`) is documented in
[WORKSPACE_HOSTING_SPIKE.md](WORKSPACE_HOSTING_SPIKE.md); its focus, positioning,
overlap and multi-monitor DPI limitations remain open.

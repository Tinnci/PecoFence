# Evidence board: design and native vertical slice

Status: implemented as a runnable vertical slice on a deterministic demo source,
2026-10-26. It builds on [the proposal](EVIDENCE_BOARD_PROPOSAL.md) and the Phase 0
contract in `crates/spm-contracts/src/board.rs`. The SPM daemon has no board producer
yet, so the live source only reaches the "waiting for SPM" state.

```bash
# Windows debug build from WSL, then the demo in an isolated portable instance
scripts/build-windows.sh dev pecofence
# Copy scripts/demo-board.ps1 and third_party/winappsdk/runtime-manifest.json
# to C:\temp\pf-board, preserving their relative directories. Run the local copy:
powershell.exe -NoProfile -File 'C:\temp\pf-board\scripts\demo-board.ps1' \
  -Binary "$(wslpath -w target/x86_64-pc-windows-msvc/debug/pecofence.exe)"
# Scripted screenshot tour: -Out <dir> [-Theme light] [-Tour tour|scaled|live] [-TextScale 150]
# Headless behaviour, layout and raster checks
cargo test -p pecofence-plugin-spm
```

The demo window says "Demo data · Step n of 3" in its header. F5 or Refresh advances one
scripted revision; clicking that header chip restarts the script.

## What the board answers

| Question | Where the answer is |
|---|---|
| What is progressing? | Column position (To do → Develop → Verify → Done) per model lane; the status line names cards that moved after each refresh |
| What needs my attention? | Coloured left bar plus a shaped glyph and a reason line on the card; "Needs attention n" filter; the compact view ranks by it |
| Who should act next? | `→ Owner: action` on every card that has a next action; the detail repeats it in full |
| What evidence supports this? | Detail: separated status facts, people grouped by meaning, and a lazy evidence timeline with source, time and the relationship each row supports |

## Initial assessment of the proposal

The proposal's architecture holds; several view hypotheses did not:

1. **Three summary columns hide where work waits.** With To do / Doing / Done, almost all
   work sits in Doing. The board splits Doing into **Develop** (implementation, review) and
   **Verify** (integration build, regression, acceptance). This is a display grouping of
   the backend's `stage`; `column`, `stage` and start/finish rules stay backend-owned.
2. **Unmapped and Done do not deserve full columns.** Unmapped is rare and needs a policy
   fix, Done needs no action. Both shrink to a 64-DIP count rail before a working column
   gets narrower than a card. A rail click opens the unmapped card or expands Done.
3. **Initials avatars are ambiguous** across Chinese and English names. Cards show the
   assignee's display name, ellipsized to at most 48 % of the card width.
4. **A source outage is not card-level attention.** The first native capture showed a
   failed Gerrit refresh marking all 15 cards and "Needs attention 15". The outage now
   appears once in the header ("Gerrit did not refresh · Totals may be low") and in the
   affected card's detail; it does not enter the attention count.
5. **The host could not scroll or navigate panels.** `PanelEvent` had no keys or wheel.
   The slice adds `PanelEvent::Key { key: PanelKey, shift }` and
   `PanelEvent::Scroll { x, y, lines }`; Tab, and Enter/Space on a focused host control,
   stay with the host.

Constraints of the native stack that shaped the design: scene strokes are rectangular (rounded
outlines are drawn as two fills); single-line text ellipsizes in DirectWrite but wrapped text
does not, so titles are clamped to two lines by measured binary search; the fence material is
translucent, so the board paints an opaque (97 %) surface; in high contrast every semantic
colour is the text colour, so every signal also has its own glyph shape and words.

## Two compositions

Both were evaluated with the demo data (15 cards, 3 models, 3 follow-ups, a Chinese title of
31 characters, a 28-character English name) and a 60-card density case
(`many_cards_scroll_and_keyboard_selection_stays_visible`).

**A. Stage board (selected).** Columns by stage group, one swimlane per model, cards of
variable height (number + assignee, two-line title, one signal-or-facts line, one next-action
line). Detail docks on the right from 1100 DIP.

**B. Attention rail.** One row per requirement grouped by model, a five-stop stage track in
the row, code/build/test pips, and the next action in a right-hand column.

| Criterion | A. Stage board | B. Attention rail |
|---|---|---|
| "What is progressing" at a glance | Distribution per lane is spatial; moved cards change columns | Must read every track |
| Long CN/EN titles | Two lines at 176–330 DIP (≈ 20–40 CJK characters) | One line, longer but truncates more often |
| Follow-up density | Follow-ups stack in To do; lane height shows the backlog | Every follow-up costs a full row |
| Who acts next | One line per card, ellipsized | Always visible in its own column |
| Keyboard | Natural 2-D movement (column, lane) | 1-D list |
| Narrow widths | Needs a tab mode | Degrades well |

A wins on the board being the visual centre and on progress being readable without reading
text. B's strength — ranking by who must act — is kept as the **compact view**, where columns
do not fit anyway.

## Layout rules

All sizes are logical DIPs before text scaling; text scale divides the available width, so
150 % and 200 % text move the layout to the next breakpoint exactly as a narrower window does.

| Width (logical) | Composition |
|---|---|
| < 600 or height < 420 | Compact: ranked list (blocked → failed → conflict → unassigned → unmapped → oldest start), counts line; a row opens a compact detail |
| Columns fit at ≥ 232 each | All columns full width |
| Columns fit at ≥ 176 with Unmapped/Done rails | Rails for Unmapped and Done |
| Otherwise | Tabs: one column at a time, all lanes stacked; ←/→ switch tabs |
| ≥ 1100 with a selection | Detail docked at 34 % (380–460) |
| < 1100 with a selection | Detail replaces the board, with "Back to board" |
| Host Capsule presentation | Project name, four counts, blocked count, stale-source glyph |

- **Lanes** are as tall as their tallest column; a lane header shows the model, card count,
  blocked and attention counts, and collapses on click. Filtering by model hides other lanes.
- **Cards** grow with their title (one or two lines). Number: 13/700. Title: 14/400. Meta:
  12. The assignee is right-aligned on the number line; "Unassigned" uses the warning tone
  and the person glyph, so it never repeats on the signal line.
- **Signal line** shows the most severe card signal with its glyph: Blocked ⊘, Test/Build
  failed ⓘ, Owner conflict ⚠, Unmapped ⚠, Code ahead of workflow ↺. Otherwise it shows the
  facts line, for example "Regression · Code merged · Test running", where workflow stage,
  code, build and test are separate words. A candidate (unconfirmed) link says so.
- **Scrolling** is one vertical offset for the board and one for the detail. Scrolled
  content is cropped at the board region: fills and hit areas are intersected; a text line is
  drawn only when it fits whole, so nothing paints over the header and nothing reflows.
- **Counts** come from the snapshot; when `totals_complete` is false every count reads "≥n".
- **Times** are shown in the project timezone with the zone named once in the header. Zones
  with daylight saving are not guessed; they are shown in UTC and say so.

## Interaction

| Input | Effect |
|---|---|
| Click card | Select and open detail; click again closes the detail |
| ↑ ↓ / Home End | Previous/next card in the column (across lanes) |
| ← → | Nearest card in the adjacent column, same lane first; in tab mode, switch tabs |
| Enter | Open the selected card's detail |
| Esc | Close the detail, then clear the selection |
| PgUp PgDn, wheel | Scroll the board, or the detail when the pointer is over it |
| F5, Refresh | `Refresh`; a second request while one runs says so and sends nothing |
| Copy summary | `BuildBriefing` for the shown revision; copied only when the response revision is still the shown one |
| Open in Meegle / Open source | `ResolveNavigation`; the status line says why nothing opened when no origin is configured |
| Copy ID / Copy reference | Copies the source identity, such as `meegle:acme/orion/story/88232` |
| Find | Native single-line editor; matches number, title and person names |
| Tab | Host focus over every control in reading order |

**Selection** is kept by `card_id`. After a refresh the status line lists moves ("Revision 58:
ORION-R-108 moved Verify → Done · Gerrit did not refresh"); a card that left the board clears
the selection and is named. **Evidence** loads on first open per `(card, revision)` with a
skeleton; a failure shows the reason and "Try again". Invocations from an older layout or
data revision are ignored.

States: loading (skeleton columns), waiting for SPM after 8 s with "Try again", load failure
with the error, empty scope, no filter match with "Clear filters", stale source (header names
the source and its age), partial source (≥ counts and the header banner), conflict (card and
detail), evidence loading/failed.

## The evaluation scenario

| Step | ORION-R-102 "Move the Search box to the bottom" | What the detail shows |
|---|---|---|
| 1 · after Gale's reply (rev 41) | To do, **Unassigned** | Responsible now: Unassigned. Code history and Contact: Morgan, "contact-from-history@1 · not an assignment". Timeline: Morgan's PS1 (10-14), Taylor's task list (10-26 10:01), Gale's reply "1 Casey 3 me 4 Morgan; task 2 is not named" (10:11) |
| 2 · after the Meegle update (rev 57) | Develop, **Casey** | Responsible now: Casey, "Meegle field · 10-27 14:00"; the Meegle row in the timeline has a filled dot and "assignment Casey ← Responsible now". Morgan is still only history and contact |

R-101 keeps Gale's Lark statement as "Earlier statement: Gale named Casey in Lark · authority
unknown"; R-103 and R-104 show "Not in Meegle" until Meegle has an owner. Additional cases:

| Case | Card | How it reads |
|---|---|---|
| Merged, regression pending | R-087 | Verify; "Regression · Code merged · Test running"; Acceptance "Not accepted" |
| Reviewer is not the developer | R-087 | Responsible now 司马云澜 (tester); Also assigned Riley; Developer Riley; Reviewer and Submitter Jordan |
| Blocked | R-093 | Red bar, ⊘ "Blocked: Waiting for the vendor BSP drop…"; next action owned by the vendor contact |
| Partial refresh | rev 58 | Header "Gerrit did not refresh · Totals may be low", "≥" counts; detail "Gerrit did not refresh · observed 10-26 09:40" |
| Owner conflict | R-099 | ⚠ "Owner conflict: Gale named Morgan in Lark…; the Meegle owner is Casey" |
| Failed regression after merge | R-124 | Danger signal; stays in Verify |
| Code ahead of workflow | R-120 | Develop with "Code ahead of workflow" |
| Unmapped workflow node | R-121 | Unmapped rail; evidence fails once in the demo to show retry |
| Acceptance moves a card | R-108 | Verify → Done in rev 58 while selected |
| Long title / name | R-112, R-093 | Two-line clamp; "Alexandria Montgomery-Okafor" ellipsized |

## Verification

Observed on 2026-10-26:

- `cargo test -p pecofence-plugin-spm`: 35 passed (20 board tests: contract validity of the
  demo fixtures, the R-102 before/after scenario, distinct facts for R-087, refresh with
  selection kept across reorder, single-flight refresh, lazy evidence and retry, keyboard
  navigation, filters, summary copy, navigation without an origin, stale invocations,
  composition per width, 60-card scrolling, two-line clamp, and geometry checks — no text
  outside the viewport and no overlapping text — for dark, light and high-contrast tokens at
  text scales 1.0/1.5/2.0 and five sizes). `spm-contracts`, `plugin-api` and `plugin-widgets`
  tests also pass; clippy reports no warnings.
- Raster snapshots (`crates/plugin-spm/tests/snapshots/board-*.png`, text as bars) for
  dark 1440, light 1120, high contrast 900, tabs 700 and compact 480.
- Windows, real Direct2D/DirectWrite, 1920×1080 at 100 % DPI, dark and light, with the
  debug-only `PECOFENCE_TEST_TEXT_SCALE` at 150 % and 200 %:
  `crates/plugin-spm/tests/snapshots/native/*.png`. Keyboard input was sent as window messages by the
  test script.

Not verified: other monitor DPI settings (changing them is a system setting; DIP layout is
DPI-independent and text scale covers the reflow), Windows high-contrast themes on screen
(covered by headless tokens only), screen readers. Windows app tests were subsequently run; see the audit below.

| View | Screenshot |
|---|---|
| Wide, step 1, R-102 unassigned | [dark-wide-step1-r102](../crates/plugin-spm/tests/snapshots/native/dark-wide-step1-r102.png) |
| Wide, step 2, R-102 assigned, light | [light-wide-step2-r102](../crates/plugin-spm/tests/snapshots/native/light-wide-step2-r102.png) |
| Wide, R-087 reviewer ≠ developer | [dark-wide-step2-r087](../crates/plugin-spm/tests/snapshots/native/dark-wide-step2-r087.png) |
| Wide, partial refresh | [dark-wide-step3-partial](../crates/plugin-spm/tests/snapshots/native/dark-wide-step3-partial.png) |
| Medium, detail replaces board | [light-medium-detail](../crates/plugin-spm/tests/snapshots/native/light-medium-detail.png) |
| Medium board with rails | [dark-medium-board](../crates/plugin-spm/tests/snapshots/native/dark-medium-board.png) |
| Narrow, tabs | [light-narrow-tabs](../crates/plugin-spm/tests/snapshots/native/light-narrow-tabs.png) |
| Compact and compact detail | [dark-compact](../crates/plugin-spm/tests/snapshots/native/dark-compact.png), [dark-compact-detail](../crates/plugin-spm/tests/snapshots/native/dark-compact-detail.png) |
| Text 150 % / 200 % | [dark-scaled-150-board](../crates/plugin-spm/tests/snapshots/native/dark-scaled-150-board.png), [light-scaled-200-r102](../crates/plugin-spm/tests/snapshots/native/light-scaled-200-r102.png) |
| Live source: loading, waiting | [dark-live-loading](../crates/plugin-spm/tests/snapshots/native/dark-live-loading.png), [dark-live-waiting](../crates/plugin-spm/tests/snapshots/native/dark-live-waiting.png) |

## Code map

| Concern | Location |
|---|---|
| Panel, state, contract handling, keys, scroll | `crates/plugin-spm/src/board/mod.rs` (provider `pecofence.spm.board`) |
| Presentation facts (columns, signals, people rows) | `board/model.rs` |
| Layout for board, tabs, detail, compact, capsule | `board/layout.rs` |
| Strings (en, zh-CN), time, text fitting | `board/text.rs` |
| Demo backend speaking the contract | `board/demo.rs`, fixtures `crates/plugin-spm/fixtures/board-demo/` from `scripts/board_demo_fixtures.py` |
| Host key and wheel forwarding | `crates/app/src/fence_window/plugin_panel.rs`, `PanelEvent::{Key, Scroll}` in `plugin-api` |
| Demo fence and test input | test-script `new-board x y w h [live]`, `input <fence> vk|wheel` |

## Smallest contract changes for production

1. **Board subscription delivery.** Confirm that the host SPM transport forwards
   `Event::Board` envelopes for a `ViewKind::Board` subscription, and `GetCardEvidence`
   completions; gate on `Feature::Board` from Hello and show "SPM does not provide boards"
   instead of waiting.
2. **`BuildBriefingRequest.view: Option<ViewKind>`** (and later the active filter), so the
   summary is built from the board rather than the project snapshot.
3. **`Relationship.role: Option<String>`** — the workflow node or role of an `assigned` row.
   R-087 has two current Meegle assignments (develop, regression); the view can only say
   "Also assigned" today.
4. **`BoardCard.follows: Option<RecordId>`** for follow-up work, so a follow-up can show its
   parent and be grouped.
5. **Navigation origins on the host.** `ResolveNavigation` returns a configured origin id and
   a relative path; the host needs an origin table (id → https base) before "Open" can open.

All board types use `deny_unknown_fields`, but the backend does not consume them yet, so these
additions are free now and lockstep later.

## Backend dependencies and next step

- SPM backend: board projection (proposal Phase 1), board cache and `Event::Board`,
  `GetCardEvidence`, board briefing, `Feature::Board` in Hello.
- people-cli: identity export and reviewed allocation claims (Phase 3 manual, Phase 4
  producer).
- Jenkins acquisition for build state (Phase 5); regression state from the Meegle test role.

**Next useful step:** implement Phase 1 projection in the backend as a pure function and
make it reproduce `crates/plugin-spm/fixtures/board-demo/step1..3-board.json` from
observations; then point this panel's `"source": "spm"` at it — the panel code path is
already the production one.

## Verification audit (2026-10-27)

The current plugin-spm suite passes all 44 tests, including unchanged raster
snapshots. Windows app/render binaries pass 185/55 tests respectively; two
existing live WinUI tests are ignored. Strict Clippy passes for Windows and
supported Linux targets, and `cargo fmt --check` passes. The isolated native
caption regression passes five times consecutively (15 maximize/restore cycles,
edge hits, outside release, capture loss and persisted close).

The native PNGs above are reference captures from 2026-10-26, preserved byte for
byte under `tests/snapshots/native`; they were not recaptured or accepted as new
pixel baselines during this audit. Multi-monitor DPI, on-screen Windows high
contrast, screen readers and production daemon board delivery remain unverified.
Navigation origins are implemented in `16cd9b0`; they still require deployment
configuration. Host chrome, the paper title sheet and captions are in `ff79ef5`.

## Delivery timeline (2026-10-27)

The content header now offers **Board** and **Timeline**. The choice, zoom,
selection, collapse and scroll state belong to the panel instance, not its
configuration. Flow and Standup are not advertised. Refresh keeps selection by
`card_id`; “Show on board” clears filters/collapses that would hide the selected
card and scrolls it into view.

`BoardCard` now carries an optional backend schedule (due, original due and
source basis), ordered non-overlapping stage spans, and at most twelve evidence
markers. `BoardSnapshot` carries milestones; blockers can carry an expected
arrival date. Missing new fields default when reading older producers. The panel
does not infer starts, due dates, dependencies or completion from activity.

The separate `board/timeline.rs` renderer presents model lanes, stage bars in a
three-step terracotta ramp, neutral dashed unstarted work, blocked hatching,
dashed remaining plans, due/original ticks, dependency lines, source-shaped
markers, milestone flags and a today guide. Week covers five weeks; month covers
61 days; quarter covers 92 days. Calendar boundaries use the project timezone
with the existing explicit UTC fallback for unsupported zones. Only week zoom
shades weekends. The number uses the existing system condensed font role;
measurement uses that same role, with no added font packaging.

Narrow fences show milestone countdowns and due-sorted groups (overdue/tomorrow,
this calendar week, later, unscheduled) on a fourteen-date track. Selecting a
wide row opens a bottom drawer with stage durations, blocked days, signed days
to due, next action and notes including the original due. Narrow details scroll
when needed. Arrows select rows, Enter opens the drawer and Escape closes it.
“Open evidence” uses the existing evidence detail. Marker hover/Tab focus sends
an additive host `PanelEvent::Inspect` and loads revision-pinned evidence without
changing selection. Bars have semantic descriptions; controls and markers have
names and the host paints focus feedback. Stages and blockers also have words
and glyphs in high contrast.

The deterministic Orion producer includes R-103 10-28 → 10-30, R-124 10-24 →
10-27, R-093 blocked from 10-18 awaiting BSP on 11-08, code freeze on 11-06,
release on 11-13, and completed R-080/R-081/R-090/R-110/R-111. Existing scenarios
remain (including R-102's assignment/start and R-108's acceptance on refresh),
so the native demo contains more rows than the static HTML composition. The
existing R-096 completion is retained. R-112 now belongs to the Lite lane as in
the approved timeline.

Verification on this implementation:

- 312 Linux tests pass across core (80), plugin-api (20), plugin-kernel (13),
  plugin-spm (54), widgets (88), contracts (37) and transport (20).
- Two Python fixture tests verify reproducibility, checked-in JSON parity,
  approved dates, current-stage consistency and lazy evidence pointers.
- Strict Clippy passes for those Linux packages and the entire Windows workspace,
  including all targets. `cargo fmt --check` passes.
- Windows test binaries built with `scripts/build-windows.sh test pecofence
  pecofence-render pecofence-spm-transport` and run through a local PowerShell
  `-NoProfile -File` driver: 185 app tests pass (two existing live WinUI tests
  ignored), 55 render tests pass, 20 transport tests pass.
- New headless PNGs: `timeline-week`, `timeline-month`, `timeline-compact`,
  `timeline-hc`, `timeline-drawer`. All ten headless baselines match. **Deviation:**
  the five prior board PNGs were intentionally regenerated for the new content
  header and approved fixture changes; they are not byte-identical to the old
  baselines. The old native reference captures remain unchanged.
- Real Direct2D/DirectWrite captures of week, month, drawer and compact in explicit
  Light and Dark paper are under
  [`tests/snapshots/native/timeline`](../crates/plugin-spm/tests/snapshots/native/timeline/).
  Every capture was visually inspected. The driver uses Tab/Enter instead of
  locale-sensitive click coordinates and sets the isolated instance's paper
  preference (app `--light` alone does not override Adaptive paper).

Reproduce the native tour by copying `scripts/demo-board.ps1` and the runtime
manifest to a local Windows folder, then running the local script with
`powershell.exe -NoProfile -File ... -Tour timeline -Theme light|dark -Out ...`.
No execution-policy override is needed.

Still unverified: production daemon delivery (there is no board producer), a
screen reader, on-screen Windows high contrast, other monitor DPI settings,
and a native Soft-paper tour. Soft uses the shared paper resolver; high contrast
and 100/150/200-percent text layouts are exercised headlessly. The narrow drawer
uses the available surface and scrolls rather than mirroring the HTML's wide
three-column drawer. Evidence previews are a fixed bottom callout, not a floating
bubble attached to the marker. Done items share a collapsed lane strip and may
overlap in time; their individual identities remain in semantic descriptions.

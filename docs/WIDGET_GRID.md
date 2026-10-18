# Widget grid (design proposal)

Status: implemented (span maths in `crates/plugin-widgets/src/grid.rs`, host in
`crates/app/src/app/fences.rs` `apply_widget_grid` and the fence window's `WM_SIZING`
/ `WM_GETMINMAXINFO`). Verified with headless tests and a Windows clippy build only;
live resizing on a Windows desktop has not been checked yet. Sizes are in DIPs at the
default settings (48 px icons, two label lines, 100% text) unless stated.

## What existed before

| Thing | Current rule | Source |
| --- | --- | --- |
| Base unit | 4 DIP; every fence metric stays on it | `layout.rs` `GridMetrics` |
| Fence icon cell | `icon + 32` wide, `icon + 44` tall (80 × 96 at 48 px) | `GridMetrics::for_icon_size` |
| Fence width | snapped to whole icon columns (`n × 80`) | `width_for_columns` |
| Window gap | 8 DIP when dragging next to another fence | `SNAP_GAP_DIP` |
| New-window gap | 12 DIP | `place_new_fence_dip` |
| Widget sizes | fixed per widget: clock 340 × 190, note/todo/calendar 420 × 370, diagram 560 × 400 | `new_widget_near` |
| Widget minimum | one size per presentation: compact 400 × 280, capsule 280 × 48 | `Presentation::minimum_size` |

So fences live on the icon grid, but widgets don't. A 420-wide widget is 5¼ icon
columns, so it never lines up with a fence above or below it. Placement also uses
a 12 DIP gap while dragging snaps to 8.

## What Android and iOS base their grids on

Both platforms build the widget grid from **the home-screen icon slot**, so a widget
is "n icons wide and m icons tall" and always lines up with app icons.

- **Android:** widgets are declared in launcher cells (`targetCellWidth`/`Height`),
  with dp minimums (`minWidth`/`Height`, `minResizeWidth`/`Height`) that are rounded
  **up** to whole cells. Cell size and margins vary by device. On a Pixel 4 (5×4
  grid), a widget of n × m cells gets about `(73n − 16) × (118m − 16)` dp in
  portrait, so the pitch is the icon cell and the margin comes off once. Content
  declares a dp minimum, and the launcher turns it into cells.
  ([layouts](https://developer.android.com/guide/topics/appwidgets/layouts),
  [sizing](https://developer.android.com/guide/topics/appwidgets))
- **iOS:** there are no free sizes, only families measured in icon slots: small = 2 × 2,
  medium = 4 × 2, large = 4 × 4 (extra large on iPad). The point size comes from
  each device's icon grid, e.g. 169 × 169 / 360 × 169 / 360 × 379 pt on 414-pt-wide
  iPhones. The commonly cited content margin is 16 pt (11 pt around graphics).
  ([WidgetFamily](https://developer.apple.com/tutorials/data/documentation/widgetkit/widgetfamily.md),
  [device sizes](https://developer.apple.com/forums/thread/671621),
  [margins](https://uxdesign.cc/designing-widgets-for-ios-macos-and-ipados-the-ultimate-guide-737fb284a9df))

The common rules:

1. The **cell comes from the icon grid**, not from the widget.
2. Widgets declare a **content minimum in device-independent units**. The host
   rounds that **up** to whole cells.
3. Sizes are **stored as spans**, so a widget keeps its shape when the grid
   changes scale.
4. A one-row widget is a different, glanceable layout (Android 4×1, iOS
   accessory), not a squashed full layout.

## Proposal for PecoFence

### 1. Base unit: 4 DIP

Keep the existing 4 DIP grid for every size, gap, inset and radius. It is already
the rule for fences and for Fluent controls (32-DIP control height, 16-DIP content
inset, 8-DIP card radius).

### 2. Cell: the fence icon cell

**One widget cell = one fence icon cell**, taken from the user's own settings:
`cell_w = icon + 32`, `cell_h = icon + 44` (80 × 96 at the default 48 px icon).
This is the desktop equivalent of the iOS/Android icon slot.

- A widget n columns wide is exactly as wide as a fence showing n icon columns, so
  fences and widgets stacked in a column line up edge to edge.
- Changing icon size rescales every widget with the desktop, as iOS widgets scale
  with each device's icon grid.
- The cell's non-square shape is intentional: it is what a fence's icon rhythm
  already looks like.

Outer window size: **`width = n × cell_w`, `height = m × cell_h`**. A window showing a
single widget is frameless (see *Frameless widgets* below), so the span is all
content. A widget merged into a tabbed group gets the title bar back, and its span
then includes it.
The 8-DIP gutter sits **between** windows, not inside the span. That matches how
fences are sized and snapped today, so no fence layout changes. The trade-off is
that two side-by-side 2-column widgets are 8 DIP wider than one 4-column widget,
exactly as with fences now. If a perfect lattice matters more later, switch both
fences and widgets to Android's `n × pitch − gutter` form together.

### 3. Gutter: 8 DIP everywhere

Use the existing 8-DIP snap gap for placing new windows too (`place_new_fence_dip`
uses 12 today). Drag-snapping keeps its 10-DIP pull distance.

### 4. The minimum grid: content first, then rounded up

Each widget declares its **content minimum in DIPs** (Android's `minWidth`). The
host computes `min span = ceil(min DIPs × text scale / cell)`, so large text or a
small icon size raises the span instead of clipping the layout. The global floors
come from the chrome and Fluent controls already in the code:

| Floor | Derivation (DIP) | Result |
| --- | --- | --- |
| One row (capsule) | 48 capsule line (`Presentation::Capsule`) ≤ 96 (84 with a title bar in a tabbed group) | **1 row** is the capsule size |
| Two rows (full) | 66 header (`CONTENT_TOP`) + 40 list row + 16 inset = 122 ≤ 192 (158 with a title bar) | **2 rows** is the smallest full layout |
| Two columns | 16 + 16 inset + 32 icon button + 4 gap + ~3 CJK title glyphs (60) = 128 ≤ 160 | **2 columns** is the narrowest |
| Controls | 32-DIP control height, 4-DIP grid; touch targets ≥ 40 | unchanged |

So the **minimum grid is 2 × 1 (capsule) and 2 × 2 (full widget)**, and every
widget's own minimum sits at or above it.

### 5. Sizes per widget

Defaults are close to today's fixed sizes. Minimums are where the current layout
code still works (checked against the todo capacity, calendar row-height and lunar
thresholds).

| Widget | Default span → DIP | Today | Minimum full | Capsule |
| --- | --- | --- | --- | --- |
| Clock | 4 × 2 → 320 × 192 | 340 × 190 | 2 × 2 | 2 × 1 |
| Note | 5 × 4 → 400 × 384 | 420 × 370 | 2 × 2 | 2 × 1 |
| Todo | 5 × 4 → 400 × 384 (5 rows per page) | 420 × 370 | 3 × 3 | 3 × 1 |
| Calendar | 5 × 4 → 400 × 384 (41-DIP rows: lunar fits) | 420 × 370 | 4 × 4 (42 × 41 cells: lunar still fits) | 3 × 1 |
| Diagram | 7 × 4 → 560 × 384 | 560 × 400 | 4 × 3 | 3 × 1 |

Spans between 1 row and the minimum full height are not offered: resizing snaps
from the capsule straight to the minimum full span, as on Android.

### 6. Behaviour

- **Store spans, not pixels:** save `(columns, rows)` with each widget's layout.
  The window size is always derived from them, so icon-size, text-scale and DPI
  changes re-derive it.
- **Resize snaps to whole cells** during `WM_SIZING`, with the minimum from
  section 4. The minimum track size replaces the per-presentation constants for
  widgets.
- **Presentation follows rows:** 1 row → `Capsule`, 2 or more → `Compact`. The
  existing 120-DIP capsule threshold in the widgets stays as a safety net.
- **Migration:** existing widgets round their saved size to the nearest span on
  first load, then never below the minimum.
- **Inside a widget**, keep the current rules: 16 DIP content inset, 8 DIP card
  radius, 32 DIP controls, 4 DIP grid. The window corner stays Windows' own 8 DIP.

## Implementation notes

- Spans are stored as `span: { columns, rows }` on the window's `Container`
  (optional; omitted for file fences and in files written before this change).
- The cell is the widget content's own icon size and label lines, with the live
  label line height. **Fence spacing is ignored** for widgets (they have no spacing
  control), which settles the first open question.
- Widgets snap only to other windows' edges, not to a desktop-wide lattice.
- Re-derived on load, DPI change, icon-size change, text scaling and icon-title font
  changes, and after every interactive resize (`FenceBoundsChanged`).
- In a tabbed group the title bar's height follows the system font and is part of
  the span, so at large text the content is a little smaller. The text-scaled
  minimum rounds up to cover that. Frameless widgets are not affected.
- Deferred: a rolled-up widget keeps its stored span but is not resized until it is
  expanded. Tabbed groups use the active tab's grid.

## Frameless widgets

Like iOS, Android, macOS and KDE Plasma widgets, a window that shows a single widget
has no title bar: the widget's own header (title, summary, actions) is the only
chrome, and the span is all content.

- **Moving:** drag any part of the widget that is not a control (the header text,
  the margins, the paper ground). The window treats those areas as its caption, so
  drag-snapping, the 8 DIP gutter and drop-to-merge work as for fences.
- **Fence actions** (rename, lock, appearance, delete): right-click anywhere on the
  widget, or press Shift+F10 / the menu key. *Roll up* is disabled: there is no title
  bar to roll into, and a saved rolled state is cleared on load.
- **Tabs:** dropping a fence or widget onto a widget makes a tabbed group, which
  gets the title bar and tab strip back. Detaching back to a single widget makes it
  frameless again.
- **Implementation:** the window keeps a zero title height in its own theme copy
  (`FenceWindow::set_frameless`), so every title-relative layout, hit test and paint
  path follows without special cases. `WM_NCHITTEST` returns `HTCAPTION` wherever the
  widget's hit tree has no control.
- **Not done:** there is no visible hover affordance for dragging. Dragging, menus,
  merge-by-drop and screen-reader names have not been checked on a Windows desktop.

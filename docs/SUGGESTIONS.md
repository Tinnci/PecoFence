# Organizing suggestions and `pecofence-cli`

PecoFence suggests how to organize fences, based only on what the saved workspace already
records: where you dragged items by hand, how often and how recently you opened them, file
types, and modification times. Nothing is learned remotely and nothing is applied without a
click. The engine is `pecofence_core::suggest`. Fence windows and the CLI order items with the
same function, `pecofence_core::order::order_items`, so a preview matches the fence exactly.

## Where suggestions appear

- **Fence menu → 建议 (Suggestions).** Up to four entries that change or fill that fence. One
  click applies the entry, through the same operation as the menu or drag it replaces. The
  inbox menu also offers the cleanup fence.
- **`pecofence-cli suggest`.** The full ranked list, each entry with its reason, for a person
  or an agent to review. The CLI is read-only.

## Use cases and rules

Each rule has a threshold, so a sparse or mixed desktop gets no suggestion rather than a guess.

| You do this | Suggestion | Fires when | Never fires when |
| --- | --- | --- | --- |
| Keep dragging `.pdf` files into "Work" | *Always put .pdf files in "Work"*: a rule ahead of the broader ones, applied immediately | ≥ 3 manual placements of the extension, and ≥ 80 % of them in that fence (placements left in the inbox count against it) | An existing rule already files that extension there; shortcut types (`.lnk`, `.url`), which vary too much to file by extension |
| Download a photo onto the desktop and it lands in the inbox | *Move "4" to "Photos"* | A fence where ≥ 50 % of items share the extension (≥ 2), or ≥ 60 % share the kind (≥ 3) | Two fences fit within 0.1 of each other (Games and Tools both full of shortcuts); the item is covered by a rule suggestion; Office lock files, partial downloads, Recycle Bin and other shell items |
| Open three of thirty apps all day | *Sort: By frequent use* | ≥ 6 items, ≥ 10 opens, one opened in the last 30 days, and the top fifth of items take ≥ 50 % of the weighted opens | The fence is sorted by type, date, size or frequency already (your choice), or arranged by hand |
| Keep a mixed fence of documents, images and music | *Sort: By type* | ≥ 3 kinds with ≥ 2 items each, and no kind above 70 % | Same as above |
| Work on a fence of documents that change weekly | *Sort: By date modified* | ≥ 60 % files (documents, images, music, video, archives), and ≥ 2 changed in the last 7 days | Same as above |
| Let installers and archives pile up | *Gather idle installers and archives* (the 待清理 template) | ≥ 3 installers or archives not opened or modified for 30 days, and no cleanup fence yet | Items you placed by hand. Nothing is ever deleted |

Confidence is a 0–1 number used only for ranking. Ties are ordered by suggestion id, so the
output is deterministic.

## Sorting changes

- **By type** groups by kind, not by how the extension is spelled. The groups are folders,
  programs and `.lnk` shortcuts, web shortcuts, documents, images, music, video, archives,
  installers, then everything else. Items are then ordered by extension and natural name.
  `setup.exe` is grouped with the installers, not the programs, and `Report 9` comes before
  `Report 10`.
- **By frequent use** (renamed from "By open count") weights each item's opens by how recent
  the last one was, halving every 30 days. An app opened 20 times this week outranks one
  opened 200 times four months ago. Opens recorded before `lastOpened` existed count at
  one-eighth. Ties fall back to the raw count, then the name. The stored mode is still
  `openCount`, so existing workspaces keep working and the schema is unchanged.

## `pecofence-cli`

A separate console program, shipped next to `pecofence.exe`. `pecofence.exe` is a GUI
subsystem binary and cannot print to a terminal. The CLI depends only on `pecofence-core`.

```sh
pecofence-cli fences
pecofence-cli suggest --json
pecofence-cli suggest --fence Desktop --kind move
pecofence-cli order --fence Apps --by frequent --limit 10
```

| Command | Purpose | Default bound |
| --- | --- | --- |
| `fences` | Id prefix, kind (`inbox`, `collection`, `portal`, `widget`), item count and sort of each fence | All (≤ 64 per layout) |
| `suggest` | Ranked suggestions with `summary`, `reason`, `affects`, `examples` and a typed `action`; filter by `--fence` and repeated `--kind sort\|rule\|move\|cleanup` | `--limit 10` |
| `order` | The order a fence shows, or would show with `--by MODE`, with each item's sort key (opens and score, modified time, bytes, type group) | `--limit 50` |

**Workspace.** The CLI searches in this order:
1. `--workspace PATH` (the file, or its folder).
2. `$PECOFENCE_WORKSPACE`.
3. A portable `config\workspace.v2.json` next to the program.
4. `%APPDATA%\PecoFence`.

`meta.workspaceSource` reports which one was used. Every result reflects the last save
(`meta.savedAt`), and PecoFence saves shortly after each change. Multi-monitor layouts are
selected with `--layout N`, and `fences` reports how many layouts exist.

**Fences** are matched by an id prefix (at least 4 characters) or an exact, case-insensitive
title. Two fences with the same title produce `FENCE_AMBIGUOUS`, which lists both ids.

**Output.** With `--json`, the CLI writes one envelope, in the same form as
`scripts/package_desktop.py`:
- Success, on stdout: `{"schema_version":"1.0","ok":true,"data":…,"meta":{…,"readOnly":true,"total":…,"truncated":…}}`.
- Failure, on stderr: `{"ok":false,"error":{"code","message","retryable":false,"suggested_action"}}`.

Exit codes are 0 on success (an empty result included, which comes with an explanation in
human output), 1 when the workspace is missing or unreadable, and 2 for invalid arguments or an
unknown or ambiguous fence. Error codes are `INVALID_ARGUMENT`, `FENCE_NOT_FOUND`,
`FENCE_AMBIGUOUS`, `NOT_A_FILE_FENCE`, `WORKSPACE_UNAVAILABLE` and `WORKSPACE_INVALID`.

**Edge cases covered by tests** (`crates/cli/src/tests.rs`, `crates/core/src/suggest.rs`):
- An empty workspace, or a missing layout.
- A missing, corrupt or wrong-schema workspace. The error points to `--workspace` and to
  `workspace.v2.backups`.
- A folder portal or widget passed to `order`. A portal's items are listed live and are not
  saved, so `order` rejects it.
- `--limit` out of range, `--kind` or `--by` misspelt, and options given to the wrong command.
- A preview leaves the saved fence untouched.

**Why read-only.** PecoFence owns the workspace while it runs and saves it transactionally.
A second writer would race that save. Applying a suggestion from the command line would need a
local request channel into the running app. That is a separate decision about which local
processes may change the desktop, and it is not part of this tool.

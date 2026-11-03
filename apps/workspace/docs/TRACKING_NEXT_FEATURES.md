# Next tracking features

Status: design for implementation. Date: 2026-11-03.
Scope: SPM CLI, its local store, and the Tauri Workspace. This document describes
proposed behavior; it does not mark the features as shipped.

## Outcome and order

An SPM should be able to identify stalled ownership, reopen daily views, inspect
an item's evidence, share local knowledge, and judge a version's readiness.

| Order | Deliverable | Acceptance outcome | Dependency |
| --- | --- | --- | --- |
| 1 | Owner workload | One click from Overview identifies owners with stalled open software work | Existing work-item read model |
| 2 | Saved views | Three daily views survive restart and open in one click | Versioned local view storage |
| 3 | Activity evidence | Every event identifies its source and time precision | Connector history capability check |
| 4 | Annotation sharing | Preview a file, resolve conflicts, and import atomically | Stable item identity and import preview |
| 5 | Version readiness | Show required items, source inclusion, validation, and remaining blockers separately | Explicit release scope and evidence links |

Implement one vertical slice at a time: domain rules, store migration, CLI/native
contract, UI, and verification. Avoid one large schema migration spanning all five.
The summary-cache and schema-v3 work already present in the SPM working tree is
owned separately; this design does not change or prescribe its migration number.

## Existing foundation

- `src/WorkItems.tsx` has stages, attention and origin filters, activity/holding-time
  sorting, software classifications, local edits, and the activity dialog.
- `src/model-navigation.ts` stores preferences in TanStack Query memory. Stage,
  attention and origin survive tab switches; search and sorting have component state.
- `spm focus history` returns local edits and observed status transitions.
- `spm focus local-export` exports local layers; `local-import` currently replaces
  each imported item's layer. A field-level merge preview is new work.
- Shared branches can have several explicitly associated Jenkins tasks. A source
  switch is tracked per task, with optional manual historical build ranges.
- `docs/DELIVERY_PLANNING_CONTRACT.md` at the repository root defines a separate
  native delivery-plan protocol. Keep its existing negotiated revisions intact.

## Shared rules

Use the existing software scope and classification evidence. Unclassified records
stay visible in classification counts and never silently become software work.
Overview gate measures retain their configured full-scope basis; workload cards
explicitly use the software work-item scope.

Compute views from an immutable report revision and its local-layer revision.
Refreshing an item patches affected views without resetting the user's filters.
Show each source's last successful read and stale/error state. A failed source
retains its last good records, which remain visibly stale in aggregates.

An item reference must include `source_instance`, `source`, and native `item_id`.
`source_instance` is a locally configured stable server/space identifier, not a
credential or a translated display name. Current annotations use source plus ID;
legacy identities need explicit instance resolution before cross-instance imports.
Never combine two people or items solely because their display labels match.

Keep Jira, Meegle and Jenkins read-only. Views, plans, notes, imports and approvals
below are local records. Browser links open canonical source pages. Unknown dates,
missing owner identity, and unverified build inclusion stay explicit.

## 1. Owner workload

Add a compact **Owners** table to Overview, opening the existing issues or
requirements tab with the selected owner and current scope. Add **Group by owner**
to those tabs; the default list remains available.

| Column | Definition |
| --- | --- |
| Owner | Source-native account ID and display name; unassigned is its own row |
| Open | Distinct items whose effective stage is neither done nor cancelled |
| Blocked | Open items at the effective blocked stage |
| No activity for 14 days | Open items whose last meaningful activity is at least 14 days old |
| Status age | Oldest known current-status duration among open items |
| Next check due | Open items with a local next-check date on/before today's China date |

Use local corrections for effective owner/stage, with a correction indicator and
access to the original remote value. Unknown activity/status dates have a separate
unknown count. An `updated` fallback is marked as such and cannot establish the
absence of human activity. Group aliases only after explicit local identity mapping.

Default ordering is blocked count, stale count, then open count, descending; ties
use stable owner identity. These are item counts, not estimated hours or capacity.
An item appears once per selected kind/scope even if multiple source filters return it.
Clicking an owner preserves other filters; **Clear owner** removes only that filter.
A changed owner moves the item and updates both totals in one cache transaction.

## 2. Saved views

Put a Base UI Select beside the existing filters, with **Save view**, **Rename**,
**Duplicate**, and **Delete** in the existing menu system. A modified saved view
shows an unsaved-change indicator. Switching views does not overwrite the saved one.

Proposed record, persisted through SPM rather than browser-only storage:

```json
{
  "schema_version": 1,
  "id": "view-uuid",
  "model": "E6751SM",
  "kind": "issues",
  "name": "Blocked software work",
  "query": {
    "stage": "open",
    "attention": "blocked",
    "origin": "all",
    "owner_ids": [],
    "tags": [],
    "search": "",
    "sort": "held",
    "group_by": "owner"
  }
}
```

Store stable enums and identities, never translated labels or item snapshots.
Proposed CLI operations are `views list/save/remove`; native actions call literal
arguments with JSON stdin. Define their DTOs before wiring routes; these commands
do not exist yet. Apply a single validated query object to avoid intermediate reads.

Start with suggested presets: Blocked, No activity for 14 days, and Next check due.
Creating a named copy makes a preset editable. Bound names to 80 characters, search
to 200, and views to 50 per model/kind. Existing unnamed preferences migrate once
as **Last used**, without creating duplicate presets. Unsupported enum values
produce a recoverable validation message and leave the current view usable.

## 3. Activity with evidence

Keep one chronological timeline containing source history, local observations,
comments and local edits. Event contracts need: stable event ID, item reference,
kind, actor when known, event time, observation time, precision, provenance, and
optional before/after values. Sort by event time; fall back to observation time
with a visible **Observed** label. Date-only events stay date-only.

A locally observed transition gives the observation time, not the upstream change
time. Never turn the first observation into a fabricated status-change event.
Use source event IDs for deduplication; fallback fingerprints include provenance,
item, event kind, time, actor and values. Merge equivalent observation/source events
only when their relationship is established, retaining both provenance records.

Verify Meegle's supported status-history fields and their meaning before adding a
connector call. If unavailable, show **History observed on this device**. Surface
history read failures inside the dialog without disabling local annotation edits.
Render remote comments as plain text. Local import events identify their batch.

## 4. Sharing local annotations

Add **Export local annotations** and **Import annotations** under model settings.
The native file picker selects the file. Default export is the current model's
referenced local layers; all-model export is an explicit scope choice.

A new versioned package carries source-instance identities, export time, selected
models, and local tags, corrections, notes and next-check dates. Exclude raw source
rows, comments fetched from servers, schemas, authentication data, and executable
paths. Export local knowledge as data; imported links must pass URL validation.

Import flow: select file -> validate identity/version/size -> preview added,
unchanged, conflicting and rejected items -> choose conflict resolutions -> apply.
Default choices preserve existing local values. Offer keep local, take incoming,
and an explicit per-field merge. Tags can union; notes deduplicate by stable note
ID. Corrections and next-check dates require a choice when both differ. A missing
field in an old package is not a request to clear it; clearing requires an explicit
operation. Unknown instances require mapping before their items can be applied.

Preview returns a digest and current local revision. Apply rechecks both; concurrent
edits require another preview. Commit selected changes and the import audit event
in one SQLite transaction. Invalid JSON/version or an unresolved identity blocks
apply; individually rejected entries remain visible and are excluded explicitly.
Export the affected pre-import layers for local rollback. Reimporting an identical
package must not duplicate notes, edits, or audit events for unchanged records.

## 5. Version readiness and local commitments

Add a **Plan** tab after Requirements. The header selects lifecycle (development,
upgrade or OTA maintenance), release line, build target(s), and target date. Show a
milestone list and a required-items table first; a timeline is an optional second view.

A local plan stores: ID/revision, model, release-line ID, lifecycle, target date,
milestones, and item commitments. Each commitment records item identity, must/should
priority, owner, planned milestone/date, dependencies, inclusion evidence per build
target, validation evidence, and optional waiver with author/date/reason.

Shared SYS/VND branches attach to several targets without duplicating commitments.
An item required on A and B tracks inclusion and validation independently for each.
Full job paths identify tasks within a configured Jenkins instance. Human-readable
version names stay separate from job names, build numbers and manifest revisions.

| Readiness facet | Evidence |
| --- | --- |
| Work resolved | Source stage/disposition, with any local correction shown |
| Code merged | Explicitly linked Gerrit changes and their status |
| Included | Pinned repository revisions/manifest comparison, or a labelled manual record |
| Validated | Local test result reference, outcome, target and date |
| Accepted exception | Explicit local waiver with reason and reviewer |

Show facets side by side. A successful build alone does not fill inclusion or
validation. Abandoned changes remain history and do not satisfy merged evidence.
A missing Change-Id link is a missing evidence entry. Source restoration/reopening
can reduce readiness and must be visible as a change since the last review.

Progress uses a stable denominator: required commitments in the selected plan
revision, not the current result of a changing issue filter. Show counts for resolved,
included, validated, waived, blocked and unknown separately; those categories may
overlap. Adding/removing requirements produces a scope-change event and a revised
baseline. Marking an item optional does not delete its history.

Forecasts require entered duration estimates, owner capacity and dependencies.
Without them show target dates, overdue work, unresolved dependencies and unknown
finish dates. With them show a conditional finish date and the assumptions used;
never infer completion percentages from elapsed calendar time. A lifecycle changes
default milestone templates, not evidence requirements or past source mappings.

## UI implementation choices

Reuse Base UI Select, Tabs, Dialog, Collapsible, Checkbox and Tooltip through the
existing local wrappers. Use ToggleGroup for compact single-choice chips, semantic
tables for owner/readiness comparisons, Lucide icons with text when width permits,
and the shared context menu for item, source and build links. Reuse TanStack Query
for reports; durable user data is stored through SPM. Keep English and Chinese
catalogs aligned. At narrow widths, owner and plan tables become readable rows;
full IDs and evidence remain available by keyboard, not hover alone.

## Validation and delivery gates

| Slice | Required checks |
| --- | --- |
| Owner workload | Unassigned/duplicate labels; corrected owner/stage; unknown dates; stale sources; no double counting; owner changes during refresh |
| Saved views | Restart persistence; model/kind isolation; migration idempotence; invalid enums; deleted owners; atomic filter changes; keyboard navigation |
| Activity | Equal timestamps; source/observed duplicates; date-only times; unavailable history; failed history reads; long comments; local patch after edit |
| Sharing | Legacy instance mapping; mixed valid/invalid entries; conflicting clears; stale preview; atomic rollback; repeated import; file-size limits |
| Readiness | Shared A/B source with different builds; missing manifests; abandoned/restored changes; cancelled/reopened items; scope revision; missing estimates |

Use synthetic records for automated tests. Then verify the built Windows app with
explicitly scoped real reads, English/Chinese, 760px width and keyboard navigation.
Keep test edits local and remove temporary annotations. Validate affected CLI/native
contracts before installing. Commit and push each complete slice to main, preserving
unrelated work. No remote task execution is needed for any acceptance check.

## First implementation slice

Start with owner identity and deterministic workload aggregation, then connect the
Overview Owners table to a single validated work-view query. Persist named views in
the next slice using that query contract. Activity provenance and import preview
follow; release readiness reuses their stable identities and audit records.

Before coding the first slice, settle two input contracts in code: source-native owner
IDs (display names remain presentation) and source-instance identity for existing
local layers. Do not silently merge records while migrating those identities.

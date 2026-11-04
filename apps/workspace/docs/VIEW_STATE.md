# Work-list view state

Issues and requirements keep independent view records for each model under
`tinnci.workviews.v2.<model>.<kind>` in this device's WebView storage. Each version-2
record contains the current settings, named views with stable UUIDs, and the last
selected view ID. Settings include search, stage, attention, origin, field filters,
grouping and ordered sort keys. Query remains the UI subscription layer; stored
preferences are separate from fetched source data.

Named views support create, update, rename, duplicate and delete. Editing the list
leaves the selected saved snapshot unchanged and marks it modified. Applying a view
writes one validated current snapshot before publishing its fields to the UI.
Search is capped at 200 characters, view names at 80, and named views at 50 per list.
Duplicate names are rejected by the menu. Applying a view resets temporary expansion
and pagination state, which are not persisted.

The first read imports legacy `tinnci.views.v1` lists into version 2 and assigns
stable IDs. The original legacy record is retained. Invalid fields fall back to
supported defaults; filter values are canonicalized for reliable equality checks.
Unsupported versions, oversized records and malformed JSON are preserved on disk,
with session-only changes and a visible storage message. Storage write failures
also retain session state and display the message.

These records remain local to the installed desktop profile. SPM-owned shared
views and annotation identity migration are separate follow-up work.

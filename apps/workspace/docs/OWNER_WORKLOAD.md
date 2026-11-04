# Owner follow-up

The model overview adds owner counts independently for issues and requirements.
Each row has open, blocked, inactive for 14 days and next-check-due counts. Only
non-done/non-cancelled items count. Rows sort by blocked, inactive, due and open
counts, then stable bucket key. Eight rows appear initially; the rest expand.

Buckets use source instance/space plus the currently available owner display name.
Unassigned work has its own bucket. Same-name owners in different spaces remain
separate, with the source address shown when needed. Native owner account IDs are
not currently present in the work-item DTO and remain future identity work.

Overlapping queries deduplicate by source/address, choosing the latest item
revision, before both list rendering and aggregation. Scope, owner and metric
filters carry the count through to the destination list. Overview shortcuts start
from a clean current view so persisted searches or old filters cannot hide counted
items. Named saved views remain unchanged and can show their modified indicator.
The source-space filter is visible, removable and supported by persisted views.

Staleness requires a known meaningful activity time; generic updated timestamps
remain available for sorting but do not establish inactivity. Missing activity
counts are shown separately. Due dates use the China calendar day, including
checks whose timestamps fall on that day. Statistics use the same rules as list
attention filters. The clock updates every minute without fetching sources.

The UI reads existing work-item snapshots and never fetches flow histories to
compute counts. Sources with stale/error snapshots retain their counts alongside
a cached-data note. No remote records or owner assignments are changed.

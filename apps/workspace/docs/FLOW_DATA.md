# Flow data ownership

| Component | Responsibility |
| --- | --- |
| WorkItems | Filters, pagination, grouping, row expansion; triggers list refresh and export preparation. |
| Flow | Renders history and readable comments, owns local expand/collapse state. Memoizes shaped history by data and language. |
| flow-cache | Query identity, freshness, shared request queue, per-item errors, prefetch, explicit refresh, export barrier. |
| TanStack Query | Memory cache, in-flight deduplication, subscriptions, background fetch state and retention. |
| Tauri bridge | Validates arguments, invokes the CLI, enforces process deadlines and decodes the typed response. |
| SPM CLI | Resolves source addresses, owns persistent cache and calls the source connector. |

Previously, prefetch, row expansion and export loaded independently. Presence-only
cache checks ignored freshness, one-item batch responses were missed, errors were
swallowed, and memory freshness restarted on every disk-cache hit. Refreshing the
list left history caches untouched.

All history consumers now use the same per-item query. Query identity includes
profile, model, source scope, item ID and item update timestamp. A shared queue
combines pending requests by model and refresh policy, sends at most ten IDs per
call, and runs one batch at a time. Single and multi-item responses share the same
handling. Successful rows are cached individually; missing rows become errors.
Export waits for all requests to settle and stops if any requested history failed.

SPM keeps a bounded key per source space/type/item, with a schema version and the
item update timestamp inside the value. Changed items invalidate the stored value
without accumulating timestamped disk keys. Open histories expire after five
minutes, closed histories after 24 hours. The original fetched_at reaches the UI,
so reading from disk never restarts the memory TTL. Older CLI responses without
fetched_at remain compatible and use the normal memory TTL.

Opening a row immediately displays existing memory data while an expired query
refreshes. A changed item gets a new query identity. Hover and initial top-row
prefetch share that query and queue with export. Only Meegle rows fetch histories.
Explicit history refresh bypasses the SPM history cache. List refresh invalidates
all model histories and force-refreshes active/open ones; other histories reload
on demand. A failed refresh retains existing memory data and exposes the query
error in the row.

Persistent history remains owned by SPM, rather than adding a second browser disk
cache. This keeps process restarts useful without duplicating storage migrations
and invalidation policies across SQLite and browser storage.

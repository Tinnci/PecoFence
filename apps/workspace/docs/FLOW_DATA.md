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
profile, model, source, source scope, exact source address, item ID and item update timestamp. A shared queue
combines pending requests by model and refresh policy, sends at most ten IDs and
matching source addresses per call, and runs one batch at a time. Interactive
reads take priority over queued export and prefetch requests. Single and multi-item responses share the same
handling. Successful rows are cached individually; missing rows become errors.
Export waits for all requests to settle and stops if any requested history failed.

SPM keeps a bounded key per exact source address, with a schema version and the
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

## P0 refresh and cancellation contract

- Query metadata carries the typed resource identity. Refresh never extracts IDs
  from positional query-key elements. The CLI resolves each supplied address only
  against the model's already-read items. Legacy ID-only calls reject ambiguity.
- The native bridge gives history reads their own semaphore, so summary refresh
  cannot block interaction for several minutes. Each batch still caps reads at ten.
- Forced refresh cancels the previous query subscription before scheduling a new
  source read. Concurrent forced refreshes share one promise. Successful responses
  from cancelled subscriptions cannot overwrite the newly fetched memory result.
- Prefetch returns an idempotent cleanup. Hover exit, blur and list unmount release
  their ownership. Only unobserved, unpromoted speculative work is cancelled once
  all prefetch owners have left. Pending reads are removed before IPC; an already
  dispatched CLI call finishes and may warm the disk cache while its cancelled
  frontend subscription ignores the response.
- Partial batches return structured per-item errors with source addresses. A failed
  job keeps its slot in the response ordering, including worker failures. Successful
  siblings remain usable and expanded export reports the failing resource.
- Changing the item revision can show the previous revision while the same source
  address/scope refreshes. Source changes never inherit another source's placeholder.

Local annotation identities and persistent saved views are separate follow-up
work; this change scopes the identity upgrade to flow resources without migrating
existing annotation data.

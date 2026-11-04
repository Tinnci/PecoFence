# Current source fields

Opening a Meegle work-item detail mounts a separate Source fields section. The
section reads current values through `spm focus source-fields`, not operation
history. List loading does not fetch detail payloads.

The CLI first resolves the exact address and native ID against an already-read
item in the followed model. Its space/type select the source's field metadata.
Configured fields matching the requested business labels are then read with one
`workitem get --fields [...]` call. Field IDs are never hardcoded. The configured
set covers issue name/number, project, hardware, Android version, priority,
component/module, source tags, affected version, reproduction probability and
time, source due date, resolution, cause, fix, test advice, self-test, branch
self-check, parent issue, TR flags, origin, reporting and description.

Values preserve full text. Option IDs become source labels, named related values
retain their names/links, and typed date values retain the source precision in
China time. Returned null/empty values differ from fields absent from the payload.
Empty/unavailable fields can be expanded; populated fields appear by default.
The native DTO contains only field key, label, kind, readable value and availability.

Query identity includes profile/model, source address, scope, native ID and item
revision. Memory and SPM disk caches expire after five minutes using the original
fetched_at. Manual refresh cancels the older frontend subscription and bypasses
the detail cache; schema refresh also updates the shared metadata cache. Concurrent
refreshes deduplicate. List refresh invalidates details and updates active panels.
Errors retain last-good data. A dedicated native gate allows at most two detail
reads alongside the separate history and list channels.

Source fields are separate from local annotations: source tags are not local tags,
and source due dates are not local next-check dates. Source values remain read-only.
Flow field-change values also retain full text; the flow cache schema is advanced
to avoid reusing the older 200-character projection.

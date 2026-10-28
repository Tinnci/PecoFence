# Delivery planning read model

The public contract adds a generic, bounded `DeliveryPlanView` and a read-only
`GetDeliveryPlan` request. Solver algorithms, customer rules, source credentials,
private fixtures and operational data stay in the private SPM backend.

Wire protocol major remains 2; minor 1 introduces the method. The client sends
only existing Hello feature names and uses the negotiated minor for subsequent
requests. Minor-0 servers remain usable for existing methods; planning requests
are rejected locally before sending to such a server. Backends must use the
negotiated minor on every response/event and filter GetCapabilities methods using
`RpcMethod::minimum_minor()`. A minor-0 peer must never receive the new method
name in capabilities or a new delivery-plan payload. The legacy minor-0 Hello
fixture stays unchanged. Crate version 0.1.0 remains separate from wire version.

Requests select project, delivery scope and plan ID explicitly. Task pages are
bounded to 200 entries; a continuation must pin the returned plan revision.
Project-snapshot revisions and plan revisions are independent identifiers.
Summary milestones, gate readiness, WIP and buffers describe the full selected
plan; tasks and applicable resource waits describe the selected page. DTOs carry
backend-computed times and reasons, not formulas to be evaluated in the panel.
A future gate release remains conditional forecast, never approval.

The panel must validate responses and reject mixed scope/revision pages. Unknown
finish dates stay absent. Do not infer successful tests, external acceptance or
approval from reaching a planned date, completing a build or having a sent copy.

Synthetic contract tests cover bounded pages, pinned continuation, malformed
actuals, unknown fields and envelope validation. Public protocol tests and
transport tests must pass before publishing this revision; backend integration,
negotiated-minor regression tests and native panel validation are separate work.

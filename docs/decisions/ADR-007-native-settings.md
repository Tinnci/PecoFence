# ADR-007: Native Settings direction and packaging boundary

Status: accepted direction following the maintainer's decision to replace WebView
UI. Native Settings is **not implemented** and WebView2 has **not been removed**.
The functional scope and removal acceptance criteria are defined in
[USER_REQUIREMENTS.md](../USER_REQUIREMENTS.md).

## Observed scope

WebView2 hosts `ui/settings.html`. Desktop fences, menus and the SPM panel already
use native Windows/Direct2D rendering. The product website has been removed
separately. Removing the desktop browser therefore starts with the Settings host,
not a rewrite of every panel.

Current `windows-webview` linkage imports the loader at process startup. Deleting
HTML or omitting the DLL would break startup, not produce a native build.

## Chosen approach

Use Win32 standard/common controls for interactive settings forms/accessibility, with
the existing native rendering/material stack where needed. WinUI 3 is plausible
for richer controls, but introduces Windows App SDK deployment/runtime decisions;
it is not automatically the smaller/no-runtime option.

Keep application ownership: workspace/revisions, typed `SettingsCommand`,
validation, conflict handling and asynchronous save/exit belong to Rust use cases.
Native controls should not mutate `Config` directly. Extract a typed read model
from the current JSON projection once a real native caller exists; do not add an
unused host trait or a second mutable Settings copy now.

## First executable slice and removal gates

1. Native General settings: language/theme/autostart/visibility and truthful
   pending/committed/error state, using the existing typed use cases.
2. Replace content/container editing, multi-condition rules, snapshots/import/
   recovery and closing choices. Verify stale ownership/revision handling.
3. Verify keyboard/focus, UI Automation/screen reader, high contrast, all languages,
   multiple DPI/text scaling, responsive layout and real Windows effects.
4. Remove WebView host/HTML/client/browser tests and `windows-webview` dependency
   only after native workflow coverage replaces them. Keep domain/protocol tests
   where they verify real application semantics; remove web-only contracts.
5. Confirm the executable has no static/delay or dynamic WebView dependency and
   no browser child processes. The common package pipeline must then omit the
   loader and report no WebView runtime requirement.

No production switch, parallel ownership model or placeholder native panel is
added by the CI/packaging work. Current web checks remain mandatory while that
host exists. Compare cold start, private memory, shipped dependencies, executable
size and accessibility—not just “native” branding—before completing replacement.

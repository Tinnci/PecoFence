# ADR-007: Native Settings direction and packaging boundary

Status: accepted. The five Reactor-hosted native WinUI 3 task pages and thirteen
rule-condition forms are implemented. Reported headless tests and a synthetic
WinUI tour including close/reopen pass; this is not a claim of full feature parity,
real-Windows accessibility or release acceptance. Scope and remaining acceptance
are tracked in [USER_REQUIREMENTS.md](../USER_REQUIREMENTS.md) and
[VERIFICATION_GATES.md](../VERIFICATION_GATES.md).

## Observed scope

Before this replacement, WebView2 hosted `ui/settings.html`; fences, menus and the
SPM panel already used native Windows/Direct2D rendering. The desktop browser was
limited to Settings, so the selected change replaces that host rather than
rewriting the other panels. The former host's loader linkage is historical context,
not a current Settings runtime requirement.

## Chosen approach

Use Reactor with native WinUI 3 controls for Settings. No browser-hosted UI is
selected. WinUI 3 brings Windows App SDK deployment and resource costs; choosing
native controls does not imply a smaller package or lower startup cost.

Use the native `TitleBar` and adaptive `NavigationView` for a Windows Settings-style
layout. One title-bar hamburger expands/collapses the left pane; compact and
minimal widths use native overlay navigation. WinUI owns caption buttons, drag
regions and navigation semantics, rather than custom hit testing or a fixed
sidebar. Changing navigation must preserve editor drafts.

Keep application ownership: workspace/revisions, typed `SettingsView`, `Request`,
`Receipt` and `DocumentStamp`, validation, conflict handling and asynchronous
save/exit belong to the existing Rust application boundary. Settings controls own
input drafts and presentation only; they do not mutate `Config` directly or retain
a second mutable Settings copy.

One main STA `Reactor::run_with` owns the existing native `App`, desktop and tray
lifetimes. Creating, closing or reopening a Settings component window must not quit
the application or tray. Do not run a separate application lifetime/event loop per
Settings window. The former browser `Ready`/JSON bridge and client scripts are not
part of this direction. Do not add a generic service framework or new persistence
framework for this UI replacement.

The implementation requires a public, exact Microsoft `windows-rs` Git pin at
`2672e615d9cc0771448a781f3b2fe34e7fd08c6a`: published version 0.100 does not expose
the public lifetime API needed to keep a zero-window tray application alive.
Reactor itself is vendored from that same revision, with its MIT and Apache-2.0
licenses and a narrow native-dialog teardown fix. Its upstream destructor called
`Hide()` while borrowing the dialog scheduler; synchronous `Closed` reentrancy
panicked when closing a window or exiting with a modal open. Hiding only during
adapter destruction also accesses an already closed XAML root. The patch prepares
a terminal root-scoped shutdown barrier, hides outside scheduler borrows while
the root is valid, then waits for `Closed` before native window close or application
exit. Late declarations cannot reopen dialogs on a shutting-down root. Programmatic
close, title-bar `WM_CLOSE` and application exit share this rule; close/exit requests
can therefore complete asynchronously. The owned weak-capture HWND subclass uses
Windows Common Controls v6, declared by the application's separate build manifest.
Remaining SDK packages stay on the exact public Git revision, not rolling
`master`, a private backend or an unreviewed source. Keep the local exception
explicit and remove it when a verified upstream fix supports these lifecycle cases.

The selected distribution is self-contained with Windows App SDK Runtime 2.5.1
and excludes WebView2 payload. A correctly built self-contained package does not
require users to install Windows App SDK Runtime separately. The Windows SDK is a
development/build toolchain and is distinct from the packaged Windows App SDK
runtime. Do not add an unconditional `windows-reactor-setup` WebView2 helper.
The App SDK runtime increases deployment and resource cost. Full Windows CI enforces
the agreed 6.5 MiB (6,815,744-byte) executable cap. Final release size and any
startup/resource measurements remain pending; do not claim improvements from the
native implementation or the EXE cap.

## Native source and remaining acceptance

1. The five task pages are implemented: General and appearance; window and content;
   organizing rules; layout, backup and recovery; and About and diagnostics. Rule
   editing includes thirteen condition forms. N01–N08 remain the required feature-
   parity baseline; implementation alone is not behavioral acceptance.
2. The pages use the existing typed application/session and persistence use cases.
   Reported headless tests and a synthetic WinUI tour including close/reopen pass.
   Continue checking N01–N08 workflows; keep domain and protocol test claims scoped
   to the semantics they actually exercise.
3. Complete real-Windows keyboard/focus, UI Automation and Narrator checks; high
   contrast; all ten languages and long strings; 100%/150%/200% DPI and text
   scaling; and OS integration behavior. These are release acceptance checks, not
   gates that must be manually repeated on every commit.
4. Inspect PE imports, dynamic loading paths, full process tree and the final
   packaged payload, then start and use every Settings task on a clean supported
   Windows machine. Verify the exact self-contained Windows App SDK Runtime 2.5.1
   inventory and absence of WebView2 runtime, loader, helper and UI assets. A
   sampled local tour loaded 19 app-local SDK DLLs and observed no browser module
   or child process; this does not establish absence on every path. Final package
   and release evidence remain pending.

There is no planned production WebView fallback or parallel Settings ownership
model. Core persistence, conflict, recovery and exit use cases remain
application-owned. Record actual startup, idle-resource, shipped-dependency and
executable-size measurements before making performance/size claims. The selected
6.5 MiB CI ceiling does not establish a performance claim. Manual desktop
acceptance remains required before release, as defined in
[VERIFICATION_GATES.md](../VERIFICATION_GATES.md).

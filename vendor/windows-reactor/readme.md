## windows-reactor

Windows Reactor is a typed declarative UI library for building native WinUI 3 applications in
Rust. Components own Rust state, describe the current view, and receive typed messages from
controls.

Applications use the default features. The `test` feature exposes headless hosts, runtime
protocols, schema metadata, and diagnostics for framework tests and benchmarks; it is not needed
to build a UI. Applications use typed control builders, callbacks, and reference integration
methods rather than generic property/event records or imperative requests.

Components can declare `WindowVisuals::initial_position` or restore `initial_placement` before a
window first appears. `ViewContext::on_window_placement` reports restored outer bounds in physical
screen pixels and maximized state; applications own persistence. Initial placement is not replayed
on later renders.

* [Getting
  started](https://github.com/microsoft/windows-rs/blob/master/docs/crates/windows-reactor.md)

Start by adding the following to your Cargo.toml file:

```toml
[dependencies.windows-reactor]
version = "0.100"
```

```rust,no_run
use windows_reactor::*;

#[derive(Clone, Copy)]
enum Message {
    Increment,
    Reset,
}

struct Counter {
    count: i32,
}

impl Component for Counter {
    type Input = ();
    type Message = Message;

    fn create(_input: &(), _context: &ComponentContext<Self>) -> Self {
        Self { count: 0 }
    }

    fn update(&mut self, message: Message, _context: &ComponentContext<Self>) {
        match message {
            Message::Increment => self.count += 1,
            Message::Reset => self.count = 0,
        }
    }

    fn view(&self, _input: &(), context: &mut ViewContext<Self>) -> View {
        let content = StackPanel::new().spacing(8.0).children((
            format!("Count: {}", self.count),
            Button::new()
                .on_click(context.message(Message::Increment))
                .content("Increment"),
            Button::new()
                .on_click(context.message(Message::Reset))
                .content("Reset"),
        ));

        context.window_frame("Counter", content)
    }
}

fn main() {
    App::run_component::<Counter>(()).unwrap();
}
```

Ordinary unpackaged applications use an installed Windows App SDK framework package. For a
self-contained application, add `windows-reactor-setup` as a build dependency and call
`windows_reactor_setup::as_self_contained()` from `build.rs`.

Control icon slots accept the shared `Icon` content value. Reactor realizes it as the native
`IconElement` or `IconSource` required by each control, so the same value works with APIs such as
`AutoSuggestBox::query_icon`, `AppBarButton::icon`, and `TitleBar::icon`. Standalone visual icons
continue to use `SymbolIcon`, `FontIcon`, `BitmapIcon`, `ImageIcon`, and `PathIcon`.

## Vendoring provenance

This copy is the `crates/libs/reactor` package from Microsoft’s `windows-rs`
repository at commit
`2672e615d9cc0771448a781f3b2fe34e7fd08c6a` (crate tree
`0f27b6f95eb2177098543dd9bad6b6e84d9ad997`), imported from the local Cargo Git
cache at that exact revision.

PecoFence keeps this copy for a focused native-window shutdown correction.
Before programmatic `Window.Close`, title-bar `WM_CLOSE`, or
`Application.Exit`, it suppresses and hides active root-owned content dialogs
after releasing the scheduler borrow, then waits for the active dialog's
`Closed` generation before allowing the owning XAML root to close. The
root-scoped shutdown barrier and its tests are in
`src/native/winui.rs`. Its terminal root state rejects late reopen/new-dialog
declarations while native close is pending. Component-window creation and the asynchronous
`Closed` continuation are in `src/native/winui/hosting.rs`; the owned HWND
subclass used to intercept title-bar `WM_CLOSE` is in
`src/native/winui/window_placement.rs`; and live-window preparation before
application exit is in `src/native/app.rs`. The subclass uses a weak capture
of the window state and an RAII registration whose callback allocation stays
alive until `RemoveWindowSubclass`.

The upstream source, tests, public API snapshot, and package identity
(`windows-reactor` `0.100.0`) are retained. The inherited workspace
dependencies and lints are flattened in `Cargo.toml`; direct Windows SDK
dependencies still use the same pinned `windows-rs` commit as the rest of
PecoFence.

The upstream `license-mit` and `license-apache-2.0` files are retained. This
crate is excluded from workspace-member auto-enrollment so Cargo continues to
report it as a third-party dependency for license-notice generation.

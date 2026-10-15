# Application context menus

PecoFence renders its tray, fence, item, header, and creation menus with native
WinUI 3 `MenuFlyout`, `MenuFlyoutSubItem`, and `ToggleMenuFlyoutItem`. The controls
provide themed typography, spacing, rounded surfaces, keyboard navigation,
accessibility, and light dismissal. Checked and disabled states are preserved.
Shortcut labels use `KeyboardAcceleratorTextOverride`; they describe the app's
existing shortcuts rather than registering additional global accelerators.

The platform menu builder retains command IDs and a presentation-independent
tree. An application-scoped renderer projects that tree to Reactor. Its transient
XAML host supplies a valid `XamlRoot` and closes before the selected command runs.
The small modal bridge preserves existing command lifetimes and uses the app's
existing `AppCell` reentrancy barrier. Flyout callbacks only record selection and
completion; they never borrow the application. Tray menus open above their
anchor, while context menus open below it, subject to WinUI screen-edge placement.
The current application light/dark theme is applied on every opening.

Two native Windows protocol menus remain separate: **Windows file actions…**
delegates to the selected files' `IContextMenu` implementation, including shell
extensions; the OLE right-drag effect chooser retains its default-action semantics.
Those explicit handoffs use `HMENU`. Arbitrary shell extensions are not projected
into XAML controls. The Explorer MSIX extension is a separate shell integration,
described in [Explorer context menus](EXPLORER_CONTEXT_MENU.md).

## Verification

Build the app's test executable and stage the self-contained runtime:

```sh
scripts/build-windows.sh test pecofence pecofence-platform
```

On Windows, run `scripts/test-modern-menu.ps1 -TestBinary <app-test.exe>`.
It launches an isolated synthetic application with no desktop fences, hotkeys,
startup changes, or user workspace access. UI Automation checks native toggle and
disabled semantics, selects a command with Enter, invokes a nested command, and
cancels with Esc across repeated openings. It saves menu-only images for visual
inspection. The harness only sends keyboard input while the test process owns
the foreground. Test teardown terminates only that isolated test process if needed.

`scripts/generate-menu-bindings.py --check` verifies the small additional WinUI
projection against Cargo's pinned windows-rs metadata. Run without `--check` to
regenerate it after an intentional dependency change. The generated projection
uses the same `windows-core` ABI as the existing vendored Reactor bindings.

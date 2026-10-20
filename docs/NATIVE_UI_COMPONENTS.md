# Native WinUI components

The Settings window and application menus share the application's WinUI 3
lifetime. Use native controls and theme resources for ordinary forms and
feedback; desktop fences keep their existing composition and activation model.

| Component | PecoFence use |
| --- | --- |
| `NavigationView`, `TitleBar` | Adaptive navigation across the six Settings pages, native window controls and title-bar hamburger. |
| `ToggleSwitch` | Immediate general, window, portal and automatic-routing settings. Labelled themed cards preserve accessible names and read-only state. |
| `CheckBox` | Multi-select conditions and rule drafts that require an explicit Apply action. |
| `ComboBox`, `NumberBox`, `Slider` | Choices, opacity and tint strength; aligned field widths and native keyboard input. |
| `TextBox` | Settings names, colors and rule drafts. Retained controls preserve text and focus through background updates. |
| `InfoBar` | Save/load errors, application notices and review feedback. Persistent problems stay visible until resolved. |
| `ProgressBar`, `ProgressRing` | A nonblocking progress bar during submission/saving; a ring while the Settings workspace is unavailable. No invented percentage. |
| `ContentDialog` | Destructive or replacing operations. The title and primary button name the action; Cancel stays available. Existing session/token validation remains authoritative. |
| `MenuFlyout`, `MenuFlyoutSubItem`, `ToggleMenuFlyoutItem` | Application context menus; see [menu behavior](APPLICATION_MENUS.md). |
| `Mica`, `DesktopAcrylic`, Fluent icons | Settings backdrop, native navigation icons, and system light/dark resources. |

Settings headings, save state and notices remain above the scrolling page body.
Normal saved state is quiet text; errors use an InfoBar rather than color alone.
Card backgrounds and borders use `CardBackgroundFillColorDefaultBrush` and
`CardStrokeColorDefaultBrush`, resolved by WinUI on theme changes. Page and section
headings expose native accessibility heading levels.

Navigation display mode describes the available width, not the open state of the
pane. Adaptive defaults apply until the first hamburger interaction. After that,
an explicit close remains closed across resize events; native pane changes still
synchronize light dismissal. The title-bar callback has stable identity so a
background render cannot retire queued clicks.

Other useful native controls include `Expander` for optional advanced settings,
`AutoSuggestBox` for searchable content pickers, `ColorPicker` for custom colors,
and `CommandBarFlyout` for compact editing actions. Introduce them when the
interaction needs them, rather than adding more controls to every page.

The desktop's note/task text-entry popup currently hosts Win32 `EDIT`; it is not
a WinUI `TextBox`. Moving that surface to XAML requires preserving the existing
no-activation desktop behavior, blur-save, Esc cancellation and multiline
Ctrl+Enter contract. Likewise, the emergency save-failure dialog remains usable
outside a Settings XAML root. Do not remove these behaviors merely for styling.

## Settings layout

The helpers in [pages.rs](../crates/app/src/settings_host/pages.rs) keep pages consistent:

- **Sections.** `section()` starts a group with a level-2 heading. Group by task
  (desktop and startup, appearance, fence behavior, Peek), not by control type. Use
  `note()` for one explanatory line under a heading, and only when the behavior is not
  obvious from the labels.
- **Rows.** An immediate setting is a `row()`: the label on the left, the control on
  the right, in a themed card, like Windows Settings' `SettingsCard`. `switch()`,
  `field_combo()` and `slider_row()` build on it and give the control the label as its
  UI Automation name. Controls in rows share one width.
- **Forms stay forms.** Text entry, rule drafts and other edits that need an explicit
  Apply keep header-above fields and buttons; do not force them into rows.
- **Disable moot settings.** When one setting makes another meaningless, show the
  dependent one disabled, not hidden, so its value and place stay visible: hover
  expansion while click-to-expand is on, the Peek shortcut and dimming while Peek is
  off, tint strength without a tint color. The read-only state (`view.writable`)
  disables everything.

Microsoft's [guidelines for app settings](https://learn.microsoft.com/en-us/windows/apps/design/app-settings/guidelines-for-app-settings)
are the reference: apply simple changes immediately, group related settings under
clear headings, keep labels short and ship sensible defaults. They suggest radio
buttons for short exclusive lists; PecoFence uses combo boxes in rows to keep pages
compact.

## Verification

`settings_host::tests` covers all six pages, typed switch commands, read-only
state, error/progress feedback and retention of dirty inputs. Its opt-in native
tour exercises all pages, ten languages, three adaptive navigation widths,
light/dark windows, dialog cancellation and repeated reopening with the staged
self-contained runtime. Use a separate instance and configuration directory for
visual checks and real switch-persistence tests.

Run `scripts/test-settings-navigation.ps1 -Binary <pecofence.exe>` on Windows for
the pane regression. It uses a separate workspace, invokes the real native
hamburger, switches through the first five of the six pages, resizes through compact/minimal/expanded
widths, and checks the content gutter using DPI-adjusted UI Automation bounds.

Guidance: [toggle switches](https://learn.microsoft.com/en-us/windows/apps/design/controls/toggles),
[InfoBar](https://learn.microsoft.com/en-us/windows/apps/design/controls/infobar),
[progress controls](https://learn.microsoft.com/en-us/windows/apps/design/controls/progress-controls),
and [dialogs](https://learn.microsoft.com/en-us/windows/apps/design/controls/dialogs-and-flyouts/dialogs).

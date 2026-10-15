# Native WinUI components

The Settings window and application menus share the application's WinUI 3
lifetime. Use native controls and theme resources for ordinary forms and
feedback; desktop fences keep their existing composition and activation model.

| Component | PecoFence use |
| --- | --- |
| `NavigationView`, `TitleBar` | Adaptive navigation across the five Settings pages, native window controls and title-bar hamburger. |
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

Other useful native controls include `Expander` for optional advanced settings,
`AutoSuggestBox` for searchable content pickers, `ColorPicker` for custom colors,
and `CommandBarFlyout` for compact editing actions. Introduce them when the
interaction needs them, rather than adding more controls to every page.

The desktop's note/task text-entry popup currently hosts Win32 `EDIT`; it is not
a WinUI `TextBox`. Moving that surface to XAML requires preserving the existing
no-activation desktop behavior, blur-save, Esc cancellation and multiline
Ctrl+Enter contract. Likewise, the emergency save-failure dialog remains usable
outside a Settings XAML root. Do not remove these behaviors merely for styling.

## Verification

`settings_host::tests` covers all five pages, typed switch commands, read-only
state, error/progress feedback and retention of dirty inputs. Its opt-in native
tour exercises all pages, ten languages, three adaptive navigation widths,
light/dark windows, dialog cancellation and repeated reopening with the staged
self-contained runtime. Use a separate instance and configuration directory for
visual checks and real switch-persistence tests.

Guidance: [toggle switches](https://learn.microsoft.com/en-us/windows/apps/design/controls/toggles),
[InfoBar](https://learn.microsoft.com/en-us/windows/apps/design/controls/infobar),
[progress controls](https://learn.microsoft.com/en-us/windows/apps/design/controls/progress-controls),
and [dialogs](https://learn.microsoft.com/en-us/windows/apps/design/controls/dialogs-and-flyouts/dialogs).

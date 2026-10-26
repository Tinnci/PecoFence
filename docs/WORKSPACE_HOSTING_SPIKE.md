# Workspace hosting spike (2026-10-26)

Question: can the SPM workspace (a WinUI 3 window on the Reactor lifetime) host the
existing Direct2D board as a child HWND?

Method: debug-only code. `crates/app/src/workspace_spike.rs` opens a WinUI window
(title row 48, navigation 168, board column, detail 330 DIP).
`crates/app/src/app/spike.rs` re-parents an existing board `FenceWindow` into it
with `pecofence_platform::child_host`. `scripts/spike-workspace-host.ps1` drives
real mouse and keyboard input on Windows and writes `probes.jsonl` plus one
screenshot per step. Machine: one 1920×1080 monitor at 96 DPI, dark theme.

Run (Windows PowerShell; copy the script to a local folder first):

    powershell -NoProfile -File C:\temp\pf-spike\spike-workspace-host.ps1 `
      -Binary <pecofence.exe> -Manifest <third_party\winappsdk\runtime-manifest.json> `
      -Out C:\temp\pf-spike\out

## Observed

| Area | Result |
|---|---|
| Rendering | Works. The board's composition target survives `SetParent` and draws inside the WinUI window. |
| Z-order | Works. The board is above `Microsoft.UI.Content.DesktopChildSiteBridge` (index 2 of 4 children), and a hit test at its centre lands on it. |
| Resize | Works when the shell places it. After `spike-move` to 1000×700 and 1700×1000 the board sat exactly at 168,48 and re-laid out (compact list when narrow, columns when wide). |
| Position | Broken. Fence behaviour still runs on the child. The first position was 200,33 instead of 168,48. One click in the board's header area started a caption move, which shifted it by −235,+149 px and kept following the pointer across later steps. |
| Keyboard focus | Broken. Focus stayed in WinUI's `InputSiteWindowClass` after clicking the board, and arrow keys did not reach the board. |
| WinUI text input | Inconclusive. Typed text did not reach the TextBox; the board's move capture probably took that click. |
| Dialog over the board | Inconclusive. No dialog and no dimming appeared. Whatever caused that, WinUI cannot draw above a child HWND, so a dialog or flyout that overlaps the board rectangle would be hidden. |
| Activation from another app | Not tested: the driver could not take the foreground away from the workspace. |
| DPI | Child and host both reported 96. Moving between monitors was not possible here. In code, the fence handles only `WM_DPICHANGED`, which a child window never receives (children get `WM_DPICHANGED_BEFOREPARENT`/`AFTERPARENT`). |

## Conclusion

Hosting the board as a child HWND is viable, but not by re-parenting `FenceWindow`.
Its top-level behaviours (caption and resize hit-testing, anchoring, the z-order
guard, `WS_EX_NOACTIVATE`, glass sampling, top-level DPI handling) fight the
host. The production path needs a small panel surface: a chrome-free child HWND
that mounts a `PanelInstance`, takes focus on click and via F6 from the shell,
handles `WM_DPICHANGED_AFTERPARENT`, and never moves itself. Dialogs and flyouts
must not overlap the board rectangle; open the issue-update confirmation as its
own top-level WinUI window instead of an in-window ContentDialog.

Next check: build that panel surface and repeat this script, adding a
two-monitor DPI move.

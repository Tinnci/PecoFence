# Windows 11 Explorer context menu

The MSIX build registers a native `IExplorerCommand` COM DLL for
`Directory\Background`. The `PecoFence` flyout contains **New fence**, **New note**,
**New to-do list**, and **Settings**. Titles follow the Windows UI language; Chinese simplified is
the source language and English is the fallback.

## Flow

1. `packaging/msix/AppxManifest.xml` registers the DLL and command CLSID. The
   package contains `pecofence_shell_extension.dll` beside `pecofence.exe`.
2. `crates/shell-extension` constructs the fixed flyout without reading settings,
   contacting the app, or loading WebView2 on Explorer's menu-building path.
3. On invocation the DLL captures the pointer's signed screen coordinates and
   sends an allowlisted integer command plus both coordinates to the main app's
   hidden control window with a 500 ms timeout. If the app is absent, it starts
   `pecofence.exe --shell-command <verb> --shell-x <x> --shell-y <y>` from the
   package directory.
4. `crates/shell-contract` defines the wire IDs, x64 coordinate packing, and
   window identity. The app validates the ID, queues the captured position on
   its UI thread, and places new fences near the click. The existing
   single-instance mutex routes a cold-start race back to the first instance.
   A legacy action-only message is accepted during staged upgrades.

The sender distinguishes an absent window from an uncertain delivery. A timed-out
message may still be handled later, so it does not start another process or resend
that action. This keeps creation commands at most once in that failure case.

The Explorer extension never sends file paths or arbitrary command lines. These
commands act on the PecoFence desktop, independent of the File Explorer folder.
The current manifest target is a **folder background**; Windows' packaged context
menu schema documents `Directory\Background` but does not document the desktop
wallpaper as an equivalent target. The PecoFence desktop and fence right-click
menus remain the app's native Win32 menus.

This follows Microsoft's [packaged Explorer command registration](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/integrate-packaged-app-with-file-explorer)
and [bounded window-message delivery](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendmessagetimeoutw)
guidance. Only integers cross the process boundary; no pointers or variable-length
payloads require cross-process marshalling.

## Build and verify

From WSL with the Windows SDK/MSVC libraries mounted:

```bash
scripts/build-windows.sh release pecofence pecofence-watchdog pecofence-shell-extension
scripts/build-windows.sh test pecofence-shell-extension
```

Run the emitted `pecofence_shell_extension-*.exe` test on Windows. Then use
`scripts/make-msix.ps1 -SkipBuild -TargetDir target/x86_64-pc-windows-msvc`
from PowerShell to validate the manifest with MakePri and MakeAppx. The resulting
unsigned MSIX can be inspected without installation.

On a Windows 11 test machine, install a test-signed MSIX, right-click inside an
Explorer folder's empty area, check all four flyout actions with the app both
running and stopped, and verify single-instance behavior. Restart Explorer after
package updates if it still shows an old registration. Test the packaged app's
launch fallback and menu positioning there; a WSL cross-build cannot establish
those behaviors.

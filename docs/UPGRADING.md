# Opening the new workspace format

This edition uses a deliberately breaking **schema-2 workspace**. Containers own
window geometry and ordered tabs; content instances own file collections, folder
portals or extension configuration. Old flat-fence documents are not supported.

## Existing installations

Exit the old application before starting this version. Keep your original files
and backups; there is no automatic import, field alias or migration chain.

New configuration lives in `%APPDATA%\PecoFence\workspace.v2.json`. Portable mode
uses `config\workspace.v2.json` beside the executable. This version does not fall
back to `%APPDATA%\OpenFence`, `config.json` or old backup directories.

When old data is detected in the selected directory, Settings shows the loading
issue instead of silently creating defaults. Explicitly choose **Create workspace**
or import a supported schema-2 document. Old files remain untouched.

Unreadable, corrupt or unsupported primary documents are not overwritten by
ordinary autosave. A verified backup is offered as a read-only recovery candidate.
Only explicitly accepting recovery, importing or creating a new workspace enables
replacement. Existing primary bytes are retained in a unique
`workspace.v2.replaced-<UUID>.json` archive before that replacement.

## Saving and backups

The primary filename is `workspace.v2.json`; the previous version is
`workspace.v2.bak`, and daily backups use `workspace.v2.backups`.

Saving returns success only after primary replacement. An optional backup failure
is shown as degraded persistence, not as a failed primary save. This distinction
does not promise power-loss durability, asynchronous revision-based saving or
atomic protection against arbitrary external editors.

## Desktop integration

The executables are `pecofence.exe` and `pecofence-watchdog.exe`. Settings use
native Win32 controls; current packages include the self-contained Windows App
SDK Runtime 2.5.1 and do not need `WebView2Loader.dll` or the Microsoft Edge
WebView2 Runtime. Older installations may still contain the loader, which older
app versions used. Exit older versions first: instance-lock and
desktop-icon recovery mechanisms still protect Explorer ownership independently
of workspace format compatibility.

Portable and development launches do not automatically synchronize startup
entries. Changing the autostart switch in Settings is an explicit opt-in/out.
Existing OS-level environment aliases and recovery markers have not all been
removed by this workspace-model change.

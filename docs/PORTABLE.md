# PecoFence portable edition

Upgrading from an older workspace format? Exit the old application first and keep
your files. The new format does not migrate them. See the [upgrade guide](UPGRADING.md).

Extract the complete ZIP and run `pecofence.exe`. Keep these files together:

- `pecofence.exe`
- `pecofence-watchdog.exe`
- `WebView2Loader.dll`

Windows 11 x64 and Microsoft Edge WebView2 Runtime are required.

Right-click the tray icon to open Settings or exit. Under General, choose your
display language: English, Simplified/Traditional Chinese, Japanese, Korean,
German, French, Spanish, Portuguese (Brazil), Russian or Follow system.

By default, settings are saved in `%APPDATA%\PecoFence\workspace.v2.json`. Launch with
`--portable` to use a `config` folder beside the executable.
Old `config.json` data is retained but not imported. Settings offers an explicit
new-workspace action or import of a supported schema-2 document.
Portable startup does not synchronize Windows' autostart entry; changing the
autostart toggle in Settings remains an explicit opt-in/out.

Double-click empty desktop space to hide/show fences. **Ctrl+Alt+Space** brings
them above other windows. Drag a title to move a fence; double-click it to roll up.

Automatic organizing rules only change group membership. File operations you
initiate—moving, renaming, copying and deleting—operate on real files.

Exit restores Windows desktop icons. If needed, use **Restore Windows desktop
icons** from the tray menu or Settings → About.

The ZIP is an unsigned portable build. It does not contain your configuration.
Languages work offline; the glass background uses static desktop wallpaper.

New packages include `package.json` with build provenance, payload hashes and
runtime requirements. Keep the full verified payload together. SHA-256 checks
integrity, not publisher identity; use downloads from a trusted source.

# Portable paired updates

The portable updater switches one PecoFence and one `spmd` installation rooted in one
directory. It stages both ZIP packages under `versions/`, verifies each `.zip.sha256`
sidecar, and leaves the old binaries available for rollback. It does not update MSIX
installations or download releases. Obtain ZIPs from a trusted release source: a
checksum stored beside a ZIP detects corruption but is not a publisher signature.

For the initial flat layout, place `pecofence.exe`, `pecofence-watchdog.exe`,
`WebView2Loader.dll`, `spmd.exe`, and portable `config/` under `InstallRoot`.
Subsequent updates use `active.json` to find the active version. Keep one paired
instance per root. Named instances also need a distinct root and the same `-Instance`
value on each update.

Persistent or live daemon example (use the **existing** database absolute path):

```powershell
powershell -ExecutionPolicy Bypass -File scripts/update-portable-pair.ps1 `
  -InstallRoot 'C:\Apps\PecoFence' `
  -PecoFenceZip 'C:\Downloads\pecofence-0.0.3-x64.zip' `
  -SpmZip 'C:\Downloads\spm-0.1.0-x64.zip' `
  -DbPath 'C:\Apps\PecoFence\data\spmd-v2.sqlite'
```

For a fixture pair, use `-Fixture <absolute catalog.json path>` instead of `-DbPath`.
Add `-LiveConfig <absolute config path>` for a live daemon. Add `-Instance <name>`
for a named pair and `-NoHideIcons` for a lab pair.

The updater claims an exclusive `update.lock` file, checks that no unexpected
PecoFence or spmd process is running inside the root, and stages the packages.
It asks the old daemon to exit, copies the closed SQLite database and WAL files,
starts the new daemon, and requires a v2 Hello with the PecoFence feature set.
The staged PecoFence binary then performs its own `--probe-spm` handshake using
its pinned contract before the old desktop UI is closed. The updater asks the old
PecoFence control window to quit through its normal save and
icon-restoration path and waits for the new control window. Only then does it
write `active.json`. If a candidate fails, it stops it, restores the database
backup and restarts the old pair. It updates an existing default-instance Run
entry only when it points at the previous PecoFence executable; unrelated startup
entries are preserved.

An older binary without `spmd --stop` or PecoFence's update-exit message cannot
be shut down gracefully by this script. Exit that old pair manually, then run the
updater once; later updates can use the managed handoff. The desktop windows are
recreated during a PecoFence update, so a brief visual interruption remains.
The SPM panel reconnects after the daemon switch. Keep the same absolute `-DbPath`
across versions; named instances created before the isolated default database
filename change must pass their original path explicitly.

The Windows smoke test stages disposable binaries in `%TEMP%` and performs two
consecutive switches. `-FailNewFence -Persistent` injects a bad candidate to
exercise SQLite rollback:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/test-update-portable-pair.ps1 `
  -PecoFenceBinDir <Windows build directory> -SpmBinDir <Windows build directory> `
  -FixtureDir <spm-contracts fixtures directory> -WebViewLoader <WebView2Loader.x64.dll>
# Add -FailNewFence -Persistent to test a failed candidate with SQLite rollback.
```

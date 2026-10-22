# Process lifecycle: closing, watchdog and status

PecoFence closes through one transaction: stop accepting changes, write the final save, restore
the desktop icons, then let WinUI shut down. Every run leaves a **session record** that says how
it ended. `pecofence-cli status` reads that record, and `pecofence-cli quit` starts the same close
from outside.

## Files

All per user, in `%LOCALAPPDATA%\PecoFence\`. A named test instance (`PECOFENCE_INSTANCE=lab`)
gets its own files (`session.lab.json`, `pecofence.lab.log`).

| File | Written by | Purpose |
| --- | --- | --- |
| `session.json` | app, watchdog | pid, executable, version, config folder, start time, state, how it ended |
| `pecofence.log` | app | this run's log, started fresh |
| `pecofence.previous.log` | app | the run before, kept at startup; after a crash it is the evidence |
| `icons-hidden.marker` | app | present while PecoFence hides the desktop icons |

Only the process that holds the instance lock writes these. A second launch, for example a
Start-menu click while PecoFence runs, exits without touching the log or the record.

## States

`running` → `closing` (the final save is in progress, or a save-failure dialog is open) → `exited`.
Cancelling the close returns the record to `running`. An exited record says how the app ended:

| `exit` | Meaning |
| --- | --- |
| `normal` | The final save completed, and WinUI shut down. |
| `pumpFallback` | The final save completed, but WinUI did not exit within 3 s, so the native message pump ended the process. Nothing is lost; the fallback runs only after the save commits. |
| `discardedChanges` | A save failed, and the user chose *Continue* (discard) in the dialog. `unsaved` is true. |
| `unclosed` | The process ended without the close transaction, for example at Windows sign-out or after a fatal UI error. |

`crashed` comes from the watchdog. The app ended while its record was still open, and the
record now holds the exit code (`0xC0000005` for an access violation, `0xFFFFFFFF` for a forced
`Stop-Process`).

## Watchdog

`pecofence-watchdog <pid> <marker> <session>` runs for every session. Before this change it ran
only while desktop icons were hidden. It waits for the app's process to exit, then:

1. Marks the record as crashed if it still belongs to that pid and is still open. A newer
   instance, or a clean exit, has already replaced or closed it.
2. If the icon marker is present, shows the desktop icons again, retrying for about 10 s. It
   stops if a new PecoFence instance takes over.

If the watchdog itself is killed together with the app (for example `taskkill /T`, or the machine
losing power), the record stays open. `status` then reports `endedUnexpectedly`, and the next
start logs it.

## `pecofence-cli status` and `quit`

```sh
pecofence-cli status --json
pecofence-cli quit --wait 60
```

- **`status`** is read-only. It derives the state from the record and the live process. It checks
  the pid's creation time, so a reused pid is never mistaken for PecoFence.
  - States: `running`, `closing`, `exited` (with `exit` and `unsaved`), `crashed` (with
    `exitCode` / `exitCodeHex`), `endedUnexpectedly`, `noRecord` (never started since records
    were added), or `unverified` (on a platform where processes cannot be checked).
  - It also reports uptime, whether the watchdog is alive, when the workspace was last saved,
    and both log paths.
- **`quit`** posts `WM_APP_QUIT_REQUEST` (`0x800E`, `pecofence_shell_contract`) to the control
  window. That is the same request as the tray's Exit, so the app saves first.
  - It waits up to `--wait` seconds (default 30; `0` returns at once) and reports the recorded
    exit kind.
  - It succeeds without doing anything when PecoFence is not running.
  - It **never force-closes**. A timeout (exit code 1, `QUIT_TIMEOUT`) usually means a
    save-failure dialog is waiting for an answer.
- **Upgrades** should stop the app with `pecofence-cli quit` and check for `exit: normal` before
  replacing binaries. Use `status` afterwards to confirm the new process is `running`, with its
  watchdog.

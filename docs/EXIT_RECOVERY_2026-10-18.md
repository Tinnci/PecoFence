# Native exit recovery — 2026-10-18

## Observed failure

The installed 0.0.4 process accepted Quit. Its log recorded desktop-icon restoration and the Quit command returning. The control window and process remained alive beyond 20 seconds. The restore is reached only after the document close transaction settles.

Sending WM_QUIT to the UI thread obtained from that same process's control window caused normal process exit immediately. No process termination was used. Cancelling directory watcher reads found no pending reads to cancel and did not resolve the wait. This evidence points to the UI exit path rather than proving a watcher or worker deadlock.

## Cause and limits

The native application requested WinUI exit through an asynchronous dispatcher callback. Queue acceptance is not exit completion. There was no bounded fallback after the save transaction committed, so a stalled callback or incomplete WinUI exit could leave the app alive indefinitely.

The incident did not capture a native stack or dispatcher trace that distinguishes callback delivery from WinUI Exit completion. Added phase logs distinguish these cases on a future occurrence. The debug fault-injection test verifies the missing-delivery case; it is not a claim to reproduce the internal WinUI trigger.

## Change

- Arm a 3-second native-window timer only after the final save receipt or an explicit discard decision.
- Keep WinUI exit as the normal path. If the message pump remains alive after the deadline, post WM_QUIT on its owning UI thread.
- Check the committed close state again before fallback. Pending writes, a failed save and a cancelled close cannot enable it. A nested borrow defers the timer until it unwinds.
- Clear the timer when returning to the workspace and at native shutdown.
- Log persistence commit, dispatcher queueing, callback delivery, WinUI exit acceptance and native fallback separately.

## Verification

The save-coordinator regression covers an in-flight write, cancellation, successful completion, a failed save and explicit discard. An isolated debug UI test skips the reactor exit request, waits for the fallback, and verifies saved workspace recovery. Release TODO/note tests exercise normal exit, reload, widget/file tab switches and detach without modifying the real workspace.

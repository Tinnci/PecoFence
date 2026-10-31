#!/usr/bin/env python3
"""Launch a temporary workspace instance, measure it, verify handoff, then let it exit."""
import argparse
import json
import queue
import subprocess
import threading
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    args = parser.parse_args()
    executable = str(args.executable.resolve())
    started = time.monotonic()
    with subprocess.Popen(
        [executable, "--smoke-test-ms", "20000"],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
    ) as child:
        lines = queue.Queue()
        threading.Thread(target=lambda: lines.put(child.stdout.readline()), daemon=True).start()
        try:
            line = lines.get(timeout=12)
            event = json.loads(line)
            if event.get("event") != "first_frame":
                raise ValueError("first-frame signal missing")
            startup_ms = (time.monotonic() - started) * 1000
            root = int(event["pid"])
            second = subprocess.run([executable, "--open", "setup"], timeout=5, capture_output=True)
            time.sleep(2)
            # Only process metadata is read; include the WebView2 descendants.
            script = f"""
$rows = @(Get-CimInstance Win32_Process)
$ids = @({root})
do {{
  $old = $ids.Count
  $ids += @($rows | Where-Object {{ $_.ParentProcessId -in $ids -and $_.ProcessId -notin $ids }} | ForEach-Object {{ $_.ProcessId }})
}} while ($ids.Count -gt $old)
$processes = @(Get-Process -Id $ids -ErrorAction SilentlyContinue | Where-Object {{ $_.ProcessName -in @('tinnci-workspace','msedgewebview2') }})
$private = @(Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | Where-Object {{ $_.IDProcess -in $processes.Id }})
[pscustomobject]@{{
  app_count = @($processes | Where-Object {{ $_.ProcessName -eq 'tinnci-workspace' }}).Count
  process_count = $processes.Count
  app_working_set_mib = [math]::Round((Get-Process -Id {root}).WorkingSet64 / 1MB, 2)
  tree_working_set_mib = [math]::Round(($processes | Measure-Object WorkingSet64 -Sum).Sum / 1MB, 2)
  tree_private_working_set_mib = [math]::Round(($private | Measure-Object WorkingSetPrivate -Sum).Sum / 1MB, 2)
}} | ConvertTo-Json -Compress
"""
            observed = subprocess.run(
                ["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", script],
                capture_output=True, timeout=10, check=True,
            )
            memory = json.loads(observed.stdout.decode("utf-8-sig"))
            if second.returncode or memory["app_count"] != 1:
                raise RuntimeError("single-instance handoff failed")
            exit_code = child.wait(timeout=25)
            print(json.dumps({
                "launch_to_first_frame_ms": round(startup_ms, 2),
                "backend_to_first_frame_ms": round(event["elapsed_ms"], 2),
                "second_launch_exit": second.returncode,
                "exit_code": exit_code,
                **memory,
            }, indent=2))
        except (queue.Empty, ValueError) as exc:
            raise SystemExit("No first-frame signal; close any existing Workspace instance and retry") from exc


if __name__ == "__main__":
    main()

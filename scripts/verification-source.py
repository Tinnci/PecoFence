"""Collect Git provenance inside the verification runner's bounded process tree."""
import argparse
import json
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    # Ignore optional fsmonitor programs: provenance collection needs only a
    # read-only local snapshot, not arbitrary monitor hooks or Git credentials.
    commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], text=True, encoding="utf-8"
    ).strip()
    changes = subprocess.check_output(
        ["git", "--no-optional-locks", "-c", "core.fsmonitor=false",
         "status", "--porcelain"],
        text=True, encoding="utf-8",
    )
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(
        json.dumps({"commit": commit, "dirty": bool(changes.strip())}),
        encoding="utf-8",
    )
    print(f"Source: {commit}; dirty={bool(changes.strip())}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"Source provenance: {error}", file=sys.stderr)
        raise SystemExit(2) from error

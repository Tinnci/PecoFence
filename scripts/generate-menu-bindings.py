#!/usr/bin/env python3
"""Regenerate the narrow WinUI menu projection from Cargo's pinned metadata."""

import argparse
import json
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "vendor/windows-reactor/src/native/menu_bindings.rs"
HEADER = "// Generated from the pinned windows-rs WinUI metadata; see scripts/generate-menu-bindings.py.\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if checked-in bindings differ")
    args = parser.parse_args()
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--locked", "--format-version", "1"],
            cwd=ROOT,
        )
    )
    core = next(
        p for p in metadata["packages"] if p["name"] == "windows-core" and p["version"] == "0.100.0"
    )
    winmd = Path(core["manifest_path"]).parents[3] / "crates/tools/reactor-metadata/winmd"
    if not winmd.is_dir():
        raise SystemExit(f"Pinned WinUI metadata is missing: {winmd}")
    with tempfile.TemporaryDirectory(prefix="pecofence-menu-bindings-") as directory:
        generated = Path(directory) / "bindings.rs"
        config = Path(directory) / "bindings.txt"
        config.write_text(
            f"""--in {winmd}
--in default
--out {generated}
--flat --minimal --filter
Microsoft.UI.Xaml.Controls.IToggleMenuFlyoutItem
Microsoft.UI.Xaml.Controls.IToggleMenuFlyoutItemFactory.CreateInstance
Microsoft.UI.Xaml.Controls.IMenuFlyoutItem.put_KeyboardAcceleratorTextOverride
""",
            encoding="utf-8",
        )
        subprocess.run(
            ["cargo", "run", "--locked", "-p", "tool_bindgen", "--", str(config)],
            cwd=ROOT,
            check=True,
        )
        content = HEADER + generated.read_text(encoding="utf-8")
    if args.check:
        if OUTPUT.read_text(encoding="utf-8") != content:
            raise SystemExit("Menu bindings differ; run scripts/generate-menu-bindings.py")
    else:
        OUTPUT.write_text(content, encoding="utf-8")


if __name__ == "__main__":
    main()

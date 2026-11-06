#!/usr/bin/env python3
"""Draws the app icon at each Windows size with its cells on whole pixels, so 100–300 % display
scales stay sharp instead of blurring a downscaled image. Needs ImageMagick."""
import subprocess
import tempfile
from pathlib import Path

BIG = ["..134..", ".11344.", "1113444", "1123434", "1113444", ".11244.", "..134.."]
SMALL = [".134.", "11344", "11344", "11344", ".134."]
LEVELS = {"1": "#3f424c", "2": "#767a86", "3": "#b7b9c0", "4": "#f6f6f3"}
TILE = "#16181d"
# size: (cell, gap); five columns up to 32 px, seven above.
FIT = {
    16: (2, 1), 20: (2, 2), 24: (3, 1), 32: (4, 2), 40: (4, 1), 48: (5, 1),
    64: (6, 2), 96: (10, 2), 128: (13, 3), 256: (27, 6),
}
OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"


def draw(size: int, path: Path) -> None:
    cell, gap = FIT[size]
    rows = SMALL if size <= 32 else BIG
    start = (size - (len(rows) * cell + (len(rows) - 1) * gap)) // 2
    radius = size * 22 // 100
    shapes = []
    for y, row in enumerate(rows):
        for x, level in enumerate(row):
            if level == ".":
                continue
            left, top = start + x * (cell + gap), start + y * (cell + gap)
            shapes += ["-fill", LEVELS[level], "-draw",
                       f"rectangle {left},{top} {left + cell - 1},{top + cell - 1}"]
    big = size * 8
    subprocess.run(
        ["magick", "-size", f"{big}x{big}", "xc:none", "-fill", TILE, "-draw",
         f"roundrectangle 0,0 {big - 1},{big - 1} {radius * 8},{radius * 8}",
         "-filter", "Box", "-resize", f"{size}x{size}", "+antialias", *shapes, f"PNG32:{path}"],
        check=True,
    )


with tempfile.TemporaryDirectory() as tmp:
    frames = []
    for size in FIT:
        frames.append(Path(tmp) / f"{size}.png")
        draw(size, frames[-1])
    subprocess.run(["magick", *map(str, frames), str(OUT / "icon.ico")], check=True)
    for name, size in {"32x32": 32, "64x64": 64, "128x128": 128, "128x128@2x": 256}.items():
        (OUT / f"{name}.png").write_bytes((Path(tmp) / f"{size}.png").read_bytes())

#!/usr/bin/env python3
"""Subset the bundled warm-paper widget fonts (Noto Serif SC / Noto Sans SC).

The note, TODO and calendar widgets title in Noto Serif SC SemiBold and set body
text in Noto Sans SC Regular. The full Simplified Chinese OTFs are 8-12 MB each,
so this keeps Basic Latin, Latin-1, general punctuation, the GB2312 symbol rows
and the 3,755 level-1 (common) GB2312 hanzi. Characters outside the subset fall
back to the system CJK font through DirectWrite font fallback.

Inputs are the upstream `SubsetOTF/SC` files from
https://github.com/notofonts/noto-cjk (Sans 2.004, Serif 2.003):

    uv run --with fonttools==4.60.1 python scripts/subset-widget-fonts.py \
        NotoSansSC-Regular.otf NotoSerifSC-SemiBold.otf

Outputs are written to `third_party/noto-cjk/` and loaded at run time by
`pecofence_platform::fonts`.
"""

from __future__ import annotations

import argparse
import pathlib
import sys

from fontTools import subset

OUT_DIR = pathlib.Path(__file__).resolve().parent.parent / "third_party" / "noto-cjk"
OUTPUT_NAMES = {
    "NotoSansSC-Regular": "PecoNotoSansSC-Regular.otf",
    "NotoSerifSC-SemiBold": "PecoNotoSerifSC-SemiBold.otf",
}
LATIN_RANGES = [
    (0x0020, 0x007E),  # Basic Latin
    (0x00A0, 0x00FF),  # Latin-1 Supplement
    (0x2000, 0x206F),  # General Punctuation
    (0x20AC, 0x20AC),  # Euro sign
    (0x2190, 0x2193),  # Arrows
    (0x3000, 0x303F),  # CJK Symbols and Punctuation
    (0xFF01, 0xFF5E),  # Fullwidth ASCII variants
]


def gb2312_codepoints() -> set[int]:
    """GB2312 symbol rows (01-09) and level-1 hanzi (rows 16-55)."""
    rows = list(range(0xA1, 0xAA)) + list(range(0xB0, 0xD8))
    points: set[int] = set()
    for row in rows:
        for cell in range(0xA1, 0xFF):
            try:
                char = bytes((row, cell)).decode("gb2312")
            except UnicodeDecodeError:
                continue
            points.add(ord(char))
    return points


def unicodes() -> list[int]:
    points = gb2312_codepoints()
    for first, last in LATIN_RANGES:
        points.update(range(first, last + 1))
    return sorted(points)


def subset_font(source: pathlib.Path, target: pathlib.Path, points: list[int]) -> None:
    options = subset.Options()
    options.name_IDs = ["*"]
    options.name_languages = ["*"]
    options.notdef_outline = True
    options.layout_features = ["*"]
    # CJK vertical metrics and alternates are unused by horizontal widget text.
    options.drop_tables += ["vhea", "vmtx", "VORG"]
    font = subset.load_font(str(source), options)
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=points)
    subsetter.subset(font)
    subset.save_font(font, str(target), options)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("sources", nargs="+", type=pathlib.Path)
    args = parser.parse_args()
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    points = unicodes()
    for source in args.sources:
        name = OUTPUT_NAMES.get(source.stem)
        if name is None:
            print(f"unexpected source font {source.name}", file=sys.stderr)
            return 2
        target = OUT_DIR / name
        subset_font(source, target, points)
        print(f"{target.name}: {target.stat().st_size / 1024:.0f} KiB, {len(points)} code points")
    return 0


if __name__ == "__main__":
    sys.exit(main())

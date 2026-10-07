"""Check README translations for missing files and broken relative links."""
import re
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LANGUAGES = ["zh-CN", "zh-TW", "ja", "ko", "de", "fr", "es", "pt-BR", "ru"]
LINK = re.compile(r'\]\(([^)\s]+)\)|href="([^"]+)"|src="([^"]+)"')
HEADING = re.compile(r"^(#{1,6})\s+(.*?)\s*$", re.M)


def anchor(text):
    text = re.sub(r"[*_`]", "", text).strip().lower()
    text = "".join(c for c in text if c.isalnum() or c in " -_" or unicodedata.category(c).startswith("M"))
    return text.replace(" ", "-")


def heading_anchors(text):
    anchors = set()
    occurrences = {}
    for _, heading in HEADING.findall(text):
        base = anchor(heading)
        occurrence = occurrences.get(base, 0)
        occurrences[base] = occurrence + 1
        anchors.add(base if occurrence == 0 else f"{base}-{occurrence}")
    return anchors


def check(path, problems):
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        problems.append(f"{path.name}: cannot read UTF-8 content ({error})")
        return

    if text.startswith("﻿") or "\r" in text:
        problems.append(f"{path.name}: BOM or CRLF line endings")
    anchors = heading_anchors(text)
    for match in LINK.finditer(text):
        target = next(group for group in match.groups() if group)
        if target.startswith(("http://", "https://", "mailto:")):
            continue
        if target.startswith("#"):
            if target[1:] not in anchors:
                problems.append(f"{path.name}: anchor {target} has no heading")
            continue
        file, _, fragment = target.partition("#")
        resolved = (path.parent / file).resolve()
        if not resolved.exists():
            problems.append(f"{path.name}: missing link target {target}")
        elif fragment and resolved.suffix == ".md":
            try:
                linked_text = resolved.read_text(encoding="utf-8")
            except (OSError, UnicodeError) as error:
                problems.append(f"{path.name}: cannot read UTF-8 link target {target} ({error})")
                continue
            other = heading_anchors(linked_text)
            if fragment not in other:
                problems.append(f"{path.name}: {target} anchor not found")


def check_repository(root=ROOT):
    problems = []
    readmes = [(root / "README.md", "README.md")]
    for language in LANGUAGES:
        readmes.append((root / "docs/readme" / f"README.{language}.md",
                        f"docs/readme/README.{language}.md"))

    checked = 0
    for path, label in readmes:
        if not path.is_file():
            problems.append(f"missing {label}")
            continue
        checked += 1
        check(path, problems)
    return checked, problems


def main():
    checked, problems = check_repository()
    for problem in problems:
        print(problem)
    print(f"{checked} READMEs checked, {len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())

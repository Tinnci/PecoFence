"""Check native source translations, coverage, and interpolation placeholders."""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LANGUAGES = ("en", "ja", "zh-TW", "ko", "de", "fr", "es", "pt-BR", "ru")
CJK = re.compile(r"[\u3400-\u9fff]")
LITERAL = r'"(?:\\.|[^"\\])*"'
I18N_CALL = re.compile(r"\bi18n::(?:text|format)\s*\(\s*(" + LITERAL + ")")
# Settings form metadata also stores labels in arrays and enum match arms,
# then translates them through tr(label). Audit those literals, not just calls.
SETTINGS_LABEL = re.compile("(" + LITERAL + ")")


def rust_literal(token):
    """Decode a regular Rust string, including `\\u{...}` and line continuation."""
    value = token[1:-1]
    result = []
    index = 0
    escapes = {"n": "\n", "r": "\r", "t": "\t", "0": "\0", "\\": "\\", '"': '"', "'": "'"}
    while index < len(value):
        character = value[index]
        index += 1
        if character != "\\":
            result.append(character)
            continue
        escape = value[index]
        index += 1
        if escape in escapes:
            result.append(escapes[escape])
        elif escape == "u" and value[index] == "{":
            end = value.index("}", index)
            result.append(chr(int(value[index + 1:end].replace("_", ""), 16)))
            index = end + 1
        elif escape == "x":
            result.append(chr(int(value[index:index + 2], 16)))
            index += 2
        elif escape in "\r\n":
            while index < len(value) and value[index].isspace():
                index += 1
        else:
            raise ValueError(f"unsupported Rust string escape in {token!r}")
    return "".join(result)


def source_keys():
    keys = set()
    for path in (ROOT / "crates").rglob("*.rs"):
        if path.name in ("bindings.rs", "gpu_bindings.rs"):
            continue
        source = path.read_text(encoding="utf-8")
        patterns = [I18N_CALL]
        if "settings_host" in path.parts or path.name == "settings_host.rs":
            if path.name == "tests.rs":
                continue
            source = re.split(r"#\[cfg\(test\)\]\s*mod\s+tests\s*\{", source, maxsplit=1)[0]
            patterns.append(SETTINGS_LABEL)
        for pattern in patterns:
            for match in pattern.finditer(source):
                key = rust_literal(match[1])
                if CJK.search(key):
                    keys.add(key)
    # The native palette stores constant source names and translates at serialization.
    keys.update(("红", "橙", "黄", "绿", "青", "蓝", "紫", "粉", "灰"))
    return keys


def placeholders(value):
    return sorted(re.findall(r"\{\d+\}|%1", value))


def check():
    keys = source_keys()
    failures = []
    for language in LANGUAGES:
        path = ROOT / "locales" / (language + ".json")
        catalog = json.loads(path.read_text(encoding="utf-8"))
        for key in sorted(keys - catalog.keys()):
            failures.append(f"{language}: missing {key!r}")
        for key, value in catalog.items():
            if not isinstance(value, str) or not value.strip():
                failures.append(f"{language}: empty/invalid {key!r}")
            elif placeholders(key) != placeholders(value):
                failures.append(f"{language}: placeholders differ for {key!r}")
    if failures:
        raise SystemExit("\n".join(failures))
    print(f"OK: {len(keys)} source messages covered in all {len(LANGUAGES)} languages")


if __name__ == "__main__":
    check()

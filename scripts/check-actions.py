"""Check direct action pins and (optionally) their declared runtimes. Not a YAML validator."""
from pathlib import Path
import re
import sys
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
USES = re.compile(r"^\s*(?:-\s*)?uses:\s*([^\s#]+)", re.MULTILINE)
PIN = re.compile(r"([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)(/[^@]+)?@([0-9a-f]{40})$")


def check(root=ROOT, online=False, fetch=None):
    errors = []
    manifests = sorted((root / ".github/workflows").glob("*.y*ml"))
    manifests += sorted((root / ".github/actions").rglob("action.y*ml"))
    dependency_config = (root / ".github/dependabot.yml").read_text(encoding="utf-8")
    directories = set(re.findall(r"^\s+(?:-\s*)?directory:\s*['\"]?([^'\"\s]+)", dependency_config, re.MULTILINE))
    if "/" not in directories:
        errors.append("Dependabot must cover root workflows")
    references = set()
    for path in manifests:
        text = path.read_text(encoding="utf-8")
        if re.search(r"^\s*pull_request_target\s*:", text, re.MULTILINE):
            errors.append(f"{path}: privileged pull_request_target is not permitted")
        if path.parent.is_relative_to(root / ".github/actions") and "/" + path.parent.relative_to(root).as_posix() not in directories:
            errors.append(f"{path}: local composite action needs explicit Dependabot coverage")
        for value in USES.findall(text):
            value = value.strip("'\"")
            if value.startswith("./"):
                target = (root / value).resolve()
                if not target.is_relative_to(root.resolve()):
                    errors.append(f"{path}: local action escapes the repository")
                    continue
                if not (target / "action.yml").is_file() and not (target / "action.yaml").is_file():
                    errors.append(f"{path}: missing local action {value}")
                continue
            if not PIN.fullmatch(value):
                errors.append(f"{path}: action must use a full commit SHA: {value}")
            else:
                references.add(value)
    if online:
        for reference in sorted(references):
            match = PIN.fullmatch(reference)
            repository, subpath, sha = match.groups()
            url = f"https://raw.githubusercontent.com/{repository}/{sha}{subpath or ''}/action.yml"
            try:
                if fetch:
                    text = fetch(url)
                else:
                    request = urllib.request.Request(url, headers={"User-Agent": "PecoFence-action-policy"})
                    with urllib.request.urlopen(request, timeout=30) as response:
                        data = response.read(65537)
                    if len(data) > 65536:
                        raise ValueError("oversized action metadata")
                    text = data.decode("utf-8")
                runs = re.search(r"^runs:\s*\n(.*?)(?=^\S|\Z)", text, re.MULTILINE | re.DOTALL)
                using = re.search(r"^\s+using:\s*['\"]?([^'\"\s]+)", runs[1], re.MULTILINE) if runs else None
                runtime = using[1] if using else ""
                if runtime.startswith("node") and runtime[4:].isdigit():
                    if int(runtime[4:]) < 24:
                        errors.append(f"{reference}: deprecated runtime {runtime}")
                elif runtime not in ("composite", "docker"):
                    errors.append(f"{reference}: unknown action runtime {runtime!r}")
                print(f"{reference}: {runtime}")
            except (OSError, ValueError) as error:
                errors.append(f"{reference}: cannot verify action metadata: {error}")
    return errors


if __name__ == "__main__":
    errors = check(online="--online" in sys.argv)
    if errors:
        raise SystemExit("\n".join(errors))
    print("Direct action pins and Dependabot coverage passed")

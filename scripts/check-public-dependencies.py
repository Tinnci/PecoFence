"""Keep desktop builds independent of private repositories and credentials.

The approved source boundary is this checkout and crates.io. Changing that
boundary requires an explicit policy change, not a credential added to CI.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
PRIVATE_BUILD_MARKERS = ("PRIVATE_REPO_TOKEN", "SPM_READ_TOKEN", "github.com/Tinnci/spm")


def inside(path: Path, root: Path) -> bool:
    return path.resolve().is_relative_to(root.resolve())


def dependency_tables(value: dict):
    for key, child in value.items():
        if isinstance(child, dict):
            if key in ("dependencies", "dev-dependencies", "build-dependencies"):
                yield child
            elif key == "patch":
                yield from child.values()
            elif key == "replace":
                yield child
            else:
                yield from dependency_tables(child)


def check(root: Path, metadata: dict | None = None) -> list[str]:
    root = root.resolve()
    errors = []
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    dependency = manifest["workspace"]["dependencies"].get("spm-contracts")
    if dependency != {"version": "=0.1.0", "path": "crates/spm-contracts"}:
        errors.append("spm-contracts must use the exact public workspace path/version")
    members = manifest["workspace"]["members"]
    if "crates/spm-contracts" not in members:
        errors.append("spm-contracts must be a tested workspace member")
    pending = [root / "Cargo.toml"]
    for member in members:
        matches = list(root.glob(member))
        if not matches:
            errors.append(f"missing workspace member: {member}")
        pending.extend(path / "Cargo.toml" for path in matches)
    visited = set()
    while pending:
        path = pending.pop().resolve()
        if not inside(path, root):
            errors.append("dependency manifest escapes the public checkout")
            continue
        if path in visited:
            continue
        visited.add(path)
        if not path.is_file():
            errors.append(f"missing manifest: {path.relative_to(root)}")
            continue
        document = tomllib.loads(path.read_text(encoding="utf-8"))
        for table in dependency_tables(document):
            for name, entry in table.items():
                if not isinstance(entry, dict):
                    continue
                if "git" in entry or "registry" in entry:
                    errors.append(f"{path.relative_to(root)}: unapproved source for {name}")
                if "path" in entry:
                    target = (path.parent / entry["path"]).resolve()
                    if not inside(target, root):
                        errors.append(f"{path.relative_to(root)}: external path for {name}")
                    else:
                        pending.append(target / "Cargo.toml")
    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    contracts = [p for p in lock["package"] if p["name"] == "spm-contracts"]
    if len(contracts) != 1 or contracts[0].get("source") or contracts[0]["version"] != "0.1.0":
        errors.append("Cargo.lock must contain one local spm-contracts 0.1.0")
    for package in lock["package"]:
        if package.get("source") not in (None, REGISTRY):
            errors.append(f"Cargo.lock: unapproved source for {package['name']}")
    for config in (root / ".cargo/config", root / ".cargo/config.toml"):
        if config.is_file():
            settings = tomllib.loads(config.read_text(encoding="utf-8"))
            if (settings.get("source") or settings.get("patch") or settings.get("paths")
                    or settings.get("replace")
                    or set(settings.get("registries", {})) - {"crates-io"}):
                errors.append("repository Cargo config overrides approved dependency sources")
    for directory in (root / ".github/workflows", root / ".github/actions"):
        if directory.exists():
            for path in directory.rglob("*"):
                if path.suffix in (".yml", ".yaml") and path.is_file():
                    text = path.read_text(encoding="utf-8")
                    if any(marker in text for marker in PRIVATE_BUILD_MARKERS):
                        errors.append(f"{path.relative_to(root)}: private build access reintroduced")
    if metadata is not None:
        for package in metadata["packages"]:
            source = package.get("source")
            if source is None:
                if not inside(Path(package["manifest_path"]), root):
                    errors.append(f"resolved external local package: {package['name']}")
            elif source != REGISTRY:
                errors.append(f"resolved unapproved source: {package['name']}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--resolved", action="store_true",
                        help="also inspect Cargo's locked, offline resolved dependency graph")
    args = parser.parse_args()
    metadata = None
    if args.resolved:
        result = subprocess.run(
            ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
            cwd=ROOT, text=True, capture_output=True,
        )
        if result.returncode:
            print(result.stderr, file=sys.stderr)
            return result.returncode
        metadata = json.loads(result.stdout)
    errors = check(ROOT, metadata)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("Public dependency boundary: checkout + crates.io; no private build credentials")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

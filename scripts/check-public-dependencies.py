"""Keep desktop builds independent of private repositories and credentials.

The approved sources are this checkout, crates.io, and the one immutable public
Microsoft Windows Rust SDK revision below. Changing that boundary requires an
explicit policy change, not a credential added to CI.
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
WINDOWS_RS_URL = "https://github.com/microsoft/windows-rs"
WINDOWS_RS_REV = "2672e615d9cc0771448a781f3b2fe34e7fd08c6a"
WINDOWS_RS_SOURCE = (
    f"git+{WINDOWS_RS_URL}?rev={WINDOWS_RS_REV}#{WINDOWS_RS_REV}"
)
WINDOWS_REACTOR_VERSION = "0.100.0"
WINDOWS_REACTOR_PATH = "vendor/windows-reactor"
WINDOWS_REACTOR_DEPENDENCY = {
    "version": f"={WINDOWS_REACTOR_VERSION}",
    "path": WINDOWS_REACTOR_PATH,
}
# Explicit permitted SDK package names and versions: this is not a `windows-*`
# prefix allowlist. Keep it to the packages in PecoFence's resolved SDK graph.
WINDOWS_RS_PACKAGES = {
    "windows-collections": "0.100.0",
    "windows-core": "0.100.0",
    "windows-future": "0.100.0",
    "windows-implement": "0.100.0",
    "windows-interface": "0.100.0",
    "windows-link": "0.100.0",
    "windows-reference": "0.100.0",
    "windows-result": "0.100.0",
    "windows-strings": "0.100.0",
    "windows-threading": "0.100.0",
    "windows-time": "0.100.0",
}
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


def approved_windows_rs_dependency(name: str, entry: dict) -> bool:
    package = entry.get("package", name)
    return (
        package == name
        and package in WINDOWS_RS_PACKAGES
        and entry.get("git") == WINDOWS_RS_URL
        and entry.get("rev") == WINDOWS_RS_REV
        and "branch" not in entry
        and "tag" not in entry
        and "registry" not in entry
        and "path" not in entry
    )


def approved_windows_reactor_dependency(name: str, entry: dict) -> bool:
    return name == "windows-reactor" and entry == WINDOWS_REACTOR_DEPENDENCY


def approved_workspace_windows_reactor_dependency(name: str, entry: dict) -> bool:
    return (
        name == "windows-reactor"
        and entry.get("workspace") is True
        and not {
            "branch", "git", "path", "registry", "rev", "tag", "version", "package"
        }.intersection(entry)
    )


def approved_windows_rs_package(package: dict) -> bool:
    return (
        package.get("source") == WINDOWS_RS_SOURCE
        and package.get("name") in WINDOWS_RS_PACKAGES
        and package.get("version") == WINDOWS_RS_PACKAGES.get(package.get("name"))
    )


def check(root: Path, metadata: dict | None = None) -> list[str]:
    root = root.resolve()
    errors = []
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    dependency = manifest["workspace"]["dependencies"].get("spm-contracts")
    if dependency != {"version": "=0.1.0", "path": "crates/spm-contracts"}:
        errors.append("spm-contracts must use the exact public workspace path/version")
    excluded = manifest["workspace"].get("exclude", [])
    if not isinstance(excluded, list) or WINDOWS_REACTOR_PATH not in excluded:
        errors.append("workspace must exclude the vendored windows-reactor package")
    windows_reactor = manifest["workspace"]["dependencies"].get("windows-reactor")
    if not isinstance(windows_reactor, dict) or not approved_windows_reactor_dependency(
        "windows-reactor", windows_reactor
    ):
        errors.append("workspace windows-reactor must use the exact vendored path/version")
    reactor_manifest = root / WINDOWS_REACTOR_PATH / "Cargo.toml"
    if reactor_manifest.is_file():
        reactor_package = tomllib.loads(
            reactor_manifest.read_text(encoding="utf-8")
        ).get("package", {})
        if (
            reactor_package.get("name") != "windows-reactor"
            or reactor_package.get("version") != WINDOWS_REACTOR_VERSION
        ):
            errors.append(
                "vendor/windows-reactor must declare the exact windows-reactor 0.100.0 identity"
            )
    else:
        errors.append("missing manifest: vendor/windows-reactor/Cargo.toml")
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
                if entry.get("package", name) == "windows-reactor":
                    if path == root / "Cargo.toml":
                        approved_reactor = approved_windows_reactor_dependency(name, entry)
                    else:
                        approved_reactor = approved_workspace_windows_reactor_dependency(
                            name, entry
                        )
                    if not approved_reactor:
                        errors.append(
                            f"{path.relative_to(root)}: windows-reactor must use the exact vendored path/version"
                        )
                if "git" in entry and not approved_windows_rs_dependency(name, entry):
                    errors.append(f"{path.relative_to(root)}: unapproved Git source for {name}")
                elif "registry" in entry:
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
    windows_reactors = [
        p for p in lock["package"] if p["name"] == "windows-reactor"
    ]
    if (
        len(windows_reactors) != 1
        or windows_reactors[0].get("source") is not None
        or windows_reactors[0].get("version") != WINDOWS_REACTOR_VERSION
    ):
        errors.append(
            "Cargo.lock must contain one local windows-reactor 0.100.0"
        )
    windows_rs_packages = []
    for package in lock["package"]:
        source = package.get("source")
        if source == WINDOWS_RS_SOURCE:
            if not approved_windows_rs_package(package):
                errors.append(
                    "Cargo.lock: unapproved package at pinned Windows SDK source: "
                    f"{package['name']}"
                )
            windows_rs_packages.append(package)
        elif source not in (None, REGISTRY):
            errors.append(f"Cargo.lock: unapproved source for {package['name']}")
    windows_rs_versions = {
        (package["name"], package["version"]) for package in windows_rs_packages
    }
    for package in lock["package"]:
        if package.get("source") == REGISTRY and (
            package["name"], package["version"]
        ) in windows_rs_versions:
            errors.append(
                f"Cargo.lock: {package['name']} {package['version']} is sourced from both "
                "the pinned Windows SDK Git revision and crates.io"
            )
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
        resolved_windows_reactors = [
            package for package in metadata["packages"]
            if package.get("name") == "windows-reactor"
        ]
        resolved_windows_rs_packages = []
        for package in metadata["packages"]:
            source = package.get("source")
            if source is None:
                if not inside(Path(package["manifest_path"]), root):
                    errors.append(f"resolved external local package: {package['name']}")
            elif source != REGISTRY:
                if source != WINDOWS_RS_SOURCE:
                    errors.append(f"resolved unapproved source: {package['name']}")
                elif not approved_windows_rs_package(package):
                    errors.append(
                        f"resolved unapproved package at pinned Windows SDK source: {package['name']}"
                    )
                else:
                    resolved_windows_rs_packages.append(package)
        expected_reactor_manifest = (root / WINDOWS_REACTOR_PATH / "Cargo.toml").resolve()
        if (
            len(resolved_windows_reactors) != 1
            or resolved_windows_reactors[0].get("source") is not None
            or resolved_windows_reactors[0].get("version") != WINDOWS_REACTOR_VERSION
            or Path(resolved_windows_reactors[0].get("manifest_path", "")).resolve()
            != expected_reactor_manifest
        ):
            errors.append(
                "resolved graph must contain local windows-reactor 0.100.0 from vendor/windows-reactor"
            )
        resolved_windows_rs_versions = {
            (package["name"], package["version"]) for package in resolved_windows_rs_packages
        }
        for package in metadata["packages"]:
            if package.get("source") == REGISTRY and (
                package["name"], package.get("version")
            ) in resolved_windows_rs_versions:
                errors.append(
                    f"resolved graph: {package['name']} {package['version']} is sourced from "
                    "both the pinned Windows SDK Git revision and crates.io"
                )
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
    print(
        "Public dependency boundary: checkout + crates.io + 11 packages at the pinned "
        "windows-rs revision + vendored windows-reactor; no private build credentials"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

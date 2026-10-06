"""Verified Windows payloads. Standard library only; never launches packaged binaries."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import struct
import subprocess
import sys
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
EXES = ("pecofence.exe", "pecofence-watchdog.exe")
BASE_FILES = {*EXES, "LICENSE", "NOTICE", "README.md", "UPGRADING.md", "THIRD-PARTY-LICENSES.txt"}
WEB_FILES = {"WebView2Loader.dll", "LICENSE-WebView2Loader.txt"}
MANIFEST = "package.json"
PACKAGE_INPUTS = ("LICENSE", "NOTICE", "docs/PORTABLE.md", "docs/UPGRADING.md",
                  "third_party/webview2/WebView2Loader.x64.dll", "third_party/webview2/LICENSE.txt")
MAX_PAYLOAD = 64 * 1024 * 1024
VERSION = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def json_bytes(value) -> bytes:
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode("utf-8")


def command(*args: str, root: Path = ROOT) -> str:
    return subprocess.check_output(args, cwd=root, text=True, encoding="utf-8").strip()


def pe_image(data: bytes) -> dict:
    """Read PE32+ architecture, subsystem and static/delay imports, without executing it."""
    def number(offset, fmt):
        if offset < 0 or offset + struct.calcsize(fmt) > len(data):
            raise ValueError("truncated PE image")
        return struct.unpack_from(fmt, data, offset)[0]

    if data[:2] != b"MZ":
        raise ValueError("payload is not a Windows PE image")
    pe = number(0x3C, "<I")
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("invalid PE signature")
    machine, sections = number(pe + 4, "<H"), number(pe + 6, "<H")
    optional, optional_size = pe + 24, number(pe + 20, "<H")
    if machine != 0x8664 or number(optional, "<H") != 0x20B:
        raise ValueError("expected an x64 PE32+ image")
    if optional_size < 112 or optional + optional_size + 40 * sections > len(data):
        raise ValueError("truncated PE headers")
    image_base = number(optional + 24, "<Q")

    def offset(rva):
        for index in range(sections):
            section = optional + optional_size + index * 40
            virtual = number(section + 12, "<I")
            raw_size, raw = number(section + 16, "<I"), number(section + 20, "<I")
            if virtual <= rva < virtual + raw_size:
                pos = raw + rva - virtual
                if pos < len(data):
                    return pos
        raise ValueError("PE import RVA is outside file-backed sections")

    def name(rva):
        start = offset(rva)
        end = data.find(b"\0", start, min(len(data), start + 256))
        if end < 0:
            raise ValueError("invalid PE import name")
        value = data[start:end].decode("ascii").lower()
        if not re.fullmatch(r"[a-z0-9_.-]+\.dll", value):
            raise ValueError("unsafe PE import name")
        return value

    imports = set()
    count = number(optional + 108, "<I")
    for directory, width in ((1, 20), (13, 32)):
        if count <= directory:
            continue
        entry = optional + 112 + directory * 8
        if entry + 8 > optional + optional_size:
            raise ValueError("truncated PE data directories")
        rva, size = number(entry, "<I"), number(entry + 4, "<I")
        if not rva:
            continue
        table = offset(rva)
        for index in range(min(size // width, 512)):
            pos = table + index * width
            if pos + width > len(data):
                raise ValueError("truncated PE import table")
            if data[pos:pos + width] == bytes(width):
                break
            if directory == 1:
                imports.add(name(number(pos + 12, "<I")))
            else:
                attributes, address = number(pos, "<I"), number(pos + 4, "<I")
                imports.add(name(address if attributes & 1 else address - image_base))
        else:
            raise ValueError("unterminated PE import table")
    return {
        "architecture": "x64", "subsystem": number(optional + 68, "<H"),
        "dll": bool(number(pe + 22, "<H") & 0x2000), "imports": sorted(imports),
    }


def build_inputs(root: Path = ROOT) -> dict:
    version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    if not VERSION.fullmatch(version):
        raise ValueError("invalid workspace version")
    files = command("git", "ls-files", "--cached", "--others", "--exclude-standard", "-z", root=root).split("\0")
    fingerprint = hashlib.sha256()
    for name in sorted(set(filter(None, files))):
        path = root / name
        if path.is_symlink():
            raise ValueError(f"source symlinks are not supported by build receipts: {name}")
        file_digest = digest(path.read_bytes()) if path.is_file() else "missing"
        fingerprint.update(name.encode("utf-8") + b"\0" + file_digest.encode("ascii") + b"\0")
    return {
        "version": version, "commit": command("git", "rev-parse", "HEAD", root=root),
        "dirty": bool(command("git", "status", "--porcelain", "--untracked-files=all", root=root)),
        "sourceSha256": fingerprint.hexdigest(), "cargoLockSha256": digest((root / "Cargo.lock").read_bytes()),
        "rustc": command("rustc", "--version", root=root),
        "packageInputs": {name: digest((root / name).read_bytes()) if (root / name).is_file() else "missing"
                          for name in PACKAGE_INPUTS},
    }


def record_build(release: Path, captured: Path, root: Path = ROOT) -> dict:
    inputs = json.loads(captured.read_text(encoding="utf-8"))
    if inputs != build_inputs(root):
        raise ValueError("source/toolchain changed during compilation; rebuild before packaging")
    files = {name: (release / name).read_bytes() for name in EXES}
    for name, data in files.items():
        image = pe_image(data)
        if image["dll"] or image["subsystem"] != 2:
            raise ValueError(f"{name} must be an x64 GUI executable")
    receipt = {"schema": 1, "inputs": inputs, "binaries": {name: digest(data) for name, data in files.items()}}
    (release / "build-receipt.json").write_bytes(json_bytes(receipt))
    return receipt


def load_receipt(release: Path, version: str | None = None, clean: bool = False, root: Path = ROOT) -> dict:
    path = release / "build-receipt.json"
    if not path.is_file():
        raise ValueError("missing build receipt; run scripts/build-desktop.ps1 before -SkipBuild")
    receipt = json.loads(path.read_text(encoding="utf-8"))
    if receipt.get("schema") != 1 or receipt.get("inputs") != build_inputs(root):
        raise ValueError("stale build receipt; source/version/toolchain changed")
    if version and version != receipt["inputs"]["version"]:
        raise ValueError("-Version must equal the compiled workspace version")
    if clean and receipt["inputs"]["dirty"]:
        raise ValueError("release packaging requires a clean source tree")
    for name in EXES:
        if digest((release / name).read_bytes()) != receipt["binaries"][name]:
            raise ValueError(f"binary changed after its verified build: {name}")
    return receipt


def payload(release: Path, stage: Path, receipt: dict, kind: str, root: Path = ROOT) -> dict:
    if stage.exists():
        raise ValueError("payload staging directory must be new; never overwrite an extracted user's config")
    binaries = {name: (release / name).read_bytes() for name in EXES}
    for name, data in binaries.items():
        if digest(data) != receipt["binaries"][name]:
            raise ValueError(f"binary changed before payload staging: {name}")
    def asset(name):
        data = (root / name).read_bytes()
        if digest(data) != receipt["inputs"]["packageInputs"][name]:
            raise ValueError(f"asset changed after its verified build: {name}")
        return data
    images = {name: pe_image(data) for name, data in binaries.items()}
    imports = {dll for image in images.values() for dll in image["imports"]}
    webview = "webview2loader.dll" in imports
    files = dict(binaries)
    for source, target in [
        ("LICENSE", "LICENSE" if kind == "portable" else "LICENSE.txt"),
        ("NOTICE", "NOTICE"), ("docs/UPGRADING.md", "UPGRADING.md"),
    ]:
        files[target] = asset(source)
    if kind == "portable":
        files["README.md"] = asset("docs/PORTABLE.md")
    if webview:
        loader = asset("third_party/webview2/WebView2Loader.x64.dll")
        if not pe_image(loader)["dll"]:
            raise ValueError("WebView2Loader must be an x64 DLL")
        files["WebView2Loader.dll"] = loader
        files["LICENSE-WebView2Loader.txt"] = asset("third_party/webview2/LICENSE.txt")
    stage.mkdir(parents=True)
    for name, data in files.items():
        (stage / name).write_bytes(data)
    subprocess.run([sys.executable, str(root / "scripts/write-license-notices.py"),
                    str(stage / "THIRD-PARTY-LICENSES.txt")], cwd=root, check=True)
    files["THIRD-PARTY-LICENSES.txt"] = (stage / "THIRD-PARTY-LICENSES.txt").read_bytes()
    manifest = {
        "schema": 1, "product": "PecoFence", "kind": kind, "architecture": "x64",
        "version": receipt["inputs"]["version"], "build": receipt["inputs"],
        "runtimeRequirements": {"webview2Evergreen": webview,
                                "vcRuntime": any(dll.startswith(("vcruntime", "msvcp")) for dll in imports)},
        "images": images,
        "files": {name: {"bytes": len(data), "sha256": digest(data)} for name, data in sorted(files.items())},
    }
    (stage / MANIFEST).write_bytes(json_bytes(manifest))
    return manifest


def write_zip(stage: Path, target: Path) -> None:
    """Deterministic given payload and compression toolchain, not a compiler reproducibility claim."""
    with zipfile.ZipFile(target, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in sorted(stage.iterdir()):
            if not path.is_file() or path.is_symlink():
                raise ValueError("portable payload must contain only root files")
            entry = zipfile.ZipInfo(path.name, (1980, 1, 1, 0, 0, 0))
            entry.create_system = 3
            entry.external_attr = (stat.S_IFREG | 0o644) << 16
            entry.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(entry, path.read_bytes(), compresslevel=9)


def validate_files(files: dict[str, bytes]) -> dict:
    if MANIFEST not in files:
        raise ValueError("missing package manifest")
    manifest = json.loads(files[MANIFEST].decode("utf-8"))
    if not isinstance(manifest, dict):
        raise ValueError("package manifest must be an object")
    if (manifest.get("schema"), manifest.get("product"), manifest.get("kind"), manifest.get("architecture")) != (1, "PecoFence", "portable", "x64"):
        raise ValueError("unsupported package identity/format")
    version = manifest.get("version")
    if not isinstance(version, str) or len(version) > 128 or not VERSION.fullmatch(version):
        raise ValueError("invalid package version")
    requirements = manifest.get("runtimeRequirements")
    if not isinstance(requirements, dict) or set(requirements) != {"webview2Evergreen", "vcRuntime"} \
            or any(type(value) is not bool for value in requirements.values()):
        raise ValueError("invalid runtime requirements")
    build = manifest.get("build")
    if not isinstance(build, dict) or build.get("version") != version \
            or not re.fullmatch(r"[0-9a-f]{40}", str(build.get("commit", ""))) \
            or type(build.get("dirty")) is not bool \
            or any(not re.fullmatch(r"[0-9a-f]{64}", str(build.get(key, ""))) for key in ("sourceSha256", "cargoLockSha256")):
        raise ValueError("invalid build provenance")
    expected = BASE_FILES | (WEB_FILES if manifest["runtimeRequirements"]["webview2Evergreen"] else set())
    if set(files) != expected | {MANIFEST} or set(manifest["files"]) != expected:
        raise ValueError("unexpected/missing payload; configs, keys and extra executables are not allowed")
    for name in expected:
        record = manifest["files"][name]
        if record != {"bytes": len(files[name]), "sha256": digest(files[name])}:
            raise ValueError(f"payload checksum/size mismatch: {name}")
    imports = set()
    for name in EXES:
        image = pe_image(files[name])
        if image["dll"] or image["subsystem"] != 2 or image != manifest["images"][name]:
            raise ValueError(f"invalid/mismatched executable image: {name}")
        imports.update(image["imports"])
    if ("webview2loader.dll" in imports) != manifest["runtimeRequirements"]["webview2Evergreen"]:
        raise ValueError("WebView requirement disagrees with executable imports")
    if any(dll.startswith(("vcruntime", "msvcp")) for dll in imports) != requirements["vcRuntime"]:
        raise ValueError("VC runtime requirement disagrees with executable imports")
    if WEB_FILES <= expected and not pe_image(files["WebView2Loader.dll"])["dll"]:
        raise ValueError("invalid WebView loader image")
    return manifest


def read_package(path: Path, expected_sha256: str) -> tuple[dict, dict[str, bytes]]:
    if not re.fullmatch(r"[0-9a-fA-F]{64}", expected_sha256) or path.stat().st_size > MAX_PAYLOAD:
        raise ValueError("invalid expected hash or oversized archive")
    data = path.read_bytes()
    if digest(data) != expected_sha256.lower():
        raise ValueError("archive SHA-256 mismatch")
    # Use the same bytes that were hashed, not a file reopened after verification.
    import io
    files = {}
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        if len(entries) > 16 or sum(entry.file_size for entry in entries) > MAX_PAYLOAD:
            raise ValueError("oversized/excessive ZIP payload")
        folded = set()
        for entry in entries:
            name = entry.filename
            if not re.fullmatch(r"[A-Za-z0-9_.-]+", name) or name in (".", ".."):
                raise ValueError("ZIP path traversal/ADS/subdirectory rejected")
            if name.lower() in folded or stat.S_ISLNK(entry.external_attr >> 16):
                raise ValueError("duplicate case-insensitive filename or symlink")
            folded.add(name.lower())
            files[name] = archive.read(entry)
    return validate_files(files), files


def install_package(path: Path, expected: str, destination: Path, dry_run=False) -> Path:
    manifest, files = read_package(path, expected)
    target = destination / f"{manifest['version']}-{expected.lower()[:12]}"
    if dry_run:
        return target
    if target.exists():
        if target.is_symlink() or getattr(target.lstat(), "st_file_attributes", 0) & 0x400:
            raise ValueError("refusing an existing install symlink/junction")
        existing = {p.name: p.read_bytes() for p in target.iterdir() if p.is_file() and not p.is_symlink()}
        if len(existing) != len(list(target.iterdir())) or existing != files:
            raise ValueError("existing install was modified; refusing to overwrite files or user config")
        return target
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".install-", dir=destination) as folder:
        stage = Path(folder) / "payload"
        stage.mkdir()
        for name, data in files.items():
            (stage / name).write_bytes(data)
        stage.rename(target)  # Never overwrite another version or running binary.
    return target


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    capture = commands.add_parser("capture")
    capture.add_argument("--out", type=Path, required=True)
    record = commands.add_parser("record-build")
    record.add_argument("--release", type=Path, required=True)
    record.add_argument("--captured", type=Path, required=True)
    for name in ("portable", "stage-msix"):
        sub = commands.add_parser(name)
        sub.add_argument("--release", type=Path, required=True)
        sub.add_argument("--version")
        sub.add_argument("--require-clean", action="store_true")
        sub.add_argument("--out", type=Path, required=True)
    for name in ("verify", "install"):
        sub = commands.add_parser(name)
        sub.add_argument("--archive", type=Path, required=True)
        sub.add_argument("--sha256", required=True)
        if name == "install":
            sub.add_argument("--destination", type=Path, required=True)
            sub.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    if args.command == "capture":
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_bytes(json_bytes(build_inputs()))
    elif args.command == "record-build":
        record_build(args.release, args.captured)
    elif args.command in ("portable", "stage-msix"):
        receipt = load_receipt(args.release, args.version, args.require_clean)
        if args.command == "stage-msix":
            payload(args.release, args.out, receipt, "msix")
        else:
            args.out.mkdir(parents=True, exist_ok=True)
            version = receipt["inputs"]["version"]
            target = args.out / f"pecofence-{version}-x64.zip"
            with tempfile.TemporaryDirectory(prefix=".package-", dir=args.out) as folder:
                stage, archive = Path(folder) / "payload", Path(folder) / "payload.zip"
                payload(args.release, stage, receipt, "portable")
                write_zip(stage, archive)
                checksum = digest(archive.read_bytes())
                read_package(archive, checksum)
                if receipt["inputs"] != build_inputs():
                    raise ValueError("source/toolchain changed during packaging; output was not published")
                os.replace(archive, target)
            target.with_suffix(".zip.sha256").write_text(f"{checksum}  {target.name}\n", encoding="ascii")
            print(f"packed {target}\nSHA256 {checksum}")
    elif args.command == "verify":
        manifest, _ = read_package(args.archive, args.sha256)
        print(json.dumps(manifest, ensure_ascii=False, indent=2))
    else:
        print(install_package(args.archive, args.sha256, args.destination, args.dry_run))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, zipfile.BadZipFile, subprocess.CalledProcessError) as error:
        raise SystemExit(f"package: {error}") from error

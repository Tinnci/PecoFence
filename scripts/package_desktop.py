"""Verified Windows payloads. Standard library only; never launches packaged binaries."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
from pathlib import PurePosixPath
import re
import stat
import struct
import subprocess
import sys
import tempfile
import tomllib
import zipfile

import winappsdk_runtime as runtime

ROOT = Path(__file__).resolve().parents[1]
EXES = ("pecofence.exe", "pecofence-watchdog.exe")
BASE_FILES = {
    *EXES, "LICENSE", "NOTICE", "README.md", "UPGRADING.md", "THIRD-PARTY-LICENSES.txt",
    "WINDOWS-APP-SDK-LICENSE.txt", "WINDOWS-APP-SDK-RUNTIME.json",
}
MANIFEST = "package.json"
PACKAGE_INPUTS = (
    "LICENSE", "NOTICE", "docs/PORTABLE.md", "docs/UPGRADING.md",
    "third_party/winappsdk/WINDOWS-APP-SDK-LICENSE.txt",
    "third_party/winappsdk/runtime-manifest.json",
    "third_party/winappsdk/app.manifest",
    "third_party/winappsdk/runtime.txt",
)
MAX_ARCHIVE_BYTES = 128 * 1024 * 1024
MAX_UNPACKED_BYTES = 160 * 1024 * 1024
MAX_ARCHIVE_ENTRIES = 512
MAX_MANIFEST_BYTES = 2 * 1024 * 1024
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
    magic = number(optional, "<H")
    if (machine, magic) == (0x8664, 0x20B):
        architecture, image_base_fmt, image_base_offset = "x64", "<Q", 24
        directory_count_offset, directory_offset = 108, 112
    elif (machine, magic) == (0x14C, 0x10B):
        architecture, image_base_fmt, image_base_offset = "x86", "<I", 28
        directory_count_offset, directory_offset = 92, 96
    else:
        raise ValueError("expected an x64 PE32+ or x86 PE32 image")
    if optional_size < directory_offset or optional + optional_size + 40 * sections > len(data):
        raise ValueError("truncated PE headers")
    image_base = number(optional + image_base_offset, image_base_fmt)

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

    static_imports, delay_imports = set(), set()
    count = number(optional + directory_count_offset, "<I")
    for directory, width in ((1, 20), (13, 32)):
        if count <= directory:
            continue
        entry = optional + directory_offset + directory * 8
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
                static_imports.add(name(number(pos + 12, "<I")))
            else:
                attributes, address = number(pos, "<I"), number(pos + 4, "<I")
                delay_imports.add(name(address if attributes & 1 else address - image_base))
        else:
            raise ValueError("unterminated PE import table")
    return {
        "architecture": architecture, "subsystem": number(optional + 68, "<H"),
        "dll": bool(number(pe + 22, "<H") & 0x2000),
        "staticImports": sorted(static_imports), "delayImports": sorted(delay_imports),
        "imports": sorted(static_imports | delay_imports),
    }


def reject_webview_imports(images: dict[str, dict]) -> None:
    imports = {name for image in images.values() for name in image["imports"]}
    webview = sorted(name for name in imports if "webview" in name)
    if webview:
        raise ValueError("WebView imports are not supported: " + ", ".join(webview))


def _runtime_snapshot(directory: Path) -> tuple[dict, dict[str, bytes], dict]:
    manifest = runtime.load_runtime_manifest()
    files, _ = runtime.validate_runtime_directory(directory, manifest, pe_image)
    snapshot = {
        "manifestSha256": digest(runtime.manifest_bytes(manifest)),
        "files": {
            name: {"bytes": len(data), "sha256": digest(data)}
            for name, data in sorted(files.items())
        },
    }
    return manifest, files, snapshot


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
    images = {name: pe_image(data) for name, data in files.items()}
    reject_webview_imports(images)
    for name, image in images.items():
        if image["architecture"] != "x64" or image["dll"] or image["subsystem"] != 2:
            raise ValueError(f"{name} must be an x64 GUI executable")
    _, _, runtime_receipt = _runtime_snapshot(release)
    receipt = {
        "schema": 2, "inputs": inputs,
        "binaries": {name: digest(data) for name, data in files.items()},
        "runtime": runtime_receipt,
    }
    (release / "build-receipt.json").write_bytes(json_bytes(receipt))
    return receipt


def load_receipt(release: Path, version: str | None = None, clean: bool = False, root: Path = ROOT) -> dict:
    path = release / "build-receipt.json"
    if not path.is_file():
        raise ValueError("missing build receipt; run scripts/build-desktop.ps1 before -SkipBuild")
    receipt_bytes = path.read_bytes()
    receipt = json.loads(receipt_bytes.decode("utf-8"))
    if not isinstance(receipt, dict) or set(receipt) != {"schema", "inputs", "binaries", "runtime"} \
            or receipt_bytes != json_bytes(receipt) \
            or receipt.get("schema") != 2 or receipt.get("inputs") != build_inputs(root):
        raise ValueError("stale build receipt; source/version/toolchain changed")
    binaries = receipt.get("binaries")
    if not isinstance(binaries, dict) or set(binaries) != set(EXES) \
            or any(not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value)
                   for value in binaries.values()):
        raise ValueError("invalid executable inventory in build receipt")
    if version and version != receipt["inputs"]["version"]:
        raise ValueError("-Version must equal the compiled workspace version")
    if clean and receipt["inputs"]["dirty"]:
        raise ValueError("release packaging requires a clean source tree")
    for name in EXES:
        if digest((release / name).read_bytes()) != binaries[name]:
            raise ValueError(f"binary changed after its verified build: {name}")
    _, _, runtime_receipt = _runtime_snapshot(release)
    if receipt.get("runtime") != runtime_receipt:
        raise ValueError("Windows App SDK runtime changed after its verified build")
    return receipt


def payload(release: Path, stage: Path, receipt: dict, kind: str, root: Path = ROOT) -> dict:
    if stage.exists():
        raise ValueError("payload staging directory must be new; never overwrite an extracted user's config")
    if kind not in ("portable", "msix"):
        raise ValueError(f"unsupported payload kind: {kind}")
    binaries = {name: (release / name).read_bytes() for name in EXES}
    for name, data in binaries.items():
        if digest(data) != receipt["binaries"][name]:
            raise ValueError(f"binary changed before payload staging: {name}")

    def asset(name):
        data = (root / name).read_bytes()
        if digest(data) != receipt["inputs"]["packageInputs"][name]:
            raise ValueError(f"asset changed after its verified build: {name}")
        return data

    runtime_manifest, runtime_files, runtime_receipt = _runtime_snapshot(release)
    if receipt.get("runtime") != runtime_receipt:
        raise ValueError("Windows App SDK runtime changed before payload staging")
    runtime_data = asset("third_party/winappsdk/runtime-manifest.json")
    if runtime_data != runtime.manifest_bytes(runtime_manifest):
        raise ValueError("runtime manifest is not the canonical audited inventory")
    images = {name: pe_image(data) for name, data in binaries.items()}
    reject_webview_imports(images)
    runtime_records, runtime_images = runtime.validate_manifest(
        runtime_manifest, check_checked_in_files=False
    )
    package_runtime_files = {}
    for name, data in runtime_files.items():
        if name in binaries or name.casefold() in {item.casefold() for item in BASE_FILES}:
            raise ValueError(f"runtime path collides with a package asset: {name}")
        record = runtime_records[name]
        package_runtime_files[name] = {
            "bytes": len(data), "sha256": digest(data), "image": record["image"],
        }
    reject_webview_imports(runtime_images)
    imports = {
        dll for image in (*images.values(), *runtime_images.values()) for dll in image["imports"]
    }
    files = dict(binaries)
    for source, target in [
        ("LICENSE", "LICENSE" if kind == "portable" else "LICENSE.txt"),
        ("NOTICE", "NOTICE"), ("docs/UPGRADING.md", "UPGRADING.md"),
        ("third_party/winappsdk/WINDOWS-APP-SDK-LICENSE.txt", "WINDOWS-APP-SDK-LICENSE.txt"),
    ]:
        files[target] = asset(source)
    if kind == "portable":
        files["README.md"] = asset("docs/PORTABLE.md")
    files["WINDOWS-APP-SDK-RUNTIME.json"] = runtime_data
    files.update(runtime_files)
    stage.mkdir(parents=True)
    for name, data in sorted(files.items()):
        safe_name = runtime.safe_relative(name)
        target = stage.joinpath(*PurePosixPath(safe_name).parts)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    subprocess.run([sys.executable, str(root / "scripts/write-license-notices.py"),
                    str(stage / "THIRD-PARTY-LICENSES.txt")], cwd=root, check=True)
    files["THIRD-PARTY-LICENSES.txt"] = (stage / "THIRD-PARTY-LICENSES.txt").read_bytes()
    runtime_info = {
        "package": runtime_manifest["package"],
        "manifestSha256": digest(runtime_data),
        "licenseSha256": runtime_manifest["license"]["sha256"],
        "activationManifestSha256": runtime_manifest["appManifest"]["sha256"],
        "fileCount": len(package_runtime_files),
        "totalBytes": sum(record["bytes"] for record in package_runtime_files.values()),
        "files": package_runtime_files,
    }
    manifest = {
        "schema": 2, "product": "PecoFence", "kind": kind, "architecture": "x64",
        "version": receipt["inputs"]["version"], "build": receipt["inputs"],
        "runtimeRequirements": {
            "webview2Evergreen": False, "windowsAppSdkSelfContained": True,
            "vcRuntime": any(dll.startswith(("vcruntime", "msvcp")) for dll in imports),
        },
        "runtime": runtime_info,
        "images": images,
        "files": {name: {"bytes": len(data), "sha256": digest(data)} for name, data in sorted(files.items())},
    }
    (stage / MANIFEST).write_bytes(json_bytes(manifest))
    return manifest


def write_zip(stage: Path, target: Path) -> None:
    """Deterministic given payload and compression toolchain, not a compiler reproducibility claim."""
    if not stage.is_dir() or runtime._is_reparse(stage):
        raise ValueError("portable payload staging root must be a regular directory")
    with zipfile.ZipFile(target, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        paths = []
        for current, directories, names in os.walk(stage, topdown=True, followlinks=False):
            base = Path(current)
            for name in list(directories):
                path = base / name
                if path.is_symlink() or getattr(path.lstat(), "st_file_attributes", 0) & runtime.REPARSE_POINT:
                    raise ValueError(f"portable payload contains a symlink/junction: {path}")
            for name in names:
                path = base / name
                if path.is_symlink() or getattr(path.lstat(), "st_file_attributes", 0) & runtime.REPARSE_POINT \
                        or not path.is_file():
                    raise ValueError(f"portable payload contains an unsupported filesystem entry: {path}")
                relative = path.relative_to(stage).as_posix()
                runtime.safe_relative(relative)
                paths.append((relative, path))
        for name, path in sorted(paths):
            entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            entry.create_system = 3
            entry.external_attr = (stat.S_IFREG | 0o644) << 16
            entry.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(entry, path.read_bytes(), compresslevel=9)


def validate_files(files: dict[str, bytes]) -> dict:
    if MANIFEST not in files:
        raise ValueError("missing package manifest")
    if len(files[MANIFEST]) > MAX_MANIFEST_BYTES:
        raise ValueError("package manifest exceeds its 2 MiB safety limit")
    manifest = json.loads(files[MANIFEST].decode("utf-8"))
    if not isinstance(manifest, dict):
        raise ValueError("package manifest must be an object")
    if files[MANIFEST] != json_bytes(manifest):
        raise ValueError("package manifest is not canonical JSON")
    if set(manifest) != {
        "schema", "product", "kind", "architecture", "version", "build",
        "runtimeRequirements", "runtime", "images", "files",
    }:
        raise ValueError("package manifest has unknown or missing fields")
    if (manifest.get("schema"), manifest.get("product"), manifest.get("kind"), manifest.get("architecture")) != (2, "PecoFence", "portable", "x64"):
        raise ValueError("unsupported package identity/format")
    version = manifest.get("version")
    if not isinstance(version, str) or len(version) > 128 or not VERSION.fullmatch(version):
        raise ValueError("invalid package version")
    requirements = manifest.get("runtimeRequirements")
    if not isinstance(requirements, dict) or set(requirements) != {
        "webview2Evergreen", "windowsAppSdkSelfContained", "vcRuntime",
    } \
            or any(type(value) is not bool for value in requirements.values()):
        raise ValueError("invalid runtime requirements")
    if requirements["webview2Evergreen"]:
        raise ValueError("WebView runtime is not supported")
    if not requirements["windowsAppSdkSelfContained"]:
        raise ValueError("package does not declare its Windows App SDK runtime as self-contained")
    build = manifest.get("build")
    if not isinstance(build, dict) or set(build) != {
        "version", "commit", "dirty", "sourceSha256", "cargoLockSha256", "rustc", "packageInputs",
    } or build.get("version") != version \
            or not re.fullmatch(r"[0-9a-f]{40}", str(build.get("commit", ""))) \
            or type(build.get("dirty")) is not bool \
            or any(not re.fullmatch(r"[0-9a-f]{64}", str(build.get(key, "")))
                   for key in ("sourceSha256", "cargoLockSha256")) \
            or not isinstance(build.get("rustc"), str) \
            or not isinstance(build.get("packageInputs"), dict) \
            or set(build["packageInputs"]) != set(PACKAGE_INPUTS) \
            or any(not re.fullmatch(r"[0-9a-f]{64}", str(value))
                   for value in build["packageInputs"].values()):
        raise ValueError("invalid build provenance")
    if any("webview" in name.casefold() for name in files):
        raise ValueError("WebView payload is not supported")
    if "WINDOWS-APP-SDK-RUNTIME.json" not in files:
        raise ValueError("missing pinned Windows App SDK runtime provenance")
    runtime_data = files["WINDOWS-APP-SDK-RUNTIME.json"]
    if len(runtime_data) > MAX_MANIFEST_BYTES:
        raise ValueError("Windows App SDK runtime manifest exceeds its 2 MiB safety limit")
    embedded_runtime = json.loads(runtime_data.decode("utf-8"))
    if runtime_data != runtime.manifest_bytes(embedded_runtime) \
            or embedded_runtime != runtime.load_runtime_manifest():
        raise ValueError("Windows App SDK runtime manifest differs from the pinned inventory")
    runtime_files, runtime_images = runtime.validate_manifest(
        embedded_runtime, check_checked_in_files=False
    )
    runtime_names = set(runtime_files)
    if runtime_names & (BASE_FILES | {MANIFEST}):
        raise ValueError("Windows App SDK runtime path collides with package metadata")
    expected_files = BASE_FILES | runtime_names
    if set(files) != expected_files | {MANIFEST} \
            or not isinstance(manifest.get("files"), dict) \
            or set(manifest["files"]) != expected_files:
        raise ValueError("unexpected/missing payload; configs, keys and extra executables are not allowed")
    for name in expected_files:
        record = manifest["files"].get(name)
        if not isinstance(record, dict) or set(record) != {"bytes", "sha256"} \
                or type(record.get("bytes")) is not int \
                or record != {"bytes": len(files[name]), "sha256": digest(files[name])}:
            raise ValueError(f"payload checksum/size mismatch: {name}")
    runtime_info = manifest.get("runtime")
    expected_runtime_info = {
        "package": embedded_runtime["package"],
        "manifestSha256": digest(runtime_data),
        "licenseSha256": embedded_runtime["license"]["sha256"],
        "activationManifestSha256": embedded_runtime["appManifest"]["sha256"],
        "fileCount": len(runtime_files),
        "totalBytes": sum(record["bytes"] for record in runtime_files.values()),
        "files": {
            name: {
                "bytes": record["bytes"], "sha256": record["sha256"],
                "image": record["image"],
            }
            for name, record in sorted(runtime_files.items())
        },
    }
    if runtime_info != expected_runtime_info:
        raise ValueError("runtime inventory/provenance disagrees with the pinned manifest")
    if digest(files["WINDOWS-APP-SDK-LICENSE.txt"]) != embedded_runtime["license"]["sha256"]:
        raise ValueError("Windows App SDK redistribution license hash mismatch")
    runtime_bytes = 0
    for name, record in runtime_files.items():
        data = files[name]
        runtime_bytes += len(data)
        if len(data) != record["bytes"] or digest(data) != record["sha256"]:
            raise ValueError(f"Windows App SDK runtime checksum/size mismatch: {name}")
        expected_image = record["image"]
        if expected_image is not None and pe_image(data) != expected_image:
            raise ValueError(f"Windows App SDK runtime PE import inventory mismatch: {name}")
    if runtime_bytes != expected_runtime_info["totalBytes"]:
        raise ValueError("Windows App SDK runtime byte total mismatch")
    imports = set()
    images = manifest.get("images")
    if not isinstance(images, dict) or set(images) != set(EXES):
        raise ValueError("invalid executable image inventory")
    for name in EXES:
        image = pe_image(files[name])
        if image["architecture"] != "x64" or image["dll"] or image["subsystem"] != 2 \
                or image != images[name]:
            raise ValueError(f"invalid/mismatched executable image: {name}")
        imports.update(image["imports"])
    reject_webview_imports(images)
    for image in runtime_images.values():
        imports.update(image["imports"])
    reject_webview_imports(runtime_images)
    if any(dll.startswith(("vcruntime", "msvcp")) for dll in imports) != requirements["vcRuntime"]:
        raise ValueError("VC runtime requirement disagrees with packaged image imports")
    return manifest


def read_package(path: Path, expected_sha256: str) -> tuple[dict, dict[str, bytes]]:
    if not isinstance(expected_sha256, str) or not re.fullmatch(r"[0-9a-fA-F]{64}", expected_sha256):
        raise ValueError("invalid expected archive SHA-256")
    with path.open("rb") as stream:
        data = stream.read(MAX_ARCHIVE_BYTES + 1)
    if len(data) > MAX_ARCHIVE_BYTES:
        raise ValueError("portable archive exceeds the 128 MiB compressed-size safety limit")
    if digest(data) != expected_sha256.lower():
        raise ValueError("archive SHA-256 mismatch")
    # Use the same bytes that were hashed, not a file reopened after verification.
    import io
    files = {}
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        if len(entries) > MAX_ARCHIVE_ENTRIES or sum(entry.file_size for entry in entries) > MAX_UNPACKED_BYTES:
            raise ValueError("portable archive exceeds the 512-entry/160 MiB uncompressed safety limits")
        folded = set()
        total = 0
        for entry in entries:
            name = runtime.safe_relative(entry.filename)
            if name != entry.filename or entry.is_dir():
                raise ValueError("ZIP paths must be canonical files without directory entries")
            key = name.casefold()
            if key in folded:
                raise ValueError("duplicate case-insensitive ZIP path")
            folded.add(key)
            mode = entry.external_attr >> 16
            kind = stat.S_IFMT(mode)
            if stat.S_ISLNK(mode):
                raise ValueError("ZIP symlink is not allowed")
            if kind not in (0, stat.S_IFREG) or entry.external_attr & runtime.REPARSE_POINT:
                raise ValueError("ZIP contains a non-regular or reparse-point entry")
            if entry.flag_bits & 0x1:
                raise ValueError("encrypted ZIP entries are not allowed")
            if entry.compress_type != zipfile.ZIP_DEFLATED:
                raise ValueError("only deflated ZIP entries are supported")
            content = archive.read(entry)
            total += len(content)
            if len(content) != entry.file_size or total > MAX_UNPACKED_BYTES:
                raise ValueError("ZIP payload expands beyond its declared safety limit")
            files[name] = content
        paths = set(files)
        prefixes = {}
        for name in paths:
            parts = name.split("/")
            for index in range(1, len(parts) + 1):
                prefix = "/".join(parts[:index])
                key = prefix.casefold()
                previous = prefixes.setdefault(key, prefix)
                if previous != prefix:
                    raise ValueError(f"case-insensitive ZIP path alias: {prefix}")
                if index < len(parts) and key in folded:
                    raise ValueError(f"ZIP file is used as a parent directory: {name}")
    return validate_files(files), files


def _existing_install_files(root: Path) -> tuple[dict[str, bytes], set[str]]:
    if not root.is_dir() or runtime._is_reparse(root):
        raise ValueError("existing install is not a regular directory")
    files: dict[str, bytes] = {}
    directories: set[str] = set()
    folded: set[str] = set()
    prefixes: dict[str, str] = {}
    stack = [root]
    while stack:
        directory = stack.pop()
        for child in directory.iterdir():
            if runtime._is_reparse(child):
                raise ValueError("existing install contains a symlink/junction/reparse point")
            relative = child.relative_to(root).as_posix()
            runtime.safe_relative(relative)
            key = relative.casefold()
            if key in folded:
                raise ValueError("existing install contains a case-insensitive path alias")
            folded.add(key)
            parts = relative.split("/")
            for index in range(1, len(parts) + 1):
                prefix = "/".join(parts[:index])
                prefix_key = prefix.casefold()
                previous = prefixes.setdefault(prefix_key, prefix)
                if previous != prefix:
                    raise ValueError("existing install contains a case-insensitive path alias")
            if child.is_dir():
                directories.add(relative)
                stack.append(child)
            elif child.is_file():
                files[relative] = child.read_bytes()
            else:
                raise ValueError("existing install contains an unsupported filesystem entry")
    file_keys = {name.casefold() for name in files}
    for name in files:
        parts = name.split("/")
        if any("/".join(parts[:index]).casefold() in file_keys for index in range(1, len(parts))):
            raise ValueError("existing install has a file/directory path collision")
    return files, directories


def _expected_directories(files: dict[str, bytes]) -> set[str]:
    return {
        "/".join(parts[:index])
        for name in files
        for parts in (name.split("/"),)
        for index in range(1, len(parts))
    }


def _write_install_tree(root: Path, files: dict[str, bytes]) -> None:
    root.mkdir()
    for name, data in sorted(files.items()):
        safe_name = runtime.safe_relative(name)
        target = root
        for part in PurePosixPath(safe_name).parts[:-1]:
            target = target / part
            try:
                target.mkdir()
            except FileExistsError:
                if not target.is_dir() or runtime._is_reparse(target):
                    raise ValueError(f"unsafe installation directory: {target}")
        target = target / PurePosixPath(safe_name).name
        with target.open("xb") as output:
            output.write(data)


def install_package(path: Path, expected: str, destination: Path, dry_run=False) -> Path:
    manifest, files = read_package(path, expected)
    target = destination / f"{manifest['version']}-{expected.lower()[:12]}"
    if dry_run:
        return target
    runtime._check_parent_chain(destination)
    if os.path.lexists(target):
        runtime._check_parent_chain(target)
        existing, directories = _existing_install_files(target)
        if existing != files or directories != _expected_directories(files):
            raise ValueError("existing install was modified; refusing to overwrite files or user config")
        return target
    destination.mkdir(parents=True, exist_ok=True)
    runtime._check_parent_chain(destination)
    with tempfile.TemporaryDirectory(prefix=".install-", dir=destination) as folder:
        stage = Path(folder) / "payload"
        _write_install_tree(stage, files)
        verified, directories = _existing_install_files(stage)
        if verified != files or directories != _expected_directories(files):
            raise ValueError("staged install did not match the verified package")
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
                checked, checked_files = read_package(archive, checksum)
                if receipt["inputs"] != build_inputs():
                    raise ValueError("source/toolchain changed during packaging; output was not published")
                archive_size = archive.stat().st_size
                uncompressed_size = sum(len(data) for data in checked_files.values())
                os.replace(archive, target)
            target.with_suffix(".zip.sha256").write_text(f"{checksum}  {target.name}\n", encoding="ascii")
            print(
                f"packed {target} ({archive_size:,} ZIP bytes; {uncompressed_size:,} unpacked bytes)\n"
                f"Windows App SDK {runtime.PACKAGE_VERSION}: {checked['runtime']['fileCount']} files, "
                f"{checked['runtime']['totalBytes']:,} bytes\nSHA256 {checksum}"
            )
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

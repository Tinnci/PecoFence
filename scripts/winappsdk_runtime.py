"""Pinned Windows App SDK runtime staging and verification."""
from __future__ import annotations

import base64
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import tempfile
import urllib.request
import zipfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
THIRD_PARTY = ROOT / "third_party" / "winappsdk"
RUNTIME_MANIFEST = THIRD_PARTY / "runtime-manifest.json"
RUNTIME_LIST = THIRD_PARTY / "runtime.txt"
APP_MANIFEST = THIRD_PARTY / "app.manifest"
DEFAULT_CACHE = ROOT / "target" / ".cache" / "windows-app-sdk"

PACKAGE_ID = "Microsoft.WindowsAppSDK.Runtime"
PACKAGE_VERSION = "2.5.1"
PACKAGE_URL = (
    "https://api.nuget.org/v3-flatcontainer/microsoft.windowsappsdk.runtime/2.5.1/"
    "microsoft.windowsappsdk.runtime.2.5.1.nupkg"
)
PACKAGE_REGISTRATION_URL = (
    "https://api.nuget.org/v3/registration5-semver1/"
    "microsoft.windowsappsdk.runtime/2.5.1.json"
)
PACKAGE_SIZE = 169_661_368
PACKAGE_SHA256 = "2fbd27012291be50cafed6942306964281d37167af6a6af3887e1163ec5cf063"
PACKAGE_SHA512_BASE64 = (
    "WOuScR3S99XeYTk7A3uYaZidAvOWjgzFCLvoGbcYf+tTnsbZHqtsZaSRsCF142Kst"
    "njDIrxjktH3NpPZyu0MIA=="
)
MSIX_ENTRY = "tools/MSIX/win10-x64/Microsoft.WindowsAppRuntime.2.msix"
MSIX_SIZE = 48_675_401
MSIX_SHA256 = "c1f5c71cf3a87cfb824c6257f4c1ee8787833b84fcc7dbd18c129786574f11b9"
APPX_MANIFEST_ENTRY = "AppxManifest.xml"
APPX_MANIFEST_SHA256 = "7a2310145ceb52d33ac2366e7fd5960c2377be2b93125310882d6d8395c6737b"
LICENSE_ENTRY = "license.txt"
LICENSE_SHA256 = "5b11e6347756e40fe0274bc08c97f89201b94f0d50181a09a00f1f4740840501"

REACTOR_REPOSITORY = "https://github.com/microsoft/windows-rs"
REACTOR_COMMIT = "2672e615d9cc0771448a781f3b2fe34e7fd08c6a"
REACTOR_RUNTIME_LIST = "crates/libs/reactor-setup/assets/runtime.txt"
REACTOR_RUNTIME_LIST_SHA256 = "26b2ca391f260ffcf00c083747f407a34836ae41abce3af9fa707cea107741a4"
REACTOR_LICENSE_MIT_SHA256 = "c2cfccb812fe482101a8f04597dfc5a9991a6b2748266c47ac91b6a5aae15383"
REACTOR_LICENSE_APACHE_SHA256 = "c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b"
SELF_CONTAINED_MARKER = "windows-reactor-self-contained"
ADDITIONAL_RUNTIME_FILES = (
    {
        "path": "Microsoft.Graphics.Display.dll",
        "reason": (
            "The pinned 2.5.1 MSIX activation manifest registers "
            "Microsoft.Graphics.Display.DisplayInformation from this DLL; "
            "the Reactor runtime.txt list omits the registered file."
        ),
    },
)

MAX_NUPKG_BYTES = 180_000_000
MAX_NUPKG_ENTRIES = 128
MAX_NUPKG_UNPACKED = 512 * 1024 * 1024
MAX_MSIX_BYTES = 60_000_000
MAX_MSIX_ENTRIES = 1024
MAX_MSIX_UNPACKED = 256 * 1024 * 1024
MAX_RUNTIME_FILES = 512
MAX_RUNTIME_BYTES = 128 * 1024 * 1024
MAX_SINGLE_FILE = 64 * 1024 * 1024
REPARSE_POINT = 0x400
ASM = "urn:schemas-microsoft-com:asm.v1"
ASM_V3 = "urn:schemas-microsoft-com:asm.v3"
WINRT_V1 = "urn:schemas-microsoft-com:winrt.v1"
ET.register_namespace("", ASM)
ET.register_namespace("asmv3", ASM_V3)
ET.register_namespace("winrtv1", WINRT_V1)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _normalized_text_bytes(data: bytes) -> bytes:
    return data.replace(b"\r\n", b"\n")


def manifest_bytes(value: dict) -> bytes:
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode("utf-8")


def manifest_digest(value: dict) -> str:
    return digest(manifest_bytes(value))


def fail(message: str):
    raise ValueError(message)


def safe_relative(value: str, *, directory: bool = False) -> str:
    if not isinstance(value, str) or not value or "\\" in value or ":" in value:
        fail(f"unsafe runtime path: {value!r}")
    if value.startswith("/") or "\0" in value:
        fail(f"unsafe runtime path: {value!r}")
    normalized = value[:-1] if directory and value.endswith("/") else value
    if not normalized or normalized.endswith("/") or "//" in normalized:
        fail(f"unsafe runtime path: {value!r}")
    reserved = {"CON", "PRN", "AUX", "NUL"}
    reserved |= {f"COM{index}" for index in range(1, 10)}
    reserved |= {f"LPT{index}" for index in range(1, 10)}
    for part in normalized.split("/"):
        if part in ("", ".", "..") or part.endswith((".", " ")):
            fail(f"unsafe runtime path: {value!r}")
        if not re.fullmatch(r"[A-Za-z0-9_.+()\[\]!%-]+", part):
            fail(f"unsafe runtime path: {value!r}")
        if part.split(".", 1)[0].upper() in reserved:
            fail(f"unsafe Windows device path: {value!r}")
    return normalized


def zip_entries(
    archive: zipfile.ZipFile, *, max_entries: int, max_unpacked: int,
    allow_directories: bool,
) -> dict[str, zipfile.ZipInfo]:
    entries: dict[str, zipfile.ZipInfo] = {}
    folded: set[str] = set()
    files: set[str] = set()
    prefixes: dict[str, str] = {}
    total = 0
    infos = archive.infolist()
    if len(infos) > max_entries:
        fail("archive has too many entries")
    for entry in infos:
        is_dir = entry.is_dir()
        if is_dir and not allow_directories:
            fail("package archive contains an unexpected directory entry")
        name = safe_relative(entry.filename, directory=is_dir)
        if name.casefold() in folded:
            fail(f"case-insensitive duplicate archive path: {entry.filename}")
        folded.add(name.casefold())
        parts = name.split("/")
        for index in range(1, len(parts) + 1):
            prefix = "/".join(parts[:index])
            previous = prefixes.setdefault(prefix.casefold(), prefix)
            if previous != prefix:
                fail(f"case-insensitive archive parent alias: {entry.filename}")
        if entry.flag_bits & 0x1:
            fail(f"encrypted archive entry: {entry.filename}")
        mode = entry.external_attr >> 16
        kind = stat.S_IFMT(mode)
        if entry.external_attr & REPARSE_POINT:
            fail(f"archive reparse point is not allowed: {entry.filename}")
        if stat.S_ISLNK(mode):
            fail(f"archive symlink is not allowed: {entry.filename}")
        if kind not in (0, stat.S_IFREG, stat.S_IFDIR):
            fail(f"unsupported archive file type: {entry.filename}")
        if (is_dir and kind == stat.S_IFREG) or (not is_dir and kind == stat.S_IFDIR):
            fail(f"archive file type mismatch: {entry.filename}")
        if not is_dir:
            if entry.file_size > MAX_SINGLE_FILE:
                fail(f"archive member is too large: {entry.filename}")
            total += entry.file_size
            if total > max_unpacked:
                fail("archive expands beyond the size limit")
            files.add(name.casefold())
        entries[name] = entry
    for name in entries:
        parts = name.split("/")
        if any("/".join(parts[:i]).casefold() in files for i in range(1, len(parts))):
            fail(f"archive file is used as a parent directory: {name}")
    return entries


def read_exact_entry(
    archive: zipfile.ZipFile, entries: dict[str, zipfile.ZipInfo], name: str,
) -> bytes:
    safe_relative(name)
    entry = entries.get(name)
    if entry is None or entry.is_dir() or entry.filename != name:
        fail(f"missing or case-mismatched archive member: {name}")
    return archive.read(entry)


def _package_record() -> dict:
    return {
        "id": PACKAGE_ID, "version": PACKAGE_VERSION, "url": PACKAGE_URL,
        "registrationUrl": PACKAGE_REGISTRATION_URL, "size": PACKAGE_SIZE,
        "sha256": PACKAGE_SHA256, "sha512Base64": PACKAGE_SHA512_BASE64,
    }


def runtime_roots(manifest: dict) -> list[str]:
    upstream = manifest.get("upstreamRuntimeList")
    if not isinstance(upstream, dict) or (
        upstream.get("repository"), upstream.get("commit"), upstream.get("path"),
        upstream.get("sha256"),
    ) != (
        REACTOR_REPOSITORY, REACTOR_COMMIT, REACTOR_RUNTIME_LIST,
        REACTOR_RUNTIME_LIST_SHA256,
    ):
        fail("runtime manifest has invalid Reactor runtime-list provenance")
    data = RUNTIME_LIST.read_bytes()
    if digest(_normalized_text_bytes(data)) != REACTOR_RUNTIME_LIST_SHA256:
        fail("checked-in Reactor runtime list has changed")
    names = [line.strip() for line in data.decode("utf-8").splitlines() if line.strip()]
    additions = manifest.get("additionalRuntimeFiles")
    if additions != list(ADDITIONAL_RUNTIME_FILES):
        fail("runtime manifest has an unreviewed runtime-file addition")
    names.extend(item["path"] for item in additions)
    folded: set[str] = set()
    for name in names:
        safe_relative(name)
        if "/" in name or name.casefold() in folded or "webview" in name.casefold():
            fail(f"invalid or duplicate runtime root: {name}")
        folded.add(name.casefold())
    if manifest.get("runtimeRoots") != names:
        fail("runtime roots differ from the pinned Reactor list and reviewed additions")
    return names


def validate_file_records(manifest: dict) -> tuple[dict[str, dict], dict[str, dict]]:
    records = manifest.get("files")
    if not isinstance(records, list) or not records or len(records) > MAX_RUNTIME_FILES:
        fail("invalid runtime file inventory")
    files: dict[str, dict] = {}
    folded: set[str] = set()
    images: dict[str, dict] = {}
    total = 0
    if manifest.get("msix", {}).get("entry") != MSIX_ENTRY:
        fail("runtime manifest has an unexpected MSIX package path")
    for record in records:
        if not isinstance(record, dict):
            fail("invalid runtime file record")
        path = safe_relative(record.get("path"))
        source = safe_relative(record.get("source"))
        if path.casefold() in folded or "webview" in path.casefold():
            fail(f"invalid or duplicate runtime file path: {path}")
        if source != f"{MSIX_ENTRY}!/{path}":
            fail(f"runtime file source does not match its package path: {path}")
        folded.add(path.casefold())
        size, sha = record.get("bytes"), record.get("sha256")
        if type(size) is not int or not 0 < size <= MAX_SINGLE_FILE:
            fail(f"invalid runtime file size: {path}")
        if not isinstance(sha, str) or not re.fullmatch(r"[0-9a-f]{64}", sha):
            fail(f"invalid runtime file hash: {path}")
        total += size
        if total > MAX_RUNTIME_BYTES:
            fail("runtime inventory exceeds the size limit")
        image = record.get("image")
        if image is not None:
            expected = {
                "architecture", "subsystem", "dll", "staticImports", "delayImports", "imports",
            }
            if not isinstance(image, dict) or set(image) != expected:
                fail(f"invalid PE import record: {path}")
            if image["architecture"] not in ("x64", "x86") or type(image["dll"]) is not bool \
                    or type(image["subsystem"]) is not int:
                fail(f"invalid PE image metadata: {path}")
            for key in ("staticImports", "delayImports", "imports"):
                values = image[key]
                if not isinstance(values, list) or any(
                    not isinstance(value, str)
                    or not re.fullmatch(r"[a-z0-9_.-]+\.dll", value)
                    for value in values
                ) or values != sorted(set(values)):
                    fail(f"invalid PE import names: {path}")
            if image["imports"] != sorted(set(image["staticImports"]) | set(image["delayImports"])):
                fail(f"PE import set is inconsistent: {path}")
            if any("webview" in value.casefold() for value in image["imports"]):
                fail(f"WebView import is not allowed in runtime image: {path}")
            images[path] = image
        elif path.lower().endswith((".dll", ".mui")):
            fail(f"runtime PE image is not inventoried: {path}")
        files[path] = record
    if list(files) != sorted(files, key=str.casefold):
        fail("runtime file inventory is not in canonical path order")
    roots = {name.casefold() for name in manifest.get("runtimeRoots", [])}
    if {path.split("/", 1)[0].casefold() for path in files} - roots:
        fail("runtime inventory contains a path outside the strict root allowlist")
    return files, images


def validate_activation_manifest(data: bytes, files: dict[str, dict]) -> dict[str, int]:
    if b"webview" in data.lower() or SELF_CONTAINED_MARKER.encode() not in data:
        fail("self-contained activation manifest has an invalid marker or WebView reference")
    try:
        root = ET.fromstring(data)
    except ET.ParseError as error:
        raise ValueError(f"invalid self-contained activation manifest: {error}") from error
    if root.tag != f"{{{ASM}}}assembly":
        fail("self-contained activation manifest has an unexpected XML root")
    if [node.text for node in root if node.tag == f"{{{ASM}}}description"] != [SELF_CONTAINED_MARKER]:
        fail("self-contained activation marker is missing or duplicated")
    runtime_names = {path.casefold() for path in files}
    activation_map: dict[str, int] = {}
    for file_node in root.iter(f"{{{ASM_V3}}}file"):
        name = safe_relative(file_node.attrib.get("name", ""))
        if "/" in name or name.casefold() not in runtime_names or "webview" in name.casefold():
            fail(f"activation manifest references an unstaged runtime file: {name}")
        count = 0
        for item in file_node:
            if item.tag == f"{{{WINRT_V1}}}activatableClass":
                if not item.attrib.get("name") or item.attrib.get("threadingModel") not in (
                    "both", "sta", "mta",
                ):
                    fail(f"invalid activation factory in checked-in manifest: {name}")
                count += 1
        if count:
            activation_map[name] = count
    required = {
        "coremessagingxp.dll", "dcompi.dll", "microsoft.ui.dll",
        "microsoft.ui.input.dll", "microsoft.ui.windowing.dll",
        "microsoft.ui.windowing.core.dll", "microsoft.ui.xaml.dll",
        "microsoft.ui.xaml.controls.dll",
    }
    if not required <= {name.casefold() for name in activation_map}:
        fail("checked-in activation manifest omits required WinUI factory groups")
    return activation_map


def validate_manifest(
    manifest: dict, *, check_checked_in_files: bool = False
) -> tuple[dict[str, dict], dict[str, dict]]:
    if not isinstance(manifest, dict) or manifest.get("schemaVersion") != 1:
        fail("unsupported Windows App SDK runtime manifest")
    if manifest.get("package") != _package_record():
        fail("runtime manifest does not match the pinned public NuGet package")
    runtime_roots(manifest)
    if manifest.get("msix") != {
        "entry": MSIX_ENTRY, "bytes": MSIX_SIZE, "sha256": MSIX_SHA256,
        "appxManifestEntry": APPX_MANIFEST_ENTRY,
        "appxManifestSha256": APPX_MANIFEST_SHA256,
    }:
        fail("runtime manifest has an unexpected x64 MSIX source")
    if manifest.get("license") != {
        "packageEntry": LICENSE_ENTRY,
        "path": "third_party/winappsdk/WINDOWS-APP-SDK-LICENSE.txt",
        "sha256": LICENSE_SHA256,
    }:
        fail("runtime manifest has an unexpected Microsoft license source")
    activation = manifest.get("appManifest")
    if (
        not isinstance(activation, dict)
        or activation.get("path") != "third_party/winappsdk/app.manifest"
        or activation.get("sourceAppxManifestSha256") != APPX_MANIFEST_SHA256
        or activation.get("selfContainedMarker") != SELF_CONTAINED_MARKER
        or not isinstance(activation.get("sha256"), str)
        or not re.fullmatch(r"[0-9a-f]{64}", activation["sha256"])
    ):
        fail("runtime manifest has invalid self-contained activation-manifest provenance")
    files, images = validate_file_records(manifest)
    if check_checked_in_files:
        for record in (manifest["license"], activation):
            path = ROOT / record["path"]
            data = path.read_bytes() if path.is_file() else b""
            # Git line-ending conversion does not change the generated XML.
            # Package payloads and the extracted license remain byte-exact.
            if record is activation:
                data = _normalized_text_bytes(data)
            if not data or digest(data) != record["sha256"]:
                fail(f"checked-in runtime provenance file changed: {record['path']}")
        checked_activation = validate_activation_manifest(APP_MANIFEST.read_bytes(), files)
        if checked_activation != activation.get("files"):
            fail("activation-manifest factory inventory differs from runtime provenance")
    return files, images


def load_runtime_manifest(path: Path = RUNTIME_MANIFEST) -> dict:
    try:
        data = path.read_bytes()
        manifest = json.loads(data.decode("utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot load pinned Windows App SDK runtime manifest: {error}") from error
    files, _ = validate_manifest(manifest, check_checked_in_files=False)
    if path.resolve() == RUNTIME_MANIFEST.resolve():
        validate_manifest(manifest, check_checked_in_files=True)
        if digest(data) != manifest_digest(manifest):
            fail("runtime-manifest JSON is not canonical")
    return manifest


def _create_activation_manifest(
    appx_bytes: bytes, runtime_paths: set[str],
) -> tuple[bytes, dict[str, int]]:
    try:
        source = ET.fromstring(appx_bytes)
    except ET.ParseError as error:
        raise ValueError(f"invalid Windows App SDK package activation manifest: {error}") from error
    groups: dict[str, list[ET.Element]] = {}
    proxies: dict[str, ET.Element] = {}
    for extension in source.iter():
        if extension.tag.rsplit("}", 1)[-1] != "Extension":
            continue
        category = extension.attrib.get("Category")
        if category == "windows.activatableClass.inProcessServer":
            server = next(
                (x for x in extension if x.tag.rsplit("}", 1)[-1] == "InProcessServer"),
                None,
            )
            if server is None:
                fail("MSIX activation server is missing InProcessServer")
            node = next((x for x in server if x.tag.rsplit("}", 1)[-1] == "Path"), None)
            path = safe_relative((node.text or "").strip() if node is not None else "")
            if "/" in path:
                fail(f"unexpected nested activation DLL path: {path}")
            classes = [
                x for x in server
                if x.tag.rsplit("}", 1)[-1] == "ActivatableClass"
                and "webview" not in x.attrib.get("ActivatableClassId", "").casefold()
            ]
            if path.casefold() in runtime_paths:
                if not classes:
                    fail(f"empty activation class list for runtime DLL: {path}")
                groups.setdefault(path, []).extend(classes)
        elif category == "windows.activatableClass.proxyStub":
            proxy = next(
                (x for x in extension if x.tag.rsplit("}", 1)[-1] == "ProxyStub"),
                None,
            )
            if proxy is None:
                fail("MSIX proxy-stub registration is missing ProxyStub")
            node = next((x for x in proxy if x.tag.rsplit("}", 1)[-1] == "Path"), None)
            path = safe_relative((node.text or "").strip() if node is not None else "")
            if "/" in path:
                fail(f"unexpected nested proxy-stub DLL path: {path}")
            if path.casefold() in runtime_paths:
                proxies[path] = proxy

    required = {
        "coremessagingxp.dll", "dcompi.dll", "microsoft.ui.dll",
        "microsoft.ui.input.dll", "microsoft.ui.windowing.dll",
        "microsoft.ui.windowing.core.dll", "microsoft.ui.xaml.dll",
        "microsoft.ui.xaml.controls.dll",
    }
    if not required <= {name.casefold() for name in groups}:
        fail("activation manifest omits required WinUI runtime factory groups")

    assembly = ET.Element(f"{{{ASM}}}assembly", {"manifestVersion": "1.0"})
    ET.SubElement(assembly, f"{{{ASM}}}description").text = SELF_CONTAINED_MARKER
    activation_map: dict[str, int] = {}
    grouped = {name.casefold(): (name, classes) for name, classes in groups.items()}
    proxied = {name.casefold(): (name, proxy) for name, proxy in proxies.items()}
    for key in sorted(set(grouped) | set(proxied)):
        name = grouped.get(key, proxied.get(key))[0]
        file_node = ET.SubElement(assembly, f"{{{ASM_V3}}}file", {"name": name})
        if key in grouped:
            _, classes = grouped[key]
            seen: set[str] = set()
            for item in classes:
                class_id = item.attrib.get("ActivatableClassId", "")
                threading = item.attrib.get("ThreadingModel", "")
                if not class_id or threading not in ("both", "sta", "mta"):
                    fail(f"invalid activation class metadata for {name}")
                if class_id.casefold() in seen:
                    fail(f"duplicate activation class in package manifest: {class_id}")
                seen.add(class_id.casefold())
                ET.SubElement(file_node, f"{{{WINRT_V1}}}activatableClass", {
                    "name": class_id, "threadingModel": threading,
                })
            activation_map[name] = len(classes)
        if key in proxied:
            _, proxy = proxied[key]
            class_id = proxy.attrib.get("ClassId", "")
            if not re.fullmatch(r"[0-9a-fA-F-]{36}", class_id):
                fail(f"invalid proxy-stub class identifier for {name}")
            ET.SubElement(file_node, f"{{{ASM_V3}}}comClass", {
                "clsid": "{" + class_id.upper() + "}",
            })
            for interface in proxy:
                if interface.tag.rsplit("}", 1)[-1] != "Interface":
                    continue
                iid = interface.attrib.get("InterfaceId", "")
                interface_name = interface.attrib.get("Name", "")
                if not interface_name or not re.fullmatch(r"[0-9a-fA-F-]{36}", iid):
                    fail(f"invalid proxy interface metadata for {name}")
                ET.SubElement(file_node, f"{{{ASM_V3}}}comInterfaceProxyStub", {
                    "name": interface_name, "iid": "{" + iid.upper() + "}",
                })
    ET.indent(assembly, space="    ")
    body = ET.tostring(assembly, encoding="utf-8", xml_declaration=True)
    declaration, rest = body.split(b"?>", 1)
    provenance = (
        "<!-- Derived from Microsoft.WindowsAppSDK.Runtime 2.5.1 "
        f"(NuGet SHA-256 {PACKAGE_SHA256}); activation data from its "
        f"AppxManifest.xml (SHA-256 {APPX_MANIFEST_SHA256}), filtered to the "
        "checked-in x64 runtime inventory. -->\n"
    ).encode("ascii")
    return declaration + b"?>\n" + provenance + rest, activation_map


def build_candidate(package_bytes: bytes, parse_pe) -> tuple[dict, bytes, bytes]:
    if len(package_bytes) != PACKAGE_SIZE or digest(package_bytes) != PACKAGE_SHA256:
        fail("NuGet runtime package SHA-256 mismatch")
    if base64.b64encode(hashlib.sha512(package_bytes).digest()).decode("ascii") != PACKAGE_SHA512_BASE64:
        fail("NuGet runtime package SHA-512 mismatch")
    runtime_list = RUNTIME_LIST.read_bytes()
    if digest(_normalized_text_bytes(runtime_list)) != REACTOR_RUNTIME_LIST_SHA256:
        fail("upstream Reactor runtime list SHA-256 mismatch")
    roots = [line.strip() for line in runtime_list.decode("utf-8").splitlines() if line.strip()]
    roots.extend(item["path"] for item in ADDITIONAL_RUNTIME_FILES)
    with zipfile.ZipFile(io.BytesIO(package_bytes)) as package:
        package_entries = zip_entries(
            package, max_entries=MAX_NUPKG_ENTRIES,
            max_unpacked=MAX_NUPKG_UNPACKED, allow_directories=True,
        )
        msix_data = read_exact_entry(package, package_entries, MSIX_ENTRY)
        license_data = read_exact_entry(package, package_entries, LICENSE_ENTRY)
    if len(msix_data) != MSIX_SIZE or digest(msix_data) != MSIX_SHA256:
        fail("pinned Windows App SDK MSIX hash mismatch")
    if digest(license_data) != LICENSE_SHA256:
        fail("pinned Windows App SDK license hash mismatch")
    with zipfile.ZipFile(io.BytesIO(msix_data)) as msix:
        msix_entries = zip_entries(
            msix, max_entries=MAX_MSIX_ENTRIES,
            max_unpacked=MAX_MSIX_UNPACKED, allow_directories=True,
        )
        appx_data = read_exact_entry(msix, msix_entries, APPX_MANIFEST_ENTRY)
        if digest(appx_data) != APPX_MANIFEST_SHA256:
            fail("pinned AppxManifest.xml hash mismatch")
        all_files = {name: item for name, item in msix_entries.items() if not item.is_dir()}
        folded = {name.casefold(): name for name in all_files}
        selected: dict[str, bytes] = {}
        for root in roots:
            key = root.casefold()
            direct = folded.get(key)
            children = [name for name in all_files if name.casefold().startswith(key + "/")]
            if direct is not None and children:
                fail(f"runtime root is both a file and directory: {root}")
            if direct is not None:
                selected[direct] = msix.read(all_files[direct])
            elif children:
                for name in children:
                    selected[name] = msix.read(all_files[name])
            else:
                fail(f"required runtime root is absent from pinned MSIX: {root}")
    if len(selected) > MAX_RUNTIME_FILES or sum(map(len, selected.values())) > MAX_RUNTIME_BYTES:
        fail("selected Windows App SDK runtime exceeds the size limit")
    if any("webview" in path.casefold() for path in selected):
        fail("WebView payload is not allowed in runtime inventory")
    app_manifest, activation_map = _create_activation_manifest(
        appx_data, {path.casefold() for path in selected}
    )
    records = []
    for path, data in sorted(selected.items(), key=lambda item: item[0].casefold()):
        image = _inspect_runtime_pe(path, data, parse_pe)
        records.append({
            "path": path, "source": f"{MSIX_ENTRY}!/{path}",
            "bytes": len(data), "sha256": digest(data), "image": image,
        })
    manifest = {
        "schemaVersion": 1,
        "package": _package_record(),
        "upstreamRuntimeList": {
            "repository": REACTOR_REPOSITORY,
            "commit": REACTOR_COMMIT,
            "path": REACTOR_RUNTIME_LIST,
            "sha256": REACTOR_RUNTIME_LIST_SHA256,
            "license": "MIT OR Apache-2.0",
        },
        "runtimeRoots": roots,
        "additionalRuntimeFiles": list(ADDITIONAL_RUNTIME_FILES),
        "msix": {
            "entry": MSIX_ENTRY, "bytes": MSIX_SIZE, "sha256": MSIX_SHA256,
            "appxManifestEntry": APPX_MANIFEST_ENTRY,
            "appxManifestSha256": APPX_MANIFEST_SHA256,
        },
        "license": {
            "packageEntry": LICENSE_ENTRY,
            "path": "third_party/winappsdk/WINDOWS-APP-SDK-LICENSE.txt",
            "sha256": LICENSE_SHA256,
        },
        "appManifest": {
            "path": "third_party/winappsdk/app.manifest",
            "sha256": digest(app_manifest),
            "sourceAppxManifestSha256": APPX_MANIFEST_SHA256,
            "files": activation_map,
            "selfContainedMarker": SELF_CONTAINED_MARKER,
        },
        "files": records,
    }
    return manifest, app_manifest, license_data


def _inspect_runtime_pe(path: str, data: bytes, parse_pe) -> dict | None:
    suffix = PurePosixPath(path).suffix.lower()
    if data[:2] != b"MZ":
        if suffix in (".dll", ".mui"):
            fail(f"runtime PE image is malformed: {path}")
        return None
    image = parse_pe(data)
    if "webview" in path.casefold() or any(
        "webview" in name.casefold() for name in image["imports"]
    ):
        fail(f"WebView runtime image is not allowed: {path}")
    if suffix == ".dll" and image["architecture"] != "x64":
        fail(f"non-x64 runtime DLL is not allowed: {path}")
    if suffix == ".mui":
        if image["architecture"] not in ("x64", "x86") or image["imports"]:
            fail(f"unexpected executable/importing MUI resource: {path}")
    elif suffix != ".dll":
        fail(f"unexpected PE image in runtime inventory: {path}")
    return image


def extract_runtime_files(
    package_bytes: bytes, manifest: dict, parse_pe,
) -> tuple[dict[str, bytes], dict[str, dict]]:
    expected, expected_images = validate_file_records(manifest)
    if (
        len(package_bytes) != PACKAGE_SIZE
        or digest(package_bytes) != PACKAGE_SHA256
        or base64.b64encode(hashlib.sha512(package_bytes).digest()).decode("ascii")
        != PACKAGE_SHA512_BASE64
    ):
        fail("NuGet runtime package failed its pinned SHA-256/SHA-512 digest")
    with zipfile.ZipFile(io.BytesIO(package_bytes)) as package:
        package_entries = zip_entries(
            package, max_entries=MAX_NUPKG_ENTRIES,
            max_unpacked=MAX_NUPKG_UNPACKED, allow_directories=True,
        )
        msix_data = read_exact_entry(package, package_entries, MSIX_ENTRY)
        license_data = read_exact_entry(package, package_entries, LICENSE_ENTRY)
    if len(msix_data) != MSIX_SIZE or len(msix_data) > MAX_MSIX_BYTES \
            or digest(msix_data) != MSIX_SHA256:
        fail("pinned Windows App SDK MSIX hash/size mismatch")
    if digest(license_data) != LICENSE_SHA256:
        fail("pinned Windows App SDK license hash mismatch")
    with zipfile.ZipFile(io.BytesIO(msix_data)) as msix:
        entries = zip_entries(
            msix, max_entries=MAX_MSIX_ENTRIES,
            max_unpacked=MAX_MSIX_UNPACKED, allow_directories=True,
        )
        appx_data = read_exact_entry(msix, entries, APPX_MANIFEST_ENTRY)
        if digest(appx_data) != APPX_MANIFEST_SHA256:
            fail("pinned Windows App SDK AppxManifest.xml hash mismatch")
        all_files = {name: entry for name, entry in entries.items() if not entry.is_dir()}
        folded = {name.casefold(): name for name in all_files}
        selected: dict[str, bytes] = {}
        for root in runtime_roots(manifest):
            key = root.casefold()
            direct = folded.get(key)
            children = [name for name in all_files if name.casefold().startswith(key + "/")]
            if direct is not None and children:
                fail(f"runtime root is both a file and directory: {root}")
            if direct is not None:
                selected[direct] = msix.read(all_files[direct])
            elif children:
                for name in children:
                    selected[name] = msix.read(all_files[name])
            else:
                fail(f"required runtime root is absent from pinned MSIX: {root}")
    if len(selected) > MAX_RUNTIME_FILES or sum(map(len, selected.values())) > MAX_RUNTIME_BYTES:
        fail("selected Windows App SDK runtime exceeds the size limit")
    if set(selected) != set(expected):
        missing = sorted(set(expected) - set(selected), key=str.casefold)
        extra = sorted(set(selected) - set(expected), key=str.casefold)
        fail(f"runtime inventory differs from pinned package (missing={missing}, extra={extra})")
    actual_images: dict[str, dict] = {}
    for path, data in selected.items():
        record = expected[path]
        if len(data) != record["bytes"] or digest(data) != record["sha256"]:
            fail(f"runtime file hash/size mismatch: {path}")
        image = _inspect_runtime_pe(path, data, parse_pe)
        if image != record["image"]:
            fail(f"runtime PE import inventory changed: {path}")
        if image is not None:
            actual_images[path] = image
    if actual_images != expected_images:
        fail("runtime PE image inventory differs from its per-file manifest")
    if any("webview" in path.casefold() for path in selected):
        fail("WebView payload is not allowed in runtime inventory")
    return selected, actual_images


def _is_reparse(path: Path) -> bool:
    try:
        info = path.lstat()
    except FileNotFoundError:
        return False
    return stat.S_ISLNK(info.st_mode) or bool(
        getattr(info, "st_file_attributes", 0) & REPARSE_POINT
    )


def _check_parent_chain(path: Path) -> None:
    absolute = Path(os.path.abspath(path))
    for item in reversed((absolute, *absolute.parents)):
        if item.exists() and _is_reparse(item):
            fail(f"runtime destination traverses a symlink/junction: {item}")


def _tree_files(root: Path, roots: list[str]) -> dict[str, Path]:
    if not root.is_dir() or _is_reparse(root):
        fail(f"runtime directory is missing or is a symlink/junction: {root}")
    found: dict[str, Path] = {}
    for name in roots:
        matches = [item for item in root.iterdir() if item.name.casefold() == name.casefold()]
        if len(matches) > 1:
            fail(f"case-insensitive runtime root collision: {name}")
        if not matches:
            continue
        stack = [matches[0]]
        while stack:
            current = stack.pop()
            if _is_reparse(current):
                fail(f"runtime symlink/junction is not allowed: {current}")
            if current.is_file():
                found[current.relative_to(root).as_posix()] = current
                continue
            if not current.is_dir():
                fail(f"unsupported runtime filesystem entry: {current}")
            for child in current.iterdir():
                if _is_reparse(child):
                    fail(f"runtime symlink/junction is not allowed: {child}")
                if child.is_dir():
                    stack.append(child)
                elif child.is_file():
                    found[child.relative_to(root).as_posix()] = child
                else:
                    fail(f"unsupported runtime filesystem entry: {child}")
    folded: set[str] = set()
    for name in found:
        safe_relative(name)
        if name.casefold() in folded:
            fail(f"case-insensitive runtime path collision: {name}")
        folded.add(name.casefold())
    return found


def validate_runtime_directory(
    directory: Path, manifest: dict | None = None, parse_pe=None,
) -> tuple[dict[str, bytes], dict[str, dict]]:
    manifest = manifest or load_runtime_manifest()
    expected, expected_images = validate_file_records(manifest)
    if parse_pe is None:
        from package_desktop import pe_image
        parse_pe = pe_image
    root = Path(directory)
    _check_parent_chain(root)
    found = _tree_files(root, runtime_roots(manifest))
    if {name.casefold() for name in found} != {name.casefold() for name in expected}:
        missing = sorted(set(expected) - set(found), key=str.casefold)
        extra = sorted(
            (name for name in found if name.casefold() not in {item.casefold() for item in expected}),
            key=str.casefold,
        )
        fail(f"missing/extra staged runtime file (missing={missing}, extra={extra})")
    staged: dict[str, bytes] = {}
    actual_images: dict[str, dict] = {}
    for name, record in expected.items():
        actual = next(item for item in found if item.casefold() == name.casefold())
        data = found[actual].read_bytes()
        if len(data) != record["bytes"] or digest(data) != record["sha256"]:
            fail(f"staged runtime file hash/size mismatch: {actual}")
        image = _inspect_runtime_pe(actual, data, parse_pe)
        if image != record["image"]:
            fail(f"staged runtime PE import inventory changed: {actual}")
        if image is not None:
            actual_images[name] = image
        staged[name] = data
    if actual_images != expected_images:
        fail("staged runtime PE inventory does not match runtime manifest")
    return staged, actual_images


def _stage_files(directory: Path, runtime_files: dict[str, bytes], manifest: dict) -> None:
    root = Path(directory)
    _check_parent_chain(root)
    root.mkdir(parents=True, exist_ok=True)
    if _is_reparse(root) or not root.is_dir():
        fail(f"runtime destination is not a regular directory: {root}")
    records = {record["path"]: record for record in manifest["files"]}
    expected = {name.casefold() for name in records}
    existing = _tree_files(root, runtime_roots(manifest))
    for name, path in existing.items():
        match = next((item for item in records if item.casefold() == name.casefold()), None)
        if match is None:
            fail(f"unexpected file in a Windows App SDK runtime directory: {name}")
        data = path.read_bytes()
        record = records[match]
        if len(data) != record["bytes"] or digest(data) != record["sha256"]:
            fail(f"refusing to overwrite a modified runtime file: {name}")
    for name, data in runtime_files.items():
        if name.casefold() not in expected:
            fail(f"runtime stage attempted an unmanifested file: {name}")
        target = root.joinpath(*PurePosixPath(name).parts)
        for parent in (target, *target.parents):
            if parent == root.parent:
                break
            if parent.exists() and _is_reparse(parent):
                fail(f"runtime destination traverses a symlink/junction: {parent}")
        if target.exists():
            if not target.is_file() or _is_reparse(target):
                fail(f"runtime destination is not a regular file: {target}")
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        try:
            with target.open("xb") as output:
                output.write(data)
        except FileExistsError:
            if not target.is_file() or _is_reparse(target) or digest(target.read_bytes()) != digest(data):
                fail(f"runtime destination changed during staging: {target}")
    validate_runtime_directory(root, manifest)


def _verified_package(cache_dir: Path, manifest: dict) -> bytes:
    if manifest.get("package") != _package_record():
        fail("runtime manifest does not match the pinned public NuGet package")
    cache_dir = Path(cache_dir)
    _check_parent_chain(cache_dir)
    cache_dir.mkdir(parents=True, exist_ok=True)
    if _is_reparse(cache_dir):
        fail(f"NuGet cache directory is a symlink/junction: {cache_dir}")
    archive = cache_dir / f"{PACKAGE_ID}.{PACKAGE_VERSION}.nupkg"
    if archive.exists():
        if _is_reparse(archive) or not archive.is_file():
            fail(f"cached NuGet archive is not a regular file: {archive}")
        data = archive.read_bytes()
        if (
            len(data) != PACKAGE_SIZE or digest(data) != PACKAGE_SHA256
            or base64.b64encode(hashlib.sha512(data).digest()).decode("ascii") != PACKAGE_SHA512_BASE64
        ):
            fail(f"cached NuGet package failed its pinned digest: {archive}")
        return data
    request = urllib.request.Request(
        PACKAGE_URL, headers={"User-Agent": "PecoFence pinned Windows App SDK runtime staging"}
    )
    temporary = None
    try:
        with urllib.request.urlopen(request, timeout=120) as response:
            length = response.headers.get("Content-Length")
            if length is not None and int(length) != PACKAGE_SIZE:
                fail("NuGet server returned an unexpected package size")
            with tempfile.NamedTemporaryFile(
                mode="wb", prefix=".windows-app-sdk-", suffix=".download",
                dir=cache_dir, delete=False,
            ) as output:
                temporary = Path(output.name)
                sha256, sha512, size = hashlib.sha256(), hashlib.sha512(), 0
                while chunk := response.read(1024 * 1024):
                    size += len(chunk)
                    if size > PACKAGE_SIZE or size > MAX_NUPKG_BYTES:
                        fail("NuGet download exceeded the pinned package size")
                    sha256.update(chunk)
                    sha512.update(chunk)
                    output.write(chunk)
        if size != PACKAGE_SIZE or sha256.hexdigest() != PACKAGE_SHA256:
            fail("downloaded NuGet package SHA-256 mismatch")
        if base64.b64encode(sha512.digest()).decode("ascii") != PACKAGE_SHA512_BASE64:
            fail("downloaded NuGet package SHA-512 mismatch")
        os.replace(temporary, archive)
        return archive.read_bytes()
    finally:
        if temporary is not None and temporary.exists():
            temporary.unlink()


def stage_runtime(
    destinations: list[Path], *, cache_dir: Path = DEFAULT_CACHE,
    manifest: dict | None = None, parse_pe=None,
) -> dict[str, dict]:
    manifest = manifest or load_runtime_manifest()
    if parse_pe is None:
        from package_desktop import pe_image
        parse_pe = pe_image
    package = _verified_package(Path(cache_dir), manifest)
    files, images = extract_runtime_files(package, manifest, parse_pe)
    for destination in destinations:
        _stage_files(Path(destination), files, manifest)
    return images


def copy_runtime(
    source: Path, destination: Path, *, manifest: dict | None = None, parse_pe=None,
) -> None:
    manifest = manifest or load_runtime_manifest()
    files, _ = validate_runtime_directory(source, manifest, parse_pe)
    _stage_files(Path(destination), files, manifest)


def runtime_summary(manifest: dict | None = None) -> dict:
    manifest = manifest or load_runtime_manifest()
    validate_manifest(manifest)
    return {
        "package_id": PACKAGE_ID,
        "version": PACKAGE_VERSION,
        "package_sha256": PACKAGE_SHA256,
        "package_sha512": PACKAGE_SHA512_BASE64,
        "runtime_manifest_sha256": manifest_digest(manifest),
        "runtime_sha256": {
            record["path"]: record["sha256"] for record in manifest["files"]
        },
    }

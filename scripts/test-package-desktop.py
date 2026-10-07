"""Pure packaging/install tests with synthetic PE files; no desktop or machine installation."""
import json
from pathlib import Path
from pathlib import PurePosixPath
import stat
import struct
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import package_desktop as pkg


def image(web=False, dll=False, delay=False, architecture="x64"):
    data = bytearray(1024)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    machine, magic = (0x8664, 0x20B) if architecture == "x64" else (0x14C, 0x10B)
    struct.pack_into("<HH", data, 0x84, machine, 1)
    struct.pack_into("<HH", data, 0x94, 240, 0x2000 if dll else 0)
    optional = 0x98
    struct.pack_into("<H", data, optional, magic)
    if architecture == "x64":
        struct.pack_into("<Q", data, optional + 24, 0x140000000)
        directory_count, directory_offset = 108, 112
    else:
        struct.pack_into("<I", data, optional + 28, 0x400000)
        directory_count, directory_offset = 92, 96
    struct.pack_into("<H", data, optional + 68, 2)
    struct.pack_into("<I", data, optional + directory_count, 16)
    section = optional + 240
    struct.pack_into("<IIII", data, section + 8, 512, 0x1000, 512, 512)
    if web:
        directory, width = (13, 32) if delay else (1, 20)
        struct.pack_into("<II", data, optional + directory_offset + directory * 8, 0x1000, width * 2)
        if delay:
            struct.pack_into("<II", data, 512, 1, 0x1080)
        else:
            struct.pack_into("<I", data, 512 + 12, 0x1080)
        data[640:659] = b"WebView2Loader.dll\0"
    return bytes(data)


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.release = self.root / "release"
        self.release.mkdir()
        self.inputs = {
            "version": "0.0.3", "commit": "a" * 40, "dirty": False,
            "sourceSha256": "b" * 64, "cargoLockSha256": "c" * 64, "rustc": "rustc test",
        }
        (self.root / "docs").mkdir()
        (self.root / "scripts").mkdir()
        sdk_dir = self.root / "third_party/winappsdk"
        sdk_dir.mkdir(parents=True)
        for name in (
            "LICENSE", "NOTICE", "docs/PORTABLE.md", "docs/UPGRADING.md",
            "third_party/winappsdk/WINDOWS-APP-SDK-LICENSE.txt",
            "third_party/winappsdk/app.manifest", "third_party/winappsdk/runtime.txt",
        ):
            (self.root / name).write_text(name, encoding="utf-8")
        runtime_one = image(dll=True)
        runtime_two = image(dll=True, architecture="x86")
        self.runtime_manifest = {
            "package": {"id": "Microsoft.WindowsAppSDK.Runtime", "version": "2.5.1"},
            "license": {"sha256": pkg.digest((self.root / "third_party/winappsdk/WINDOWS-APP-SDK-LICENSE.txt").read_bytes())},
            "appManifest": {"sha256": "d" * 64},
            "files": [
                {"path": "Microsoft.UI.Xaml.dll", "bytes": len(runtime_one),
                 "sha256": pkg.digest(runtime_one), "image": pkg.pe_image(runtime_one)},
                {"path": "zh-CN/Microsoft.UI.Xaml.dll.mui", "bytes": len(runtime_two),
                 "sha256": pkg.digest(runtime_two), "image": pkg.pe_image(runtime_two)},
            ],
        }
        self.runtime_data = pkg.runtime.manifest_bytes(self.runtime_manifest)
        (sdk_dir / "runtime-manifest.json").write_bytes(self.runtime_data)
        self.runtime_bytes = {
            "Microsoft.UI.Xaml.dll": runtime_one,
            "zh-CN/Microsoft.UI.Xaml.dll.mui": runtime_two,
        }
        for name, data in self.runtime_bytes.items():
            path = self.release.joinpath(*PurePosixPath(name).parts)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        self.runtime_patches = [
            patch.object(pkg.runtime, "load_runtime_manifest", return_value=self.runtime_manifest),
            patch.object(pkg.runtime, "validate_manifest", side_effect=self.validate_runtime_manifest),
            patch.object(pkg.runtime, "validate_runtime_directory", side_effect=self.validate_runtime_directory),
        ]
        for item in self.runtime_patches:
            item.start()
            self.addCleanup(item.stop)
        (self.root / "scripts/write-license-notices.py").write_text(
            "import sys\nfrom pathlib import Path\nPath(sys.argv[1]).write_text('license notice',encoding='utf-8')\n",
            encoding="utf-8",
        )
        self.inputs["packageInputs"] = {name: pkg.digest((self.root / name).read_bytes()) for name in pkg.PACKAGE_INPUTS}

    def validate_runtime_manifest(self, _manifest, *, check_checked_in_files=False):
        records = {record["path"]: record for record in self.runtime_manifest["files"]}
        return records, {name: record["image"] for name, record in records.items()}

    def validate_runtime_directory(self, directory, _manifest, parse_pe):
        files, images = {}, {}
        for record in self.runtime_manifest["files"]:
            name = record["path"]
            path = Path(directory).joinpath(*PurePosixPath(name).parts)
            data = path.read_bytes()
            if len(data) != record["bytes"] or pkg.digest(data) != record["sha256"]:
                raise ValueError(f"staged runtime file hash/size mismatch: {name}")
            image_info = parse_pe(data)
            if image_info != record["image"]:
                raise ValueError(f"staged runtime PE image mismatch: {name}")
            files[name] = data
            images[name] = image_info
        return files, images

    def tearDown(self):
        self.temp.cleanup()

    def receipt(self, web=False, delay=False, web_exe=None, architecture="x64"):
        web_exe = web_exe or pkg.EXES[0]
        for name in pkg.EXES:
            (self.release / name).write_bytes(
                image(web=web and name == web_exe, delay=delay, architecture=architecture)
            )
        captured = self.root / "inputs.json"
        captured.write_bytes(pkg.json_bytes(self.inputs))
        with patch.object(pkg, "build_inputs", return_value=self.inputs):
            return pkg.record_build(self.release, captured, self.root)

    def package(self, web=False, suffix=""):
        receipt = self.receipt(web)
        stage = self.root / f"payload{suffix}"
        manifest = pkg.payload(self.release, stage, receipt, "portable", self.root)
        archive = self.root / f"package{suffix}.zip"
        pkg.write_zip(stage, archive)
        return archive, pkg.digest(archive.read_bytes()), manifest

    def rewrite(self, archive, transform):
        with zipfile.ZipFile(archive) as old:
            files = {name: old.read(name) for name in old.namelist()}
        transform(files)
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as new:
            for name, data in files.items():
                new.writestr(name, data)
        return pkg.digest(archive.read_bytes())

    def test_pe_static_and_delay_import_detection(self):
        for delay in (False, True):
            self.assertEqual(pkg.pe_image(image(web=True, delay=delay))["imports"], ["webview2loader.dll"])
        self.assertEqual(pkg.pe_image(image())["imports"], [])
        self.assertTrue(pkg.pe_image(image(dll=True))["dll"])

    def test_build_rejects_static_and_delay_webview_imports(self):
        for name in pkg.EXES:
            for delay in (False, True):
                with self.subTest(executable=name, delay=delay), self.assertRaisesRegex(
                    ValueError, "WebView imports"
                ):
                    self.receipt(web=True, delay=delay, web_exe=name)

    def test_build_rejects_x86_gui_executables_even_when_the_pe_is_well_formed(self):
        self.assertEqual(pkg.pe_image(image(architecture="x86"))["architecture"], "x86")
        with self.assertRaisesRegex(ValueError, "x64 GUI executable"):
            self.receipt(architecture="x86")

    def test_invalid_architecture_and_truncated_images_rejected(self):
        invalid = bytearray(image())
        struct.pack_into("<H", invalid, 0x84, 0x14C)
        for data in (b"", b"MZ", image()[:150], bytes(invalid)):
            with self.assertRaises(ValueError):
                pkg.pe_image(data)

    def test_build_receipt_refuses_source_changed_during_compilation(self):
        self.receipt()
        captured = self.root / "inputs.json"
        changed = {**self.inputs, "sourceSha256": "d" * 64}
        with patch.object(pkg, "build_inputs", return_value=changed):
            with self.assertRaisesRegex(ValueError, "changed during"):
                pkg.record_build(self.release, captured, self.root)

    def test_skip_build_cannot_mislabel_version_or_reuse_modified_binary(self):
        self.receipt()
        with patch.object(pkg, "build_inputs", return_value=self.inputs):
            with self.assertRaisesRegex(ValueError, "Version"):
                pkg.load_receipt(self.release, "9.9.9", root=self.root)
            (self.release / pkg.EXES[0]).write_bytes(image() + b"changed")
            with self.assertRaisesRegex(ValueError, "binary changed"):
                pkg.load_receipt(self.release, root=self.root)

    def test_build_receipt_binds_all_runtime_bytes(self):
        self.receipt()
        runtime_path = self.release / "zh-CN/Microsoft.UI.Xaml.dll.mui"
        runtime_path.write_bytes(runtime_path.read_bytes() + b"changed")
        with patch.object(pkg, "build_inputs", return_value=self.inputs):
            with self.assertRaisesRegex(ValueError, "runtime file hash/size mismatch"):
                pkg.load_receipt(self.release, root=self.root)

    def test_release_requires_clean_inputs(self):
        self.inputs["dirty"] = True
        self.receipt()
        with patch.object(pkg, "build_inputs", return_value=self.inputs):
            with self.assertRaisesRegex(ValueError, "clean"):
                pkg.load_receipt(self.release, clean=True, root=self.root)

    def test_zip_deterministic_and_manifest_validated(self):
        archive, checksum, manifest = self.package()
        second = self.root / "again.zip"
        pkg.write_zip(self.root / "payload", second)
        self.assertEqual(archive.read_bytes(), second.read_bytes())
        checked, _ = pkg.read_package(archive, checksum)
        self.assertEqual(checked, manifest)
        self.assertFalse(checked["runtimeRequirements"]["webview2Evergreen"])

    def test_native_payload_does_not_bundle_webview_loader(self):
        archive, checksum, _ = self.package()
        manifest, files = pkg.read_package(archive, checksum)
        self.assertFalse(manifest["runtimeRequirements"]["webview2Evergreen"])
        self.assertTrue(manifest["runtimeRequirements"]["windowsAppSdkSelfContained"])
        self.assertEqual(manifest["runtime"]["fileCount"], 2)
        self.assertIn("zh-CN/Microsoft.UI.Xaml.dll.mui", files)
        self.assertIn("WINDOWS-APP-SDK-LICENSE.txt", files)
        self.assertFalse(any("webview" in name.casefold() for name in files))

    def test_nested_runtime_bytes_cannot_be_changed_with_a_recomputed_zip_hash(self):
        archive, _, _ = self.package()
        checksum = self.rewrite(
            archive,
            lambda files: files.update({"zh-CN/Microsoft.UI.Xaml.dll.mui": b"tampered"}),
        )
        with self.assertRaisesRegex(ValueError, "checksum/size mismatch"):
            pkg.read_package(archive, checksum)

    def test_webview_import_cannot_be_smuggled_into_a_package_manifest(self):
        for name in pkg.EXES:
            for delay in (False, True):
                with self.subTest(executable=name, delay=delay):
                    archive, _, _ = self.package(suffix=f"-{name}-{delay}")
                    def add_import(files):
                        data = image(web=True, delay=delay)
                        files[name] = data
                        manifest = json.loads(files[pkg.MANIFEST])
                        manifest["images"][name] = pkg.pe_image(data)
                        manifest["files"][name] = {
                            "bytes": len(data), "sha256": pkg.digest(data),
                        }
                        files[pkg.MANIFEST] = pkg.json_bytes(manifest)
                    checksum = self.rewrite(archive, add_import)
                    with self.assertRaisesRegex(ValueError, "WebView imports"):
                        pkg.read_package(archive, checksum)

    def test_webview_payload_cannot_be_added_to_a_package(self):
        archive, _, _ = self.package()
        checksum = self.rewrite(
            archive, lambda files: files.update({"WebView2Loader.dll": image(dll=True)}))
        with self.assertRaisesRegex(ValueError, "WebView payload"):
            pkg.read_package(archive, checksum)

    def test_staging_rechecks_binary_and_asset_bytes_before_writing(self):
        receipt = self.receipt()
        (self.release / pkg.EXES[0]).write_bytes(image() + b"changed")
        stage = self.root / "stage"
        with self.assertRaisesRegex(ValueError, "binary changed"):
            pkg.payload(self.release, stage, receipt, "portable", self.root)
        self.assertFalse(stage.exists())
        receipt = self.receipt()
        (self.root / "NOTICE").write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "asset changed"):
            pkg.payload(self.release, stage, receipt, "portable", self.root)
        self.assertFalse(stage.exists())

    def test_hash_failure_never_creates_destination(self):
        archive, _, _ = self.package()
        destination = self.root / "install"
        with self.assertRaisesRegex(ValueError, "SHA-256"):
            pkg.install_package(archive, "0" * 64, destination)
        self.assertFalse(destination.exists())

    def test_payload_checksum_failure(self):
        archive, _, _ = self.package()
        checksum = self.rewrite(archive, lambda files: files.update({"NOTICE": b"changed"}))
        with self.assertRaisesRegex(ValueError, "checksum"):
            pkg.read_package(archive, checksum)

    def test_zip_paths_case_duplicates_and_unexpected_keys_rejected(self):
        archive, _, _ = self.package()
        for name in ("../workspace.v2.json", "config/workspace.v2.json", r"C:\evil.exe", "NOTICE:stream"):
            with self.subTest(name=name):
                checksum = self.rewrite(archive, lambda files: files.update({name: b"bad"}))
                expected = "unsafe runtime path" if name != "config/workspace.v2.json" else "unexpected/missing payload"
                with self.assertRaisesRegex(ValueError, expected):
                    pkg.read_package(archive, checksum)
                self.rewrite(archive, lambda files: files.pop(name if name in files else name.replace("\\", "/")))
        for name in ("notice", "signing.pfx"):
            checksum = self.rewrite(archive, lambda files: files.update({name: b"bad"}))
            with self.assertRaises(ValueError):
                pkg.read_package(archive, checksum)
            self.rewrite(archive, lambda files: files.pop(name))

    def test_case_alias_and_file_as_parent_paths_are_rejected(self):
        archive, _, _ = self.package()
        for name in ("ZH-cn/extra.mui", "zh-CN"):
            with self.subTest(name=name):
                checksum = self.rewrite(archive, lambda files: files.update({name: b"bad"}))
                with self.assertRaises(ValueError):
                    pkg.read_package(archive, checksum)
                self.rewrite(archive, lambda files: files.pop(name))

    def test_archive_bounds_and_reparse_attributes_are_enforced(self):
        archive = self.root / "hostile.zip"
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as zipped:
            zipped.writestr("one", b"one")
            zipped.writestr("two", b"two")
        checksum = pkg.digest(archive.read_bytes())
        with patch.object(pkg, "MAX_ARCHIVE_ENTRIES", 1):
            with self.assertRaisesRegex(ValueError, "512-entry"):
                pkg.read_package(archive, checksum)
        with patch.object(pkg, "MAX_UNPACKED_BYTES", 2):
            with self.assertRaisesRegex(ValueError, "160 MiB"):
                pkg.read_package(archive, checksum)

        package, _, _ = self.package()
        with zipfile.ZipFile(package) as old:
            entries = [(item.filename, old.read(item)) for item in old.infolist()]
        with zipfile.ZipFile(package, "w", compression=zipfile.ZIP_DEFLATED) as new:
            for name, data in entries:
                item = zipfile.ZipInfo(name)
                item.compress_type = zipfile.ZIP_DEFLATED
                if name == "NOTICE":
                    item.create_system = 0
                    item.external_attr = pkg.runtime.REPARSE_POINT
                else:
                    item.create_system = 3
                    item.external_attr = (stat.S_IFREG | 0o644) << 16
                new.writestr(item, data)
        with self.assertRaisesRegex(ValueError, "reparse-point"):
            pkg.read_package(package, pkg.digest(package.read_bytes()))

    def test_zip_symlink_rejected(self):
        archive, _, _ = self.package()
        entry = zipfile.ZipInfo("symlink")
        entry.create_system = 3
        entry.compress_type = zipfile.ZIP_DEFLATED
        entry.external_attr = (stat.S_IFLNK | 0o777) << 16
        with zipfile.ZipFile(archive, "a") as zipped:
            zipped.writestr(entry, "../user-config")
        with self.assertRaisesRegex(ValueError, "symlink"):
            pkg.read_package(archive, pkg.digest(archive.read_bytes()))

    def test_webview_runtime_requirement_cannot_be_enabled(self):
        archive, _, _ = self.package()
        def enable_runtime(files):
            value = json.loads(files[pkg.MANIFEST])
            value["runtimeRequirements"]["webview2Evergreen"] = True
            files[pkg.MANIFEST] = pkg.json_bytes(value)
        checksum = self.rewrite(archive, enable_runtime)
        with self.assertRaisesRegex(ValueError, "WebView runtime"):
            pkg.read_package(archive, checksum)

    def test_install_is_immutable_idempotent_and_preserves_extra_user_config(self):
        archive, checksum, _ = self.package()
        destination = self.root / "install"
        target = pkg.install_package(archive, checksum, destination)
        self.assertEqual(pkg.install_package(archive, checksum, destination), target)
        self.assertEqual(
            (target / "zh-CN/Microsoft.UI.Xaml.dll.mui").read_bytes(),
            self.runtime_bytes["zh-CN/Microsoft.UI.Xaml.dll.mui"],
        )
        config = target / "config"
        config.mkdir()
        (config / "workspace.v2.json").write_text("user data", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "user config"):
            pkg.install_package(archive, checksum, destination)
        self.assertEqual((config / "workspace.v2.json").read_text(encoding="utf-8"), "user data")

    def test_dry_run_does_not_write_or_launch_anything(self):
        archive, checksum, _ = self.package()
        destination = self.root / "install"
        target = pkg.install_package(archive, checksum, destination, dry_run=True)
        self.assertEqual(target.parent, destination)
        self.assertFalse(destination.exists())

    def test_install_refuses_reparse_destination_ancestors(self):
        archive, checksum, _ = self.package()
        destination = self.root / "install"
        destination.mkdir()
        with patch.object(
            pkg.runtime, "_is_reparse", side_effect=lambda path: Path(path) == destination
        ):
            with self.assertRaisesRegex(ValueError, "symlink/junction"):
                pkg.install_package(archive, checksum, destination)
        self.assertEqual(list(destination.iterdir()), [])

    def test_msix_and_zip_share_the_same_compiled_payload(self):
        receipt = self.receipt()
        portable = pkg.payload(self.release, self.root / "portable", receipt, "portable", self.root)
        msix = pkg.payload(self.release, self.root / "msix", receipt, "msix", self.root)
        for name in {*pkg.EXES, "NOTICE", "THIRD-PARTY-LICENSES.txt"}:
            self.assertEqual(portable["files"][name], msix["files"][name])
        self.assertIn("LICENSE.txt", msix["files"])
        self.assertNotIn("README.md", msix["files"])
        self.assertFalse(any("webview" in name.casefold() for name in portable["files"]))
        self.assertFalse(any("webview" in name.casefold() for name in msix["files"]))


if __name__ == "__main__":
    unittest.main()

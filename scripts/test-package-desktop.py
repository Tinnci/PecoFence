"""Pure packaging/install tests with synthetic PE files; no desktop or machine installation."""
import json
from pathlib import Path
import stat
import struct
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import package_desktop as pkg


def image(web=False, dll=False, delay=False):
    data = bytearray(1024)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HH", data, 0x84, 0x8664, 1)
    struct.pack_into("<HH", data, 0x94, 240, 0x2000 if dll else 0)
    optional = 0x98
    struct.pack_into("<H", data, optional, 0x20B)
    struct.pack_into("<Q", data, optional + 24, 0x140000000)
    struct.pack_into("<H", data, optional + 68, 2)
    struct.pack_into("<I", data, optional + 108, 16)
    section = optional + 240
    struct.pack_into("<IIII", data, section + 8, 512, 0x1000, 512, 512)
    if web:
        directory, width = (13, 32) if delay else (1, 20)
        struct.pack_into("<II", data, optional + 112 + directory * 8, 0x1000, width * 2)
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
        (self.root / "third_party/webview2").mkdir(parents=True)
        for name in ("LICENSE", "NOTICE", "docs/PORTABLE.md", "docs/UPGRADING.md", "third_party/webview2/LICENSE.txt"):
            (self.root / name).write_text(name, encoding="utf-8")
        (self.root / "third_party/webview2/WebView2Loader.x64.dll").write_bytes(image(dll=True))
        (self.root / "scripts/write-license-notices.py").write_text(
            "import sys\nfrom pathlib import Path\nPath(sys.argv[1]).write_text('license notice',encoding='utf-8')\n",
            encoding="utf-8",
        )
        self.inputs["packageInputs"] = {name: pkg.digest((self.root / name).read_bytes()) for name in pkg.PACKAGE_INPUTS}

    def tearDown(self):
        self.temp.cleanup()

    def receipt(self, web=True):
        (self.release / pkg.EXES[0]).write_bytes(image(web=web))
        (self.release / pkg.EXES[1]).write_bytes(image())
        captured = self.root / "inputs.json"
        captured.write_bytes(pkg.json_bytes(self.inputs))
        with patch.object(pkg, "build_inputs", return_value=self.inputs):
            return pkg.record_build(self.release, captured, self.root)

    def package(self, web=True):
        receipt = self.receipt(web)
        stage = self.root / "payload"
        manifest = pkg.payload(self.release, stage, receipt, "portable", self.root)
        archive = self.root / "package.zip"
        pkg.write_zip(stage, archive)
        return archive, pkg.digest(archive.read_bytes()), manifest

    def rewrite(self, archive, transform):
        with zipfile.ZipFile(archive) as old:
            files = {name: old.read(name) for name in old.namelist()}
        transform(files)
        with zipfile.ZipFile(archive, "w") as new:
            for name, data in files.items():
                new.writestr(name, data)
        return pkg.digest(archive.read_bytes())

    def test_pe_static_delay_and_native_runtime_detection(self):
        for delay in (False, True):
            self.assertEqual(pkg.pe_image(image(web=True, delay=delay))["imports"], ["webview2loader.dll"])
        self.assertEqual(pkg.pe_image(image())["imports"], [])
        self.assertTrue(pkg.pe_image(image(dll=True))["dll"])

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
        self.assertTrue(checked["runtimeRequirements"]["webview2Evergreen"])

    def test_native_payload_does_not_bundle_webview_loader(self):
        archive, checksum, _ = self.package(False)
        manifest, files = pkg.read_package(archive, checksum)
        self.assertFalse(manifest["runtimeRequirements"]["webview2Evergreen"])
        self.assertTrue(pkg.WEB_FILES.isdisjoint(files))

    def test_staging_rechecks_binary_and_asset_bytes_before_writing(self):
        receipt = self.receipt()
        (self.release / pkg.EXES[0]).write_bytes(image())
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
                with self.assertRaisesRegex(ValueError, "traversal"):
                    pkg.read_package(archive, checksum)
                self.rewrite(archive, lambda files: files.pop(name if name in files else name.replace("\\", "/")))
        for name in ("notice", "signing.pfx"):
            checksum = self.rewrite(archive, lambda files: files.update({name: b"bad"}))
            with self.assertRaises(ValueError):
                pkg.read_package(archive, checksum)
            self.rewrite(archive, lambda files: files.pop(name))

    def test_zip_symlink_rejected(self):
        archive, _, _ = self.package()
        entry = zipfile.ZipInfo("symlink")
        entry.create_system = 3
        entry.external_attr = (stat.S_IFLNK | 0o777) << 16
        with zipfile.ZipFile(archive, "a") as zipped:
            zipped.writestr(entry, "../user-config")
        with self.assertRaisesRegex(ValueError, "symlink"):
            pkg.read_package(archive, pkg.digest(archive.read_bytes()))

    def test_webview_requirement_cannot_be_hidden(self):
        archive, _, _ = self.package()
        def remove_runtime(files):
            value = json.loads(files[pkg.MANIFEST])
            value["runtimeRequirements"]["webview2Evergreen"] = False
            for name in pkg.WEB_FILES:
                files.pop(name)
                value["files"].pop(name)
            files[pkg.MANIFEST] = pkg.json_bytes(value)
        checksum = self.rewrite(archive, remove_runtime)
        with self.assertRaisesRegex(ValueError, "requirement"):
            pkg.read_package(archive, checksum)

    def test_install_is_immutable_idempotent_and_preserves_extra_user_config(self):
        archive, checksum, _ = self.package()
        destination = self.root / "install"
        target = pkg.install_package(archive, checksum, destination)
        self.assertEqual(pkg.install_package(archive, checksum, destination), target)
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

    def test_msix_and_zip_share_the_same_compiled_payload(self):
        receipt = self.receipt()
        portable = pkg.payload(self.release, self.root / "portable", receipt, "portable", self.root)
        msix = pkg.payload(self.release, self.root / "msix", receipt, "msix", self.root)
        for name in {*pkg.EXES, *pkg.WEB_FILES, "NOTICE", "THIRD-PARTY-LICENSES.txt"}:
            self.assertEqual(portable["files"][name], msix["files"][name])
        self.assertIn("LICENSE.txt", msix["files"])
        self.assertNotIn("README.md", msix["files"])


if __name__ == "__main__":
    unittest.main()

"""Offline runtime provenance and hostile archive validation tests."""
import io
from pathlib import Path
import stat
import tempfile
import unittest
from unittest import mock
import zipfile

import winappsdk_runtime as runtime


class RuntimeTests(unittest.TestCase):
    def test_checked_in_inventory_and_activation_manifest(self):
        manifest = runtime.load_runtime_manifest()
        files, images = runtime.validate_manifest(manifest)
        self.assertEqual(len(files), 206)
        self.assertEqual(len(images), 200)
        self.assertEqual(sum(record["bytes"] for record in files.values()), 59_158_503)
        self.assertTrue(all("webview" not in name.lower() for name in files))

    def test_pinned_package_identity_cannot_change(self):
        manifest = runtime.load_runtime_manifest()
        manifest["package"]["version"] = "2.5.2"
        with self.assertRaises(ValueError):
            runtime.validate_manifest(manifest)

    def test_license_remains_byte_exact_but_xml_allows_git_line_endings(self):
        manifest = runtime.load_runtime_manifest()
        target = runtime.ROOT / "target"
        target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=target) as temp:
            root = Path(temp)
            directory = root / "third_party/winappsdk"
            directory.mkdir(parents=True)
            license_path = directory / "WINDOWS-APP-SDK-LICENSE.txt"
            license_path.write_bytes((runtime.THIRD_PARTY / license_path.name).read_bytes())
            activation = directory / "app.manifest"
            xml = runtime.APP_MANIFEST.read_bytes().replace(b"\r\n", b"\n")
            with mock.patch.object(runtime, "ROOT", root), mock.patch.object(runtime, "APP_MANIFEST", activation):
                for data in (xml, xml.replace(b"\n", b"\r\n")):
                    activation.write_bytes(data)
                    runtime.validate_manifest(manifest, check_checked_in_files=True)
                activation.write_bytes(xml.replace(b"Microsoft.UI.Xaml", b"Malicious.UI.Xaml", 1))
                with self.assertRaises(ValueError):
                    runtime.validate_manifest(manifest, check_checked_in_files=True)
                activation.write_bytes(xml)
                license_path.write_bytes(b"changed attribution")
                with self.assertRaises(ValueError):
                    runtime.validate_manifest(manifest, check_checked_in_files=True)

    def test_unsafe_windows_paths_are_rejected(self):
        for value in ("../x.dll", "/x.dll", "C:/x.dll", "x.dll:ads", "a\\b", "a//b",
                      "a/../b", "CON.dll", "a/NUL", "a./b", "a /b", "a/b.", ""):
            with self.subTest(value=value), self.assertRaises(ValueError):
                runtime.safe_relative(value)

    def test_valid_nested_resources_are_preserved(self):
        self.assertEqual(runtime.safe_relative("zh-CN/Microsoft.UI.Xaml.dll.mui"),
                         "zh-CN/Microsoft.UI.Xaml.dll.mui")

    def entries(self, pairs, **limits):
        data = io.BytesIO()
        with zipfile.ZipFile(data, "w") as archive:
            for name, contents in pairs:
                archive.writestr(name, contents)
        data.seek(0)
        with zipfile.ZipFile(data) as archive:
            return runtime.zip_entries(
                archive, max_entries=limits.get("entries", 10),
                max_unpacked=limits.get("bytes", 100), allow_directories=True,
            )

    def test_case_aliases_are_rejected(self):
        with self.assertRaises(ValueError):
            self.entries([("x.dll", b"a"), ("X.DLL", b"b")])

    def test_implicit_parent_case_aliases_are_rejected_in_either_order(self):
        entries = [("Locale/a.mui", b"a"), ("locale/b.mui", b"b")]
        for pairs in (entries, list(reversed(entries))):
            with self.subTest(pairs=pairs), self.assertRaises(ValueError):
                self.entries(pairs)

    def test_dos_reparse_entries_are_rejected(self):
        for attributes in (runtime.REPARSE_POINT, (stat.S_IFREG | 0o644) << 16 | runtime.REPARSE_POINT):
            link = zipfile.ZipInfo("alias.dll")
            link.external_attr = attributes
            with self.subTest(attributes=attributes), self.assertRaises(ValueError):
                self.entries([(link, b"target.dll")])

    def test_case_insensitive_file_as_parent_is_rejected(self):
        with self.assertRaises(ValueError):
            self.entries([("locale", b"a"), ("Locale/x.mui", b"b")])

    def test_nested_file_as_parent_is_rejected(self):
        with self.assertRaises(ValueError):
            self.entries([("a/b", b"a"), ("a/b/c", b"b")])

    def test_symlink_entries_are_rejected(self):
        link = zipfile.ZipInfo("alias.dll")
        link.create_system = 3
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        with self.assertRaises(ValueError):
            self.entries([(link, b"target.dll")])

    def test_entry_and_expansion_limits_are_enforced(self):
        for limits in ({"entries": 1}, {"bytes": 1}):
            with self.subTest(limits=limits), self.assertRaises(ValueError):
                self.entries([("a", b"a"), ("b", b"b")], **limits)

    def test_traversal_archive_member_is_rejected(self):
        with self.assertRaises(ValueError):
            self.entries([("../escape.dll", b"a")])

    def test_webview_payload_cannot_be_added_to_inventory(self):
        manifest = runtime.load_runtime_manifest()
        manifest["files"][0]["path"] = "WebView2Loader.dll"
        with self.assertRaises(ValueError):
            runtime.validate_manifest(manifest, check_checked_in_files=False)


if __name__ == "__main__":
    unittest.main()

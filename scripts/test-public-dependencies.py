import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "public_dependencies", Path(__file__).with_name("check-public-dependencies.py"))
POLICY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(POLICY)


class BoundaryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.write("Cargo.toml", """
[workspace]
members = ["crates/spm-contracts", "crates/app"]
exclude = ["vendor/windows-reactor"]
[workspace.dependencies]
spm-contracts = { version = "=0.1.0", path = "crates/spm-contracts" }
windows-reactor = { version = "=0.100.0", path = "vendor/windows-reactor" }
""")
        self.write("crates/spm-contracts/Cargo.toml", """
[package]
name = "spm-contracts"
version = "0.1.0"
""")
        self.write("crates/app/Cargo.toml", """
[dependencies]
windows-reactor = { workspace = true, features = ["test"] }
""")
        self.write("vendor/windows-reactor/Cargo.toml", """
[package]
name = "windows-reactor"
version = "0.100.0"

[dependencies]
windows-core = { version = "=0.100.0", git = "https://github.com/microsoft/windows-rs", rev = "2672e615d9cc0771448a781f3b2fe34e7fd08c6a" }
""")
        self.write("Cargo.lock", """
[[package]]
name = "spm-contracts"
version = "0.1.0"

[[package]]
name = "windows-core"
version = "0.100.0"
source = "git+https://github.com/microsoft/windows-rs?rev=2672e615d9cc0771448a781f3b2fe34e7fd08c6a#2672e615d9cc0771448a781f3b2fe34e7fd08c6a"

[[package]]
name = "windows-reactor"
version = "0.100.0"
""")

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def test_local_contract_and_public_registry_pass(self):
        with (self.root / "Cargo.lock").open("a") as file:
            file.write(f'\n[[package]]\nname = "serde"\nversion = "1.0.0"\nsource = "{POLICY.REGISTRY}"\n')
        self.assertEqual(POLICY.check(self.root), [])

    def test_exact_windows_rs_pin_and_transitive_package_pass(self):
        with (self.root / "Cargo.lock").open("a") as file:
            file.write(
                f'\n[[package]]\nname = "windows-collections"\nversion = "0.100.0"\n'
                f'source = "{POLICY.WINDOWS_RS_SOURCE}"\n'
            )
        self.assertEqual(POLICY.check(self.root), [])

    def test_windows_rs_dependency_rejects_mutable_or_spoofed_pins(self):
        path = self.root / "vendor/windows-reactor/Cargo.toml"
        valid = 'git = "https://github.com/microsoft/windows-rs", rev = "2672e615d9cc0771448a781f3b2fe34e7fd08c6a"'
        invalid = (
            'git = "https://github.com/microsoft/windows-rs", branch = "main"',
            'git = "https://github.com/microsoft/windows-rs", rev = "main"',
            'git = "https://github.com/microsoft/windows-rs", tag = "v0.100.0", rev = "2672e615d9cc0771448a781f3b2fe34e7fd08c6a"',
            'git = "https://user@github.com/microsoft/windows-rs", rev = "2672e615d9cc0771448a781f3b2fe34e7fd08c6a"',
            'git = "https://github.com.evil/microsoft/windows-rs", rev = "2672e615d9cc0771448a781f3b2fe34e7fd08c6a"',
            'git = "https://github.com/microsoft/windows-rs.evil", rev = "2672e615d9cc0771448a781f3b2fe34e7fd08c6a"',
            'git = "https://github.com/microsoft/windows-rs", rev = "2672e615d9cc0771448a781f3b2fe34e7fd08c6b"',
        )
        for source in invalid:
            with self.subTest(source=source):
                path.write_text(path.read_text().replace(valid, source), encoding="utf-8")
                self.assertTrue(POLICY.check(self.root))
                path.write_text(path.read_text().replace(source, valid), encoding="utf-8")

    def test_windows_rs_dependency_name_must_be_allowlisted(self):
        path = self.root / "vendor/windows-reactor/Cargo.toml"
        path.write_text(path.read_text().replace(
            "windows-core = {",
            'unreviewed-sdk-crate = { package = "windows-core",',
        ), encoding="utf-8")
        errors = POLICY.check(self.root)
        self.assertTrue(any("unapproved Git source" in error for error in errors))

    def test_windows_reactor_requires_exact_local_path_and_version(self):
        path = self.root / "Cargo.toml"
        original = path.read_text()
        for replacement in (
            'windows-reactor = { version = "=0.99.0", path = "vendor/windows-reactor" }',
            'windows-reactor = { version = "=0.100.0", path = "vendor/elsewhere" }',
            'windows-reactor = { version = "=0.100.0", path = "vendor/windows-reactor", package = "other" }',
            'windows-reactor = { version = "=0.100.0", git = "https://github.com/microsoft/windows-rs", rev = "2672e615d9cc0771448a781f3b2fe34e7fd08c6a" }',
        ):
            with self.subTest(replacement=replacement):
                path.write_text(
                    original.replace(
                        'windows-reactor = { version = "=0.100.0", path = "vendor/windows-reactor" }',
                        replacement,
                    ),
                    encoding="utf-8",
                )
                self.assertTrue(POLICY.check(self.root))
        path.write_text(original, encoding="utf-8")

    def test_workspace_member_cannot_override_inherited_reactor_source(self):
        app = self.root / "crates/app/Cargo.toml"
        original = app.read_text()
        app.write_text(
            original.replace(
                'windows-reactor = { workspace = true, features = ["test"] }',
                'windows-reactor = { path = "../../vendor/windows-reactor", version = "=0.100.0" }',
            ),
            encoding="utf-8",
        )
        self.assertTrue(POLICY.check(self.root))

    def test_windows_reactor_manifest_identity_and_workspace_exclusion_are_exact(self):
        manifest = self.root / "vendor/windows-reactor/Cargo.toml"
        original = manifest.read_text()
        manifest.write_text(original.replace('name = "windows-reactor"', 'name = "other"'),
                            encoding="utf-8")
        self.assertTrue(POLICY.check(self.root))
        manifest.write_text(original.replace('version = "0.100.0"', 'version = "0.99.0"'),
                            encoding="utf-8")
        self.assertTrue(POLICY.check(self.root))
        manifest.write_text(original, encoding="utf-8")

        workspace = self.root / "Cargo.toml"
        original = workspace.read_text()
        workspace.write_text(original.replace('exclude = ["vendor/windows-reactor"]\n', ''),
                             encoding="utf-8")
        self.assertTrue(POLICY.check(self.root))
        workspace.write_text(original, encoding="utf-8")

    def test_private_git_pin_is_rejected(self):
        path = self.root / "Cargo.toml"
        path.write_text(path.read_text().replace(
            'path = "crates/spm-contracts"', 'git = "https://github.com/private/backend"'))
        self.assertTrue(POLICY.check(self.root))

    def test_transitive_git_lock_is_rejected(self):
        with (self.root / "Cargo.lock").open("a") as file:
            file.write('\n[[package]]\nname = "hidden"\nversion = "1.0.0"\nsource = "git+https://example.com/private"\n')
        self.assertTrue(POLICY.check(self.root))

    def test_windows_rs_lock_source_and_package_inventory_are_exact(self):
        path = self.root / "Cargo.lock"
        original = path.read_text()
        wrong_revision = original.replace(
            POLICY.WINDOWS_RS_REV,
            "f" * 40,
        )
        unreviewed = (
            original
            + f'\n[[package]]\nname = "unreviewed-package"\nversion = "0.100.0"\n'
            + f'source = "{POLICY.WINDOWS_RS_SOURCE}"\n'
        )
        git_reactor_duplicate = (
            original
            + f'\n[[package]]\nname = "windows-reactor"\nversion = "0.100.0"\n'
            + f'source = "{POLICY.WINDOWS_RS_SOURCE}"\n'
        )
        for name, mutated in (
            ("wrong revision", wrong_revision),
            ("unreviewed package", unreviewed),
            ("Git reactor duplicate", git_reactor_duplicate),
        ):
            with self.subTest(name=name):
                path.write_text(mutated, encoding="utf-8")
                self.assertTrue(POLICY.check(self.root))
        path.write_text(original, encoding="utf-8")

    def test_windows_rs_lock_requires_the_permitted_sdk_package_version(self):
        path = self.root / "Cargo.lock"
        path.write_text(path.read_text().replace(
            'name = "windows-core"\nversion = "0.100.0"',
            'name = "windows-core"\nversion = "0.99.0"',
        ), encoding="utf-8")
        self.assertTrue(POLICY.check(self.root))

    def test_windows_reactor_lock_requires_one_local_exact_version(self):
        path = self.root / "Cargo.lock"
        path.write_text(path.read_text().replace(
            'name = "windows-reactor"\nversion = "0.100.0"',
            'name = "windows-reactor"\nversion = "0.99.0"',
        ), encoding="utf-8")
        self.assertTrue(POLICY.check(self.root))

    def test_same_version_windows_types_cannot_mix_git_and_registry(self):
        with (self.root / "Cargo.lock").open("a") as file:
            file.write(
                f'\n[[package]]\nname = "windows-core"\nversion = "0.100.0"\n'
                f'source = "{POLICY.REGISTRY}"\n'
            )
        self.assertTrue(any(
            "sourced from both" in error for error in POLICY.check(self.root)
        ))

    def test_external_path_and_nested_target_git_are_rejected(self):
        path = self.root / "crates/spm-contracts/Cargo.toml"
        with path.open("a") as file:
            file.write("""
[target.'cfg(windows)'.dependencies]
hidden = { git = "https://example.com/private" }
external = { path = "../../../private" }
""")
        self.assertGreaterEqual(len(POLICY.check(self.root)), 2)

    def test_registry_override_is_rejected(self):
        self.write(".cargo/config.toml", '[source.crates-io]\nreplace-with = "private"\n')
        self.assertTrue(POLICY.check(self.root))

    def test_git_patch_cannot_bypass_the_boundary(self):
        with (self.root / "Cargo.toml").open("a") as file:
            file.write('\n[patch.crates-io]\nhidden = { git = "https://example.com/private" }\n')
        self.assertTrue(POLICY.check(self.root))

    def test_config_patch_and_path_override_are_rejected_before_fetch(self):
        self.write(".cargo/config.toml",
                   '[patch.crates-io]\nhidden = { git = "https://example.com/private" }\n')
        self.assertTrue(POLICY.check(self.root))
        self.write(".cargo/config.toml", 'paths = ["../private"]\n')
        self.assertTrue(POLICY.check(self.root))

    def test_workflow_credentials_are_rejected(self):
        self.write(".github/actions/build/action.yml",
                   "env:\n  TOKEN: ${{ secrets.PRIVATE_REPO_TOKEN }}\n")
        self.assertTrue(POLICY.check(self.root))

    def test_ordinary_release_token_is_not_a_private_build_token(self):
        self.write(".github/workflows/release.yml",
                   "env:\n  GH_TOKEN: ${{ github.token }}\n")
        self.assertEqual(POLICY.check(self.root), [])

    def test_resolved_sources_and_paths_are_checked(self):
        metadata = json.loads(json.dumps({"packages": [
            {"name": "local", "source": None,
             "manifest_path": str(self.root / "crates/spm-contracts/Cargo.toml")},
            {"name": "serde", "source": POLICY.REGISTRY},
            {"name": "windows-reactor", "version": "0.100.0", "source": None,
             "manifest_path": str(self.root / "vendor/windows-reactor/Cargo.toml")},
            {"name": "windows-core", "version": "0.100.0",
             "source": POLICY.WINDOWS_RS_SOURCE},
        ]}))
        self.assertEqual(POLICY.check(self.root, metadata), [])
        metadata["packages"][0]["manifest_path"] = str(self.root.parent / "private/Cargo.toml")
        metadata["packages"][1]["source"] = "git+https://example.com/private"
        self.assertEqual(len(POLICY.check(self.root, metadata)), 2)

    def test_resolved_graph_rejects_wrong_windows_rs_revision_and_mixed_identity(self):
        metadata = {"packages": [
            {"name": "windows-reactor", "version": "0.100.0", "source": None,
             "manifest_path": str(self.root / "vendor/windows-reactor/Cargo.toml")},
            {"name": "windows-core", "version": "0.100.0",
             "source": POLICY.WINDOWS_RS_SOURCE},
            {"name": "windows-core", "version": "0.100.0",
             "source": POLICY.REGISTRY},
        ]}
        errors = POLICY.check(self.root, metadata)
        self.assertTrue(any("sourced from both" in error for error in errors))
        metadata["packages"][0]["source"] = POLICY.WINDOWS_RS_SOURCE.replace(
            POLICY.WINDOWS_RS_REV, "f" * 40)
        self.assertTrue(any(
            "unapproved source" in error for error in POLICY.check(self.root, metadata)
        ))


if __name__ == "__main__":
    unittest.main()

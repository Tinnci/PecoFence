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
members = ["crates/spm-contracts"]
[workspace.dependencies]
spm-contracts = { version = "=0.1.0", path = "crates/spm-contracts" }
""")
        self.write("crates/spm-contracts/Cargo.toml", """
[package]
name = "spm-contracts"
version = "0.1.0"
""")
        self.write("Cargo.lock", """
[[package]]
name = "spm-contracts"
version = "0.1.0"
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

    def test_private_git_pin_is_rejected(self):
        path = self.root / "Cargo.toml"
        path.write_text(path.read_text().replace(
            'path = "crates/spm-contracts"', 'git = "https://github.com/private/backend"'))
        self.assertTrue(POLICY.check(self.root))

    def test_transitive_git_lock_is_rejected(self):
        with (self.root / "Cargo.lock").open("a") as file:
            file.write('\n[[package]]\nname = "hidden"\nversion = "1.0.0"\nsource = "git+https://example.com/private"\n')
        self.assertTrue(POLICY.check(self.root))

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
        ]}))
        self.assertEqual(POLICY.check(self.root, metadata), [])
        metadata["packages"][0]["manifest_path"] = str(self.root.parent / "private/Cargo.toml")
        metadata["packages"][1]["source"] = "git+https://example.com/private"
        self.assertEqual(len(POLICY.check(self.root, metadata)), 2)


if __name__ == "__main__":
    unittest.main()

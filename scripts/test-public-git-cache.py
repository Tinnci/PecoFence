import importlib.util
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import unittest
import zlib

SPEC = importlib.util.spec_from_file_location(
    "public_git_cache", Path(__file__).with_name("check-public-git-cache.py"))
CACHE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CACHE)


def run_git(cwd, *arguments):
    environment = CACHE._git_environment()
    environment.pop("GIT_TEMPLATE_DIR", None)
    environment.pop("GIT_NAMESPACE", None)
    result = subprocess.run(
        ["git", "-C", str(cwd), *arguments],
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        env=environment,
    )
    if result.returncode:
        raise AssertionError(f"git {' '.join(arguments)} failed: {result.stderr or result.stdout}")
    return result.stdout


@unittest.skipUnless(shutil.which("git"), "Git is required for public cache validation tests")
class PublicGitCacheTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.shared_temp = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.shared_temp.cleanup)
        cls.shared_root = Path(cls.shared_temp.name)
        cls.source = cls.shared_root / "source"
        cls.database_template = cls.shared_root / "windows-rs-db"
        run_git(cls.shared_root, "init", "-b", "main", str(cls.source))
        run_git(cls.source, "config", "user.name", "Cache Test")
        run_git(cls.source, "config", "user.email", "cache-test@example.invalid")
        (cls.source / "README").write_text("public source fixture\n", encoding="utf-8")
        run_git(cls.source, "add", "README")
        run_git(cls.source, "commit", "-m", "initial")
        cls.revision = run_git(cls.source, "rev-parse", "HEAD").strip()
        run_git(cls.shared_root, "clone", "--bare", str(cls.source), str(cls.database_template))
        run_git(cls.database_template, "config", "--unset-all", "remote.origin.url")

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.database_root = self.root / "cargo-home" / "git" / "db"
        self.database = self.database_root / CACHE.WINDOWS_RS_DB_NAME
        self.database_root.mkdir(parents=True)
        shutil.copytree(self.database_template, self.database)

    def git(self, cwd, *arguments):
        try:
            return run_git(cwd, *arguments)
        except AssertionError as error:
            self.fail(str(error))

    def validate(self, **overrides):
        expected_revision = overrides.pop("expected_revision", self.revision)
        return CACHE.validate_database_root(
            self.database_root,
            expected_revision=expected_revision,
            **overrides,
        )

    def test_valid_public_database_passes(self):
        self.assertEqual(self.validate(), self.database)

    def test_missing_or_empty_cache_is_only_allowed_before_fetch(self):
        empty = self.root / "empty-db"
        self.assertIsNone(CACHE.validate_database_root(empty, allow_empty=True))
        empty.mkdir()
        self.assertIsNone(CACHE.validate_database_root(empty, allow_empty=True))
        with self.assertRaisesRegex(CACHE.CacheValidationError, "missing"):
            CACHE.validate_database_root(empty.parent / "missing")
        with self.assertRaisesRegex(CACHE.CacheValidationError, "exactly one database"):
            CACHE.validate_database_root(empty)

    def test_cargo_database_without_remote_is_valid_and_conflicting_remote_is_rejected(self):
        self.assertEqual(self.validate(), self.database)
        self.git(
            self.database, "config", "remote.origin.url", CACHE.WINDOWS_RS_URL
        )
        self.assertEqual(self.validate(), self.database)
        self.git(
            self.database, "config", "remote.origin.url", "https://example.invalid/other"
        )
        with self.assertRaisesRegex(CACHE.CacheValidationError, "conflicting remote identity"):
            self.validate()

    def test_credential_configuration_is_rejected(self):
        self.git(self.database, "config", "credential.helper", "fixture-helper")
        with self.assertRaisesRegex(CACHE.CacheValidationError, "credential"):
            self.validate()

    def test_alternates_grafts_shallow_replace_and_commondir_are_rejected(self):
        paths = (
            "objects/info/alternates",
            "objects/info/http-alternates",
            "info/grafts",
            "shallow",
            "commondir",
            "refs/replace",
        )
        for relative in paths:
            with self.subTest(path=relative):
                poisoned = self.database / relative
                if relative == "refs/replace":
                    poisoned.mkdir(parents=True)
                else:
                    poisoned.parent.mkdir(parents=True, exist_ok=True)
                    poisoned.write_text("poison\n", encoding="utf-8")
                with self.assertRaisesRegex(
                    CACHE.CacheValidationError,
                    "alternate, shallow, graft, replace, or promisor",
                ):
                    self.validate()
                if poisoned.is_dir():
                    poisoned.rmdir()
                else:
                    poisoned.unlink()

    def test_promisor_pack_marker_is_rejected(self):
        pack_directory = self.database / "objects" / "pack"
        pack_directory.mkdir(parents=True, exist_ok=True)
        (pack_directory / "partial.pack.promisor").write_text("", encoding="utf-8")
        with self.assertRaisesRegex(CACHE.CacheValidationError, "promisor metadata"):
            self.validate()

    def test_packed_replacement_ref_is_rejected(self):
        (self.database / "packed-refs").write_bytes(
            f"{self.revision} refs/replace/{self.revision}\n".encode("ascii")
        )
        with self.assertRaisesRegex(CACHE.CacheValidationError, "replacement refs"):
            self.validate()

    def test_symlink_in_object_database_is_rejected(self):
        target = self.root / "external"
        target.mkdir()
        link = self.database / "objects" / "external-link"
        try:
            link.symlink_to(target, target_is_directory=True)
        except (OSError, NotImplementedError):
            self.skipTest("this Windows account cannot create directory symlinks")
        with self.assertRaisesRegex(CACHE.CacheValidationError, "symbolic link or reparse"):
            self.validate()

    def test_wrong_or_unavailable_revision_is_rejected(self):
        with self.assertRaisesRegex(CACHE.CacheValidationError, "expected revision"):
            self.validate(expected_revision="f" * 40)
        with self.assertRaisesRegex(CACHE.CacheValidationError, "full 40-character"):
            self.validate(expected_revision="2672e615")

    def test_corrupt_git_object_is_rejected(self):
        loose_objects = [
            path for path in (self.database / "objects").rglob("*")
            if path.is_file() and len(path.parent.name) == 2
        ]
        self.assertTrue(loose_objects, "fixture should contain loose Git objects")
        object_path = loose_objects[0]
        object_path.chmod(stat.S_IREAD | stat.S_IWRITE)
        contents = bytearray(object_path.read_bytes())
        contents[-1] ^= 0x01
        object_path.write_bytes(contents)
        with self.assertRaises(CACHE.CacheValidationError):
            self.validate()

    def test_valid_compressed_blob_with_wrong_content_hash_is_rejected(self):
        oid = self.git(self.source, "rev-parse", f"{self.revision}:README").strip()
        object_path = self.database / "objects" / oid[:2] / oid[2:]
        data = b"tampered source fixture\n"
        object_path.chmod(stat.S_IREAD | stat.S_IWRITE)
        object_path.write_bytes(zlib.compress(f"blob {len(data)}\0".encode() + data))
        with self.assertRaisesRegex(CACHE.CacheValidationError, "content hash mismatch"):
            self.validate()

    def test_extra_git_database_is_rejected(self):
        extra = self.database_root / "another-public-db"
        shutil.copytree(self.database, extra)
        with self.assertRaisesRegex(CACHE.CacheValidationError, "exactly one database"):
            self.validate()

    def test_wrong_cargo_source_identity_directory_is_rejected(self):
        renamed = self.database_root / "other-source"
        self.database.rename(renamed)
        with self.assertRaisesRegex(CACHE.CacheValidationError, "does not identify"):
            self.validate()

    def test_nested_git_cannot_mask_the_validated_database(self):
        nested = self.database / ".git"
        shutil.copytree(self.database_template, nested)
        with self.assertRaisesRegex(CACHE.CacheValidationError, "unapproved root entry"):
            self.validate()

    def test_unrelated_payload_is_not_cached_as_git_objects(self):
        (self.database / "unrelated-build.exe").write_bytes(b"fixture")
        with self.assertRaisesRegex(CACHE.CacheValidationError, "unapproved root entry"):
            self.validate()

    def test_active_hooks_are_rejected_but_samples_are_allowed(self):
        hooks = self.database / "hooks"
        hooks.mkdir(exist_ok=True)
        (hooks / "reference-transaction").write_text("fixture hook\n", encoding="utf-8")
        with self.assertRaisesRegex(CACHE.CacheValidationError, "active Git hook"):
            self.validate()


if __name__ == "__main__":
    unittest.main()

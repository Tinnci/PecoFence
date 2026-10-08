import importlib.util
from pathlib import Path
import re
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("actions_policy", Path(__file__).with_name("check-actions.py"))
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)

ROOT = Path(__file__).resolve().parents[1]


class ActionPolicyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / ".github/workflows").mkdir(parents=True)
        (self.root / ".github/actions/build-desktop").mkdir(parents=True)
        (self.root / ".github/dependabot.yml").write_text(
            "updates:\n  - directory: /\n  - directory: /.github/actions/build-desktop\n", encoding="utf-8")
        self.workflow = self.root / ".github/workflows/ci.yml"
        self.workflow.write_text("steps:\n  - uses: ./.github/actions/build-desktop\n", encoding="utf-8")
        self.action = self.root / ".github/actions/build-desktop/action.yml"
        self.action.write_text("steps:\n  - uses: actions/checkout@" + "a" * 40 + "\n", encoding="utf-8")

    def tearDown(self):
        self.temp.cleanup()

    def test_explicit_local_composite_coverage(self):
        self.assertEqual(policy.check(self.root), [])
        (self.root / ".github/dependabot.yml").write_text("updates:\n  - directory: /\n", encoding="utf-8")
        self.assertTrue(any("explicit Dependabot" in error for error in policy.check(self.root)))

    def test_mutable_pins_and_privileged_pr_event_rejected(self):
        self.action.write_text("steps:\n  - uses: actions/checkout@v4\n", encoding="utf-8")
        self.workflow.write_text("on:\n  pull_request_target:\nsteps:\n", encoding="utf-8")
        errors = policy.check(self.root)
        self.assertTrue(any("commit SHA" in e for e in errors))
        self.assertTrue(any("pull_request_target" in e for e in errors))

    def test_runtime_verification_catches_node20_but_accepts_node24(self):
        for runtime, valid in [("node20", False), ("node24", True), ("composite", True), ("unknown", False)]:
            with self.subTest(runtime=runtime):
                errors = policy.check(self.root, True, lambda _: f"runs:\n  using: '{runtime}'\n  main: dist/index.js\n")
                self.assertEqual(not errors, valid)

    def test_network_failure_does_not_claim_runtime_verified(self):
        def failed(_): raise OSError("offline")
        self.assertTrue(policy.check(self.root, True, failed))


class CiActionContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.action_text = (ROOT / ".github/actions/build-desktop/action.yml").read_text(
            encoding="utf-8"
        )
        cls.workflow_text = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")

    def test_python_minor_is_fixed_and_action_mode_defaults_to_full(self):
        self.assertIn("python-version: '3.13'", self.action_text)
        self.assertNotIn("check-latest: true", self.action_text)
        self.assertRegex(
            self.action_text,
            r"verification-mode:\n\s+description:.*\n\s+default: Full",
        )
        self.assertIn("-Mode $env:VERIFICATION_MODE", self.action_text)
        self.assertIn("-ReportPath target/verification.json", self.action_text)
        self.assertIn(
            "-SourceOnly -ReportPath target/source-verification.json",
            self.action_text,
        )

    def test_git_cache_is_exactly_scoped_and_validated_around_fetch(self):
        source_preflight = self.action_text.index("./scripts/build-and-verify.ps1 -SourceOnly")
        cache_start = self.action_text.index("- name: Restore pinned public windows-rs Git object database")
        cache_end = self.action_text.index("- name: Validate restored public Git object database")
        fetch_start = self.action_text.index("- name: Fetch locked public dependencies")
        save_start = self.action_text.index("- name: Save validated public windows-rs Git object database")
        self.assertLess(source_preflight, cache_start)
        self.assertLess(cache_start, cache_end)
        self.assertLess(cache_end, fetch_start)
        self.assertLess(fetch_start, save_start)

        git_cache = self.action_text[cache_start:cache_end]
        self.assertIn("${{ env.CARGO_HOME }}/git/db", git_cache)
        self.assertIn("2672e615d9cc0771448a781f3b2fe34e7fd08c6a", git_cache)
        self.assertIn("uses: actions/cache/restore@", git_cache)
        self.assertNotIn("restore-keys:", git_cache)
        self.assertNotIn("target/", git_cache)
        self.assertNotIn("${{ env.CARGO_HOME }}/git/checkouts", git_cache)
        git_cache_save = self.action_text[save_start:]
        self.assertIn("uses: actions/cache/save@", git_cache_save)
        self.assertIn("steps.windows-rs-git-db.outputs.cache-hit != 'true'", git_cache_save)
        self.assertIn("${{ env.CARGO_HOME }}/git/db", git_cache_save)
        restore_key = re.search(r"^\s+key:\s*(.+)$", git_cache, re.MULTILINE)
        save_section = git_cache_save.split("\n    - name:", 1)[0]
        save_key = re.search(r"^\s+key:\s*(.+)$", save_section, re.MULTILINE)
        self.assertIsNotNone(restore_key)
        self.assertIsNotNone(save_key)
        self.assertEqual(restore_key[1], save_key[1])
        self.assertIn(
            "python scripts/check-public-git-cache.py --database-root $databaseRoot --allow-empty",
            self.action_text,
        )
        self.assertIn(
            "python scripts/check-public-git-cache.py --database-root $databaseRoot",
            self.action_text,
        )

    def test_quick_and_full_are_independent_and_full_keeps_portable_artifact(self):
        quick = self.workflow_text.split("  quick:\n", 1)[1].split("  build:\n", 1)[0]
        build = self.workflow_text.split("  build:\n", 1)[1]
        self.assertIn("verification-mode: Quick", quick)
        self.assertIn("verification-mode: Full", build)
        self.assertNotIn("needs:", quick)
        self.assertNotIn("needs:", build)
        self.assertIn("name: pecofence-portable", build)
        self.assertIn("if: always()", quick)
        self.assertIn("if: always()", build)
        self.assertIn("target/source-verification.json", quick)
        self.assertIn("target/verification.json", quick)
        self.assertIn("target/source-verification.json", build)
        self.assertIn("target/verification.json", build)


if __name__ == "__main__":
    unittest.main()

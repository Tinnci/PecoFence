import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("actions_policy", Path(__file__).with_name("check-actions.py"))
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)


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


if __name__ == "__main__":
    unittest.main()

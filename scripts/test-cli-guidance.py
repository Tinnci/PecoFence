"""Offline packaging CLI contracts; never installs or starts the desktop app."""

import json
import subprocess
import sys
import unittest
from pathlib import Path

import package_desktop as package

SCRIPT = Path(__file__).with_name("package_desktop.py")


class GuidanceTests(unittest.TestCase):
    def test_bad_checksum_rejected_before_archive_access(self):
        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "verify",
                "--archive",
                "missing.zip",
                "--sha256",
                "bad",
                "--json",
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, "")
        error = json.loads(result.stderr)["error"]
        self.assertIn("64-character", error["message"])
        self.assertIn("--help", error["suggested_action"])

    def test_missing_archive_is_a_structured_failure(self):
        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--json",
                "verify",
                "--archive",
                "missing.zip",
                "--sha256",
                "0" * 64,
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertEqual(json.loads(result.stderr)["error"]["code"], "PACKAGE_FAILED")

    def test_each_command_describes_effects_and_next_step(self):
        parser = package.build_parser()
        for action in parser._subparsers._group_actions[0].choices.values():
            self.assertTrue(action.description)
            self.assertIn("--json", action.format_help())
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "install", "--help"],
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertIn("--dry-run", result.stdout)
        self.assertIn("never launches", result.stdout)


if __name__ == "__main__":
    unittest.main()

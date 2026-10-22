"""Offline packaging CLI contracts; never installs or starts the desktop app."""

import json
import re
import subprocess
import sys
import unittest
from pathlib import Path

import package_desktop as package

SCRIPT = Path(__file__).with_name("package_desktop.py")
ROOT = Path(__file__).resolve().parents[1]


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

    def test_skill_names_only_real_cli_commands_and_flags(self):
        skill = (ROOT / "skills" / "pecofence" / "SKILL.md").read_text(encoding="utf-8")
        source = (ROOT / "crates" / "cli" / "src" / "main.rs").read_text(encoding="utf-8")
        help_text = source.split('const HELP: &str = "', 1)[1].split('";', 1)[0]
        commands = set(re.findall(r"pecofence-cli ([a-z]+)", help_text))
        for command in re.findall(r"`(?:pecofence-cli )?([a-z]+)(?: [^`]*)?`", skill):
            if command in {"status", "fences", "suggest", "order", "quit"}:
                self.assertIn(command, commands)
        used = set(re.findall(r"`[^`]*?(--[a-z]+)", skill)) - {"--help"}
        cli_flags = set(re.findall(r"--[a-z]+", help_text))
        package_help = subprocess.run(
            [sys.executable, str(SCRIPT), "verify", "--help"], capture_output=True, text=True
        ).stdout + subprocess.run(
            [sys.executable, str(SCRIPT), "install", "--help"], capture_output=True, text=True
        ).stdout
        for flag in used:
            self.assertTrue(
                flag in cli_flags or flag in package_help or flag in {"--shell", "--validate", "--probe"},
                f"skill mentions {flag}, which no documented command accepts",
            )
        self.assertIn("never force", skill.lower().replace("never forces", "never force"))


if __name__ == "__main__":
    unittest.main()

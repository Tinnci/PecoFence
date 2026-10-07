import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "readme_translations", Path(__file__).with_name("check-readme-translations.py"))
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class ReadmeTranslationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.write("README.md", "# English\n")
        for language in CHECKER.LANGUAGES:
            self.write(f"docs/readme/README.{language}.md", f"# {language}\n")

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def test_editorial_layout_can_differ_when_links_are_valid(self):
        self.write("docs/guide.md", "# Guide\n")
        self.write("docs/assets/illustration.png", "placeholder")
        self.write(
            "docs/readme/README.fr.md",
            """# Bienvenue

## Deuxième partie

| Sujet | Détail |
| :--- | :--- |
| Un | Exemple |

![Illustration](../assets/illustration.png)

<details>
<summary>En savoir plus</summary>

```text
exemple
```
</details>

[Guide](../guide.md#guide)
""",
        )

        checked, problems = CHECKER.check_repository(self.root)

        self.assertEqual(checked, 10)
        self.assertEqual(problems, [])

    def test_missing_translation_is_reported(self):
        (self.root / "docs/readme/README.ja.md").unlink()

        checked, problems = CHECKER.check_repository(self.root)

        self.assertEqual(checked, 9)
        self.assertIn("missing docs/readme/README.ja.md", problems)

    def test_missing_relative_file_and_anchors_are_reported(self):
        self.write(
            "README.md",
            """# Present

[Missing file](missing.md)
[Missing page anchor](#absent)
[Missing linked-file anchor](docs/guide.md#absent)
""",
        )
        self.write("docs/guide.md", "# Present\n")

        _, problems = CHECKER.check_repository(self.root)

        self.assertTrue(any("missing link target missing.md" in problem for problem in problems))
        self.assertTrue(any("anchor #absent has no heading" in problem for problem in problems))
        self.assertTrue(any(
            "docs/guide.md#absent anchor not found" in problem for problem in problems
        ))

    def test_invalid_utf8_is_reported_without_aborting(self):
        path = self.root / "README.md"
        path.write_bytes(b"# Invalid UTF-8: \xff\n")

        checked, problems = CHECKER.check_repository(self.root)

        self.assertEqual(checked, 10)
        self.assertTrue(any(
            "README.md: cannot read UTF-8 content" in problem for problem in problems
        ))

    def test_duplicate_heading_anchors_use_incrementing_suffixes(self):
        self.write(
            "README.md",
            """# Repeat
# Repeat

[Second heading](#repeat-1)
""",
        )

        _, problems = CHECKER.check_repository(self.root)

        self.assertEqual(problems, [])

        self.write(
            "README.md",
            """# Repeat
# Repeat

[Nonexistent third heading](#repeat-2)
""",
        )
        _, problems = CHECKER.check_repository(self.root)
        self.assertTrue(any(
            "anchor #repeat-2 has no heading" in problem for problem in problems
        ))


if __name__ == "__main__":
    unittest.main()

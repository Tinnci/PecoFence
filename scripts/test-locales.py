"""Offline regressions for native Rust label scanning."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("check_locales", Path(__file__).with_name("check-locales.py"))
locales = importlib.util.module_from_spec(spec)
spec.loader.exec_module(locales)


class NativeLabelTests(unittest.TestCase):
    def test_native_font_icon_is_not_a_translation_key(self):
        glyph = locales.rust_literal(r'"\u{E713}"')
        self.assertEqual(glyph, "\ue713")
        self.assertIsNone(locales.CJK.search(glyph))

    def test_escaped_chinese_labels_are_decoded(self):
        self.assertEqual(locales.rust_literal(r'"\u{4E2D}\u{6587}"'), "中文")
        self.assertEqual(locales.rust_literal(r'"\u{4_E2D}"'), "中")

    def test_literal_backslash_is_not_a_unicode_escape(self):
        self.assertEqual(locales.rust_literal(r'"\\u{E713}"'), r"\u{E713}")

    def test_rust_nul_is_not_python_octal(self):
        self.assertEqual(locales.rust_literal(r'"\01"'), "\0" + "1")

    def test_whitespace_and_line_continuation(self):
        self.assertEqual(locales.rust_literal('"中\\\n  文"'), "中文")
        self.assertEqual(locales.rust_literal('"中\n文"'), "中\n文")
        self.assertEqual(locales.rust_literal(r'"中\t\"文\"\x21"'), '中\t"文"!')

    def test_placeholder_validation_is_preserved(self):
        self.assertEqual(locales.placeholders("修订 {0} / %1 / {1}"), ["%1", "{0}", "{1}"])


if __name__ == "__main__":
    unittest.main()

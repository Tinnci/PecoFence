"""Exercise site publication identity and real generated pages without deploying."""
import html
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from xml.etree import ElementTree

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("build_site", ROOT / "scripts/build-site.py")
site = importlib.util.module_from_spec(spec)
spec.loader.exec_module(site)


class PublicationLocationTests(unittest.TestCase):
    def test_unconfigured_site_is_a_local_preview_without_cname(self):
        self.assertEqual(site.publication_location({}), ("http://localhost:8000", None))

    def test_publishing_without_a_url_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "Publishing requires"):
            site.publication_location({}, deploy=True)

    def test_project_pages_url_does_not_become_a_custom_domain(self):
        self.assertEqual(
            site.publication_location({"baseUrl": "https://publisher.example/project/"}, deploy=True),
            ("https://publisher.example/project", None),
        )

    def test_custom_domain_must_match_the_publishing_url(self):
        config = {"baseUrl": "https://desktop.example", "customDomain": "desktop.example"}
        self.assertEqual(site.publication_location(config, deploy=True), ("https://desktop.example", "desktop.example"))
        with self.assertRaisesRegex(ValueError, "must match"):
            site.publication_location(config, base="https://other.example", deploy=True)

    def test_custom_domain_cannot_contain_a_project_path(self):
        with self.assertRaisesRegex(ValueError, "without a project path"):
            site.publication_location({
                "baseUrl": "https://desktop.example/project", "customDomain": "desktop.example",
            }, deploy=True)

    def test_local_override_does_not_emit_a_production_cname(self):
        self.assertEqual(
            site.publication_location({
                "baseUrl": "https://desktop.example", "customDomain": "desktop.example",
            }, base="http://localhost:9000"),
            ("http://localhost:9000", None),
        )

    def test_untrusted_url_shapes_are_rejected(self):
        for url in (
            "not-a-url", "ftp://desktop.example", "https://user:password@desktop.example",
            "https://@desktop.example", "https://desktop.example?token=value",
            "https://desktop.example#fragment", 'https://desktop.example/"quote',
            "https://desktop.example:invalid", r"https://desktop.example\path",
        ):
            with self.subTest(url=url), self.assertRaises(ValueError):
                site.publication_location({}, base=url)

    def test_publishing_local_addresses_and_http_is_rejected(self):
        for url in (
            "http://desktop.example", "https://localhost", "https://localhost.",
            "https://demo.localhost", "https://127.0.0.2", "https://[::1]", "https://192.168.1.10",
        ):
            with self.subTest(url=url), self.assertRaisesRegex(ValueError, "Publishing requires"):
                site.publication_location({}, base=url, deploy=True)

    def test_custom_domain_must_be_a_hostname(self):
        for domain in ("https://desktop.example", "desktop.example/path", "desktop.example\nother.example"):
            with self.subTest(domain=domain), self.assertRaisesRegex(ValueError, "must be a hostname"):
                site.publication_location({"customDomain": domain})

    def test_configuration_types_are_checked(self):
        for config in ({"baseUrl": 0}, {"customDomain": True}):
            with self.subTest(config=config), self.assertRaises(ValueError):
                site.publication_location(config)


class GeneratedSiteTests(unittest.TestCase):
    def test_preview_and_deployment_use_this_repository(self):
        config = json.loads((ROOT / "site/site.json").read_text(encoding="utf-8"))
        repository = config["repository"].rstrip("/")
        domain = config.get("customDomain")
        deployment_base = f"https://{domain}" if domain else "https://publisher.example/project&notes"
        cache = ROOT / ".cache"
        cache.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="site-test-", dir=cache) as temporary:
            out = Path(temporary) / "site"
            for base, deploy in (("http://localhost:8000", False), (deployment_base, True)):
                command = [sys.executable, str(ROOT / "scripts/build-site.py"), "--strict", "--out", str(out), "--base", base]
                if deploy:
                    command.append("--deploy")
                subprocess.run(command, cwd=ROOT, check=True, capture_output=True, text=True)
                if deploy and domain:
                    self.assertEqual((out / "CNAME").read_text(encoding="utf-8").strip(), domain)
                else:
                    self.assertFalse((out / "CNAME").exists())
                for language in config["languages"]:
                    page = (out / language["dir"] / "index.html").read_text(encoding="utf-8")
                    self.assertIn(f'href="{repository}/releases"', page)
                    self.assertIn(f'href="{repository}"', page)
                    self.assertIn(f'href="{html.escape(base)}/{language["dir"]}"', page)
                    self.assertNotIn("data-cf-beacon", page)
                    self.assertNotIn("apps.microsoft.com", page)
                    self.assertNotIn("{{", page)
                    self.assertNotIn("{t:", page)
                self.assertIn(f"{base}/sitemap.xml", (out / "robots.txt").read_text(encoding="utf-8"))
                sitemap = ElementTree.parse(out / "sitemap.xml")
                urls = [node.text for node in sitemap.iter("{http://www.sitemaps.org/schemas/sitemap/0.9}loc")]
                self.assertIn(f"{base}/zh-CN/", urls)

    def test_rejected_deployment_preserves_existing_output(self):
        cache = ROOT / ".cache"
        cache.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="site-test-", dir=cache) as temporary:
            out = Path(temporary)
            sentinel = out / "existing-output.txt"
            sentinel.write_text("keep me", encoding="utf-8")
            for base in ("", "http://localhost:8000"):
                result = subprocess.run(
                    [sys.executable, str(ROOT / "scripts/build-site.py"), "--deploy", "--out", str(out), "--base", base],
                    cwd=ROOT, capture_output=True, text=True,
                )
                self.assertEqual(result.returncode, 2)
                self.assertIn("Publishing requires", result.stderr)
                self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep me")


if __name__ == "__main__":
    unittest.main()

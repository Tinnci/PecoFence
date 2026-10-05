"""Exercise real Settings IPC in an isolated portable instance on Windows."""
import argparse
import json
import os
import re
import shutil
import subprocess
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LANGUAGES = ("en", "ja", "zh-TW", "ko", "de", "fr", "es", "pt-BR", "ru", "zh-CN")


def workspace_sample():
    """Generate the exact supported schema through the portable core, not a GUI first run."""
    result = subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "-p", "pecofence-core",
         "--example", "validate_workspace", "--", "--sample"],
        cwd=ROOT, check=True, capture_output=True, encoding="utf-8",
    )
    return json.loads(result.stdout)


def autostart_value():
    import winreg
    values = {}
    for name in ("PecoFence", "openFence"):
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Run") as key:
                values[name] = winreg.QueryValueEx(key, name)
        except FileNotFoundError:
            values[name] = None
    return values


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/pecofence.exe")
    parser.add_argument("--legacy-env", action="store_true", help="Exercise the previous instance-variable prefix")
    args = parser.parse_args()
    binary = args.binary.resolve()
    instance = "locale-smoke-" + uuid.uuid4().hex[:12]
    stage = ROOT / ".cache" / instance
    stage.mkdir(parents=True)
    shutil.copy2(binary, stage / "pecofence.exe")
    shutil.copy2(binary.parent / "pecofence-watchdog.exe", stage / "pecofence-watchdog.exe")
    shutil.copy2(ROOT / "third_party/webview2/WebView2Loader.x64.dll", stage / "WebView2Loader.dll")
    environment = dict(
        os.environ, PECOFENCE_INSTANCE=instance, RUST_LOG="info",
        LOCALAPPDATA=str(stage / "local-appdata"), APPDATA=str(stage / "roaming-appdata"),
    )
    if args.legacy_env:
        environment.pop("PECOFENCE_INSTANCE", None)
        environment["OPENFENCE_INSTANCE"] = instance
    before = autostart_value()

    def run(*extra):
        result = subprocess.run(
            [str(stage / "pecofence.exe"), "--portable", "--no-hide-icons", *extra],
            cwd=stage, env=environment, timeout=35, creationflags=subprocess.CREATE_NO_WINDOW,
        )
        if result.returncode:
            raise RuntimeError(f"Native instance failed with exit code {result.returncode}")

    # Preparing a seed does not start the desktop app or mutate Windows settings.
    config_path = stage / "config/workspace.v2.json"
    config_path.parent.mkdir()
    config = workspace_sample()
    settings = config["settings"]
    settings.update(language="zh-CN", autostart=False, hideRealIcons=False)
    settings["peek"]["enabled"] = False
    settings["quickHide"]["enabled"] = False
    custom_title = "名称 {1} custom"
    config["layouts"][0]["contents"][0]["title"] = custom_title
    config_path.write_text(json.dumps(config, ensure_ascii=False), encoding="utf-8")
    lines = ["sleep 1200"]
    for language in LANGUAGES:
        settings["language"] = language
        lines.extend((
            "message " + json.dumps({"type": "patchSettings", "settings": settings}, ensure_ascii=False),
            "sleep 180",
        ))
    lines.extend(("sleep 700", "exit"))
    script = stage / "languages.txt"
    script.write_text("\n".join(lines), encoding="utf-8")
    run("--open-settings", "--test-script", str(script), "--exit-after", "18000")
    saved = json.loads(config_path.read_text(encoding="utf-8"))
    log_path = Path(environment["LOCALAPPDATA"]) / "PecoFence" / f"pecofence.{instance}.log"
    log = log_path.read_text(encoding="utf-8")
    assert "settings: page ready" in log, "Embedded Settings document did not initialize"
    for language in LANGUAGES:
        assert re.search(r'interface language changed.*language="?'+re.escape(language)+r'(?:"|\s|$)', log), language
    assert saved["settings"]["language"] == "zh-CN", "Language preference was not saved"
    assert saved["layouts"][0]["contents"][0]["title"] == custom_title, "Custom content name changed"
    assert autostart_value() == before, "Portable test changed the installed autostart entry"
    report = {"passed": True, "languages": LANGUAGES, "settings_ready": True,
              "preference_saved": True, "custom_name_preserved": True, "autostart_preserved": True}
    (stage / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"PASS: native Settings initialized; {len(LANGUAGES)} live switches; configuration, names and autostart verified")
    print(f"Report: {stage / 'report.json'}")


if __name__ == "__main__":
    main()

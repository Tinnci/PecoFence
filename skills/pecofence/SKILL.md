---
name: pecofence
description: "Build, verify and install PecoFence Windows desktop packages, and diagnose its workspace or SPM connection. Use the packaging CLI; the GUI is not a task-query API."
---

# PecoFence desktop packages

PecoFence is a Windows GUI. Its agent-facing packaging tool is `scripts/package_desktop.py` in the PecoFence checkout. Locate the checkout using the local registry (`dev-cli list --json`) when available, or the user's selected repository.

```sh
python3 scripts/package_desktop.py --help
python3 scripts/package_desktop.py verify --archive release.zip --sha256 <trusted-sha256> --summary --json
python3 scripts/package_desktop.py install --archive release.zip --sha256 <trusted-sha256> --destination versions --dry-run --json
```

Run those examples from the checkout with Python 3.11 or newer. `verify` reads the archive; install dry-run also validates it, without extracting or launching executables. Use a trusted checksum; computing a checksum from an untrusted download does not authenticate it.

- `capture`/`record-build` write provenance; `portable` assembles a package; installation creates a new immutable version directory. Inspect child help before choosing paths and require clean source for release packaging.
- `--json` emits a versioned success/error envelope; errors are on stderr. Usage errors exit 2 and operational failures exit 1. Fix verification failures before installing, rather than retrying with checks disabled.
- Native `--validate-workspace <path>` validates a workspace; `--probe-spm` contacts the daemon. These are diagnostic entrypoints, not general task read/write APIs. Do not invent a TODO command or claim a packaging check tested GUI interactions.
- For an authorized upgrade, use the recorded current installation, back up configuration after graceful exit, keep the old version for rollback, update owned startup/shortcut entries, then verify running path, hashes and configuration recovery. Preserve user layouts and TODO contents.
- Workspace data and logs are private. Do not upload them or alter the SPM database merely to inspect desktop packaging.

# Verified Windows packaging and per-user installation

## One build, two consumers

`scripts/build-desktop.ps1` captures source/version/lockfile/compiler inputs, runs
the locked release build once, then verifies the inputs did not change and records
the hashes of both x64 GUI executables in `release/build-receipt.json`.

Both ZIP and MSIX consume that receipt through
[package_desktop.py](../scripts/package_desktop.py). `-SkipBuild` no longer means
“trust whatever happens to be in release”: changed source, version, compiler or
binary hashes cause refusal. `-Version` cannot relabel a binary. Tag releases also
require a clean source tree.

```powershell
uv run --no-project --python ">=3.11" pwsh -NoProfile -File scripts/build-and-verify.ps1
# Reuse exactly those verified binaries:
uv run --no-project --python ">=3.11" pwsh -NoProfile -File scripts/make-portable.ps1 -SkipBuild -TargetDir target
```

Use an ignored target directory, normally `target` or `target/package`. Changing
source after a build requires rerunning the build helper, even for documentation
changes; this is intentionally conservative provenance, not a signature.

The ZIP contains `package.json`: build inputs, per-file sizes/SHA-256, architecture,
image imports and runtime requirements. Entries are sorted with fixed timestamps
and permissions: identical payload bytes with the same compression toolchain
produce the same ZIP. This does not
claim reproducibility of Rust/Windows compiler output.

Packing uses a fresh temporary directory and does not delete an older extracted
version or its `config` directory. The result is verified before replacing the
output archive. License and NOTICE files are retained.

## Runtime requirements and native UI

The builder reads static and delay PE imports. The current executable imports
`WebView2Loader.dll`, so it packages the x64 loader/license and declares the
**external WebView2 Evergreen Runtime** requirement. The DLL alone is not the browser
runtime. The watchdog does not need WebView.

A future binary without that import does not automatically carry the WebView
payload. This is tested with synthetic native-only PE images, not a claim that
native Settings already exists. Dynamic `LoadLibrary` dependencies and any newly
introduced runtime must be declared/validated explicitly when implemented.
See [ADR-007](decisions/ADR-007-native-settings.md).

## Install without touching another version

The application itself and manual ZIP extraction do not require Python. The
optional developer/advanced-user install helper uses Python >=3.11 and PowerShell:

```powershell
uv run --no-project --python ">=3.11" pwsh -NoProfile -File scripts/install-user.ps1 `
  -Archive dist/pecofence-0.0.3-x64.zip -WhatIf
# Remove -WhatIf to install; -CreateShortcut is optional.
```

It validates the archive hash, flat allowlisted contents, payload hashes, PE
architecture and runtime metadata **before writing**. Path traversal, alternate
data streams, case-insensitive duplicate names, symlinks, extra binaries, private
keys and embedded user config are rejected.

Default destination: `%LOCALAPPDATA%\Tinnci\PecoFence\versions\<version>-<hash>`.
Each payload is immutable. Existing versions/running files are not overwritten;
an existing modified directory or user configuration causes refusal, not cleanup.
An optional owned “PecoFence (Tinnci)” Start Menu shortcut points to the selected
version. The helper never launches the app, downloads runtimes, edits autostart,
or deletes older versions.

Configuration remains `%APPDATA%\PecoFence\workspace.v2.json` unless the app is
explicitly launched with `--portable`. Installation path separation is **not**
runtime identity/data isolation from another PecoFence edition. Exit old instances
and follow [UPGRADING.md](UPGRADING.md). Remove unwanted older version directories
only after exiting those instances; preserve configuration separately.

SHA-256 proves integrity, not publisher authenticity. Obtain the ZIP and checksum
from a trusted source. The new helper refuses old ZIPs without a package manifest;
they can still be handled by their documented manual extraction workflow.

## MSIX and signing

Own Partner Center identity is still mandatory and unset by default. MSIX uses
the same verified executables/conditional runtime payload, then SDK-generated
manifest/assets/resources. Stable three-part versions must fit MSIX's 16-bit
components. See [STORE.md](STORE.md).

Test signing keeps a non-exportable key in the current-user certificate store,
signs by thumbprint, exports only the public `.cer`, then removes the temporary key
in `finally`. No private PFX/default password is written to `dist`. Test-signed
MSIX/certificates are not production release assets. We do not install/trust a
certificate automatically.

Real installation, shortcut creation, SDK packing and signing need deliberate
Windows validation. Automated tests use fake PE files and isolated destinations;
they never start PecoFence or modify user registry/certificate state.

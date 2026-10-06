# Optional Microsoft Store packaging (MSIX)

This edition has no configured Store identity or listing. Packaging and Partner
Center uploads are local/manual, not a CI Store release. An upstream publisher,
listing or certification result is not this edition's publishing authority.
See [INDEPENDENCE.md](INDEPENDENCE.md) for the remaining decisions.

## Obtain your own identity

Reserve a product in your own Microsoft Partner Center account if Store publishing
is desired. Copy the assigned values from **Product management → Product identity**
into `packaging/msix/identity.json`, or a separate JSON passed with `-Identity`:

```json
{
  "identityName": "<Partner Center-assigned package identity name>",
  "publisher": "<Partner Center-assigned publisher>",
  "publisherDisplayName": "<Partner Center-assigned publisher display name>"
}
```

All angle-bracket values above are **placeholders**, not usable identities. The
checked-in publishing identity fields are `null`. Do not guess values or reuse
another publisher's identity: the script fails early until the required fields
are supplied, and the manifest must match Partner Center. Identity values are
not credentials; keep account tokens and signing private keys out of source files.

## Prerequisites and build

- Windows 11 x64 and the native Rust/MSVC build prerequisites in
  [DEVELOPMENT.md](DEVELOPMENT.md).
- Windows SDK tools `MakeAppx`, `MakePri` and, for local signing, `SignTool`.
  One installation option is `winget install Microsoft.WindowsSDK.10.0.26100`.
- Python >=3.11 with Pillow for generating tile assets. Use `uv` for an isolated
  environment and install Pillow from PyPI:

  ```powershell
  uv venv --python ">=3.11" .venv-msix
  uv pip install --python .venv-msix/Scripts/python.exe --index-url https://pypi.org/simple Pillow
  ```

After configuring your own identity:

```powershell
./scripts/make-msix.ps1 -Python .venv-msix/Scripts/python.exe
./scripts/make-msix.ps1 -Python .venv-msix/Scripts/python.exe -SkipBuild
./scripts/make-msix.ps1 -Python .venv-msix/Scripts/python.exe -TestSign
```

The first command builds and packs unsigned; `-SkipBuild` requires the verified
build receipt in `target/package/release`; `-TestSign` additionally creates a
self-signed copy. ZIP/MSIX payloads share the implementation in [PACKAGING.md](PACKAGING.md).
To use a separate identity JSON, add `-Identity "path/to/your-identity.json"`
to any command. With an existing Python/Pillow installation, omit `-Python`.

`scripts/make-msix-assets.py` renders logos from `site/assets/mark.svg`.
`packaging/msix/AppxManifest.xml` supplies the manifest template: x64, Windows 11
minimum (`10.0.22000.0`), `runFullTrust` and a `windows.startupTask`.
The package version is `<Cargo.toml version>.0`; the Store requires the fourth
component to be zero. Stable version components must fit 0–65535. Portable and
MSIX packages include the license (`LICENSE` / `LICENSE.txt`), `NOTICE` and
third-party notices; verify these remain in the staged payload.

## Local install test

Exit running PecoFence instances first: the runtime single-instance identity is
still shared. On a test machine, import the generated public test certificate
from an administrator PowerShell, then install the test-signed package:

```powershell
Import-Certificate -FilePath dist/pecofence-test-signing.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople
Add-AppxPackage "dist/pecofence-<version>-x64-testsigned.msix"
```

Replace `<version>` with the built version. Trusting the certificate changes the
machine's certificate store; remove that trust when testing is finished.
The self-signed certificate is **local-test-only**, not a production code-signing
identity. Never upload the test-signed copy to the Store. Signing now uses a
temporary non-exportable key in CurrentUser/My and removes it in `finally`;
only the public `.cer` is exported, never a private PFX/default password.

Packaged startup uses the manifest startup task rather than the normal HKCU Run
entry; the in-app toggle opens Windows Startup settings. Windows virtualizes the
normal `%APPDATA%\PecoFence\workspace.v2.json` under the package's own identity-dependent
AppData location. Only supported schema-2 documents can be imported explicitly;
there is no automatic migration of portable/old-format data. Packaged writes and
uninstall behavior depend on Windows virtualization and the assigned identity.
Test first launch, updates, startup, uninstall and desktop-icon restoration with
your assigned identity rather than assuming portable/MSIX isolation.

## Manual Store submission

1. Review dependencies before upload, including static MSVC runtime linkage and
   system DLL requirements; run native/package validation on supported Windows.
2. Upload the **unsigned** `dist/pecofence-<version>-x64.msix` in your own Partner
   Center submission. The Store signs accepted packages after certification;
   Store upload does not require your own production signing certificate.
3. Decide pricing, markets, category, age ratings, support contacts and privacy
   disclosures from this edition's actual behavior and policies. No inherited
   pricing, privacy claim or certification history is promised here.
4. Supply accurate localized listings and screenshots that you may distribute.
   Explain desktop-icon hiding/restoration and full-trust desktop integration in
   certification notes, and verify the tray restoration action.

Store publishing and winget publishing are separate choices. Optional winget
automation requires `ENABLE_WINGET_PUBLISH=true`, an independently confirmed
`WINGET_PACKAGE_IDENTIFIER` variable and a `WINGET_TOKEN` secret; drafts and
prereleases are rejected for manual and automatic submissions. Establish an
accepted first package version in `winget-pkgs` before enabling the update
workflow; review its transitive tooling as described in [INDEPENDENCE.md](INDEPENDENCE.md).
No existing winget identity or Store listing is assigned to this edition.

# Third-party files

Earlier versions bundled `WebView2Loader.x64.dll` from Microsoft's
`Microsoft.Web.WebView2` NuGet package, with its license and package metadata.
The loader and browser assets have been removed for the Reactor / WinUI 3
Settings migration; the former files remain in repository history for attribution.

`winappsdk/` contains the audited self-contained Windows App SDK Runtime 2.5.1
inventory, activation manifest and redistribution license. Runtime binaries are
downloaded and verified into ignored build output by `scripts/stage-winappsdk-runtime.py`,
not committed here. The pinned NuGet archive is identified by SHA-256 and SHA-512;
the selected x64 inventory is 206 files / 59,158,503 bytes, including 200
PE images, and excludes WebView components. Build receipts, portable ZIPs, MSIX
staging and lab snapshots bind the exact inventory and file hashes. The package
includes the SDK redistribution license, inventory/provenance JSON and third-party
notices.

Downloaded NuGet archives and extracted build files remain local and are ignored.
The patched Rust Composition wrapper and its licenses are under
`vendor/windows-composition`. The separately patched Reactor teardown/guard source
and its attribution are under `vendor/windows-reactor`.

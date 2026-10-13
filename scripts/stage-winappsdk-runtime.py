"""Stage the pinned public Windows App SDK runtime beside Cargo outputs."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import urllib.error
import zipfile

import winappsdk_runtime as runtime


def _profile_directories(target_dir: Path, profile: str, target_triple: str | None) -> list[Path]:
    root = Path(target_dir)
    if target_triple:
        if not target_triple.replace("-", "").replace("_", "").isalnum():
            raise ValueError("unsafe Cargo target triple")
        root /= target_triple
    profile_dir = root / profile
    return [profile_dir, profile_dir / "deps"] if profile == "debug" else [profile_dir]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    stage = commands.add_parser("stage", help="download/verify the pinned NuGet and stage it")
    stage.add_argument("--target-dir", type=Path, required=True)
    stage.add_argument("--profile", choices=("debug", "release"), required=True)
    stage.add_argument("--target-triple")

    verify = commands.add_parser("verify", help="verify an existing staged runtime without network")
    verify.add_argument("--directory", type=Path, required=True)

    copy = commands.add_parser("copy", help="copy a verified runtime to a new deployment directory")
    copy.add_argument("--source", type=Path, required=True)
    copy.add_argument("--destination", type=Path, required=True)

    audit = commands.add_parser("audit", help="write reviewable candidate metadata from the pinned cache")
    audit.add_argument(
        "--output-dir", type=Path,
        default=runtime.DEFAULT_CACHE / "candidate",
    )

    args = parser.parse_args()
    manifest = runtime.load_runtime_manifest()
    if args.command == "stage":
        destinations = _profile_directories(args.target_dir, args.profile, args.target_triple)
        images = runtime.stage_runtime(destinations, manifest=manifest)
        print(
            f"Staged Windows App SDK Runtime {runtime.PACKAGE_VERSION} "
            f"({len(manifest['files'])} files; {len(images)} PE images) beside "
            + ", ".join(str(path) for path in destinations)
        )
    elif args.command == "verify":
        files, images = runtime.validate_runtime_directory(args.directory, manifest)
        print(f"Verified {len(files)} Windows App SDK runtime files and {len(images)} PE images.")
    elif args.command == "copy":
        runtime.copy_runtime(args.source, args.destination, manifest=manifest)
        print(f"Copied verified Windows App SDK runtime to {args.destination}")
    else:
        archive = runtime._verified_package(runtime.DEFAULT_CACHE, manifest)
        from package_desktop import pe_image
        candidate, app_manifest, license_data = runtime.build_candidate(archive, pe_image)
        output = Path(args.output_dir)
        output.mkdir(parents=True, exist_ok=True)
        (output / "runtime-manifest.json").write_bytes(runtime.manifest_bytes(candidate))
        (output / "app.manifest").write_bytes(app_manifest)
        (output / "WINDOWS-APP-SDK-LICENSE.txt").write_bytes(license_data)
        print(f"Wrote pinned runtime audit candidate files under {output}")


if __name__ == "__main__":
    try:
        main()
    except (
        ValueError, OSError, zipfile.BadZipFile, urllib.error.URLError,
        KeyError, UnicodeError, json.JSONDecodeError,
    ) as error:
        raise SystemExit(f"Windows App SDK staging: {error}") from error

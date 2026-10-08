"""Validate the one public Cargo Git object database before Cargo reuses it.

Cargo/libgit2 can omit a remote URL from its bare database config. The database
directory's Cargo SourceId hash plus the lockfile boundary checked in the
SourceOnly preflight identify that source; any configured remote must also match.
"""
from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

WINDOWS_RS_URL = "https://github.com/microsoft/windows-rs"
WINDOWS_RS_REVISION = "2672e615d9cc0771448a781f3b2fe34e7fd08c6a"
# Cargo's URL-derived name for this source in `CARGO_HOME/git/db`.
WINDOWS_RS_DB_NAME = "windows-rs-a5de4a2dc783ec71"
FULL_REVISION = re.compile(r"^[0-9a-f]{40}$")


class CacheValidationError(RuntimeError):
    pass


def _is_link_or_reparse(path: Path) -> bool:
    try:
        info = path.lstat()
    except FileNotFoundError:
        return False
    if stat.S_ISLNK(info.st_mode):
        return True
    attributes = getattr(info, "st_file_attributes", 0)
    reparse_point = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    return bool(attributes & reparse_point)


def _reject_links(path: Path) -> None:
    if _is_link_or_reparse(path):
        raise CacheValidationError(f"cache contains a symbolic link or reparse point: {path}")
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) and not stat.S_ISDIR(info.st_mode):
        raise CacheValidationError(f"cache contains a non-regular entry: {path}")
    if path.is_dir():
        for directory, child_directories, files in os.walk(path, followlinks=False):
            parent = Path(directory)
            for name in [*child_directories, *files]:
                child = parent / name
                if _is_link_or_reparse(child):
                    raise CacheValidationError(
                        f"cache contains a symbolic link or reparse point: {child}"
                    )
                info = child.lstat()
                if not stat.S_ISREG(info.st_mode) and not stat.S_ISDIR(info.st_mode):
                    raise CacheValidationError(f"cache contains a non-regular entry: {child}")


def _git_environment() -> dict[str, str]:
    environment = os.environ.copy()
    for name in (
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
    ):
        environment.pop(name, None)
    environment["GIT_CONFIG_NOSYSTEM"] = "1"
    environment["GIT_CONFIG_GLOBAL"] = os.devnull
    environment["GIT_TERMINAL_PROMPT"] = "0"
    environment["GIT_NO_REPLACE_OBJECTS"] = "1"
    return environment


def _git_config_entries(config: Path) -> list[tuple[str, str]]:
    result = subprocess.run(
        ["git", "config", "--no-includes", "--file", str(config), "--null", "--list"],
        check=False,
        capture_output=True,
        env=_git_environment(),
    )
    if result.returncode:
        raise CacheValidationError("cannot safely read cached Git configuration")
    entries = []
    for item in result.stdout.split(b"\0"):
        if not item:
            continue
        key, separator, value = item.partition(b"\n")
        if not separator:
            raise CacheValidationError("cached Git configuration has an invalid entry")
        entries.append((key.decode("utf-8", errors="replace").lower(),
                        value.decode("utf-8", errors="replace")))
    return entries


def _run_git(database: Path, *arguments: str) -> str:
    result = subprocess.run(
        ["git", f"--git-dir={database}", *arguments],
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        env=_git_environment(),
    )
    if result.returncode:
        detail = (result.stderr or result.stdout).strip()
        raise CacheValidationError(
            f"cached Git database failed `git {' '.join(arguments)}`: {detail}"
        )
    return result.stdout.strip()


def _validate_snapshot_hashes(database: Path, revision: str) -> None:
    """Hash every object Cargo can consume at the exact pinned commit.

    Rehashing all historical SDK blobs dominates restore time and verifies code
    that this build never uses. Verify history connectivity separately, and
    stream the complete pinned snapshot through one Git batch process instead.
    """
    root = _run_git(database, "rev-parse", "--verify", f"{revision}^{{tree}}")
    objects = {revision: "commit", root: "tree"}
    listing = subprocess.run(
        ["git", f"--git-dir={database}", "ls-tree", "-r", "-t", "-z", revision],
        capture_output=True, env=_git_environment(), check=False,
    )
    if listing.returncode:
        raise CacheValidationError("cannot enumerate the pinned source snapshot")
    for record in listing.stdout.split(b"\0"):
        if not record:
            continue
        metadata, _, _path = record.partition(b"\t")
        _mode, kind, oid = metadata.decode("ascii").split()
        if kind not in ("tree", "blob") or not FULL_REVISION.fullmatch(oid):
            raise CacheValidationError("pinned snapshot contains unsupported object metadata")
        objects[oid] = kind

    # A file-backed input avoids a pipe deadlock when object IDs and Git output
    # both exceed pipe capacity. Object bodies are hashed in bounded chunks.
    with tempfile.TemporaryFile() as requests, tempfile.TemporaryFile() as errors:
        requests.write("".join(f"{oid}\n" for oid in objects).encode("ascii"))
        requests.seek(0)
        process = subprocess.Popen(
            ["git", f"--git-dir={database}", "cat-file", "--batch"],
            stdin=requests, stdout=subprocess.PIPE, stderr=errors,
            env=_git_environment(),
        )
        try:
            for oid, kind in objects.items():
                header = process.stdout.readline().split()
                if len(header) != 3 or header[0] != oid.encode() or header[1] != kind.encode():
                    raise CacheValidationError("missing or mismatched pinned Git object")
                size = int(header[2])
                if size < 0:
                    raise CacheValidationError("invalid Git object size")
                digest = hashlib.sha1(usedforsecurity=False)
                digest.update(f"{kind} {size}\0".encode("ascii"))
                remaining = size
                while remaining:
                    chunk = process.stdout.read(min(remaining, 1024 * 1024))
                    if not chunk:
                        raise CacheValidationError("truncated pinned Git object")
                    digest.update(chunk)
                    remaining -= len(chunk)
                if process.stdout.read(1) != b"\n" or digest.hexdigest() != oid:
                    raise CacheValidationError("pinned Git object content hash mismatch")
            if process.wait() != 0:
                raise CacheValidationError("Git could not read the complete pinned snapshot")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            process.stdout.close()


def _validate_database(database: Path, expected_revision: str) -> None:
    _reject_links(database)
    if not database.is_dir():
        raise CacheValidationError(f"cached Git database is not a directory: {database}")

    config = database / "config"
    if _is_link_or_reparse(config) or not config.is_file():
        raise CacheValidationError(f"cached Git database has no regular config file: {config}")

    config_entries = _git_config_entries(config)
    allowed_config = {
        "core.repositoryformatversion": {"0"},
        "core.bare": {"true"},
        "core.filemode": {"true", "false"},
        "core.ignorecase": {"true", "false"},
        "core.symlinks": {"true", "false"},
        "core.logallrefupdates": {"true", "false"},
        "remote.origin.url": {WINDOWS_RS_URL},
        "remote.origin.fetch": {"+refs/heads/*:refs/remotes/origin/*"},
    }
    values: dict[str, str] = {}
    for key, value in config_entries:
        if key == "remote.origin.url" and value != WINDOWS_RS_URL:
            raise CacheValidationError("cached Git database has a conflicting remote identity")
        if key not in allowed_config or value not in allowed_config[key]:
            if key.startswith("credential."):
                raise CacheValidationError(
                    "cached Git database config contains credential settings"
                )
            raise CacheValidationError(
                "cached Git database config contains an unapproved setting"
            )
        if key in values:
            raise CacheValidationError(
                "cached Git database config repeats a setting and may override it"
            )
        values[key] = value
    if values.get("core.repositoryformatversion") != "0" or values.get("core.bare") != "true":
        raise CacheValidationError(
            "cached Git database config is not a standard bare Cargo object database"
        )

    substitutions = (
        database / "objects/info/alternates",
        database / "objects/info/http-alternates",
        database / "info/grafts",
        database / "shallow",
        database / "commondir",
        database / "refs/replace",
    )
    promised_packs = (
        tuple((database / "objects/pack").glob("*.promisor"))
        if (database / "objects/pack").is_dir()
        else ()
    )
    if any(path.exists() for path in substitutions) or promised_packs:
        raise CacheValidationError(
            "cached Git database contains alternate, shallow, graft, replace, or promisor metadata"
        )

    # Reject checkout indirection and unrelated payloads before any repository
    # discovery. Explicit --git-dir also prevents a nested .git from masking the
    # object database that Cargo will consume.
    files = {"config", "description", "HEAD", "FETCH_HEAD", "packed-refs"}
    directories = {"hooks", "info", "objects", "refs", "logs"}
    for entry in database.iterdir():
        if not ((entry.name in files and entry.is_file()) or
                (entry.name in directories and entry.is_dir())):
            raise CacheValidationError(
                f"cached Git database contains an unapproved root entry: {entry.name}"
            )
    hooks = database / "hooks"
    if hooks.is_dir():
        for hook in hooks.iterdir():
            if not hook.is_file() or not hook.name.endswith(".sample"):
                raise CacheValidationError("cached Git database contains an active Git hook")

    if _run_git(database, "rev-parse", "--is-bare-repository") != "true":
        raise CacheValidationError("cached Cargo Git database is not a bare object database")
    if _run_git(database, "for-each-ref", "--format=%(refname)", "refs/replace"):
        raise CacheValidationError("cached Git database contains replacement refs")

    if not FULL_REVISION.fullmatch(expected_revision):
        raise CacheValidationError("expected revision must be a full 40-character commit ID")
    try:
        resolved = _run_git(
            database, "rev-parse", "--verify", f"{expected_revision}^{{commit}}"
        )
    except CacheValidationError as error:
        raise CacheValidationError(
            f"cached Git database does not contain expected revision {expected_revision}: {error}"
        ) from error
    if resolved != expected_revision:
        raise CacheValidationError(
            "cached Git database resolved the expected revision to a different commit"
        )

    object_type = _run_git(database, "cat-file", "-t", expected_revision)
    if object_type != "commit":
        raise CacheValidationError("expected Windows SDK revision is not a commit object")

    _run_git(database, "fsck", "--connectivity-only", "--strict", "--no-reflogs", expected_revision)
    _validate_snapshot_hashes(database, expected_revision)


def validate_database_root(
    database_root: Path,
    *,
    expected_revision: str = WINDOWS_RS_REVISION,
    allow_empty: bool = False,
) -> Path | None:
    """Validate the Cargo Git DB root, allowing only its single approved source."""
    database_root = Path(database_root)
    if _is_link_or_reparse(database_root):
        raise CacheValidationError(f"Cargo Git database root is a link or reparse point: {database_root}")
    if not database_root.exists():
        if allow_empty:
            return None
        raise CacheValidationError(f"Cargo Git database root is missing: {database_root}")
    if not database_root.is_dir():
        raise CacheValidationError(f"Cargo Git database root is not a directory: {database_root}")

    entries = sorted(database_root.iterdir(), key=lambda path: path.name)
    if not entries and allow_empty:
        return None
    if len(entries) != 1:
        raise CacheValidationError(
            "Cargo Git database cache must contain exactly one database "
            f"(the pinned public windows-rs source); found {len(entries)} entries"
        )

    database = entries[0]
    if _is_link_or_reparse(database) or not database.is_dir():
        raise CacheValidationError(
            f"Cargo Git database cache contains a non-database entry: {database.name}"
        )
    if database.name != WINDOWS_RS_DB_NAME:
        raise CacheValidationError(
            "Cargo Git database directory does not identify the approved windows-rs source "
            f"(expected {WINDOWS_RS_DB_NAME!r}, found {database.name!r})"
        )
    _validate_database(database, expected_revision)
    return database


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--database-root",
        required=True,
        type=Path,
        help="Cargo's git/db directory in the fresh public CARGO_HOME",
    )
    parser.add_argument(
        "--allow-empty",
        action="store_true",
        help="succeed when the cache has not yet been populated (before cargo fetch)",
    )
    arguments = parser.parse_args()
    try:
        database = validate_database_root(
            arguments.database_root,
            allow_empty=arguments.allow_empty,
        )
    except (CacheValidationError, OSError) as error:
        print(f"Public Cargo Git cache validation failed: {error}", file=sys.stderr)
        return 1
    if database is None:
        print("No restored Cargo Git database; public fetch will populate it.")
    else:
        print(
            "Validated public windows-rs Cargo Git object database at "
            f"{WINDOWS_RS_REVISION}: {database}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

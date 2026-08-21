"""Keep DocFlow's release version and release catalog consistent.

This module intentionally uses only the Python standard library so local version
checks remain free, fast, and available before project dependencies are installed.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tomllib
from datetime import date
from pathlib import Path
from typing import Any, Sequence


ROOT = Path(__file__).resolve().parents[1]
SEMVER = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)
MANIFEST_PATHS = (
    "package.json",
    "package-lock.json",
    "pyproject.toml",
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
    "src-tauri/tauri.conf.json",
    "src/docflow/__init__.py",
)
RELEASE_STATUSES = {"current", "supported", "superseded", "archived", "trial"}
RELEASE_CHANNELS = {"stable", "preview", "trial"}


class VersionError(ValueError):
    """The repository's version contract is incomplete or inconsistent."""


def parse_version(value: str) -> tuple[int, int, int, tuple[str, ...]]:
    """Parse the supported SemVer subset or raise ``VersionError``."""
    match = SEMVER.fullmatch(value.strip())
    if match is None:
        raise VersionError(f"invalid semantic version: {value!r}")
    prerelease = tuple((match.group(4) or "").split(".")) if match.group(4) else ()
    if any(part.isdigit() and len(part) > 1 and part.startswith("0") for part in prerelease):
        raise VersionError(
            f"numeric prerelease identifiers cannot have leading zeroes: {value!r}"
        )
    return int(match.group(1)), int(match.group(2)), int(match.group(3)), prerelease


def canonical_version(root: Path = ROOT) -> str:
    value = (root / "VERSION").read_text(encoding="utf-8").strip()
    parse_version(value)
    return value


def _load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise VersionError(f"expected a JSON object: {path}")
    return value


def _project_version(path: Path, section: str) -> str:
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    table = data.get(section)
    version = table.get("version") if isinstance(table, dict) else None
    if not isinstance(version, str):
        raise VersionError(f"missing [{section}].version in {path}")
    return version


def _cargo_lock_version(path: Path) -> str:
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    packages = data.get("package")
    if not isinstance(packages, list):
        raise VersionError(f"missing package entries in {path}")
    matches = [item for item in packages if item.get("name") == "docflow"]
    version = matches[0].get("version") if len(matches) == 1 else None
    if not isinstance(version, str):
        raise VersionError(f"expected one docflow package in {path}")
    return version


def manifest_versions(root: Path = ROOT) -> dict[str, str]:
    package = _load_json(root / "package.json")
    package_lock = _load_json(root / "package-lock.json")
    lock_root = package_lock.get("packages", {}).get("", {})
    lock_versions = {package_lock.get("version"), lock_root.get("version")}
    if len(lock_versions) != 1 or not all(isinstance(item, str) for item in lock_versions):
        raise VersionError("package-lock.json root versions disagree")
    package_version = package.get("version")
    lock_version = next(iter(lock_versions))
    if not isinstance(package_version, str) or not isinstance(lock_version, str):
        raise VersionError("package manifests have no string version")

    tauri = _load_json(root / "src-tauri/tauri.conf.json")
    tauri_version = tauri.get("version")
    if not isinstance(tauri_version, str):
        raise VersionError("src-tauri/tauri.conf.json has no string version")
    python_source = (root / "src/docflow/__init__.py").read_text(encoding="utf-8")
    python_match = re.search(r'^__version__\s*=\s*"([^"]+)"\s*$', python_source, re.M)
    if python_match is None:
        raise VersionError("missing __version__ in src/docflow/__init__.py")

    versions: dict[str, str] = {
        "package.json": package_version,
        "package-lock.json": lock_version,
        "pyproject.toml": _project_version(root / "pyproject.toml", "project"),
        "src-tauri/Cargo.toml": _project_version(
            root / "src-tauri/Cargo.toml", "package"
        ),
        "src-tauri/Cargo.lock": _cargo_lock_version(root / "src-tauri/Cargo.lock"),
        "src-tauri/tauri.conf.json": tauri_version,
        "src/docflow/__init__.py": python_match.group(1),
    }
    for path, value in versions.items():
        if not isinstance(value, str):
            raise VersionError(f"missing version in {path}")
        parse_version(value)
    return versions


def load_release_registry(root: Path = ROOT) -> dict[str, Any]:
    return _load_json(root / "releases.json")


def release_notes(root: Path, version: str) -> str:
    parse_version(version)
    text = (root / "CHANGELOG.md").read_text(encoding="utf-8")
    heading = re.compile(rf"^##\s+\[?{re.escape(version)}\]?(?:\s+-.*)?\s*$", re.M)
    match = heading.search(text)
    if match is None:
        raise VersionError(f"CHANGELOG.md has no section for {version}")
    start = match.end()
    next_heading = re.search(r"^##\s+", text[start:], re.M)
    end = start + next_heading.start() if next_heading else len(text)
    notes = text[start:end].strip()
    if not notes:
        raise VersionError(f"CHANGELOG.md section for {version} is empty")
    return notes


def validate_repository(root: Path = ROOT, *, require_tag: bool = False) -> list[str]:
    errors: list[str] = []
    try:
        current = canonical_version(root)
    except (OSError, VersionError) as exc:
        return [str(exc)]

    try:
        versions = manifest_versions(root)
        for path, manifest_version in versions.items():
            if manifest_version != current:
                errors.append(f"{path}: {manifest_version} != VERSION {current}")
    except (OSError, VersionError, tomllib.TOMLDecodeError, json.JSONDecodeError) as exc:
        errors.append(str(exc))

    try:
        registry = load_release_registry(root)
        if registry.get("schemaVersion") != 1:
            errors.append("releases.json: schemaVersion must be 1")
        if registry.get("currentVersion") != current:
            errors.append("releases.json: currentVersion does not match VERSION")
        releases = registry.get("releases")
        if not isinstance(releases, list) or not releases:
            errors.append("releases.json: releases must be a non-empty list")
        else:
            seen_versions: set[str] = set()
            seen_tags: set[str] = set()
            current_count = 0
            for index, release in enumerate(releases):
                if not isinstance(release, dict):
                    errors.append(f"releases.json: release {index} must be an object")
                    continue
                release_version = release.get("version")
                if not isinstance(release_version, str):
                    errors.append(f"releases.json: release {index} has no version")
                    continue
                try:
                    parse_version(release_version)
                except VersionError as exc:
                    errors.append(str(exc))
                if release_version in seen_versions:
                    errors.append(f"releases.json: duplicate version {release_version}")
                seen_versions.add(release_version)
                if release.get("status") not in RELEASE_STATUSES:
                    errors.append(f"releases.json: invalid status for {release_version}")
                if release.get("channel") not in RELEASE_CHANNELS:
                    errors.append(f"releases.json: invalid channel for {release_version}")
                release_date = release.get("date")
                try:
                    if not isinstance(release_date, str):
                        raise ValueError
                    date.fromisoformat(release_date)
                except ValueError:
                    errors.append(f"releases.json: invalid date for {release_version}")
                if not isinstance(release.get("summary"), str) or not release["summary"].strip():
                    errors.append(f"releases.json: missing summary for {release_version}")
                tag = release.get("tag")
                if tag is not None:
                    if not isinstance(tag, str) or not tag.startswith("v"):
                        errors.append(f"releases.json: invalid tag for {release_version}")
                    elif tag != f"v{release_version}":
                        errors.append(
                            f"releases.json: tag {tag} does not identify {release_version}"
                        )
                    elif tag in seen_tags:
                        errors.append(f"releases.json: duplicate tag {tag}")
                    else:
                        seen_tags.add(tag)
                if release.get("status") == "current":
                    current_count += 1
                    if release_version != current:
                        errors.append(
                            f"releases.json: current release {release_version} != VERSION {current}"
                        )
                    if tag != f"v{release_version}":
                        errors.append(
                            f"releases.json: current release {release_version} "
                            f"requires tag v{release_version}"
                        )
            if current not in seen_versions:
                errors.append(f"releases.json: missing current version {current}")
            if current_count != 1:
                errors.append("releases.json: exactly one release must be current")
        release_notes(root, current)
    except (OSError, VersionError, json.JSONDecodeError) as exc:
        errors.append(str(exc))

    if require_tag:
        try:
            tags = subprocess.check_output(
                ["git", "tag", "--points-at", "HEAD"], cwd=root, text=True
            ).splitlines()
        except (OSError, subprocess.CalledProcessError) as exc:
            errors.append(f"could not inspect Git tags: {exc}")
        else:
            expected = f"v{current}"
            if expected not in tags:
                errors.append(f"HEAD is not tagged {expected}")

    return errors


def _replace_section_version(text: str, section: str, version: str, path: str) -> str:
    section_match = re.search(rf"^\[{re.escape(section)}\]\s*$", text, re.M)
    if section_match is None:
        raise VersionError(f"could not find [{section}] in {path}")
    start = section_match.end()
    next_section = re.search(r"^\[", text[start:], re.M)
    end = start + next_section.start() if next_section else len(text)
    block = text[start:end]
    updated_block, count = re.subn(
        r'(^version\s*=\s*)"[^"]+"',
        rf'\g<1>"{version}"',
        block,
        count=1,
        flags=re.M,
    )
    if count != 1:
        raise VersionError(f"could not update [{section}].version in {path}")
    return text[:start] + updated_block + text[end:]


def _replace_cargo_lock_version(text: str, version: str) -> str:
    pattern = re.compile(
        r'(^\[\[package\]\]\s*\nname = "docflow"\s*\nversion = )"[^"]+"', re.M
    )
    updated, count = pattern.subn(rf'\g<1>"{version}"', text, count=1)
    if count != 1:
        raise VersionError("could not update docflow version in src-tauri/Cargo.lock")
    return updated


def _json_text(data: dict[str, Any]) -> str:
    return json.dumps(data, ensure_ascii=False, indent=2) + "\n"


def set_version(root: Path, version: str) -> None:
    """Validate every transform, then update all runtime manifests.

    Release metadata stays an explicit review step. A malformed manifest fails
    before any file is written, so a validation error cannot leave mixed versions.
    """
    parse_version(version)
    updates: dict[Path, str] = {root / "VERSION": version + "\n"}

    package_path = root / "package.json"
    package = _load_json(package_path)
    package["version"] = version
    updates[package_path] = _json_text(package)

    lock_path = root / "package-lock.json"
    lock = _load_json(lock_path)
    packages = lock.get("packages")
    lock_root = packages.get("") if isinstance(packages, dict) else None
    if not isinstance(lock_root, dict):
        raise VersionError("package-lock.json has no root package entry")
    lock["version"] = version
    lock_root["version"] = version
    updates[lock_path] = _json_text(lock)

    pyproject_path = root / "pyproject.toml"
    updates[pyproject_path] = _replace_section_version(
        pyproject_path.read_text(encoding="utf-8"),
        "project",
        version,
        "pyproject.toml",
    )

    cargo_path = root / "src-tauri/Cargo.toml"
    updates[cargo_path] = _replace_section_version(
        cargo_path.read_text(encoding="utf-8"),
        "package",
        version,
        "src-tauri/Cargo.toml",
    )

    cargo_lock_path = root / "src-tauri/Cargo.lock"
    updates[cargo_lock_path] = _replace_cargo_lock_version(
        cargo_lock_path.read_text(encoding="utf-8"), version
    )

    tauri_path = root / "src-tauri/tauri.conf.json"
    tauri = _load_json(tauri_path)
    tauri["version"] = version
    updates[tauri_path] = _json_text(tauri)

    init_path = root / "src/docflow/__init__.py"
    source = init_path.read_text(encoding="utf-8")
    source, count = re.subn(
        r'^__version__\s*=\s*"[^"]+"\s*$',
        f'__version__ = "{version}"',
        source,
        count=1,
        flags=re.M,
    )
    if count != 1:
        raise VersionError("could not update src/docflow/__init__.py")
    updates[init_path] = source

    for path, content in updates.items():
        path.write_text(content, encoding="utf-8")


def _list_releases(root: Path, *, as_json: bool) -> None:
    releases = load_release_registry(root)["releases"]
    if as_json:
        print(json.dumps(releases, ensure_ascii=False, indent=2))
        return
    print("VERSION                 DATE         CHANNEL  STATUS       TAG")
    for release in reversed(releases):
        print(
            f"{release['version']:<23} {release['date']:<12} "
            f"{release['channel']:<8} {release['status']:<12} "
            f"{release.get('tag') or '-'}"
        )
        print(f"  {release['summary']}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT, help=argparse.SUPPRESS)
    subparsers = parser.add_subparsers(dest="command", required=True)

    check = subparsers.add_parser("check", help="validate version and release metadata")
    check.add_argument("--require-tag", action="store_true")

    set_command = subparsers.add_parser("set", help="update every runtime manifest")
    set_command.add_argument("version")

    list_command = subparsers.add_parser("list", help="print the complete release catalog")
    list_command.add_argument("--json", action="store_true")

    notes = subparsers.add_parser("notes", help="print changelog notes for one version")
    notes.add_argument("version")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    root = args.root.resolve()
    try:
        if args.command == "check":
            errors = validate_repository(root, require_tag=args.require_tag)
            if errors:
                for error in errors:
                    print(f"ERROR: {error}", file=sys.stderr)
                return 1
            print(f"DocFlow {canonical_version(root)} version contract: OK")
            return 0
        if args.command == "set":
            set_version(root, args.version)
            print(
                f"Updated runtime manifests to {args.version}. "
                "Now update releases.json and CHANGELOG.md, then run check."
            )
            return 0
        if args.command == "list":
            _list_releases(root, as_json=args.json)
            return 0
        if args.command == "notes":
            print(release_notes(root, args.version))
            return 0
    except (OSError, VersionError, KeyError, TypeError, json.JSONDecodeError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    raise AssertionError(f"unhandled command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())

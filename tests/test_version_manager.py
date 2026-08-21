from __future__ import annotations

import json
import shutil
from pathlib import Path

import pytest

from tools.version_manager import (
    MANIFEST_PATHS,
    VersionError,
    canonical_version,
    load_release_registry,
    manifest_versions,
    parse_version,
    release_notes,
    set_version,
    validate_repository,
)


ROOT = Path(__file__).resolve().parents[1]


def copy_version_contract(target_root: Path) -> None:
    for relative in [
        "VERSION",
        "CHANGELOG.md",
        "releases.json",
        *MANIFEST_PATHS,
    ]:
        source = ROOT / relative
        target = target_root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def test_repository_version_contract_is_consistent() -> None:
    version = canonical_version(ROOT)
    versions = manifest_versions(ROOT)

    assert version == "0.6.0"
    assert set(versions) == {
        "package.json",
        "package-lock.json",
        "pyproject.toml",
        "src-tauri/Cargo.toml",
        "src-tauri/Cargo.lock",
        "src-tauri/tauri.conf.json",
        "src/docflow/__init__.py",
    }
    assert set(versions.values()) == {version}
    assert validate_repository(ROOT) == []


def test_release_registry_is_unique_complete_and_machine_readable() -> None:
    registry = load_release_registry(ROOT)
    releases = registry["releases"]
    versions = [release["version"] for release in releases]

    assert registry["schemaVersion"] == 1
    assert registry["currentVersion"] == canonical_version(ROOT)
    assert len(versions) == len(set(versions))
    assert {"0.1.0", "0.5.4", "0.5.4-tauri-preview", "0.6.0-test", "0.6.0"} <= set(
        versions
    )
    assert all(parse_version(version) for version in versions)
    assert any(
        release["version"] == "0.6.0" and release["status"] == "current"
        for release in releases
    )
    json.dumps(registry)


def test_set_version_updates_every_runtime_manifest(tmp_path: Path) -> None:
    for relative in ["VERSION", *MANIFEST_PATHS]:
        source = ROOT / relative
        target = tmp_path / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)

    set_version(tmp_path, "1.2.3-rc.1")

    assert canonical_version(tmp_path) == "1.2.3-rc.1"
    assert set(manifest_versions(tmp_path).values()) == {"1.2.3-rc.1"}


def test_repository_validation_rejects_a_catalog_tag_for_another_version(
    tmp_path: Path,
) -> None:
    copy_version_contract(tmp_path)
    registry_path = tmp_path / "releases.json"
    registry = json.loads(registry_path.read_text(encoding="utf-8"))
    current = next(
        release for release in registry["releases"] if release["status"] == "current"
    )
    current["tag"] = "v9.9.9"
    registry_path.write_text(json.dumps(registry), encoding="utf-8")

    assert "releases.json: tag v9.9.9 does not identify 0.6.0" in validate_repository(
        tmp_path
    )


@pytest.mark.parametrize("value", ["1.2.3-01", "1.2.3-alpha.01"])
def test_semver_rejects_numeric_prerelease_identifiers_with_leading_zeroes(
    value: str,
) -> None:
    with pytest.raises(VersionError):
        parse_version(value)


def test_set_version_never_rewrites_a_later_toml_section(tmp_path: Path) -> None:
    copy_version_contract(tmp_path)
    pyproject = tmp_path / "pyproject.toml"
    text = pyproject.read_text(encoding="utf-8").replace(
        'version = "0.6.0"\n', "", 1
    )
    text += '\n[tool.unrelated]\nversion = "9.9.9"\n'
    pyproject.write_text(text, encoding="utf-8")

    with pytest.raises(VersionError):
        set_version(tmp_path, "1.2.3")
    assert canonical_version(tmp_path) == "0.6.0"
    assert json.loads((tmp_path / "package.json").read_text())["version"] == "0.6.0"
    assert 'version = "0.6.0"' in (tmp_path / "src-tauri/Cargo.toml").read_text()
    assert '[tool.unrelated]\nversion = "9.9.9"' in pyproject.read_text(
        encoding="utf-8"
    )


def test_repository_validation_rejects_an_impossible_release_date(
    tmp_path: Path,
) -> None:
    copy_version_contract(tmp_path)
    registry_path = tmp_path / "releases.json"
    registry = json.loads(registry_path.read_text(encoding="utf-8"))
    registry["releases"][-1]["date"] = "2026-99-99"
    registry_path.write_text(json.dumps(registry), encoding="utf-8")

    assert "releases.json: invalid date for 0.6.0" in validate_repository(tmp_path)


def test_repository_validation_requires_a_tag_for_the_current_release(
    tmp_path: Path,
) -> None:
    copy_version_contract(tmp_path)
    registry_path = tmp_path / "releases.json"
    registry = json.loads(registry_path.read_text(encoding="utf-8"))
    registry["releases"][-1]["tag"] = None
    registry_path.write_text(json.dumps(registry), encoding="utf-8")

    assert "releases.json: current release 0.6.0 requires tag v0.6.0" in (
        validate_repository(tmp_path)
    )


def test_release_notes_extracts_one_version_without_neighboring_sections() -> None:
    notes = release_notes(ROOT, "0.6.0")

    assert "Tauri" in notes
    assert "0.6.0" not in notes.splitlines()[0]
    assert "0.5.4" not in notes


def test_frontend_demo_version_comes_from_the_canonical_package_manifest() -> None:
    bridge = (ROOT / "frontend/src/lib/bridge.ts").read_text(encoding="utf-8")
    vite_config = (ROOT / "frontend/vite.config.ts").read_text(encoding="utf-8")

    assert 'version: "0.5.4-demo"' not in bridge
    assert "__DOCFLOW_VERSION__" in bridge
    assert "package.json" in vite_config
    assert "__DOCFLOW_VERSION__" in vite_config

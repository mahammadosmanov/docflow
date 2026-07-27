"""Cross-feature contracts that must hold for every distributable build."""
from __future__ import annotations

import json
import re
from pathlib import Path

import docflow_sidecar.ops  # noqa: F401 - imports register all operations
from docflow_sidecar import __version__ as sidecar_version
from docflow_sidecar.registry import all_operations

ROOT = Path(__file__).resolve().parents[2]
EXPECTED_OPERATIONS = {
    "rfi.process_scans",
    "rfi.log_add",
    "files.batch_print",
}


def _catalog(path: Path) -> dict[str, object]:
    return json.loads(path.read_text(encoding="utf-8"))


def test_release_exposes_every_supported_operation() -> None:
    operations = all_operations()
    assert {op.id for op in operations} == EXPECTED_OPERATIONS
    for operation in operations:
        json.dumps(operation.to_json())


def test_operation_schema_keys_exist_in_every_language() -> None:
    catalogs = {
        path.stem.removeprefix("app_"): _catalog(path)
        for path in (ROOT / "app" / "l10n").glob("app_*.arb")
    }
    assert set(catalogs) == {"en", "tr", "ru", "az"}

    required_keys: set[str] = set()
    for operation in all_operations():
        required_keys.update((operation.title_key, operation.description_key))
        for param in operation.schema.params:
            required_keys.add(param.label_key)
            required_keys.update(param.choice_labels.values())
            if param.help_key:
                required_keys.add(param.help_key)

    for locale, catalog in catalogs.items():
        missing = required_keys - set(catalog)
        assert not missing, f"{locale} catalog is missing operation keys: {sorted(missing)}"


def test_python_dependencies_are_exactly_pinned() -> None:
    exact_pin = re.compile(r"^[A-Za-z0-9_.-]+==[^=<>!~]+$")
    for filename in ("requirements.txt", "requirements-dev.txt"):
        lines = [
            line.strip()
            for line in (ROOT / "sidecar" / filename).read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.lstrip().startswith(("#", "-r "))
        ]
        assert lines
        assert all(exact_pin.fullmatch(line) for line in lines), (
            f"{filename} contains an unpinned dependency"
        )


def test_public_versions_are_semver_and_synchronized() -> None:
    pubspec = (ROOT / "app" / "pubspec.yaml").read_text(encoding="utf-8")
    pubspec_match = re.search(
        r"^version:\s*(\d+\.\d+\.\d+)\+(\d+)\s*$",
        pubspec,
        re.MULTILINE,
    )
    assert pubspec_match, "pubspec version must be SemVer plus a build number"
    app_version, build_number = pubspec_match.groups()

    dart_source = (ROOT / "app" / "lib" / "core" / "app_version.dart").read_text(
        encoding="utf-8"
    )
    dart_version = re.search(
        r"const docFlowVersion = '([^']+)';",
        dart_source,
    )
    dart_build = re.search(
        r"const docFlowBuildNumber = (\d+);",
        dart_source,
    )

    assert dart_version and dart_version.group(1) == app_version
    assert dart_build and dart_build.group(1) == build_number
    assert sidecar_version == app_version
    assert int(build_number) > 0
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    assert re.search(
        rf"^## {re.escape(app_version)}\s+-\s+\d{{4}}-\d{{2}}-\d{{2}}$",
        changelog,
        re.MULTILINE,
    ), "current public version must have a dated changelog entry"

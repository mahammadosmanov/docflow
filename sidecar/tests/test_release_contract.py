"""Cross-feature contracts that must hold for every distributable build."""
from __future__ import annotations

import json
import re
from pathlib import Path

import docflow_sidecar.ops  # noqa: F401 - imports register all operations
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

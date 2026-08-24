"""Run signed-page staging tests inside a registered Windows OneDrive tree."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Callable
from pathlib import Path
from typing import Any, Iterable


ONEDRIVE_ACCOUNTS_KEY = r"Software\Microsoft\OneDrive\Accounts"
PROJECT_ROOT = Path(__file__).resolve().parents[1]


def _inside(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def registered_onedrive_roots(registry: Any = None) -> list[Path]:
    """Read local OneDrive account roots from the current user's registry."""

    if registry is None:
        if os.name != "nt":
            return []
        import winreg

        registry = winreg

    try:
        accounts = registry.OpenKey(
            registry.HKEY_CURRENT_USER, ONEDRIVE_ACCOUNTS_KEY
        )
    except OSError:
        return []

    roots: set[Path] = set()
    try:
        index = 0
        while True:
            try:
                account_name = registry.EnumKey(accounts, index)
            except OSError:
                break
            index += 1
            account = None
            try:
                account = registry.OpenKey(accounts, account_name)
                raw_folder, _value_type = registry.QueryValueEx(
                    account, "UserFolder"
                )
                folder = Path(str(raw_folder)).expanduser().resolve(strict=True)
                if folder.is_dir():
                    roots.add(folder)
            except OSError:
                continue
            finally:
                if account is not None:
                    registry.CloseKey(account)
    finally:
        registry.CloseKey(accounts)
    return sorted(roots)


def select_smoke_root(requested: Path, registered_roots: Iterable[Path]) -> Path:
    """Resolve a smoke root and prove it belongs to registered OneDrive storage."""

    candidate = requested.expanduser().resolve(strict=True)
    if not candidate.is_dir():
        raise ValueError(f"smoke root is not a directory: {candidate}")
    roots = [root.expanduser().resolve(strict=True) for root in registered_roots]
    if not any(_inside(candidate, root) for root in roots):
        raise ValueError(f"smoke root is not inside a registered OneDrive tree: {candidate}")
    return candidate


def run_stage_smoke(
    smoke_root: Path,
    *,
    project_root: Path = PROJECT_ROOT,
    python: str = sys.executable,
    runner: Callable[..., Any] = subprocess.run,
) -> int:
    """Run the synthetic staging suite with all temporary files in OneDrive."""

    workspace = Path(
        tempfile.mkdtemp(prefix=".docflow-stage-smoke-", dir=smoke_root)
    )
    test_file = project_root / "tests" / "test_rfi_stage_signed_pages.py"
    profile = workspace / ".profile"
    appdata = profile / "AppData" / "Roaming"
    localappdata = profile / "AppData" / "Local"
    temp = workspace / ".tmp"
    for directory in (profile, appdata, localappdata, temp):
        directory.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment.pop("PYTHONPATH", None)
    environment.pop("PYTEST_ADDOPTS", None)
    environment.pop("PYTEST_PLUGINS", None)
    environment.update(
        {
            "HOME": str(profile),
            "USERPROFILE": str(profile),
            "APPDATA": str(appdata),
            "LOCALAPPDATA": str(localappdata),
            "TEMP": str(temp),
            "TMP": str(temp),
            "PYTEST_DISABLE_PLUGIN_AUTOLOAD": "1",
            "PYTHONDONTWRITEBYTECODE": "1",
        }
    )
    try:
        completed = runner(
            [
                python,
                "-m",
                "pytest",
                "-p",
                "no:cacheprovider",
                str(test_file),
                f"--basetemp={workspace / 'pytest'}",
                "-q",
            ],
            cwd=project_root,
            env=environment,
            check=False,
        )
        return int(completed.returncode)
    finally:
        shutil.rmtree(workspace)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Run synthetic signed-page staging tests in registered OneDrive storage."
        )
    )
    parser.add_argument(
        "root",
        nargs="?",
        default=".",
        help="existing directory inside a registered OneDrive tree (default: cwd)",
    )
    args = parser.parse_args(argv)
    try:
        smoke_root = select_smoke_root(
            Path(args.root), registered_onedrive_roots()
        )
    except (OSError, ValueError) as error:
        parser.error(str(error))
    print(f"OneDrive staging smoke root: {smoke_root}", flush=True)
    return run_stage_smoke(smoke_root)


if __name__ == "__main__":
    raise SystemExit(main())

from __future__ import annotations

from pathlib import Path

import pytest

from tools import onedrive_stage_smoke as smoke
from tools.onedrive_stage_smoke import (
    registered_onedrive_roots,
    run_stage_smoke,
    select_smoke_root,
)


class _FakeRegistry:
    HKEY_CURRENT_USER = object()

    def __init__(self, folders: dict[str, Path]) -> None:
        self.folders = folders

    def OpenKey(self, key: object, name: str) -> tuple[str, str]:
        if key is self.HKEY_CURRENT_USER:
            return ("accounts", "")
        return ("account", name)

    def EnumKey(self, key: tuple[str, str], index: int) -> str:
        if key[0] != "accounts":
            raise OSError("not an accounts key")
        try:
            return sorted(self.folders)[index]
        except IndexError as error:
            raise OSError("no more accounts") from error

    def QueryValueEx(
        self, key: tuple[str, str], name: str
    ) -> tuple[str, int]:
        assert key[0] == "account"
        assert name == "UserFolder"
        return (str(self.folders[key[1]]), 1)

    def CloseKey(self, _key: tuple[str, str]) -> None:
        pass


def test_registered_onedrive_roots_reads_each_account_folder(
    tmp_path: Path,
) -> None:
    business = tmp_path / "business"
    personal = tmp_path / "personal"
    business.mkdir()
    personal.mkdir()

    roots = registered_onedrive_roots(
        _FakeRegistry({"Business1": business, "Personal": personal})
    )

    assert roots == sorted([business.resolve(), personal.resolve()])


def test_select_smoke_root_accepts_only_a_registered_onedrive_tree(
    tmp_path: Path,
) -> None:
    registered = tmp_path / "OneDrive - Company"
    registered.mkdir()
    nested = registered / "Documents"
    nested.mkdir()
    outside = tmp_path / "outside"
    outside.mkdir()

    assert select_smoke_root(nested, [registered]) == nested.resolve()
    with pytest.raises(ValueError, match="registered OneDrive"):
        select_smoke_root(outside, [registered])


def test_run_stage_smoke_uses_and_removes_a_synthetic_onedrive_workspace(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    one_drive = tmp_path / "OneDrive - Company"
    one_drive.mkdir()
    project = tmp_path / "project"
    (project / "tests").mkdir(parents=True)
    test_file = project / "tests" / "test_rfi_stage_signed_pages.py"
    test_file.write_text("# synthetic test target\n", encoding="utf-8")
    observed: dict[str, Path] = {}
    monkeypatch.setenv("PYTHONPATH", "contaminated")
    monkeypatch.setenv("PYTEST_ADDOPTS", "tests/test_extra.py")
    monkeypatch.setenv("PYTEST_PLUGINS", "unsafe_plugin")

    def fake_runner(
        command: list[str],
        *,
        cwd: Path,
        env: dict[str, str],
        check: bool,
    ) -> object:
        assert cwd == project
        assert check is False
        assert "PYTHONPATH" not in env
        assert "PYTEST_ADDOPTS" not in env
        assert "PYTEST_PLUGINS" not in env
        assert env["PYTEST_DISABLE_PLUGIN_AUTOLOAD"] == "1"
        assert env["PYTHONDONTWRITEBYTECODE"] == "1"
        assert command[:5] == [
            "python.exe",
            "-m",
            "pytest",
            "-p",
            "no:cacheprovider",
        ]
        assert Path(command[5]) == test_file
        basetemp = Path(command[6].removeprefix("--basetemp="))
        workspace = basetemp.parent
        assert workspace.parent == one_drive
        assert Path(env["HOME"]) == workspace / ".profile"
        assert Path(env["USERPROFILE"]) == workspace / ".profile"
        assert Path(env["APPDATA"]) == workspace / ".profile" / "AppData" / "Roaming"
        assert Path(env["LOCALAPPDATA"]) == workspace / ".profile" / "AppData" / "Local"
        assert Path(env["TEMP"]) == workspace / ".tmp"
        assert Path(env["TMP"]) == workspace / ".tmp"
        basetemp.mkdir(parents=True)
        (basetemp / "evidence.txt").write_text("synthetic", encoding="utf-8")
        observed["workspace"] = workspace
        return type("Completed", (), {"returncode": 0})()

    assert run_stage_smoke(
        one_drive,
        project_root=project,
        python="python.exe",
        runner=fake_runner,
    ) == 0
    assert not observed["workspace"].exists()


def test_run_stage_smoke_ignores_external_pytest_addopts(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    one_drive = tmp_path / "OneDrive - Company"
    one_drive.mkdir()
    project = tmp_path / "project"
    (project / "tests").mkdir(parents=True)
    (project / "tests" / "test_rfi_stage_signed_pages.py").touch()
    monkeypatch.setenv("PYTEST_ADDOPTS", "tests/test_extra.py")

    def fake_runner(
        _command: list[str],
        *,
        cwd: Path,
        env: dict[str, str],
        check: bool,
    ) -> object:
        assert cwd == project
        assert check is False
        assert "PYTEST_ADDOPTS" not in env
        return type("Completed", (), {"returncode": 0})()

    assert run_stage_smoke(
        one_drive,
        project_root=project,
        python="python.exe",
        runner=fake_runner,
    ) == 0


def test_main_validates_the_requested_root_before_running(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    one_drive = tmp_path / "OneDrive - Company"
    one_drive.mkdir()
    requested = one_drive / "Documents"
    requested.mkdir()
    observed: list[Path] = []
    monkeypatch.setattr(
        smoke, "registered_onedrive_roots", lambda: [one_drive]
    )
    monkeypatch.setattr(
        smoke,
        "run_stage_smoke",
        lambda root: observed.append(root) or 0,
    )

    assert smoke.main([str(requested)]) == 0
    assert observed == [requested.resolve()]
    assert str(requested.resolve()) in capsys.readouterr().out

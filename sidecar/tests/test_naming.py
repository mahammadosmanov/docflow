"""Adlandirma kurali (sahibi karari):
    DOSYA  -> "RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf"
    KLASOR -> "AZE-SUN-INT-QMS-RFI-CIV-0064"          (onek YOK)
"""
from __future__ import annotations

import pathlib

from docflow_sidecar.fs.naming import (
    FILE_PREFIX,
    filename_for,
    foldername_for,
    target_for,
)
from docflow_sidecar.registry import get
import docflow_sidecar.ops  # noqa: F401

CODE = "AZE-SUN-INT-QMS-RFI-CIV-0064"


def test_file_gets_prefix_folder_does_not():
    assert filename_for(CODE) == f"RFI No. {CODE}.pdf"
    assert foldername_for(CODE) == CODE          # klasor: onek YOK
    assert FILE_PREFIX == "RFI No. "


def test_flat_target_when_no_folder(tmp_path):
    assert target_for(tmp_path, CODE) == tmp_path / f"RFI No. {CODE}.pdf"


def test_into_existing_folder_folder_name_stays_bare(tmp_path):
    (tmp_path / CODE).mkdir()                    # klasor kod adiyla var
    got = target_for(tmp_path, CODE)
    assert got == tmp_path / CODE / f"RFI No. {CODE}.pdf"
    assert got.parent.name == CODE               # klasor adina onek SIZMADI


def test_code_case_never_touched():
    """CLAUDE.md kural 7: kod uzerinde case donusumu YOK."""
    assert CODE in filename_for(CODE)
    assert filename_for(CODE).startswith("RFI No. AZE-")


def _plan_for(tmp_path, monkeypatch, filenames, code=CODE):
    src = tmp_path / "in"
    src.mkdir()
    for n in filenames:
        (src / n).write_bytes(b"%PDF-1.4\n")

    from docflow_sidecar.extract.rfi_code import RfiMatch

    monkeypatch.setattr(
        "docflow_sidecar.ops.rfi_process_scans.extract_first_page",
        lambda _p: (RfiMatch(code, "CIV", "0064", "strict", 1.0), "text-layer"),
    )
    return src, get("rfi.process_scans").preview(
        {"source": str(src), "dest": str(src), "keep_original": False}
    )


def test_preview_renames_with_prefix(tmp_path, monkeypatch):
    src, plan = _plan_for(tmp_path, monkeypatch, ["scan1.pdf"])
    assert len(plan.steps) == 1
    assert pathlib.Path(plan.steps[0].dst).name == f"RFI No. {CODE}.pdf"


def test_already_named_file_is_left_alone(tmp_path, monkeypatch):
    """source == dest iken IKINCI calistirma dosyayi _review'e ATMAMALI.

    Bu kontrol olmadan: hedef == dosyanin kendisi -> "hedef zaten var" dali ->
    dogru adli dosya _review'e tasinir ve duzgun arsiv dagilir.
    """
    src, plan = _plan_for(tmp_path, monkeypatch, [f"RFI No. {CODE}.pdf"])
    assert plan.steps == [], "zaten dogru adda olan dosyaya dokunulmamali"
    assert not any("_review" in (s.dst or "") for s in plan.steps)

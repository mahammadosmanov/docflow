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


def test_target_is_always_inside_the_rfi_folder(tmp_path):
    """Klasor YOKKEN bile hedef klasorun icidir — klasoru plan olusturur."""
    assert target_for(tmp_path, CODE) == tmp_path / CODE / f"RFI No. {CODE}.pdf"


def test_into_existing_folder_folder_name_stays_bare(tmp_path):
    (tmp_path / CODE).mkdir()                    # klasor kod adiyla var
    got = target_for(tmp_path, CODE)
    assert got == tmp_path / CODE / f"RFI No. {CODE}.pdf"
    assert got.parent.name == CODE               # klasor adina onek SIZMADI


def test_target_for_does_not_touch_disk(tmp_path):
    """preview dosya sistemine DOKUNMAZ (kural 1): yol hesabi klasor ACMAZ."""
    target_for(tmp_path, CODE)
    assert not (tmp_path / CODE).exists()


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


def test_preview_creates_folder_then_moves_into_it(tmp_path, monkeypatch):
    """Klasor yoksa: once mkdir, sonra move — bu SIRAYLA."""
    src, plan = _plan_for(tmp_path, monkeypatch, ["scan1.pdf"])
    assert [s.kind for s in plan.steps] == ["mkdir", "move"], "mkdir move'dan ONCE"
    assert pathlib.Path(plan.steps[0].dst).name == CODE          # klasor: onek yok
    assert pathlib.Path(plan.steps[1].dst).name == f"RFI No. {CODE}.pdf"
    assert pathlib.Path(plan.steps[1].dst).parent.name == CODE


def test_existing_folder_is_reused_not_recreated(tmp_path, monkeypatch):
    """Klasor VARSA mkdir adimi URETME — kullanicinin klasoruna dokunma."""
    src = tmp_path / "in"
    src.mkdir()
    (src / CODE).mkdir()                     # source == dest, klasor onceden var
    (src / "scan1.pdf").write_bytes(b"%PDF-1.4\n")

    from docflow_sidecar.extract.rfi_code import RfiMatch

    monkeypatch.setattr(
        "docflow_sidecar.ops.rfi_process_scans.extract_first_page",
        lambda _p: (RfiMatch(CODE, "CIV", "0064", "strict", 1.0), "text-layer"),
    )
    plan = get("rfi.process_scans").preview(
        {"source": str(src), "dest": str(src), "keep_original": False}
    )
    assert [s.kind for s in plan.steps] == ["move"], "var olan klasor icin mkdir YOK"


def test_already_filed_file_is_left_alone(tmp_path, monkeypatch):
    """source == dest iken IKINCI calistirma dosyaya DOKUNMAMALI.

    Bu kontrol olmadan: hedef == dosyanin kendisi -> "hedef zaten var" dali ->
    dogru yerdeki dosya _review'e tasinir ve duzgun arsiv dagilir.
    """
    src = tmp_path / "in"
    (src / CODE).mkdir(parents=True)
    (src / CODE / f"RFI No. {CODE}.pdf").write_bytes(b"%PDF-1.4\n")

    from docflow_sidecar.extract.rfi_code import RfiMatch

    monkeypatch.setattr(
        "docflow_sidecar.ops.rfi_process_scans.extract_first_page",
        lambda _p: (RfiMatch(CODE, "CIV", "0064", "strict", 1.0), "text-layer"),
    )
    # Not: source'ta dosya YOK (klasorun icinde), o yuzden bos plan bekleriz.
    plan = get("rfi.process_scans").preview(
        {"source": str(src), "dest": str(src), "keep_original": False}
    )
    assert plan.steps == []
    assert not any("_review" in (s.dst or "") for s in plan.steps)

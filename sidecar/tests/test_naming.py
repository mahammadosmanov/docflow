"""Adlandirma kurali (sahibi karari):
    DOSYA  -> "RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf"
    KLASOR -> "AZE-SUN-INT-QMS-RFI-CIV-0064"          (onek YOK)
"""
from __future__ import annotations

import pathlib

from docflow_sidecar.fs.naming import (
    FILE_PREFIX,
    filename_for,
    flat_target_for,
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


def test_flat_target_is_directly_inside_destination_and_does_not_touch_disk(tmp_path):
    got = flat_target_for(tmp_path, CODE)
    assert got == tmp_path / f"RFI No. {CODE}.pdf"
    assert got.parent == tmp_path
    assert list(tmp_path.iterdir()) == []


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


def test_preview_flat_mode_moves_pdf_directly_without_mkdir(tmp_path, monkeypatch):
    """Opt-in duz mod yalnizca PDF hedefi uretir; RFI klasoru uretmez."""
    src = tmp_path / "in"
    dest = tmp_path / "out"
    src.mkdir()
    dest.mkdir()
    (src / "scan1.pdf").write_bytes(b"%PDF-1.4\n")

    from docflow_sidecar.extract.rfi_code import RfiMatch

    monkeypatch.setattr(
        "docflow_sidecar.ops.rfi_process_scans.extract_first_page",
        lambda _p: (RfiMatch(CODE, "CIV", "0064", "strict", 1.0), "text-layer"),
    )
    plan = get("rfi.process_scans").preview({
        "source": str(src),
        "dest": str(dest),
        "organize_into_rfi_folders": False,
        "keep_original": False,
    })

    assert [s.kind for s in plan.steps] == ["move"]
    assert pathlib.Path(plan.steps[0].dst) == dest / f"RFI No. {CODE}.pdf"
    assert plan.steps[0].meta["intoFolder"] is False
    assert plan.steps[0].meta["folderCreated"] is False
    assert not (dest / CODE).exists(), "preview dosya sistemine dokunmamali"


def test_missing_layout_param_preserves_legacy_folder_mode(tmp_path, monkeypatch):
    """Eski ayar/API payload'lari yeni secenek yokken davranis degistirmez."""
    _src, plan = _plan_for(tmp_path, monkeypatch, ["legacy.pdf"])
    assert [s.kind for s in plan.steps] == ["mkdir", "move"]
    assert pathlib.Path(plan.steps[1].dst).parent.name == CODE


def test_flat_mode_copy_preserves_source_and_apply_undo_are_isolated(
    tmp_path, monkeypatch
):
    """Kopyala/tasi tercihi duz modda da aynidir; apply+undo geri donusludur."""
    src = tmp_path / "in"
    dest = tmp_path / "out"
    src.mkdir()
    dest.mkdir()
    original = src / "scan1.pdf"
    original.write_bytes(b"%PDF-1.4\nflat-mode\n")

    from docflow_sidecar.extract.rfi_code import RfiMatch
    from docflow_sidecar.fs import apply as fsapply

    monkeypatch.setattr(
        "docflow_sidecar.ops.rfi_process_scans.extract_first_page",
        lambda _p: (RfiMatch(CODE, "CIV", "0064", "strict", 1.0), "text-layer"),
    )
    monkeypatch.setattr(fsapply, "JOURNAL_DIR", tmp_path / "journal")
    operation = get("rfi.process_scans")
    plan = operation.preview({
        "source": str(src),
        "dest": str(dest),
        "organize_into_rfi_folders": False,
        "keep_original": True,
    })

    assert [s.kind for s in plan.steps] == ["copy"]
    result = operation.apply(plan)
    target = dest / f"RFI No. {CODE}.pdf"
    assert result.failed == 0
    assert original.exists()
    assert target.read_bytes() == original.read_bytes()
    assert not (dest / CODE).exists()

    undone = fsapply.undo(result.journal_id)
    assert undone.failed == 0
    assert original.exists()
    assert not target.exists()
    assert not (dest / CODE).exists()


def test_flat_mode_second_run_leaves_already_named_pdf_alone(tmp_path, monkeypatch):
    """source == dest iken duz cikti ikinci calistirmada review'a gitmez."""
    root = tmp_path / "files"
    root.mkdir()
    filed = root / f"RFI No. {CODE}.pdf"
    filed.write_bytes(b"%PDF-1.4\nalready-filed\n")

    from docflow_sidecar.extract.rfi_code import RfiMatch

    monkeypatch.setattr(
        "docflow_sidecar.ops.rfi_process_scans.extract_first_page",
        lambda _p: (RfiMatch(CODE, "CIV", "0064", "strict", 1.0), "text-layer"),
    )
    plan = get("rfi.process_scans").preview({
        "source": str(root),
        "dest": str(root),
        "organize_into_rfi_folders": False,
        "keep_original": False,
    })

    assert plan.steps == []
    assert filed.read_bytes() == b"%PDF-1.4\nalready-filed\n"


def test_flat_mode_existing_target_is_never_overwritten(tmp_path, monkeypatch):
    """Duz mod da mevcut hedefi korur ve yeni taramayi review'a ayirir."""
    src = tmp_path / "in"
    dest = tmp_path / "out"
    src.mkdir()
    dest.mkdir()
    incoming = src / "new-scan.pdf"
    incoming.write_bytes(b"%PDF-1.4\nnew\n")
    target = dest / f"RFI No. {CODE}.pdf"
    target.write_bytes(b"%PDF-1.4\nexisting\n")

    from docflow_sidecar.extract.rfi_code import RfiMatch

    monkeypatch.setattr(
        "docflow_sidecar.ops.rfi_process_scans.extract_first_page",
        lambda _p: (RfiMatch(CODE, "CIV", "0064", "strict", 1.0), "text-layer"),
    )
    plan = get("rfi.process_scans").preview({
        "source": str(src),
        "dest": str(dest),
        "organize_into_rfi_folders": False,
        "keep_original": False,
    })

    assert [w.key for w in plan.warnings] == ["warn.exists"]
    assert [s.kind for s in plan.steps] == ["move"]
    assert pathlib.Path(plan.steps[0].dst) == dest / "_review" / incoming.name
    assert target.read_bytes() == b"%PDF-1.4\nexisting\n"
    assert not any(s.kind == "mkdir" for s in plan.steps)


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

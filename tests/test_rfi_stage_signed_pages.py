from __future__ import annotations

import hashlib
from pathlib import Path

import fitz
import pytest

from docflow.fs import apply as apply_module
from docflow.fs.apply import undo
from docflow.fs.naming import filename_for
from docflow.ops.rfi_stage_signed_pages import StageSignedFirstPages


CODE = "AZE-SUN-INT-QMS-RFI-CIV-0064"


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _make_pdf(
    path: Path,
    pages: list[str],
    *,
    size: tuple[float, float] = (595, 842),
    metadata: dict[str, str] | None = None,
    attachment: bytes | None = None,
    toc: list[list[object]] | None = None,
    page_labels: list[dict[str, object]] | None = None,
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    document = fitz.open()
    for text in pages:
        page = document.new_page(width=size[0], height=size[1])
        page.insert_text((48, 72), text)
    if metadata:
        document.set_metadata(metadata)
    if attachment is not None:
        document.embfile_add("archive-note.txt", attachment)
    if toc:
        document.set_toc(toc)
    if page_labels:
        document.set_page_labels(page_labels)
    document.save(path)
    document.close()


def _roots(tmp_path: Path) -> tuple[Path, Path, Path, Path]:
    signed = tmp_path / "signed"
    archive = tmp_path / "archive"
    review = tmp_path / "review"
    signed.mkdir()
    archive.mkdir()
    review.mkdir()
    archive_pdf = (
        archive / "Package-A" / "Civil" / CODE / filename_for(CODE)
    )
    return signed, archive, review, archive_pdf


def _params(signed: Path, archive: Path, review: Path) -> dict[str, object]:
    return {
        "signed_scans": str(signed),
        "archive_root": str(archive),
        "review_root": str(review),
        "scan_subfolders": False,
    }


def test_stage_replaces_only_first_page_and_never_changes_inputs(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    signed, archive, review, archive_pdf = _roots(tmp_path)
    signed_pdf = signed / "fresh-signed-scan.pdf"
    _make_pdf(
        archive_pdf,
        [
            f"OLD FIRST PAGE {CODE} " + "archive text " * 5,
            "ARCHIVE SECOND PAGE must remain byte-visible",
            "ARCHIVE THIRD PAGE must remain byte-visible",
        ],
        metadata={"title": "Original RFI", "author": "AZFEN"},
        attachment=b"keep this embedded archive data",
        toc=[[1, "RFI cover", 1], [1, "RFI continuation", 2]],
        page_labels=[
            {"startpage": 0, "prefix": "RFI-", "style": "D", "firstpagenum": 1}
        ],
    )
    _make_pdf(
        signed_pdf,
        [
            f"SIGNED FIRST PAGE {CODE} " + "signed scan text " * 5,
            "SIGNED SECOND PAGE MUST BE IGNORED",
        ],
        size=(612, 792),
    )
    archive_before = _sha256(archive_pdf)
    signed_before = _sha256(signed_pdf)
    monkeypatch.setattr(apply_module, "JOURNAL_DIR", tmp_path / "journal")

    operation = StageSignedFirstPages()
    plan = operation.preview(_params(signed, archive, review))
    replacements = [
        step for step in plan.steps if step.kind == "pdf_first_page_replace"
    ]
    assert len(replacements) == 1
    assert replacements[0].meta["signedPageCount"] == 2
    assert {warning.key for warning in plan.warnings} >= {
        "warn.rfi.signedExtraPages",
        "warn.rfi.firstPageLayoutDiffers",
    }

    result = operation.apply(plan)
    assert result.failed == 0
    output = review / archive_pdf.relative_to(archive)
    assert output.is_file()
    assert _sha256(archive_pdf) == archive_before
    assert _sha256(signed_pdf) == signed_before

    with fitz.open(output) as document:
        assert document.page_count == 3
        assert "SIGNED FIRST PAGE" in document[0].get_text()
        assert "OLD FIRST PAGE" not in document[0].get_text()
        assert "SIGNED SECOND PAGE" not in "".join(
            page.get_text() for page in document
        )
        assert "ARCHIVE SECOND PAGE" in document[1].get_text()
        assert "ARCHIVE THIRD PAGE" in document[2].get_text()
        assert document.metadata["title"] == "Original RFI"
        assert document.metadata["author"] == "AZFEN"
        assert document.embfile_get("archive-note.txt") == (
            b"keep this embedded archive data"
        )
        assert document.get_toc() == [
            [1, "RFI cover", 1],
            [1, "RFI continuation", 2],
        ]
        assert document.get_page_labels() == [
            {"startpage": 0, "prefix": "RFI-", "firstpagenum": 1, "style": "D"}
        ]
        assert round(document[0].rect.width) == 612
        assert round(document[0].rect.height) == 792

    undone = undo(result.journal_id)
    assert undone.failed == 0
    assert not output.exists()
    assert not (review / "Package-A").exists()
    assert review.is_dir()
    assert _sha256(archive_pdf) == archive_before
    assert _sha256(signed_pdf) == signed_before


def test_duplicate_signed_scans_fail_closed(tmp_path: Path) -> None:
    signed, archive, review, archive_pdf = _roots(tmp_path)
    _make_pdf(archive_pdf, [f"ARCHIVE {CODE} " + "text " * 20])
    for name in ("signed-a.pdf", "signed-b.pdf"):
        _make_pdf(signed / name, [f"SIGNED {CODE} " + "text " * 20])

    plan = StageSignedFirstPages().preview(_params(signed, archive, review))
    assert plan.steps == []
    assert [warning.key for warning in plan.warnings] == [
        "warn.rfi.signedDuplicate"
    ]


def test_ambiguous_archive_match_fail_closed(tmp_path: Path) -> None:
    signed, archive, review, archive_pdf = _roots(tmp_path)
    _make_pdf(signed / "signed.pdf", [f"SIGNED {CODE} " + "text " * 20])
    _make_pdf(archive_pdf, [f"ARCHIVE A {CODE} " + "text " * 20])
    _make_pdf(
        archive / "Other-Package" / filename_for(CODE),
        [f"ARCHIVE B {CODE} " + "text " * 20],
    )

    plan = StageSignedFirstPages().preview(_params(signed, archive, review))
    assert plan.steps == []
    assert [warning.key for warning in plan.warnings] == [
        "warn.rfi.archiveAmbiguous"
    ]


def test_existing_review_output_is_never_overwritten(tmp_path: Path) -> None:
    signed, archive, review, archive_pdf = _roots(tmp_path)
    _make_pdf(signed / "signed.pdf", [f"SIGNED {CODE} " + "text " * 20])
    _make_pdf(archive_pdf, [f"ARCHIVE {CODE} " + "text " * 20])
    output = review / archive_pdf.relative_to(archive)
    _make_pdf(output, ["USER REVIEW FILE"])
    before = _sha256(output)

    plan = StageSignedFirstPages().preview(_params(signed, archive, review))
    assert plan.steps == []
    assert [warning.key for warning in plan.warnings] == [
        "warn.rfi.reviewExists"
    ]
    assert _sha256(output) == before


def test_review_root_cannot_overlap_archive_or_signed_scans(tmp_path: Path) -> None:
    signed, archive, review, _archive_pdf = _roots(tmp_path)
    inside_archive = archive / "review"
    inside_archive.mkdir()
    operation = StageSignedFirstPages()

    first = operation.preview(_params(signed, archive, inside_archive))
    second = operation.preview(_params(signed, archive, signed))
    assert first.steps == second.steps == []
    assert first.warnings[0].key == "error.rfi.reviewRootMustBeSeparate"
    assert second.warnings[0].key == "error.rfi.reviewRootMustBeSeparate"


def test_apply_refuses_an_archive_file_changed_after_preview(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    signed, archive, review, archive_pdf = _roots(tmp_path)
    _make_pdf(signed / "signed.pdf", [f"SIGNED {CODE} " + "text " * 20])
    _make_pdf(archive_pdf, [f"ARCHIVE {CODE} " + "text " * 20])
    monkeypatch.setattr(apply_module, "JOURNAL_DIR", tmp_path / "journal")
    operation = StageSignedFirstPages()
    plan = operation.preview(_params(signed, archive, review))

    _make_pdf(archive_pdf.with_name("replacement.pdf"), ["changed"])
    archive_pdf.write_bytes(archive_pdf.with_name("replacement.pdf").read_bytes())
    changed_hash = _sha256(archive_pdf)
    result = operation.apply(plan)

    output = review / archive_pdf.relative_to(archive)
    assert result.failed == 1
    assert not output.exists()
    assert _sha256(archive_pdf) == changed_hash
    undo(result.journal_id)
    assert not (review / "Package-A").exists()


def test_archive_without_page_labels_does_not_gain_artificial_labels(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    signed, archive, review, archive_pdf = _roots(tmp_path)
    _make_pdf(signed / "signed.pdf", [f"SIGNED {CODE} " + "text " * 20])
    _make_pdf(
        archive_pdf,
        [f"ARCHIVE {CODE} " + "text " * 20, "SECOND PAGE"],
        toc=[[1, "Cover", 1], [1, "Continuation", 2]],
    )
    monkeypatch.setattr(apply_module, "JOURNAL_DIR", tmp_path / "journal")
    operation = StageSignedFirstPages()
    result = operation.apply(operation.preview(_params(signed, archive, review)))

    assert result.failed == 0
    with fitz.open(review / archive_pdf.relative_to(archive)) as output:
        assert output.get_page_labels() == []
        assert output.get_toc() == [
            [1, "Cover", 1],
            [1, "Continuation", 2],
        ]

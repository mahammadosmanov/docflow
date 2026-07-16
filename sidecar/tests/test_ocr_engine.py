"""Tesseract KURULUM hatasi, DOSYA hatasi degildir.

Gercek olay: uygulama Finder'dan baslatilinca PATH /usr/bin:/bin:/usr/sbin:/sbin
oluyor ve Homebrew'daki tesseract gorunmuyordu. pytesseract "not installed"
firlatti, op bunu genel `except Exception` ile yakalayip HER dosyaya
"warn.unreadable" dedi ve 20 dosyanin 20'si icin "_review'e tasi" adimi uretti.
Yani eksik bir binary, kullanicinin butun klasorunu dagitacak bir plan uretti.
"""
from __future__ import annotations

import pathlib

import pytest

from docflow_sidecar.extract import ocr as ocr_mod
from docflow_sidecar.registry import get
import docflow_sidecar.ops  # noqa: F401  — @register tetikle


def test_engine_missing_plan_has_no_steps(tmp_path, monkeypatch):
    """Motor yoksa: SIFIR adim + tek net uyari. Dosyalara DOKUNMA."""
    src = tmp_path / "in"
    src.mkdir()
    dest = tmp_path / "out"
    dest.mkdir()
    # Icerigi onemsiz: motor kontrolu OCR'a girmeden once patlamali.
    (src / "a.pdf").write_bytes(b"%PDF-1.4\n%stub\n")
    (src / "b.pdf").write_bytes(b"%PDF-1.4\n%stub\n")

    def _boom(_path: str):
        raise ocr_mod.OcrEngineMissing("tesseract yok")

    monkeypatch.setattr("docflow_sidecar.ops.rfi_process_scans.extract_first_page", _boom)

    plan = get("rfi.process_scans").preview(
        {"source": str(src), "dest": str(dest), "keep_original": False}
    )

    assert plan.steps == [], "motor yokken TEK BIR dosya bile tasinmamali"
    assert [w.key for w in plan.warnings] == ["warn.no_ocr"], (
        "20 kere 'warn.unreadable' degil, bir kere 'kurulum eksik' de"
    )


def test_broken_file_still_goes_to_review(tmp_path, monkeypatch):
    """Motor VARKEN tek bozuk dosya: eski davranis korunur (_review'e git)."""
    src = tmp_path / "in"
    src.mkdir()
    dest = tmp_path / "out"
    dest.mkdir()
    (src / "bozuk.pdf").write_bytes(b"not really a pdf")

    def _boom(_path: str):
        raise RuntimeError("bozuk dosya")

    monkeypatch.setattr("docflow_sidecar.ops.rfi_process_scans.extract_first_page", _boom)

    plan = get("rfi.process_scans").preview(
        {"source": str(src), "dest": str(dest), "keep_original": False}
    )
    assert [w.key for w in plan.warnings] == ["warn.unreadable"]
    assert len(plan.steps) == 1
    assert "_review" in plan.steps[0].dst


def test_resolver_finds_tesseract_outside_path(monkeypatch, tmp_path):
    """GUI'nin dar PATH'i: which() bos donse bile bilinen yerden bulunmali."""
    fake = tmp_path / "tesseract"
    fake.write_text("#!/bin/sh\n")
    fake.chmod(0o755)

    monkeypatch.delenv("DOCFLOW_TESSERACT", raising=False)
    monkeypatch.setattr(ocr_mod.shutil, "which", lambda _n: None)
    monkeypatch.setattr(ocr_mod, "_CANDIDATES", (str(fake),))
    assert ocr_mod._resolve_tesseract() == str(fake)


def test_resolver_prefers_bundled_env(monkeypatch, tmp_path):
    """Paketlenmis derleme: DOCFLOW_TESSERACT her seyi ezer."""
    bundled = tmp_path / "bundled_tesseract"
    bundled.write_text("#!/bin/sh\n")
    monkeypatch.setenv("DOCFLOW_TESSERACT", str(bundled))
    monkeypatch.setattr(ocr_mod.shutil, "which", lambda _n: "/usr/bin/tesseract")
    assert ocr_mod._resolve_tesseract() == str(bundled)


def test_resolver_ignores_env_pointing_nowhere(monkeypatch):
    """Env yanlis yolu gosteriyorsa ona guvenme — PATH'e dus."""
    monkeypatch.setenv("DOCFLOW_TESSERACT", "/yok/boyle/bir/yol/tesseract")
    monkeypatch.setattr(ocr_mod.shutil, "which", lambda _n: "/usr/bin/tesseract")
    assert ocr_mod._resolve_tesseract() == "/usr/bin/tesseract"

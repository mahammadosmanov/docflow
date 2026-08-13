"""Tesseract KURULUM hatasi, DOSYA hatasi degildir.

Gercek olay: uygulama Finder'dan baslatilinca PATH /usr/bin:/bin:/usr/sbin:/sbin
oluyor ve Homebrew'daki tesseract gorunmuyordu. pytesseract "not installed"
firlatti, op bunu genel `except Exception` ile yakalayip HER dosyaya
"warn.unreadable" dedi ve 20 dosyanin 20'si icin "_review'e tasi" adimi uretti.
Yani eksik bir binary, kullanicinin butun klasorunu dagitacak bir plan uretti.
"""
from __future__ import annotations

import os
import pathlib
import shlex

import pytest

from docflow.extract import ocr as ocr_mod
from docflow.registry import get
import docflow.ops  # noqa: F401  — @register tetikle


def test_tesseract_config_is_parseable_by_windows(monkeypatch):
    """Whitelist config must survive pytesseract's Windows command parser."""
    captured: dict[str, str] = {}

    def _capture(_binary, *, lang: str, config: str) -> str:
        assert lang == "eng"
        captured["config"] = config
        return ""

    monkeypatch.setattr(ocr_mod.pytesseract, "image_to_string", _capture)
    ocr_mod._ocr(object(), psm=6)

    parsed = shlex.split(captured["config"], posix=False)
    assert parsed[:5] == ["--oem", "3", "--psm", "6", "-c"]
    assert parsed[5].startswith("tessedit_char_whitelist=")


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

    monkeypatch.setattr("docflow.ops.rfi_process_scans.extract_first_page", _boom)

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

    monkeypatch.setattr("docflow.ops.rfi_process_scans.extract_first_page", _boom)

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


def test_all_files_failing_means_install_problem_not_file_problem(tmp_path, monkeypatch):
    """HEPSI patlarsa: sifir adim + gercek hata + teshis. Dosyalara DOKUNMA.

    Gercek olay (Windows): Tesseract BULUNDU ama cagri patladi. Eski kod her
    dosya icin "okunamadi" deyip 20 dosyayi da _review'e tasiyan bir plan
    uretiyordu; gercek hata step.meta icinde gomulu kaliyor, kullanici 20 kere
    ayni anlamsiz satiri goruyordu.
    """
    src = tmp_path / "in"
    src.mkdir()
    for n in ("a.pdf", "b.pdf", "c.pdf"):
        (src / n).write_bytes(b"%PDF-1.4\n")

    def _boom(_path: str):
        raise RuntimeError("Error opening data file eng.traineddata")

    monkeypatch.setattr("docflow.ops.rfi_process_scans.extract_first_page", _boom)

    plan = get("rfi.process_scans").preview(
        {"source": str(src), "dest": str(src), "keep_original": False}
    )

    assert plan.steps == [], "hepsi patladiysa TEK dosya bile tasinmamali"
    assert [w.key for w in plan.warnings] == ["warn.ocr_broken"]
    subject = plan.warnings[0].subject
    assert "eng.traineddata" in subject, "GERCEK hata gorunur olmali"
    assert "tesseract:" in subject, "teshis bilgisi eklenmeli"


def test_single_bad_file_still_moves_and_shows_error(tmp_path, monkeypatch):
    """Tek dosya patlarsa eski davranis: _review'e git — ama hata artik GORUNUR."""
    src = tmp_path / "in"
    src.mkdir()
    (src / "bozuk.pdf").write_bytes(b"not a pdf")

    def _boom(_path: str):
        raise RuntimeError("cannot open broken document")

    monkeypatch.setattr("docflow.ops.rfi_process_scans.extract_first_page", _boom)

    plan = get("rfi.process_scans").preview(
        {"source": str(src), "dest": str(src), "keep_original": False}
    )
    assert len(plan.steps) == 1
    assert "_review" in plan.steps[0].dst
    assert "cannot open broken document" in plan.warnings[0].subject


def test_mixed_failure_does_not_trigger_install_verdict(tmp_path, monkeypatch):
    """Bazilari okunuyorsa kurulum saglamdir — tekil hatalar tekil kalir."""
    src = tmp_path / "in"
    src.mkdir()
    (src / "ok.pdf").write_bytes(b"%PDF-1.4\n")
    (src / "bad.pdf").write_bytes(b"%PDF-1.4\n")

    from docflow.extract.rfi_code import RfiMatch

    def _mixed(path: str):
        if path.endswith("bad.pdf"):
            raise RuntimeError("bozuk")
        return RfiMatch("AZE-SUN-INT-QMS-RFI-CIV-0064", "CIV", "0064", "strict", 1.0), "text-layer"

    monkeypatch.setattr("docflow.ops.rfi_process_scans.extract_first_page", _mixed)

    plan = get("rfi.process_scans").preview(
        {"source": str(src), "dest": str(src), "keep_original": False}
    )
    keys = [w.key for w in plan.warnings]
    assert "warn.ocr_broken" not in keys
    assert "warn.unreadable" in keys
    assert any(s.kind == "mkdir" for s in plan.steps), "okunan dosya normal islenmeli"


def test_tessdata_found_next_to_exe_windows_bundle(tmp_path, monkeypatch):
    """Windows bundle: tessdata exe'nin YANINDA -> TESSDATA_PREFIX oraya."""
    d = tmp_path / "tesseract"
    (d / "tessdata").mkdir(parents=True)
    (d / "tessdata" / "eng.traineddata").write_bytes(b"x")
    exe = d / "tesseract.exe"
    exe.write_text("")
    assert ocr_mod._find_tessdata(str(exe)) == str(d / "tessdata")


def test_tessdata_found_in_share_homebrew_layout(tmp_path):
    """Homebrew: exe bin/ altinda, tessdata share/ altinda — yan yana DEGIL."""
    root = tmp_path / "usr"
    (root / "bin").mkdir(parents=True)
    (root / "share" / "tessdata").mkdir(parents=True)
    (root / "share" / "tessdata" / "eng.traineddata").write_bytes(b"x")
    exe = root / "bin" / "tesseract"
    exe.write_text("")
    assert ocr_mod._find_tessdata(str(exe)) == str(root / "share" / "tessdata")


def test_tessdata_absent_returns_none(tmp_path):
    """Hicbir yerde yoksa None — uydurma yol dondurme."""
    exe = tmp_path / "tesseract"
    exe.write_text("")
    assert ocr_mod._find_tessdata(str(exe)) is None


def test_existing_tessdata_prefix_is_not_overridden(tmp_path, monkeypatch):
    """Kullanici/OS ayarladiysa ona DOKUNMA — onun bilgisi bizimkini ezer."""
    exe = tmp_path / "tesseract"
    exe.write_text("")
    (tmp_path / "tessdata").mkdir()
    (tmp_path / "tessdata" / "eng.traineddata").write_bytes(b"x")
    monkeypatch.setenv("TESSDATA_PREFIX", "/kullanicinin/yolu")
    monkeypatch.setattr(ocr_mod, "_resolve_tesseract", lambda: str(exe))
    ocr_mod._configure_tesseract()
    assert os.environ["TESSDATA_PREFIX"] == "/kullanicinin/yolu"

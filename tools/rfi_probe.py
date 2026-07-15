#!/usr/bin/env python3
"""
RFI probe — READ ONLY. Renames nothing, moves nothing.
Amac: app yazmadan once cikarim oraninin ne oldugunu olcmek.

    pip install pymupdf opencv-python-headless pytesseract numpy
    # + tesseract binary:  brew install tesseract  |  pacman -S tesseract tesseract-data-eng
    python3 rfi_probe.py /path/to/scans

Cikti: her dosya icin  KOD | yontem | guven | sure
Sonda ozet + isabet orani.

>>> Cikarim mantigi TEK KAYNAKTAN gelir: docflow_sidecar.extract. <<<
Bu arac vaktiyle regex/repair/levenshtein'i kendi icine KOPYALADI ve kopya
sidecar'dan sessizce ayristi:
  - fuzzy yol -RFI- segmentini mesafeye katiyordu, boylece
    AZE-SUN-INT-QMS-ITP-MEC-0001 (Lev 3) bir RFI HIT'i olarak sayiliyordu —
    sidecar'in bilerek ELEDIGI decoy.
  - repair tablosu 4->A eslemesini kaciriyordu; TO_DIGIT'te olu kucuk 'l' vardi.
Sonuc: probe, sidecar'in reddettigi decoy'lari HIT gosterip isabet oranini
sisiriyor -> kalibrasyon yanlis fonksiyonu olcuyordu. Artik ayni find() ve
from_ocr() cagriliyor: OLCULEN sey, SEVK EDILEN sey.
"""
import sys
import time
import pathlib

# tools/ standalone calistirilabilsin diye sidecar'i path'e ekle.
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent / "sidecar"))

import fitz  # noqa: E402  — sadece PDF acmak icin; OCR/eslesme sidecar'da

from docflow_sidecar.extract.ocr import from_ocr, from_text_layer  # noqa: E402
from docflow_sidecar.extract.rfi_code import find  # noqa: E402

# --sweep icin (dpi, blok) izgarasi. Kazanani ocr.py DPI/BLOCK sabitlerine yaz.
SWEEP = [(dpi, blk) for dpi in (150, 200, 250, 300) for blk in (21, 31, 41, 51)]


def probe(path: pathlib.Path):
    """(kod, yontem, guven, sure). Metin katmani -> yoksa ust bant OCR."""
    t = time.time()
    with fitz.open(path) as doc:
        page = doc[0]
        text = from_text_layer(page)
        if text is not None:
            m = find(text)
            if m.ok:
                return m.code, "text-layer", m.confidence, time.time() - t
        m = find(from_ocr(page))
    return (m.code or None), f"ocr:{m.method}", m.confidence, time.time() - t


def sweep(folder: str):
    """Her (dpi, blok) kombinasyonunu tum dosyalarda dener, en iyisini soyler.
    Magic-number tahminini olcume cevirir — bir kez calistir, sonucu sabitle."""
    files = sorted(p for p in pathlib.Path(folder).glob("*.pdf"))
    if not files:
        sys.exit(f"PDF yok: {folder}")
    pages = [fitz.open(f)[0] for f in files]
    print(f"{len(files)} dosya x {len(SWEEP)} konfig\n")
    rows = []
    for dpi, blk in SWEEP:
        hits = sum(find(from_ocr(pg, dpi, blk)).ok for pg in pages)
        rows.append((hits, dpi, blk))
        print(f"  dpi={dpi:3d} blok={blk:2d}  ->  {hits}/{len(files)}")
    best = max(rows)
    print(f"\nEN IYI: dpi={best[1]} blok={best[2]}  ({best[0]}/{len(files)})")
    print("Bunu docflow_sidecar/extract/ocr.py DPI/BLOCK sabitlerine yaz.")


def main(folder: str):
    files = sorted(p for p in pathlib.Path(folder).glob("*.pdf"))
    if not files:
        sys.exit(f"PDF yok: {folder}")

    hits, seen = 0, {}
    print(f"{'dosya':<38} {'kod':<32} {'yontem':<16} {'guven':>6} {'sure':>7}")
    print("-" * 104)
    for f in files:
        try:
            code, how, conf, dt = probe(f)
        except Exception as e:
            print(f"{f.name[:37]:<38} {'HATA':<32} {type(e).__name__:<16} {'':>6} {'':>7}  {e}")
            continue
        hits += bool(code)
        if code:
            seen.setdefault(code, []).append(f.name)
        print(f"{f.name[:37]:<38} {code or 'MISS':<32} {how:<16} {conf:>6.2f} {dt:>6.2f}s")

    print("-" * 104)
    print(f"isabet: {hits}/{len(files)}  ({hits / len(files) * 100:.0f}%)")
    dupes = {k: v for k, v in seen.items() if len(v) > 1}
    if dupes:
        print("\nCAKISMA (ayni RFI, birden fazla dosya — muhtemelen Rev farki):")
        for k, v in dupes.items():
            print(f"  {k}: {', '.join(v)}")


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    folder = args[0] if args else "."
    (sweep if "--sweep" in sys.argv else main)(folder)

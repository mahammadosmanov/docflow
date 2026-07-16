#!/usr/bin/env python3
"""RFI teshis araci — READ ONLY. Bir PDF neden MISS ediyor gosterir.

    python3 tools/rfi_diag.py /yol/scan.pdf        # tek dosya
    python3 tools/rfi_diag.py /yol/klasor          # klasordeki tum PDF'ler

Her dosya icin gosterir:
  1. Metin katmani var mi + icerigi (searchable PDF ise OCR'a hic gerek yok)
  2. Her OCR konfigi icin: ham cikti + match sonucu (ilk STRICT'te durur)
  3. Kod-benzeri HER token (prefix farkliysa burada gorunur — regex'i degil,
     GERCEGI gosterir; kodun formati beklenenden farkliysa teshis budur)

Ciktiyi bana yapistir; DPI/BLOCK/PSM'i ve gerekiyorsa prefix'i ona gore ayarlarim.
"""
import re
import sys
import pathlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent / "sidecar"))

import fitz  # noqa: E402

from docflow_sidecar.extract.ocr import (  # noqa: E402
    _CASCADE, _ocr, _page_to_gray, _thresh, C, from_text_layer,
)
from docflow_sidecar.extract.rfi_code import find  # noqa: E402

# Kod-benzeri gevsek desen: tireyle ayrilmis 4+ segment. Prefix'i VARSAYMAZ —
# boylece kullanicinin gercek kod formatini (farkliysa) ortaya cikarir.
CODEISH = re.compile(r"[A-Z0-9]{2,}(?:-[A-Z0-9]{2,}){3,}")


def codeish(text: str) -> list[str]:
    return sorted(set(CODEISH.findall(text.upper())))


def diag(path: pathlib.Path) -> None:
    print("=" * 78)
    print(f"DOSYA: {path.name}")
    with fitz.open(path) as doc:
        page = doc[0]

        # 1) metin katmani
        tl = from_text_layer(page)
        if tl is None:
            print("  metin katmani: YOK (goruntu tarama -> OCR sart)")
        else:
            m = find(tl)
            print(f"  metin katmani: VAR ({len(tl)} char)  match={m.method} {m.code!r}")
            print(f"    ilk 200: {tl.strip()[:200]!r}")
            ci = codeish(tl)
            if ci:
                print(f"    kod-benzeri token'lar: {ci}")

        # 2) OCR kademesi
        print("  OCR kademesi:")
        gray_cache: dict[int, "any"] = {}  # type: ignore[name-defined]
        hit = False
        for a in _CASCADE:
            g = gray_cache.setdefault(a.dpi, _page_to_gray(page, a.dpi))
            raw = _ocr(_thresh(g, a.band, a.block, C), a.psm)
            m = find(raw)
            ci = codeish(raw)
            tag = f"dpi{a.dpi} b{a.band} blk{a.block} psm{a.psm}"
            print(f"    [{tag:26}] match={m.method:8} {m.code or '-'}")
            if ci:
                print(f"        gorulen kod-benzeri: {ci}")
            if m.method == "strict":
                print(f"    >>> STRICT HIT: {m.code}  (kademe burada dururdu)")
                hit = True
                break
        if not hit:
            print("    !!! hicbir konfig STRICT vermedi — ciktilari bana yapistir")


def main(target: str) -> None:
    p = pathlib.Path(target)
    files = [p] if p.is_file() else sorted(p.glob("*.pdf")) + sorted(p.glob("*.PDF"))
    files = [f for f in files if not f.name.startswith(".")]
    if not files:
        sys.exit(f"PDF yok: {target}")
    for f in files:
        try:
            diag(f)
        except Exception as e:  # noqa: BLE001
            print(f"  HATA: {type(e).__name__}: {e}")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else ".")

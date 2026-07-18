"""GERCEK OCR duman testi — paketlenmis sidecar'i uctan uca calistirir.

NEDEN VAR: CI'daki eski duman testi sadece `list_operations` cagiriyordu. O cagri
OCR'a HIC dokunmaz — yani Tesseract bundle'da bozuk olsa bile build YESIL doner
ve hata kullanicinin masaustunde ortaya cikar. Bir kez tam olarak bu oldu.

Bu script:
  1. Icinde bilinen bir RFI kodu olan, SADECE GORUNTU iceren bir PDF uretir
     (metin katmani YOK — yoksa text-layer yolu kazanir ve OCR denenmez),
  2. Onu sidecar'a `preview` ile verir,
  3. Planin dosyayi "RFI No. {kod}.pdf" olarak adlandirdigini dogrular.

Yani PyInstaller'in fitz/cv2/pytesseract paketlemesini, Tesseract'in bulunmasini,
TESSDATA_PREFIX'i ve tum boru hattini AYNI ANDA test eder.

Kullanim:
    python tools/ci_ocr_smoke.py <sidecar-komutu> [args...]
    ornek: python tools/ci_ocr_smoke.py dist/docflow_sidecar/docflow_sidecar.exe
"""
from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile

CODE = "AZE-SUN-INT-QMS-RFI-CIV-0064"
EXPECTED = f"RFI No. {CODE}.pdf"


def make_image_only_pdf(path: pathlib.Path) -> None:
    """Metin katmani OLMAYAN PDF uret — OCR yolunu zorlamak icin sart."""
    import fitz

    # 1) Once normal (metinli) sayfa. Kod UST BANTTA olmali: OCR ilk %35'i kirpar.
    src = fitz.open()
    page = src.new_page(width=595, height=842)  # A4
    page.insert_text((60, 90), "REQUEST FOR INFORMATION", fontsize=13)
    # 16pt @ 300 DPI ~= 66px cap height; Tesseract ~30px ister.
    page.insert_text((60, 130), CODE, fontsize=16)
    page.insert_text((60, 170), "Discipline: Civil / Earthwork", fontsize=11)

    # 2) Sayfayi GORUNTUYE cevir ve goruntuyu yeni bir PDF'e goem. Boylece metin
    #    katmani kaybolur ve tek okuma yolu OCR kalir — testin amaci bu.
    pix = page.get_pixmap(dpi=200)
    out = fitz.open()
    opage = out.new_page(width=595, height=842)
    opage.insert_image(fitz.Rect(0, 0, 595, 842), pixmap=pix)
    out.save(str(path))
    out.close()
    src.close()

    # Dogrula: gercekten metinsiz mi? Degilse test OCR'i olcmuyordur.
    check = fitz.open(str(path))
    text = check[0].get_text().strip()
    check.close()
    if text:
        raise SystemExit(f"HATA: uretilen PDF'de metin katmani var ({len(text)} char) "
                         "— bu test OCR'i olcmez, duzeltilmeli")


def rpc(cmd: list[str], method: str, params: dict, cwd: str | None = None) -> dict:
    req = json.dumps({"id": 1, "method": method, "params": params}) + "\n"
    proc = subprocess.run(cmd, input=req, capture_output=True, text=True, timeout=600, cwd=cwd)
    for line in proc.stdout.splitlines():
        try:
            d = json.loads(line)
        except ValueError:
            continue
        if "result" in d:
            return d["result"]
        if "error" in d:
            raise SystemExit(f"sidecar hata dondu: {d['error']}\nstderr:\n{proc.stderr[-2000:]}")
    raise SystemExit(
        f"sidecar JSON-RPC yaniti vermedi.\nstdout:\n{proc.stdout[:2000]}\n"
        f"stderr:\n{proc.stderr[-2000:]}"
    )


def main() -> None:
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    cmd = sys.argv[1:]

    with tempfile.TemporaryDirectory() as tmp:
        d = pathlib.Path(tmp)
        src = d / "in"
        src.mkdir()
        make_image_only_pdf(src / "scan.pdf")
        print(f"goruntu-PDF uretildi: {src / 'scan.pdf'}")

        plan = rpc(cmd, "preview", {
            "op": "rfi.process_scans",
            "params": {"source": str(src), "dest": str(src), "keep_original": False},
        })

        warns = [w.get("key") for w in plan.get("warnings", [])]
        names = [pathlib.PurePath(s["dst"]).name for s in plan.get("steps", []) if s.get("dst")]
        print("uyarilar:", warns)
        print("adimlar :", names)

        if "warn.no_ocr" in warns:
            raise SystemExit(
                "BASARISIZ: OCR motoru (Tesseract) paketlenmis uygulamada BULUNAMADI.\n"
                "Bundle'da tesseract eksik ya da DOCFLOW_TESSERACT yanlis yeri gosteriyor."
            )
        if EXPECTED not in names:
            raise SystemExit(
                f"BASARISIZ: beklenen '{EXPECTED}' plan icinde yok.\n"
                f"Bulunan: {names}\nUyarilar: {warns}\n"
                "OCR calisti ama kodu okuyamadi (tessdata? dil dosyasi? render?)."
            )

    print(f"OK: paketlenmis sidecar goruntu-PDF'ten '{CODE}' okudu ve dogru adlandirdi.")


if __name__ == "__main__":
    main()

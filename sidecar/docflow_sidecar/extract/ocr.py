"""Ilk sayfadan metin cikarimi.

Strateji: metin katmani -> yoksa ust bant OCR.

OLCULMUS GERCEKLER — degistirmeden once tools/rfi_probe.py --sweep calistir:

    tam sayfa OCR                    -> MISS
    ust bant + global Otsu           -> MISS
    ust bant %35 + adaptiveThreshold -> HIT (conf 1.00)

Otsu tum bant icin TEK esik secer. Sayfadaki isik gradyani (tarayici lambasi,
fotograf golgesi) basili gri metni beyaza duzler ve kod yok olur. Adaptive her
komsuluk icin lokal esik hesaplar -> gradyandan bagimsiz.

Crop bir hiz optimizasyonu DEGIL, calisma kosulu: Tesseract ~30px cap height
ister, tam sayfada kod metni ~12px kaliyor ve cozulemiyor.
"""

from __future__ import annotations

import os

import cv2
import fitz
import numpy as np
import pytesseract

from .rfi_code import WHITELIST

# Paketlenmis derlemede tesseract binary'si bundle icinde gelir; Flutter tarafi
# DOCFLOW_TESSERACT ile yolunu verir. Yoksa PATH'teki "tesseract"e dusulur (dev).
_tess = os.environ.get("DOCFLOW_TESSERACT")
if _tess:
    pytesseract.pytesseract.tesseract_cmd = _tess

# Sayfanin ust %35'i. RFI No hep header satirinda, Date ve Rev ile ayni hizada.
BAND = 0.35

# KALIBRE EDILMEMIS. tools/rfi_probe.py --sweep ciktisini buraya yaz.
# Uyari: sahte cozunurluk dogrulugu DUSURUR — dusuk kaliteli kaynagi 300 DPI'da
# render etmek interpolasyon bulamaci uretir, bilgi eklemez.
DPI = 200
BLOCK = 31       # adaptiveThreshold blok boyutu; tek sayi olmali
C = 10           # adaptiveThreshold sabiti

TESS_CONFIG = f'--oem 3 --psm 6 -c tessedit_char_whitelist="{WHITELIST}"'

# Metin katmanini "var" saymak icin esik. Altindaysa OCR'a dus.
TEXT_LAYER_MIN_CHARS = 50


def _page_to_gray(page: fitz.Page, dpi: int) -> np.ndarray:
    pix = page.get_pixmap(dpi=dpi)
    img = np.frombuffer(pix.samples, np.uint8).reshape(pix.height, pix.width, pix.n)
    if pix.n >= 3:
        return cv2.cvtColor(img[:, :, :3], cv2.COLOR_RGB2GRAY)
    return img[:, :, 0]


def _prep(gray: np.ndarray, block: int, c: int) -> np.ndarray:
    band = gray[: int(gray.shape[0] * BAND)]
    block = max(3, block | 1)  # tek sayi zorunlu
    return cv2.adaptiveThreshold(
        band, 255, cv2.ADAPTIVE_THRESH_GAUSSIAN_C, cv2.THRESH_BINARY, block, c
    )


def from_text_layer(page: fitz.Page) -> str | None:
    """Tarayici searchable PDF uretiyorsa OCR'a hic girme. 50x hizli, %100 dogru."""
    text = page.get_text()
    return text if len(text.strip()) >= TEXT_LAYER_MIN_CHARS else None


def from_ocr(page: fitz.Page, dpi: int = DPI, block: int = BLOCK, c: int = C) -> str:
    return pytesseract.image_to_string(
        _prep(_page_to_gray(page, dpi), block, c), lang="eng", config=TESS_CONFIG
    )


def first_page_text(pdf_path: str) -> tuple[str, str]:
    """(metin, yontem) dondurur. yontem: 'text-layer' | 'ocr'"""
    with fitz.open(pdf_path) as doc:
        page = doc[0]
        if (text := from_text_layer(page)) is not None:
            return text, "text-layer"
        return from_ocr(page), "ocr"

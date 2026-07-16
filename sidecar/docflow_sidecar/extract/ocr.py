"""Ilk sayfadan RFI kodu cikarimi.

Strateji (hizli -> yavas, ilk STRICT/REPAIRED isabette durur):
  1. Metin katmani (searchable PDF) -> OCR'a hic girme, %100 dogru, 50x hizli.
  2. OCR kademesi (escalating cascade): gercek DPI render + ust bant + adaptive
     threshold. Ilk konfig iyi taramalarda isabet eder; MISS olursa DPI/blok/PSM
     ve bant genisligini kademeli buyuterek yeniden dener.

OLCULMUS GERCEKLER (gercek belge fotografi uzerinde):
    tam sayfa OCR                    -> MISS
    ust bant + global Otsu           -> MISS
    ust bant %35 + adaptiveThreshold -> HIT (conf 1.00)

Otsu tum bant icin TEK esik secer; sayfadaki isik gradyani basili gri metni
beyaza duzler. Adaptive her komsuluk icin lokal esik hesaplar -> gradyandan
bagimsiz. Ust bant crop calisma kosulu: Tesseract ~30px cap height ister, tam
sayfada kod metni ~12px kaliyor.

DIKKAT — sahte cozunurluk (dusuk kaliteli kaynagi yuksek DPI'da render etmek)
dogrulugu DUSURUR. Buradaki DPI kademesi PDF'i gercekten farkli DPI'larda
render eder (tarama yuksek cozunurlukse kazanc gercek); upscale YAPMAZ.

Kademeyi degistirmeden once gercek taramalarda `tools/rfi_diag.py <pdf>` ve
`tools/rfi_probe.py <folder> --sweep` calistir — tahmin degil, olcum.
"""

from __future__ import annotations

import os
from dataclasses import dataclass

import cv2
import fitz
import numpy as np

import pytesseract

from .rfi_code import WHITELIST, RfiMatch, find

# Paketlenmis derlemede tesseract binary'si bundle icinde gelir; Flutter tarafi
# DOCFLOW_TESSERACT ile yolunu verir. Yoksa PATH'teki "tesseract"e dusulur (dev).
_tess = os.environ.get("DOCFLOW_TESSERACT")
if _tess:
    pytesseract.pytesseract.tesseract_cmd = _tess

# Varsayilan tek-atis parametreleri (from_ocr / probe geriye donuk uyumluluk).
BAND = 0.35
DPI = 200
BLOCK = 31       # adaptiveThreshold blok boyutu; tek sayi olmali
C = 10           # adaptiveThreshold sabiti

# Metin katmanini "var" saymak icin esik. Altindaysa OCR'a dus.
TEXT_LAYER_MIN_CHARS = 50


@dataclass(frozen=True)
class _Attempt:
    dpi: int
    band: float       # sayfanin ust orani (1.0 = tam sayfa)
    block: int        # adaptiveThreshold blok (tek sayiya yuvarlanir)
    psm: int          # tesseract page segmentation mode


# Kademeli deneme sirasi. Iyi taramalar [0]'da isabet eder -> yavaslama yok.
# MISS'te asagi dogru: daha yuksek DPI, farkli blok, genis bant, farkli PSM.
# Toplam ~10 deneme ile sinirli (MISS dosya basi ~5-9 sn); isabet aninda durur.
_CASCADE: tuple[_Attempt, ...] = (
    _Attempt(300, 0.35, 31, 6),
    _Attempt(300, 0.35, 41, 6),
    _Attempt(250, 0.35, 31, 6),
    _Attempt(200, 0.35, 31, 6),
    _Attempt(300, 0.55, 31, 6),
    _Attempt(300, 0.35, 31, 11),   # sparse text
    _Attempt(350, 0.35, 41, 6),
    _Attempt(300, 0.55, 41, 4),    # tek kolon
    _Attempt(300, 1.00, 31, 11),   # tam sayfa, son care
)


def _page_to_gray(page: fitz.Page, dpi: int) -> np.ndarray:
    pix = page.get_pixmap(dpi=dpi)
    img = np.frombuffer(pix.samples, np.uint8).reshape(pix.height, pix.width, pix.n)
    if pix.n >= 3:
        return cv2.cvtColor(img[:, :, :3], cv2.COLOR_RGB2GRAY)
    return img[:, :, 0]


def _thresh(gray: np.ndarray, band: float, block: int, c: int) -> np.ndarray:
    sub = gray if band >= 1.0 else gray[: int(gray.shape[0] * band)]
    block = max(3, block | 1)  # tek sayi zorunlu
    return cv2.adaptiveThreshold(
        sub, 255, cv2.ADAPTIVE_THRESH_GAUSSIAN_C, cv2.THRESH_BINARY, block, c
    )


def _ocr(binary: np.ndarray, psm: int) -> str:
    cfg = f'--oem 3 --psm {psm} -c tessedit_char_whitelist="{WHITELIST}"'
    return pytesseract.image_to_string(binary, lang="eng", config=cfg)


def _prep(gray: np.ndarray, block: int, c: int) -> np.ndarray:
    return _thresh(gray, BAND, block, c)


def from_text_layer(page: fitz.Page) -> str | None:
    """Tarayici searchable PDF uretiyorsa OCR'a hic girme. 50x hizli, %100 dogru."""
    text = page.get_text()
    return text if len(text.strip()) >= TEXT_LAYER_MIN_CHARS else None


def from_ocr(page: fitz.Page, dpi: int = DPI, block: int = BLOCK, c: int = C) -> str:
    """Tek-atis OCR (probe/geriye donuk uyum). Uretim yolu extract_first_page."""
    return _ocr(_prep(_page_to_gray(page, dpi), block, c), psm=6)


def first_page_text(pdf_path: str) -> tuple[str, str]:
    """(metin, yontem). Geriye donuk uyum; uretim yolu extract_first_page."""
    with fitz.open(pdf_path) as doc:
        page = doc[0]
        if (text := from_text_layer(page)) is not None:
            return text, "text-layer"
        return from_ocr(page), "ocr"


def extract_first_page(pdf_path: str) -> tuple[RfiMatch, str]:
    """(RfiMatch, yontem). Metin katmani -> OCR kademesi. Ilk strict/repaired'de durur.

    yontem ornekleri: 'text-layer', 'ocr:dpi300:b0.35:blk31:psm6', 'ocr:miss'.
    Kod bulunamazsa miss RfiMatch doner (op onu _review'a yollar — asla tahmin).
    """
    with fitz.open(pdf_path) as doc:
        page = doc[0]

        text = from_text_layer(page)
        if text is not None:
            m = find(text)
            if m.method in ("strict", "repaired"):
                return m, "text-layer"

        gray_cache: dict[int, np.ndarray] = {}
        best = RfiMatch("", "", "", "miss", 0.0)
        for a in _CASCADE:
            gray = gray_cache.get(a.dpi)
            if gray is None:
                gray = _page_to_gray(page, a.dpi)
                gray_cache[a.dpi] = gray
            m = find(_ocr(_thresh(gray, a.band, a.block, C), a.psm))
            tag = f"ocr:dpi{a.dpi}:b{a.band}:blk{a.block}:psm{a.psm}"
            if m.method == "strict":
                return m, tag
            if m.method == "repaired" and best.method != "repaired":
                best = m  # ilk repaired'i sakla, yine de strict aramaya devam et

    return (best, "ocr:repaired") if best.ok else (best, "ocr:miss")

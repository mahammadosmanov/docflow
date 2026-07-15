"""RFI kod cikarimi — saf fonksiyonlar, IO yok.

Bu modul olculdu ve calisiyor. Regex'i gevsetme; asagidaki tuzaklar gercek.
Ayni sayfada ayni sekilli decoy kodlar var:

    AZE-SUN-INT-QMS-ITP-MEC-0001      <- Inspection & Test Plan referansi
    AZE-SUN-INT-ENG-STR-DRG-0901      <- teknik cizim referansi
    AZE-SUN-INT-QMS-RFI-MEC-0107      <- ISTEDIGIMIZ

Ayirt edici tek sey `-RFI-` capasi. Prefix sabit oldugu icin sadece 7 karakter
degisken (3 harf disiplin + 4 hane) — problem "rastgele metin oku"dan
"bilinen kisita karsi 7 karakter oku"ya iniyor. Dogruluk kaldiraci burada.
"""

from __future__ import annotations

import re
import unicodedata
from dataclasses import dataclass
from typing import Literal

PREFIX = "AZE-SUN-INT-QMS-RFI"
PREFIX_HEAD = "AZE-SUN-INT-QMS"   # fuzzy tolerans SADECE burada; -RFI- degil

# Sabit prefix + 3 harf disiplin + 4 hane.
STRICT = re.compile(r"AZE-SUN-INT-QMS-RFI-([A-Z]{3})-(\d{4})")

# Onarim adayi yakalamak icin: 6-7 segmentli tireli token.
LOOSE = re.compile(r"([A-Z0-9]{2,4}(?:-[A-Z0-9]{2,4}){5,6})")

# Tesseract'in tanidigi karakter kumesini daralt. Kod yalnizca buyuk
# harf + rakam + tire icerir.
WHITELIST = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-:. "

# Bilinen disiplin kodlari. Bos birakilirsa dogrulama atlanir.
# Gercek taramalardan gordukce genislet — fuzzy yolun isabetini yukseltir.
DISCIPLINES: set[str] = {"MEC", "ELE", "CIV", "STR", "INS", "PIP", "ARC", "TEL"}

Method = Literal["strict", "repaired", "fuzzy", "miss"]

# Pozisyonel OCR onarimi. Kodun son segmenti HER ZAMAN rakam, oncekiler
# HER ZAMAN harf. Bunu bildigimiz icin duzeltme deterministik — tahmin degil.
# NOT: canonical() once UPPER yapar, bu yuzden tablolar SADECE buyuk harf icermeli.
# Kucuk 'l' yazmak olu koddur — sessizce hicbir sey yapmaz.
_TO_LETTER = str.maketrans("012458", "OIZASB")
_TO_DIGIT = str.maketrans("OILSBZ", "011582")


@dataclass(frozen=True)
class RfiMatch:
    code: str
    discipline: str
    number: str
    method: Method
    confidence: float

    @property
    def ok(self) -> bool:
        return self.method != "miss"


def canonical(code: str) -> str:
    """NFC + UPPER. Bir kez cagrilir, sonra ASLA case donusumu uygulanmaz.

    Gerekce: Python locale-aware casing yapmaz. Turkce I/i ikilisi ve
    "İ".lower() -> "i̇" (i + U+0307 combining dot) dosya adi karsilastirmasini,
    SharePoint senkronunu ve macOS'un NFD normalizasyonunu sessizce bozar.
    """
    return unicodedata.normalize("NFC", code).upper()


def repair(token: str) -> str:
    """Segment tipine gore OCR karisikligini duzelt (0<->O, 1<->I, 5<->S, 8<->B)."""
    segs = token.split("-")
    if len(segs) < 2:
        return token
    return "-".join([s.translate(_TO_LETTER) for s in segs[:-1]] + [segs[-1].translate(_TO_DIGIT)])


def _levenshtein(a: str, b: str) -> int:
    if len(a) < len(b):
        a, b = b, a
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a):
        cur = [i + 1]
        for j, cb in enumerate(b):
            cur.append(min(prev[j + 1] + 1, cur[j] + 1, prev[j] + (ca != cb)))
        prev = cur
    return prev[-1]


def _build(discipline: str, number: str, method: Method, confidence: float) -> RfiMatch:
    return RfiMatch(
        code=f"{PREFIX}-{discipline}-{number}",
        discipline=discipline,
        number=number,
        method=method,
        confidence=confidence,
    )


def find(text: str) -> RfiMatch:
    """Uc kademeli eslestirme: strict -> repaired -> fuzzy.

    Tam sayfa metni degil, ust bant OCR ciktisi ver. Tam sayfa hem yavas hem
    decoy riskini artiriyor.
    """
    text = canonical(text)

    m = STRICT.search(text)
    if m:
        return _build(m.group(1), m.group(2), "strict", 1.00)

    for cand in LOOSE.findall(text):
        fixed = repair(cand)

        m = STRICT.search(fixed)
        if m:
            return _build(m.group(1), m.group(2), "repaired", 0.80)

        segs = fixed.split("-")
        if len(segs) != 7:
            continue

        # KRITIK: 5. segment belge TIPI ayirt edicisi. ASLA fuzzy eslesmez.
        # Aksi halde AZE-SUN-INT-QMS-ITP-MEC-0001 (Levenshtein mesafesi 3)
        # sessizce bir RFI'a donusur — belge yanlis etiketlenir ve kimse fark
        # etmez. Kacirmak geri alinabilir (_review/ klasorune gider);
        # yanlis etiketlemek sessiz veri bozulmasidir. Fail-safe > fail-open.
        if segs[4] != "RFI":
            continue

        head, disc, num = "-".join(segs[:4]), segs[5], segs[6]
        if not (re.fullmatch(r"[A-Z]{3}", disc) and re.fullmatch(r"\d{4}", num)):
            continue
        if DISCIPLINES and disc not in DISCIPLINES:
            continue
        d = _levenshtein(head, PREFIX_HEAD)
        if d <= 2:
            return _build(disc, num, "fuzzy", round(1 - d / 10, 2))

    return RfiMatch("", "", "", "miss", 0.0)


def filename(code: str, rev: int = 0) -> str:
    """Dosya adi rev ICERIR. Klasor adi icermez — bkz. CLAUDE.md."""
    return f"{code}_R{rev}.pdf"


def foldername(code: str) -> str:
    """Klasor adi rev ICERMEZ: bir RFI'in tum revizyonlari tek klasorde."""
    return code

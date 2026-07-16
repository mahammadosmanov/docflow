"""Hedef yol kurali — TEK kaynak. Hem rfi_process_scans hem review kullanir.

Kural (sahibi karari):
  - DOSYA adi : "RFI No. {code}.pdf"  (or. "RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf")
  - KLASOR adi: "{code}"              (or. "AZE-SUN-INT-QMS-RFI-CIV-0064") — onek YOK
  - {dest}/{code}/ klasoru ZATEN VARSA -> dosyayi onun icine koy.
  - Yoksa -> DUZ {dest}/RFI No. {code}.pdf
  - ASLA yeni klasor olusturma.

Onek neden SADECE dosyada: klasor adi kodun kendisi olmali ki var olan klasorler
(elle acilmis olanlar dahil) eslesmeye devam etsin. Onegi klasore de eklemek,
kullanicinin mevcut arsivindeki her klasoru bir anda "yok" sayardi.
"""
from __future__ import annotations

import pathlib

# Dosya adi oneki. Sondaki bosluk KASITLI: "RFI No. AZE-SUN-..." diye okunur.
FILE_PREFIX = "RFI No. "


def filename_for(code: str) -> str:
    """Dosya adi. Kod ASLA degistirilmez (CLAUDE.md kural 7 — case donusumu yok)."""
    return f"{FILE_PREFIX}{code}.pdf"


def foldername_for(code: str) -> str:
    """Klasor adi = kodun kendisi. Onek yok, revizyon yok."""
    return code


def target_for(dest: pathlib.Path, code: str) -> pathlib.Path:
    folder = dest / foldername_for(code)
    if folder.is_dir():
        return folder / filename_for(code)
    return dest / filename_for(code)

"""Hedef yol kurali — TEK kaynak. Hem rfi_process_scans hem review kullanir.

Kural (sahibi karari): dosya adi = RFI numarasi, baska hicbir sey.
  - {dest}/{code}/ klasoru ZATEN VARSA -> dosyayi onun icine koy.
  - Yoksa -> DUZ {dest}/{code}.pdf.
  - ASLA yeni klasor olusturma.
"""
from __future__ import annotations

import pathlib


def target_for(dest: pathlib.Path, code: str) -> pathlib.Path:
    folder = dest / code
    if folder.is_dir():
        return folder / f"{code}.pdf"
    return dest / f"{code}.pdf"

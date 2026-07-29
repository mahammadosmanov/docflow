"""Hedef yol kurali — TEK kaynak. Hem rfi_process_scans hem review kullanir.

Kural (sahibi karari):
  - HER RFI kendi klasorunde: {dest}/{code}/RFI No. {code}.pdf
  - Klasor VARSA         -> dosyayi onun icine koy (yeni klasor acma).
  - Klasor YOKSA         -> klasoru OLUSTUR, dosyayi icine koy.
  - DOSYA adi : "RFI No. {code}.pdf"  (or. "RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf")
  - KLASOR adi: "{code}"              (or. "AZE-SUN-INT-QMS-RFI-CIV-0064") — onek YOK

Onek neden SADECE dosyada: klasor adi kodun kendisi olmali ki var olan klasorler
(elle acilmis olanlar dahil) eslesmeye devam etsin. Onegi klasore de eklemek,
kullanicinin mevcut arsivindeki her klasoru bir anda "yok" sayardi.

Ayni RFI'in tum revizyonlari tek klasorde toplansin diye klasor adinda revizyon
YOK — bkz. CLAUDE.md "RFI adlandirma karari".
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


def folder_for(dest: pathlib.Path, code: str) -> pathlib.Path:
    """RFI'in klasoru. Var olabilir de olmayabilir de — cagiran kontrol eder."""
    return dest / foldername_for(code)


def target_for(dest: pathlib.Path, code: str) -> pathlib.Path:
    """Dosyanin gidecegi TAM yol. Klasor yoksa bile bu yolu doner; klasoru
    olusturmak plan'in isi (mkdir adimi) — preview dosya sistemine DOKUNMAZ."""
    return folder_for(dest, code) / filename_for(code)


def flat_target_for(dest: pathlib.Path, code: str) -> pathlib.Path:
    """Klasorsuz modda dosyanin gidecegi TAM yol.

    Bu ayri yardimci, mevcut ``target_for`` kuralini ve review akisini
    degistirmeden Process Scans'in opt-in duz cikti modunu destekler.
    """
    return dest / filename_for(code)

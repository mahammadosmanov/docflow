"""rfi_code saf fonksiyon — test etmemek icin mazeret yok.

Asagidaki OCR ciktilari UYDURMA DEGIL: gercek belge fotografindan alindi.
"""
import pytest
from docflow_sidecar.extract import rfi_code as rc

# Gercek Tesseract ciktisi (ust bant, adaptive threshold). Etiket "REI N0"
# olarak curumus ama kod saglam — etikete capa atmama kararinin kaniti.
REAL_OCR = (
    "REQUEST FOR INSPECTION\n"
    "D 18.02.2026 REI N0 AZE-SUN-INT-QMS-RFI-MEC-0107 -R O I\n"
    "A PCS-05IS1 R1. RZ R3. R4. RS. RG R7 AN2  NT-ENG-STRO\n"
    "I S -  D AZE-SUN-INT-ENG-STR-\nSS DRG-0901S-205\n"
)


def test_real_ocr_output_strict_hit():
    m = rc.find(REAL_OCR)
    assert m.code == "AZE-SUN-INT-QMS-RFI-MEC-0107"
    assert m.method == "strict"
    assert m.confidence == 1.00


def test_decoy_itp_rejected():
    """ITP referansi ayni sekilli — -RFI- capasi elemeli."""
    assert not rc.find("AZE-SUN-INT-QMS-ITP-MEC-0001").ok


def test_decoy_drg_rejected():
    assert not rc.find("AZE-SUN-INT-ENG-STR-DRG-0901 S-205").ok


def test_decoy_alongside_real_picks_real():
    text = "AZE-SUN-INT-QMS-ITP-MEC-0001 ... AZE-SUN-INT-QMS-RFI-MEC-0107"
    assert rc.find(text).code == "AZE-SUN-INT-QMS-RFI-MEC-0107"


@pytest.mark.parametrize("bad,good", [
    ("AZE-SUN-INT-QMS-RFI-MEC-O1O7", "AZE-SUN-INT-QMS-RFI-MEC-0107"),  # O -> 0
    ("AZE-SUN-INT-QMS-RFI-MEC-Ol07", "AZE-SUN-INT-QMS-RFI-MEC-0107"),  # l -> 1
    ("4ZE-SUN-1NT-QMS-RFI-MEC-0107", "AZE-SUN-INT-QMS-RFI-MEC-0107"),  # 1 -> I
])
def test_positional_repair(bad, good):
    m = rc.find(bad)
    assert m.ok and m.code == good


def test_canonical_never_lowercases():
    assert rc.canonical("aze-sun-int-qms-rfi-mec-0107") == "AZE-SUN-INT-QMS-RFI-MEC-0107"


def test_canonical_nfc_dotted_i():
    """NFD 'İ' (I + U+0307) NFC'ye toplanmali — yoksa dosya adi karsilastirmasi bozulur."""
    assert rc.canonical("I\u0307NT") == "\u0130NT"


def test_folder_has_no_rev_file_has_rev():
    code = "AZE-SUN-INT-QMS-RFI-MEC-0107"
    assert rc.foldername(code) == code
    assert rc.filename(code, 0) == f"{code}_R0.pdf"
    assert rc.filename(code, 1) == f"{code}_R1.pdf"


def test_miss_is_explicit_not_guess():
    m = rc.find("burada hicbir kod yok")
    assert not m.ok and m.code == "" and m.confidence == 0.0

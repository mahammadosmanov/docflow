"""Opt-in test that exercises the real PDF -> OpenCV -> Tesseract path."""
from __future__ import annotations

import os

import pytest

from docflow_sidecar.extract import extract_first_page
from tests.ocr_fixture import EXPECTED_CODE, create_ocr_fixture


@pytest.mark.skipif(
    os.environ.get("DOCFLOW_RUN_OCR_INTEGRATION") != "1",
    reason="real OCR integration is enabled by the Windows release workflow",
)
def test_real_image_only_pdf_is_recognized(tmp_path) -> None:
    pdf = create_ocr_fixture(tmp_path / "scan.pdf")
    match, source = extract_first_page(str(pdf))

    assert match.code == EXPECTED_CODE
    assert match.method in {"strict", "repaired"}
    assert source.startswith("ocr:")

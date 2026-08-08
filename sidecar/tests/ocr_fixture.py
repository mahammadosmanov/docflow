"""Create a deterministic image-only PDF for real OCR release checks."""
from __future__ import annotations

import argparse
from pathlib import Path

import cv2
import pymupdf as fitz
import numpy as np

EXPECTED_CODE = "AZE-SUN-INT-QMS-RFI-CIV-0428"


def create_ocr_fixture(output: Path) -> Path:
    output.parent.mkdir(parents=True, exist_ok=True)
    image = np.full((1000, 2200), 255, dtype=np.uint8)
    cv2.putText(
        image,
        EXPECTED_CODE,
        (90, 230),
        cv2.FONT_HERSHEY_SIMPLEX,
        2.05,
        0,
        5,
        cv2.LINE_AA,
    )
    cv2.putText(
        image,
        "REQUEST FOR INSPECTION",
        (90, 390),
        cv2.FONT_HERSHEY_SIMPLEX,
        1.45,
        0,
        4,
        cv2.LINE_AA,
    )
    ok, png = cv2.imencode(".png", image)
    if not ok:
        raise RuntimeError("OCR fixture PNG could not be encoded")

    document = fitz.open()
    page = document.new_page(width=1100, height=500)
    page.insert_image(page.rect, stream=png.tobytes())
    document.save(output)
    document.close()

    with fitz.open(output) as check:
        if check[0].get_text().strip():
            raise RuntimeError("OCR fixture unexpectedly contains a text layer")
    return output


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    print(create_ocr_fixture(args.output))


if __name__ == "__main__":
    main()

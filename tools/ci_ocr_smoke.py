"""End-to-end OCR smoke test for the packaged DocFlow executable.

The generated PDF contains only pixels, not a searchable text layer.  The
frozen application must find its bundled Tesseract installation and extract the
expected RFI code.  Success is reported by the executable's exit status.

Usage:
    python tools/ci_ocr_smoke.py path/to/docflow-sidecar[.exe]
"""

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

CODE = "AZE-SUN-INT-QMS-RFI-CIV-0064"


def make_image_only_pdf(path: pathlib.Path) -> None:
    import fitz

    source = fitz.open()
    page = source.new_page(width=595, height=842)
    page.insert_text((60, 90), "REQUEST FOR INFORMATION", fontsize=13)
    page.insert_text((60, 130), CODE, fontsize=16)
    page.insert_text((60, 170), "Discipline: Civil / Earthwork", fontsize=11)

    pixmap = page.get_pixmap(dpi=200)
    output = fitz.open()
    image_page = output.new_page(width=595, height=842)
    image_page.insert_image(fitz.Rect(0, 0, 595, 842), pixmap=pixmap)
    output.save(str(path))
    output.close()
    source.close()

    check = fitz.open(str(path))
    text = check[0].get_text().strip()
    check.close()
    if text:
        raise SystemExit(
            f"generated PDF has a text layer ({len(text)} chars); OCR was not tested"
        )


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    executable = pathlib.Path(sys.argv[1]).resolve()
    with tempfile.TemporaryDirectory() as directory:
        pdf = pathlib.Path(directory) / "scan.pdf"
        make_image_only_pdf(pdf)
        result = subprocess.run(
            [str(executable), "--ocr-smoke", str(pdf), CODE],
            capture_output=True,
            text=True,
            timeout=600,
        )
        if result.returncode:
            raise SystemExit(
                "packaged OCR smoke failed "
                f"(exit={result.returncode})\nstdout:\n{result.stdout[-2000:]}\n"
                f"stderr:\n{result.stderr[-2000:]}"
            )
    print(f"OK: packaged DocFlow extracted {CODE} from an image-only PDF")


if __name__ == "__main__":
    main()

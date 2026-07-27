"""Opt-in integration test for the real Excel COM clipboard renderer."""
from __future__ import annotations

import os

import pytest
from openpyxl import Workbook
from openpyxl.styles import Font, PatternFill

from docflow_sidecar.clipboard import _excel_clipboard_payload
from docflow_sidecar.contracts import Plan


@pytest.mark.skipif(
    os.name != "nt" or os.environ.get("DOCFLOW_RUN_EXCEL_INTEGRATION") != "1",
    reason="requires Microsoft Excel on an interactive Windows machine",
)
def test_excel_filters_exact_rfis_and_copies_header_with_native_format(tmp_path):
    path = tmp_path / "RFI Log.xlsx"
    workbook = Workbook()
    sheet = workbook.active
    sheet.title = "CIV"
    headers = [
        "RFI No.",
        "ITP No.",
        "Item No.",
        "INSPECTION DESCRIPTION",
        "FORMS",
        "TAG NO",
        "AREA",
        "DATE OF ISSUE",
        "INSPECTION DATE",
        "EXPIRY",
        "INSPECTION START TIME",
        "INSP. TYPE AZFEN",
        "INSP. TYPE INTEC",
        "Inspection Location",
        "SUBCONTRACTOR",
    ]
    for column, value in enumerate(headers, 1):
        cell = sheet.cell(9, column, value)
        cell.font = Font(bold=True, color="FFFFFF")
        cell.fill = PatternFill("solid", fgColor="4F4F4F")
        sheet.column_dimensions[cell.column_letter].width = 16
    rows = {
        10: ("AZE-SUN-INT-QMS-RFI-CIV-0100", "OLD"),
        12: ("AZE-SUN-INT-QMS-RFI-CIV-0861", "SECOND"),
        14: ("AZE-SUN-INT-QMS-RFI-CIV-0860", "FIRST"),
    }
    for row_number, (rfi, form) in rows.items():
        sheet.cell(row_number, 1, rfi)
        sheet.cell(row_number, 5, form)
        sheet.cell(row_number, 15, "AZFEN")
    workbook.save(path)
    workbook.close()

    expected = [
        "AZE-SUN-INT-QMS-RFI-CIV-0860",
        "AZE-SUN-INT-QMS-RFI-CIV-0861",
    ]
    plan = Plan(
        op_id="rfi.log_add",
        meta={
            "editor": {
                "kind": "recordTable",
                "copyEnabled": True,
                "copySource": {
                    "workbook": str(path),
                    "sheet": "CIV",
                    "headerRow": 9,
                    "startRow": 12,
                    "rfiNumbers": expected,
                    "columns": [1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 15],
                },
            }
        },
    )

    payload = _excel_clipboard_payload(plan, 2)

    assert payload is not None
    html_bytes, plain = payload
    assert html_bytes.startswith(b"Version:")
    assert "RFI No." in plain
    assert "FIRST" in plain
    assert "SECOND" in plain
    assert "OLD" not in plain
    assert plain.index("SECOND") < plain.index("FIRST")
    assert "EXPIRY" not in plain

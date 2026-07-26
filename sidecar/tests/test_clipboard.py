import re

from openpyxl import Workbook

from docflow_sidecar.clipboard import _cf_html, _payload
from docflow_sidecar.contracts import Plan


def test_mail_table_stops_at_subcontractor_and_has_plain_fallback():
    fields = [
        "rfiNo",
        "forms",
        "subcontractor",
    ]
    plan = Plan(
        op_id="rfi.log_add",
        meta={
            "editor": {
                "kind": "recordTable",
                "copyEnabled": True,
                "copyFields": fields,
                "copyHeaders": ["RFI No.", "FORMS", "SUBCONTRACTOR"],
                "rows": [
                    {
                        "rfiNo": "AZE-SUN-INT-QMS-RFI-CIV-0860",
                        "forms": "CON-CIV-001",
                        "subcontractor": "AZFEN",
                        "expiry": "MUST NOT COPY",
                        "status": "MUST NOT COPY",
                    }
                ],
            }
        },
    )

    fragment, plain, count = _payload(plan)
    assert count == 1
    assert "AZE-SUN-INT-QMS-RFI-CIV-0860" in fragment
    assert "\t" in plain
    assert "MUST NOT COPY" not in fragment
    assert "MUST NOT COPY" not in plain

    encoded = _cf_html(fragment)
    assert b"StartFragment:" in encoded
    assert b"<!--StartFragment-->" in encoded
    assert encoded.endswith(b"\0")


def test_cf_html_offsets_are_utf8_byte_offsets():
    encoded = _cf_html("<table><tr><td>Şimal sahəsi</td></tr></table>")
    fields = {
        key.decode(): int(value)
        for key, value in re.findall(
            rb"(StartHTML|EndHTML|StartFragment|EndFragment):(\d+)",
            encoded,
        )
    }
    assert encoded[fields["StartHTML"] :].startswith(b"<html>")
    assert encoded[fields["StartFragment"] :].startswith(b"<table>")
    assert encoded[fields["EndFragment"] :].startswith(b"<!--EndFragment-->")
    assert encoded[fields["EndHTML"] :].startswith(b"\0")


def test_mail_table_reads_applied_values_and_headers_from_workbook(tmp_path):
    path = tmp_path / "RFI Log.xlsx"
    workbook = Workbook()
    sheet = workbook.active
    sheet.title = "CIV"
    sheet["A9"] = "RFI No. FROM EXCEL"
    sheet["E9"] = "FORMS FROM EXCEL"
    sheet["O9"] = "SUBCONTRACTOR FROM EXCEL"
    sheet["A10"] = "AZE-SUN-INT-QMS-RFI-CIV-0860"
    sheet["E10"] = "ACTUAL FORM"
    sheet["O10"] = "ACTUAL COMPANY"
    workbook.save(path)
    workbook.close()

    plan = Plan(
        op_id="rfi.log_add",
        meta={
            "editor": {
                "kind": "recordTable",
                "copyEnabled": True,
                "copyFields": ["rfiNo", "forms", "subcontractor"],
                "copyHeaders": ["preview rfi", "preview forms", "preview company"],
                "copySource": {
                    "workbook": str(path),
                    "sheet": "CIV",
                    "headerRow": 9,
                    "startRow": 10,
                    "columns": [1, 5, 15],
                },
                "rows": [
                    {
                        "rfiNo": "AZE-SUN-INT-QMS-RFI-CIV-0860",
                        "forms": "PREVIEW FORM",
                        "subcontractor": "PREVIEW COMPANY",
                    }
                ],
            }
        },
    )

    fragment, plain, count = _payload(plan)

    assert count == 1
    assert "RFI No. FROM EXCEL" in fragment
    assert "ACTUAL FORM" in fragment
    assert "ACTUAL COMPANY" in plain
    assert "PREVIEW FORM" not in fragment

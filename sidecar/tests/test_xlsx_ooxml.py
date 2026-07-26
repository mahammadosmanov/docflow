from __future__ import annotations

from xml.etree import ElementTree as ET

from docflow_sidecar.xlsx_ooxml import MAIN, _patch_calc_chain, _patch_sheet


def test_patch_preserves_namespaces_referenced_only_by_mc_ignorable():
    raw = b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet
 xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
 xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
 xmlns:x14ac="http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac"
 xmlns:xr2="http://schemas.microsoft.com/office/spreadsheetml/2015/revision2"
 mc:Ignorable="x14ac xr2">
 <sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>
</worksheet>"""

    patched = _patch_sheet(raw, {})

    assert b"xmlns:x14ac=" in patched
    assert b"xmlns:xr2=" in patched
    assert b'mc:Ignorable="x14ac xr2"' in patched
    ET.fromstring(patched)


def test_value_replacing_formula_is_removed_from_target_sheet_calc_chain():
    sheet = b"""<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
 <sheetData><row r="10"><c r="D10" t="str"><f>1+1</f><v>2</v></c></row></sheetData>
</worksheet>"""
    replaced: set[str] = set()

    _patch_sheet(sheet, {(10, 4): ("value", "manual")}, replaced)

    assert replaced == {"D10"}

    chain = b"""<?xml version="1.0" encoding="UTF-8"?>
<calcChain xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
 <c r="D10" i="11"/><c r="E10"/><c r="D10" i="12"/>
</calcChain>"""
    patched = _patch_calc_chain(chain, 11, replaced)
    root = ET.fromstring(patched)
    entries = [
        (cell.attrib["r"], cell.attrib.get("i"))
        for cell in root.findall(f"{{{MAIN}}}c")
    ]

    assert entries == [("E10", "11"), ("D10", "12")]

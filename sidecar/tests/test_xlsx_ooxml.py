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


def test_patch_allows_only_consecutive_duplicate_identifiers():
    sheet = b"""<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
 <sheetData><row r="10"><c r="A10"><v>1</v></c></row></sheetData>
 <dataValidations count="2">
  <dataValidation type="custom" sqref="A10:A18820">
   <formula1>COUNTIF($A$16:$A$891,A10)&lt;=1</formula1>
  </dataValidation>
  <dataValidation type="list" sqref="C10:C18820">
   <formula1>INDIRECT(B10)</formula1>
  </dataValidation>
 </dataValidations>
</worksheet>"""

    patched = _patch_sheet(
        sheet,
        {},
        allow_consecutive_duplicates_column=1,
    )
    root = ET.fromstring(patched)
    validations = root.find(f"{{{MAIN}}}dataValidations")
    assert validations is not None
    formulas = [
        validation.find(f"{{{MAIN}}}formula1").text
        for validation in validations.findall(f"{{{MAIN}}}dataValidation")
    ]

    assert formulas == [
        'OR(A10="",COUNTIF($A$10:A10,A10)=1,A10=A9)',
        "INDIRECT(B10)",
    ]

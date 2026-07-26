"""OOXML .xlsx paketinde yalniz hedef hucreleri degistiren korumali yazici.

openpyxl benzeri tam-workbook serializer'lari desteklemedikleri Excel extension
parcalarini kayit sirasinda silebilir. Burada ZIP icindeki tum parcalar aynen
kopyalanir; yalniz hedef worksheet XML'i ve recalculation bayraklari degisir.
"""

from __future__ import annotations

import copy
import io
import re
import zipfile
from datetime import date, time
from pathlib import Path
from typing import Any
from xml.etree import ElementTree as ET

MAIN = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
OFFICE_REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PACKAGE_REL = "http://schemas.openxmlformats.org/package/2006/relationships"
XML_NS = "http://www.w3.org/XML/1998/namespace"
NS = {"m": MAIN, "r": OFFICE_REL}
REL_NS = {"p": PACKAGE_REL}
CELL_RE = re.compile(r"^([A-Z]+)(\d+)$")


def _namespaces(raw: bytes) -> None:
    for _event, item in ET.iterparse(io.BytesIO(raw), events=("start-ns",)):
        prefix, uri = item
        if prefix not in ("xml", "xmlns"):
            ET.register_namespace(prefix or "", uri)


def _xml(raw: bytes) -> ET.Element:
    _namespaces(raw)
    parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    return ET.fromstring(raw, parser=parser)


def _serialize(root: ET.Element) -> bytes:
    return ET.tostring(root, encoding="utf-8", xml_declaration=True)


def _sheet_path(archive: zipfile.ZipFile, sheet_name: str) -> str:
    workbook = ET.fromstring(archive.read("xl/workbook.xml"))
    relationships = ET.fromstring(archive.read("xl/_rels/workbook.xml.rels"))
    targets = {
        rel.attrib["Id"]: rel.attrib["Target"]
        for rel in relationships.findall("p:Relationship", REL_NS)
    }
    sheets = workbook.find("m:sheets", NS)
    if sheets is None:
        raise ValueError("workbook sheets bulunamadi")
    for sheet in sheets:
        if sheet.attrib.get("name") != sheet_name:
            continue
        target = targets[sheet.attrib[f"{{{OFFICE_REL}}}id"]].replace("\\", "/")
        if target.startswith("/"):
            return target.lstrip("/")
        return f"xl/{target}"
    raise KeyError(sheet_name)


def _column_number(reference: str) -> int:
    match = CELL_RE.match(reference)
    if not match:
        raise ValueError(reference)
    result = 0
    for char in match.group(1):
        result = result * 26 + ord(char) - ord("A") + 1
    return result


def _column_name(number: int) -> str:
    out = ""
    while number:
        number, remainder = divmod(number - 1, 26)
        out = chr(ord("A") + remainder) + out
    return out


def _row(sheet_data: ET.Element, row_number: int) -> ET.Element:
    for row in sheet_data.findall(f"{{{MAIN}}}row"):
        current = int(row.attrib["r"])
        if current == row_number:
            return row
        if current > row_number:
            created = ET.Element(f"{{{MAIN}}}row", {"r": str(row_number)})
            sheet_data.insert(list(sheet_data).index(row), created)
            return created
    created = ET.SubElement(sheet_data, f"{{{MAIN}}}row", {"r": str(row_number)})
    return created


def _template_style(
    sheet_data: ET.Element,
    row_number: int,
    column_number: int,
) -> str | None:
    reference_prefix = _column_name(column_number)
    best: tuple[int, str] | None = None
    for row in sheet_data.findall(f"{{{MAIN}}}row"):
        current = int(row.attrib["r"])
        for cell in row.findall(f"{{{MAIN}}}c"):
            match = CELL_RE.match(cell.attrib.get("r", ""))
            if not match or match.group(1) != reference_prefix:
                continue
            style = cell.attrib.get("s")
            if style is None:
                continue
            distance = abs(current - row_number)
            if best is None or distance < best[0]:
                best = (distance, style)
    return best[1] if best else None


def _cell(
    sheet_data: ET.Element,
    row: ET.Element,
    row_number: int,
    column_number: int,
) -> ET.Element:
    reference = f"{_column_name(column_number)}{row_number}"
    for cell in row.findall(f"{{{MAIN}}}c"):
        current = _column_number(cell.attrib["r"])
        if current == column_number:
            return cell
        if current > column_number:
            attrs = {"r": reference}
            style = _template_style(sheet_data, row_number, column_number)
            if style is not None:
                attrs["s"] = style
            created = ET.Element(f"{{{MAIN}}}c", attrs)
            row.insert(list(row).index(cell), created)
            return created
    attrs = {"r": reference}
    style = _template_style(sheet_data, row_number, column_number)
    if style is not None:
        attrs["s"] = style
    return ET.SubElement(row, f"{{{MAIN}}}c", attrs)


def _excel_date(value: date) -> int:
    return (value - date(1899, 12, 30)).days


def _excel_time(value: time) -> float:
    return (
        value.hour * 3600 + value.minute * 60 + value.second
    ) / (24 * 60 * 60)


def _write_value(cell: ET.Element, value: Any) -> None:
    for child in list(cell):
        cell.remove(child)
    cell.attrib.pop("t", None)
    if value in (None, ""):
        return
    if isinstance(value, str):
        cell.attrib["t"] = "inlineStr"
        inline = ET.SubElement(cell, f"{{{MAIN}}}is")
        text = ET.SubElement(inline, f"{{{MAIN}}}t")
        if value != value.strip():
            text.attrib[f"{{{XML_NS}}}space"] = "preserve"
        text.text = value
        return
    if isinstance(value, bool):
        cell.attrib["t"] = "b"
        ET.SubElement(cell, f"{{{MAIN}}}v").text = "1" if value else "0"
        return
    if isinstance(value, date):
        ET.SubElement(cell, f"{{{MAIN}}}v").text = str(_excel_date(value))
        return
    if isinstance(value, time):
        ET.SubElement(cell, f"{{{MAIN}}}v").text = f"{_excel_time(value):.15g}"
        return
    ET.SubElement(cell, f"{{{MAIN}}}v").text = str(value)


def _write_formula(cell: ET.Element, formula: str) -> None:
    for child in list(cell):
        cell.remove(child)
    cell.attrib.pop("t", None)
    ET.SubElement(cell, f"{{{MAIN}}}f").text = formula.removeprefix("=")


def _patch_sheet(
    raw: bytes,
    updates: dict[tuple[int, int], tuple[str, Any]],
) -> bytes:
    root = _xml(raw)
    sheet_data = root.find("m:sheetData", NS)
    if sheet_data is None:
        raise ValueError("worksheet sheetData bulunamadi")
    for (row_number, column_number), (kind, value) in sorted(updates.items()):
        row = _row(sheet_data, row_number)
        cell = _cell(sheet_data, row, row_number, column_number)
        if kind == "formula":
            _write_formula(cell, str(value))
        elif kind == "value":
            _write_value(cell, value)
        else:
            raise ValueError(f"bilinmeyen hucre yazma turu: {kind}")
    return _serialize(root)


def _patch_calculation(raw: bytes) -> bytes:
    root = _xml(raw)
    calculation = root.find("m:calcPr", NS)
    if calculation is None:
        calculation = ET.SubElement(root, f"{{{MAIN}}}calcPr")
    calculation.attrib["calcMode"] = "auto"
    calculation.attrib["fullCalcOnLoad"] = "1"
    calculation.attrib["forceFullCalc"] = "1"
    return _serialize(root)


def patch_workbook(
    source: Path,
    destination: Path,
    sheet_name: str,
    updates: dict[tuple[int, int], tuple[str, Any]],
) -> None:
    with zipfile.ZipFile(source, "r") as incoming:
        sheet_path = _sheet_path(incoming, sheet_name)
        patched_sheet = _patch_sheet(incoming.read(sheet_path), updates)
        patched_workbook = _patch_calculation(incoming.read("xl/workbook.xml"))
        with zipfile.ZipFile(destination, "w") as outgoing:
            for info in incoming.infolist():
                data = incoming.read(info.filename)
                if info.filename == sheet_path:
                    data = patched_sheet
                elif info.filename == "xl/workbook.xml":
                    data = patched_workbook
                outgoing.writestr(copy.copy(info), data)

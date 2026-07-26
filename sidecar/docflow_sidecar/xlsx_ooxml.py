"""OOXML .xlsx paketinde yalniz hedef hucreleri degistiren korumali yazici.

openpyxl benzeri tam-workbook serializer'lari desteklemedikleri Excel extension
parcalarini kayit sirasinda silebilir. Burada ZIP icindeki tum parcalar aynen
kopyalanir; yalniz hedef worksheet XML'i ve gerekiyorsa calcChain girdileri
degisir.
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
from xml.sax.saxutils import quoteattr

MAIN = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
OFFICE_REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PACKAGE_REL = "http://schemas.openxmlformats.org/package/2006/relationships"
XML_NS = "http://www.w3.org/XML/1998/namespace"
NS = {"m": MAIN, "r": OFFICE_REL}
REL_NS = {"p": PACKAGE_REL}
CELL_RE = re.compile(r"^([A-Z]+)(\d+)$")


def _namespaces(raw: bytes) -> dict[str, str]:
    namespaces: dict[str, str] = {}
    for _event, item in ET.iterparse(io.BytesIO(raw), events=("start-ns",)):
        prefix, uri = item
        namespaces.setdefault(prefix, uri)
        if prefix not in ("xml", "xmlns"):
            ET.register_namespace(prefix or "", uri)
    return namespaces


def _xml(
    raw: bytes,
    namespaces: dict[str, str] | None = None,
) -> ET.Element:
    if namespaces is None:
        _namespaces(raw)
    parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    return ET.fromstring(raw, parser=parser)


def _serialize(
    root: ET.Element,
    *,
    original_namespaces: dict[str, str] | None = None,
) -> bytes:
    data = ET.tostring(root, encoding="utf-8", xml_declaration=True)
    if not original_namespaces:
        return data

    # ElementTree yalniz QName olarak kullanilan namespace'leri yazar. Excel
    # dosyalarinda ise mc:Ignorable="xr2 xr3" gibi attribute degerleri icinde
    # kullanilan prefix'ler de zorunludur. Bunlari kaybetmek Excel'in dosyayi
    # bozuk saymasina neden olur; kaynak root bildirimlerini byte duzeyinde geri
    # ekleyerek unsupported extension'lari oldugu gibi koruyoruz.
    declaration_end = data.find(b"?>")
    root_start = data.find(b"<", declaration_end + 2 if declaration_end >= 0 else 0)
    root_end = data.find(b">", root_start)
    if root_start < 0 or root_end < 0:
        raise ValueError("XML root elementi bulunamadi")
    root_header = data[root_start:root_end]
    missing: list[bytes] = []
    for prefix, uri in original_namespaces.items():
        attribute = b"xmlns" if not prefix else f"xmlns:{prefix}".encode("ascii")
        if attribute + b"=" in root_header:
            continue
        missing.append(b" " + attribute + b"=" + quoteattr(uri).encode("utf-8"))
    if not missing:
        return data
    return data[:root_end] + b"".join(missing) + data[root_end:]


def _sheet_info(archive: zipfile.ZipFile, sheet_name: str) -> tuple[str, int]:
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
            path = target.lstrip("/")
        else:
            path = f"xl/{target}"
        return path, int(sheet.attrib["sheetId"])
    raise KeyError(sheet_name)


def _sheet_path(archive: zipfile.ZipFile, sheet_name: str) -> str:
    return _sheet_info(archive, sheet_name)[0]


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


def _write_formula(
    cell: ET.Element,
    formula: str,
    cached_value: str | None = None,
) -> None:
    for child in list(cell):
        cell.remove(child)
    cell.attrib.pop("t", None)
    ET.SubElement(cell, f"{{{MAIN}}}f").text = formula.removeprefix("=")
    if cached_value is not None:
        cell.attrib["t"] = "str"
        ET.SubElement(cell, f"{{{MAIN}}}v").text = cached_value


def _patch_sheet(
    raw: bytes,
    updates: dict[tuple[int, int], tuple[str, Any]],
    replaced_formulas: set[str] | None = None,
    allow_consecutive_duplicates_column: int | None = None,
) -> bytes:
    namespaces = _namespaces(raw)
    root = _xml(raw, namespaces)
    sheet_data = root.find("m:sheetData", NS)
    if sheet_data is None:
        raise ValueError("worksheet sheetData bulunamadi")
    for (row_number, column_number), (kind, value) in sorted(updates.items()):
        row = _row(sheet_data, row_number)
        cell = _cell(sheet_data, row, row_number, column_number)
        if (
            replaced_formulas is not None
            and kind != "formula"
            and cell.find(f"{{{MAIN}}}f") is not None
        ):
            replaced_formulas.add(f"{_column_name(column_number)}{row_number}")
        if kind == "formula":
            if isinstance(value, tuple):
                formula, cached_value = value
                _write_formula(cell, str(formula), str(cached_value))
            else:
                _write_formula(cell, str(value))
        elif kind == "value":
            _write_value(cell, value)
        else:
            raise ValueError(f"bilinmeyen hucre yazma turu: {kind}")
    if allow_consecutive_duplicates_column is not None:
        _patch_consecutive_duplicate_validation(
            root,
            allow_consecutive_duplicates_column,
        )
    return _serialize(root, original_namespaces=namespaces)


def _patch_consecutive_duplicate_validation(
    root: ET.Element,
    column_number: int,
) -> None:
    """Allow a repeated identifier only when it continues the preceding row.

    The RFI workbook's original custom validation rejects every occurrence of a
    duplicated RFI number. CIV/MEC legitimately use the same RFI number on
    adjacent detail rows, so the first occurrence remains unique and following
    adjacent rows are accepted. A later, non-adjacent reuse is still rejected.
    """

    column = _column_name(column_number)
    validations = root.find("m:dataValidations", NS)
    if validations is None:
        return
    for validation in validations.findall("m:dataValidation", NS):
        if validation.attrib.get("type") != "custom":
            continue
        formula = validation.find("m:formula1", NS)
        if formula is None or "COUNTIF" not in (formula.text or "").upper():
            continue
        for reference in validation.attrib.get("sqref", "").split():
            match = re.fullmatch(
                rf"{column}(\d+):{column}(\d+)",
                reference,
                flags=re.IGNORECASE,
            )
            if not match:
                continue
            start_row = int(match.group(1))
            previous_row = max(1, start_row - 1)
            formula.text = (
                f'OR({column}{start_row}="",'
                f"COUNTIF(${column}${start_row}:{column}{start_row},"
                f"{column}{start_row})=1,"
                f"{column}{start_row}={column}{previous_row})"
            )
            break


def _patch_calc_chain(raw: bytes, sheet_id: int, removed_refs: set[str]) -> bytes:
    namespaces = _namespaces(raw)
    root = _xml(raw, namespaces)
    current_sheet: int | None = None
    last_kept_sheet: int | None = None
    for cell in list(root):
        if "i" in cell.attrib:
            current_sheet = int(cell.attrib["i"])
        if current_sheet == sheet_id and cell.attrib.get("r") in removed_refs:
            root.remove(cell)
            continue
        if (
            current_sheet is not None
            and current_sheet != last_kept_sheet
            and "i" not in cell.attrib
        ):
            cell.attrib["i"] = str(current_sheet)
        last_kept_sheet = current_sheet
    return _serialize(root, original_namespaces=namespaces)


def _patch_calculation(raw: bytes) -> bytes:
    namespaces = _namespaces(raw)
    root = _xml(raw, namespaces)
    calculation = root.find("m:calcPr", NS)
    if calculation is None:
        calculation = ET.SubElement(root, f"{{{MAIN}}}calcPr")
    calculation.attrib["calcMode"] = "auto"
    calculation.attrib["fullCalcOnLoad"] = "1"
    calculation.attrib["forceFullCalc"] = "1"
    return _serialize(root, original_namespaces=namespaces)


def patch_workbook(
    source: Path,
    destination: Path,
    sheet_name: str,
    updates: dict[tuple[int, int], tuple[str, Any]],
    *,
    allow_consecutive_duplicates_column: int | None = None,
) -> None:
    with zipfile.ZipFile(source, "r") as incoming:
        sheet_path, sheet_id = _sheet_info(incoming, sheet_name)
        replaced_formulas: set[str] = set()
        patched_sheet = _patch_sheet(
            incoming.read(sheet_path),
            updates,
            replaced_formulas,
            allow_consecutive_duplicates_column,
        )
        calc_chain_path = "xl/calcChain.xml"
        patched_calc_chain = None
        if replaced_formulas and calc_chain_path in incoming.namelist():
            patched_calc_chain = _patch_calc_chain(
                incoming.read(calc_chain_path),
                sheet_id,
                replaced_formulas,
            )
        with zipfile.ZipFile(destination, "w") as outgoing:
            for info in incoming.infolist():
                data = incoming.read(info.filename)
                if info.filename == sheet_path:
                    data = patched_sheet
                elif info.filename == calc_chain_path and patched_calc_chain is not None:
                    data = patched_calc_chain
                outgoing.writestr(copy.copy(info), data)

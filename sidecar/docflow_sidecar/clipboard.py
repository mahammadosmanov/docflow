"""Onaylanmis plan tablosunu Outlook uyumlu HTML + duz metin olarak kopyala."""

from __future__ import annotations

import base64
import ctypes
import html
import os
import pathlib
import re
import subprocess
import tempfile
import time
import zipfile
from collections import Counter
from ctypes import wintypes
from datetime import date, datetime, time as date_time
from xml.etree import ElementTree

from openpyxl import load_workbook

from .contracts import Plan


EXCEL_CLIPBOARD_SCRIPT = r"""
$ErrorActionPreference = 'Stop'
$excel = New-Object -ComObject Excel.Application
$excel.Visible = $true
$excel.DisplayAlerts = $false
$excel.AskToUpdateLinks = $false
$sourceBook = $null
$sourceSheet = $null
$rfiRange = $null
$filterRange = $null
$copyRange = $null
$success = $false
function Set-DocFlowPhase([string]$phase) {
  [System.IO.File]::WriteAllText($env:DOCFLOW_COPY_TRACE, $phase)
}
try {
  Set-DocFlowPhase 'opening'
  $sourceBook = $excel.Workbooks.Open($env:DOCFLOW_COPY_WORKBOOK, 0, $true)
  $sourceSheet = $sourceBook.Worksheets.Item($env:DOCFLOW_COPY_SHEET)
  Set-DocFlowPhase 'opened'
  $headerRow = [int]$env:DOCFLOW_COPY_HEADER_ROW
  $rowCount = [int]$env:DOCFLOW_COPY_ROW_COUNT
  $columns = @($env:DOCFLOW_COPY_COLUMNS.Split(',') | ForEach-Object { [int]$_ })
  $expectedRfis = @($env:DOCFLOW_COPY_RFI_NUMBERS.Split('|'))
  $criteria = @($expectedRfis | Select-Object -Unique)

  if ($columns.Count -eq 0 -or $criteria.Count -eq 0) {
    throw 'No Excel columns or RFI filter values were supplied'
  }
  $sourceSheet.Calculate()
  Set-DocFlowPhase 'calculated'
  $firstColumn = ($columns | Measure-Object -Minimum).Minimum
  $lastColumn = ($columns | Measure-Object -Maximum).Maximum
  $lastDataRow = $sourceSheet.Cells.Item(
    $sourceSheet.Rows.Count,
    $firstColumn
  ).End(-4162).Row
  if ($lastDataRow -lt $headerRow) {
    throw 'RFI Log contains no data rows'
  }
  $matchedRows = New-Object System.Collections.Generic.List[int]
  $rfiRange = $sourceSheet.Range(
    $sourceSheet.Cells.Item($headerRow + 1, $firstColumn),
    $sourceSheet.Cells.Item($lastDataRow, $firstColumn)
  )
  $rfiValues = $rfiRange.Value2
  for ($offset = 1; $offset -le $lastDataRow - $headerRow; $offset++) {
    $sourceRow = $headerRow + $offset
    if ($lastDataRow -eq $headerRow + 1) {
      $rfi = [string]$rfiValues
    } else {
      $rfi = [string]$rfiValues[$offset, 1]
    }
    if ($criteria -contains $rfi) {
      $matchedRows.Add($sourceRow)
    }
  }
  if ($matchedRows.Count -ne $rowCount) {
    throw "Filtered RFI row count mismatch: expected $rowCount, found $($matchedRows.Count)"
  }
  Set-DocFlowPhase 'matched'

  $filterRange = $sourceSheet.Range(
    $sourceSheet.Cells.Item($headerRow, $firstColumn),
    $sourceSheet.Cells.Item($lastDataRow, $lastColumn)
  )
  if ($sourceSheet.FilterMode) {
    $sourceSheet.ShowAllData()
  }
  if ($criteria.Count -eq 1) {
    $filterRange.AutoFilter(1, [string]$criteria[0]) | Out-Null
  } else {
    $filterRange.AutoFilter(1, [object[]]$criteria, 7) | Out-Null
  }
  Set-DocFlowPhase 'filtered'
  for ($sourceColumn = $firstColumn; $sourceColumn -le $lastColumn; $sourceColumn++) {
    $sourceSheet.Columns.Item($sourceColumn).Hidden =
      ($columns -notcontains $sourceColumn)
  }

  $sourceBook.Windows.Item(1).Activate() | Out-Null
  $sourceSheet.Activate() | Out-Null
  $copyRange = $filterRange.SpecialCells(12)
  Set-DocFlowPhase 'visible-cells'
  $copyRange.Select() | Out-Null
  Set-DocFlowPhase 'selected'
  $null = $copyRange.Copy()
  Set-DocFlowPhase 'copied'
  $excel.DisplayAlerts = $true
  Set-DocFlowPhase 'alerts-restored'
  Start-Sleep -Milliseconds 400
  $success = $true
  [System.IO.File]::WriteAllText(
    $env:DOCFLOW_COPY_RESULT,
    "DOCFLOW_EXCEL_COPY_OK:$($matchedRows.Count)"
  )
} catch {
  [System.IO.File]::WriteAllText(
    $env:DOCFLOW_COPY_ERROR,
    [string]$_.Exception.Message
  )
  throw
} finally {
  if (-not $success) {
    if ($sourceBook -ne $null) { $sourceBook.Close($false) }
    $excel.Quit()
  }
  foreach ($comObject in @(
    $copyRange,
    $filterRange,
    $rfiRange,
    $sourceSheet,
    $sourceBook,
    $excel
  )) {
    if ($comObject -ne $null) {
      [System.Runtime.InteropServices.Marshal]::FinalReleaseComObject(
        $comObject
      ) | Out-Null
    }
  }
  [GC]::Collect()
  [GC]::WaitForPendingFinalizers()
}
if ($success) {
  # A visible Excel instance intentionally owns the native clipboard. Force
  # only the short-lived automation host to exit; Excel remains open.
  [Environment]::Exit(0)
}
"""


def _cell_text(value: object) -> str:
    if value is None:
        return ""
    if isinstance(value, datetime):
        return value.strftime("%d-%b-%y")
    if isinstance(value, date):
        return value.strftime("%d-%b-%y")
    if isinstance(value, date_time):
        return value.strftime("%H:%M")
    return str(value)


def _copy_data(
    plan: Plan,
) -> tuple[list[str], list[str], list[dict[str, str]], bool]:
    editor = plan.meta.get("editor") or {}
    fields = list(editor.get("copyFields") or [])
    headers = [str(value) for value in (editor.get("copyHeaders") or [])]
    planned_rows = list(editor.get("rows") or [])
    source = editor.get("copySource")
    if not fields or len(fields) != len(headers):
        raise ValueError("mail tablosu kolonlari gecersiz")
    if not isinstance(source, dict):
        return fields, headers, planned_rows, False

    columns = [int(value) for value in (source.get("columns") or [])]
    if len(columns) != len(fields):
        return fields, headers, planned_rows, False
    path = pathlib.Path(str(source.get("workbook") or ""))
    if not path.is_file():
        return fields, headers, planned_rows, False

    workbook = load_workbook(
        path,
        read_only=True,
        data_only=True,
        keep_links=True,
    )
    try:
        sheet_name = str(source.get("sheet") or "")
        if sheet_name not in workbook.sheetnames:
            return fields, headers, planned_rows, False
        sheet = workbook[sheet_name]
        header_row = int(source.get("headerRow") or 9)
        start_row = int(source.get("startRow") or 0)
        expected_rfis = [
            str(value) for value in (source.get("rfiNumbers") or [])
        ]
        if start_row < 1 or not expected_rfis:
            return fields, headers, planned_rows, False

        workbook_headers = [
            _cell_text(sheet.cell(header_row, column).value) for column in columns
        ]
        if all(workbook_headers):
            headers = workbook_headers

        remaining = Counter(expected_rfis)
        planned_by_rfi: dict[str, list[dict[str, str]]] = {}
        for planned in planned_rows:
            planned_by_rfi.setdefault(str(planned.get("rfiNo") or ""), []).append(planned)

        # read_only worksheets are forward-only streams. Calling
        # ``sheet.cell(row, column)`` repeatedly for increasing rows makes
        # openpyxl restart the XML reader for every access; on the production
        # RFI Log (18k+ rows) that turns this small lookup into an apparent
        # hang. Read the relevant column span once and pick values by offset.
        first_column = min(columns)
        last_column = max(columns)
        offsets = [column - first_column for column in columns]
        rows: list[dict[str, str]] = []
        source_rows = sheet.iter_rows(
            min_row=header_row + 1,
            max_row=sheet.max_row,
            min_col=first_column,
            max_col=last_column,
            values_only=True,
        )
        for values in source_rows:
            rfi = _cell_text(values[offsets[0]])
            if remaining[rfi] < 1:
                continue
            planned_candidates = planned_by_rfi.get(rfi) or []
            planned = planned_candidates.pop(0) if planned_candidates else {}
            actual = {
                field: _cell_text(values[offset])
                for field, offset in zip(fields, offsets, strict=True)
            }
            # Yeni formula hucrelerinin cached sonucu Excel acilmadan once bazi
            # okuyucularda bos olabilir. Bu durumda yalniz o hucre icin onayli
            # plan degerini kullan; diger tum veri fiziksel workbook'tan gelir.
            for field in fields:
                if not actual[field] and planned.get(field):
                    actual[field] = str(planned[field])
            rows.append(actual)
            remaining[rfi] -= 1
        if any(remaining.values()) or len(rows) != len(expected_rfis):
            return fields, headers, planned_rows, False
        return fields, headers, rows, True
    finally:
        workbook.close()


def _payload(plan: Plan) -> tuple[str, str, int]:
    editor = plan.meta.get("editor") or {}
    if editor.get("kind") != "recordTable" or not editor.get("copyEnabled"):
        raise ValueError("bu planda kopyalanabilir tablo yok")
    fields, headers, rows, _from_workbook = _copy_data(plan)

    plain_rows = ["\t".join(str(row.get(field) or "") for field in fields) for row in rows]
    plain = "\r\n".join(["\t".join(headers), *plain_rows])

    th = "".join(
        "<th style=\"border:1px solid #8b8b8b;padding:6px 8px;"
        "font-weight:700;text-align:center;vertical-align:middle;\">"
        f"{html.escape(str(label))}</th>"
        for label in headers
    )
    body_rows = []
    for row in rows:
        cells = []
        for field in fields:
            value = html.escape(str(row.get(field) or "")).replace("\n", "<br>")
            align = "center" if field not in ("description", "forms", "location") else "left"
            cells.append(
                "<td style=\"border:1px solid #9a9a9a;padding:6px 8px;"
                f"text-align:{align};vertical-align:middle;white-space:normal;\">"
                f"{value}</td>"
            )
        body_rows.append("<tr>" + "".join(cells) + "</tr>")
    table = (
        "<table cellpadding=\"0\" cellspacing=\"0\" "
        "style=\"border-collapse:collapse;font-family:Arial,sans-serif;"
        "font-size:10pt;color:#202020;\">"
        f"<thead><tr style=\"background:#dedede;\">{th}</tr></thead>"
        f"<tbody>{''.join(body_rows)}</tbody></table>"
    )
    return table, plain, len(rows)


def preview_plan_table(plan: Plan) -> dict[str, object]:
    """Return the exact workbook-backed values used by the mail clipboard."""
    editor = plan.meta.get("editor") or {}
    if editor.get("kind") != "recordTable" or not editor.get("copyEnabled"):
        raise ValueError("bu planda onizlenebilir mail tablosu yok")
    fields, headers, rows, from_workbook = _copy_data(plan)
    widths = {
        "rfiNo": 22.0,
        "itpNo": 24.0,
        "itemNo": 10.0,
        "description": 28.0,
        "forms": 20.0,
        "tagNo": 15.0,
        "area": 12.0,
        "issueDate": 12.0,
        "inspectionDate": 12.0,
        "startTime": 12.0,
        "azfenType": 10.0,
        "intecType": 10.0,
        "location": 18.0,
        "subcontractor": 14.0,
    }
    column_widths = [widths.get(field, 14.0) for field in fields]
    source = editor.get("copySource")
    if from_workbook and isinstance(source, dict):
        path = pathlib.Path(str(source.get("workbook") or ""))
        columns = [int(value) for value in (source.get("columns") or [])]
        sheet_name = str(source.get("sheet") or "")
        if path.is_file() and len(columns) == len(fields):
            column_widths = _workbook_column_widths(
                path,
                sheet_name,
                columns,
                column_widths,
            )
    return {
        "headers": headers,
        "fields": fields,
        "rows": [
            [str(row.get(field) or "") for field in fields]
            for row in rows
        ],
        "columnWidths": [
            max(7.0, min(width, 42.0)) for width in column_widths
        ],
        "fromWorkbook": from_workbook,
        "count": len(rows),
    }


def _workbook_column_widths(
    path: pathlib.Path,
    sheet_name: str,
    columns: list[int],
    fallback: list[float],
) -> list[float]:
    """Read only the OOXML column metadata, not the workbook's 18k+ cells."""
    workbook = load_workbook(path, read_only=True, data_only=True, keep_links=True)
    try:
        if sheet_name not in workbook.sheetnames:
            return fallback
        worksheet_path = str(
            getattr(workbook[sheet_name], "_worksheet_path", "")
        ).lstrip("/")
    finally:
        workbook.close()
    if not worksheet_path:
        return fallback

    try:
        with zipfile.ZipFile(path) as archive:
            root = ElementTree.fromstring(archive.read(worksheet_path))
    except (KeyError, OSError, ValueError, zipfile.BadZipFile):
        return fallback

    explicit: dict[int, float] = {}
    for element in root.iter():
        if not element.tag.endswith("}col"):
            continue
        try:
            first = int(element.attrib["min"])
            last = int(element.attrib["max"])
            width = float(element.attrib["width"])
        except (KeyError, TypeError, ValueError):
            continue
        for column in columns:
            if first <= column <= last:
                explicit[column] = width
    return [
        explicit.get(column, fallback[index])
        for index, column in enumerate(columns)
    ]


def _copy_exact_from_excel(
    plan: Plan,
    row_count: int,
) -> bool:
    editor = plan.meta.get("editor") or {}
    source = editor.get("copySource")
    if os.name != "nt" or not isinstance(source, dict) or row_count < 1:
        return False
    columns = [str(int(value)) for value in (source.get("columns") or [])]
    rfi_numbers = [
        str(value) for value in (source.get("rfiNumbers") or [])
    ]
    if not columns or len(rfi_numbers) != row_count:
        return False

    encoded = base64.b64encode(
        EXCEL_CLIPBOARD_SCRIPT.encode("utf-16-le")
    ).decode("ascii")
    timeout_seconds = max(
        5,
        int(os.environ.get("DOCFLOW_EXCEL_COPY_TIMEOUT", "90")),
    )
    with tempfile.TemporaryDirectory(prefix="docflow-excel-copy-") as temp_dir:
        result_path = pathlib.Path(temp_dir) / "result.txt"
        error_path = pathlib.Path(temp_dir) / "error.txt"
        trace_path = pathlib.Path(temp_dir) / "trace.txt"
        environment = os.environ.copy()
        environment.update(
            {
                "DOCFLOW_COPY_WORKBOOK": str(source.get("workbook") or ""),
                "DOCFLOW_COPY_SHEET": str(source.get("sheet") or ""),
                "DOCFLOW_COPY_HEADER_ROW": str(source.get("headerRow") or 9),
                "DOCFLOW_COPY_ROW_COUNT": str(row_count),
                "DOCFLOW_COPY_COLUMNS": ",".join(columns),
                "DOCFLOW_COPY_RFI_NUMBERS": "|".join(rfi_numbers),
                "DOCFLOW_COPY_RESULT": str(result_path),
                "DOCFLOW_COPY_ERROR": str(error_path),
                "DOCFLOW_COPY_TRACE": str(trace_path),
            }
        )
        process: subprocess.Popen[bytes] | None = None

        def stop_host() -> None:
            if process is None or process.poll() is not None:
                return
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=3)

        try:
            process = subprocess.Popen(
                [
                    "powershell.exe",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Sta",
                    "-EncodedCommand",
                    encoded,
                ],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=environment,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            deadline = time.monotonic() + timeout_seconds
            while time.monotonic() < deadline:
                if result_path.is_file():
                    result = result_path.read_text(
                        encoding="utf-8",
                        errors="replace",
                    )
                    if "DOCFLOW_EXCEL_COPY_OK:" in result:
                        stop_host()
                        return True
                if error_path.is_file():
                    detail = error_path.read_text(
                        encoding="utf-8",
                        errors="replace",
                    ).strip()[-800:]
                    stop_host()
                    raise RuntimeError(
                        "Excel filtreleme/kopyalama hatasi"
                        + (f": {detail}" if detail else "")
                    )
                if process.poll() is not None:
                    raise RuntimeError(
                        "Excel kopyalama islemi tamamlanmadan kapandi"
                    )
                time.sleep(0.1)

            phase = (
                trace_path.read_text(encoding="utf-8", errors="replace")
                if trace_path.is_file()
                else "unknown"
            ).strip()
            stop_host()
            raise RuntimeError(
                "Excel filtreleme ve kopyalama "
                f"{timeout_seconds} saniyede tamamlanamadi. "
                f"Son asama: {phase}. "
                "Acik Excel diyaloglarini kapatip yeniden deneyin."
            )
        except OSError as exc:
            stop_host()
            raise RuntimeError(f"Excel baslatilamadi: {exc}") from exc


def _cf_html_document(document: str) -> bytes:
    start_marker = "<!--StartFragment-->"
    end_marker = "<!--EndFragment-->"
    if start_marker not in document or end_marker not in document:
        raise ValueError("HTML fragment markers are missing")
    header_template = (
        "Version:0.9\r\n"
        "StartHTML:{start_html:010d}\r\n"
        "EndHTML:{end_html:010d}\r\n"
        "StartFragment:{start_fragment:010d}\r\n"
        "EndFragment:{end_fragment:010d}\r\n"
    )
    placeholder = header_template.format(
        start_html=0,
        end_html=0,
        start_fragment=0,
        end_fragment=0,
    )
    start_html = len(placeholder.encode("utf-8"))
    document_bytes = document.encode("utf-8")
    start_marker_bytes = start_marker.encode("utf-8")
    end_marker_bytes = end_marker.encode("utf-8")
    start_fragment = (
        start_html + document_bytes.index(start_marker_bytes)
        + len(start_marker_bytes)
    )
    end_fragment = start_html + document_bytes.index(end_marker_bytes)
    end_html = start_html + len(document_bytes)
    header = header_template.format(
        start_html=start_html,
        end_html=end_html,
        start_fragment=start_fragment,
        end_fragment=end_fragment,
    ).encode("utf-8")
    return header + document_bytes + b"\0"


def _cf_html(fragment: str) -> bytes:
    document = (
        "<html><head><meta charset=\"utf-8\"></head><body>"
        f"<!--StartFragment-->{fragment}<!--EndFragment-->"
        "</body></html>"
    )
    return _cf_html_document(document)


def _normalize_excel_cf_html(payload: bytes) -> bytes:
    """Preserve Excel's complete HTML/CSS while recalculating UTF-8 offsets."""
    text = payload.rstrip(b"\0\x01").decode("utf-8-sig", "replace")
    html_match = re.search(r"(?i)<html(?:\s|>)", text)
    if html_match is None:
        raise ValueError("Excel panosu HTML belgesi dondurmedi")
    document = text[html_match.start():].rstrip("\0\x01")
    start_match = re.search(r"(?i)<!--\s*StartFragment\s*-->", document)
    end_match = re.search(r"(?i)<!--\s*EndFragment\s*-->", document)
    if start_match is None or end_match is None or end_match.start() < start_match.end():
        body_match = re.search(r"(?is)<body[^>]*>(.*)</body>", document)
        fragment = body_match.group(1) if body_match else document
        return _cf_html(fragment)
    if start_match.group(0) != "<!--StartFragment-->":
        document = (
            document[:start_match.start()]
            + "<!--StartFragment-->"
            + document[start_match.end():]
        )
    end_match = re.search(r"(?i)<!--\s*EndFragment\s*-->", document)
    assert end_match is not None
    if end_match.group(0) != "<!--EndFragment-->":
        document = (
            document[:end_match.start()]
            + "<!--EndFragment-->"
            + document[end_match.end():]
        )
    return _cf_html_document(document)


def _set_windows_clipboard(html_bytes: bytes, plain: str) -> None:
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    user32.OpenClipboard.argtypes = [wintypes.HWND]
    user32.OpenClipboard.restype = wintypes.BOOL
    user32.EmptyClipboard.restype = wintypes.BOOL
    user32.RegisterClipboardFormatW.argtypes = [wintypes.LPCWSTR]
    user32.RegisterClipboardFormatW.restype = wintypes.UINT
    user32.SetClipboardData.argtypes = [wintypes.UINT, wintypes.HANDLE]
    user32.SetClipboardData.restype = wintypes.HANDLE
    kernel32.GlobalAlloc.argtypes = [wintypes.UINT, ctypes.c_size_t]
    kernel32.GlobalAlloc.restype = wintypes.HGLOBAL
    kernel32.GlobalLock.argtypes = [wintypes.HGLOBAL]
    kernel32.GlobalLock.restype = wintypes.LPVOID
    kernel32.GlobalUnlock.argtypes = [wintypes.HGLOBAL]

    for _ in range(10):
        if user32.OpenClipboard(None):
            break
        time.sleep(0.04)
    else:
        raise OSError("Windows panosu baska bir uygulama tarafindan kullaniliyor")

    handles: list[int] = []
    try:
        if not user32.EmptyClipboard():
            raise ctypes.WinError(ctypes.get_last_error())

        def put(fmt: int, data: bytes) -> None:
            handle = kernel32.GlobalAlloc(0x0002, len(data))  # GMEM_MOVEABLE
            if not handle:
                raise ctypes.WinError(ctypes.get_last_error())
            pointer = kernel32.GlobalLock(handle)
            if not pointer:
                raise ctypes.WinError(ctypes.get_last_error())
            ctypes.memmove(pointer, data, len(data))
            kernel32.GlobalUnlock(handle)
            if not user32.SetClipboardData(fmt, handle):
                raise ctypes.WinError(ctypes.get_last_error())
            handles.append(handle)  # basarili SetClipboardData sonrasi sahip Windows

        html_format = user32.RegisterClipboardFormatW("HTML Format")
        put(html_format, html_bytes)
        put(13, plain.encode("utf-16-le") + b"\0\0")  # CF_UNICODETEXT
    finally:
        user32.CloseClipboard()


def copy_plan_table(plan: Plan) -> int:
    _fields, _headers, rows, from_workbook = _copy_data(plan)
    count = len(rows)
    if os.name == "nt":
        if from_workbook:
            if _copy_exact_from_excel(plan, count):
                return count
            raise RuntimeError("Excel mail tablosu kaynaktan birebir kopyalanamadi")
        fragment, plain, count = _payload(plan)
        _set_windows_clipboard(_cf_html(fragment), plain)
    else:
        _fragment, plain, count = _payload(plan)
        import tkinter

        root = tkinter.Tk()
        root.withdraw()
        root.clipboard_clear()
        root.clipboard_append(plain)
        root.update()
        root.destroy()
    return count

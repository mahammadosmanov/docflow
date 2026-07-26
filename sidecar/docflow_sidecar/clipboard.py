"""Onaylanmis plan tablosunu Outlook uyumlu HTML + duz metin olarak kopyala."""

from __future__ import annotations

import base64
import ctypes
import html
import os
import pathlib
import subprocess
import tempfile
import time
from ctypes import wintypes
from datetime import date, datetime, time as date_time

from openpyxl import load_workbook

from .contracts import Plan


EXCEL_CLIPBOARD_SCRIPT = r"""
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$excel = New-Object -ComObject Excel.Application
$excel.Visible = $false
$excel.DisplayAlerts = $false
$excel.AskToUpdateLinks = $false
$sourceBook = $null
$tempBook = $null
try {
  $sourceBook = $excel.Workbooks.Open($env:DOCFLOW_COPY_WORKBOOK, 0, $true)
  $sourceSheet = $sourceBook.Worksheets.Item($env:DOCFLOW_COPY_SHEET)
  $tempBook = $excel.Workbooks.Add()
  $tempSheet = $tempBook.Worksheets.Item(1)
  $headerRow = [int]$env:DOCFLOW_COPY_HEADER_ROW
  $startRow = [int]$env:DOCFLOW_COPY_START_ROW
  $rowCount = [int]$env:DOCFLOW_COPY_ROW_COUNT
  $endRow = $startRow + $rowCount - 1
  $columns = @($env:DOCFLOW_COPY_COLUMNS.Split(',') | ForEach-Object { [int]$_ })

  $sourceSheet.Calculate()
  $firstColumn = ($columns | Measure-Object -Minimum).Minimum
  $lastColumn = ($columns | Measure-Object -Maximum).Maximum
  $headerRange = $sourceSheet.Range(
    $sourceSheet.Cells.Item($headerRow, $firstColumn),
    $sourceSheet.Cells.Item($headerRow, $lastColumn)
  )
  $null = $headerRange.Copy($tempSheet.Cells.Item(1, 1))
  $dataRange = $sourceSheet.Range(
    $sourceSheet.Cells.Item($startRow, $firstColumn),
    $sourceSheet.Cells.Item($endRow, $lastColumn)
  )
  $null = $dataRange.Copy($tempSheet.Cells.Item(2, 1))

  for ($sourceColumn = $lastColumn; $sourceColumn -ge $firstColumn; $sourceColumn--) {
    if ($columns -notcontains $sourceColumn) {
      $targetColumn = $sourceColumn - $firstColumn + 1
      $null = $tempSheet.Columns.Item($targetColumn).Delete()
    }
  }
  for ($index = 0; $index -lt $columns.Count; $index++) {
    $tempSheet.Columns.Item($index + 1).ColumnWidth =
      $sourceSheet.Columns.Item($columns[$index]).ColumnWidth
  }

  $copyRange = $tempSheet.Range(
    $tempSheet.Cells.Item(1, 1),
    $tempSheet.Cells.Item($rowCount + 1, $columns.Count)
  )
  $null = $copyRange.Copy()
  Start-Sleep -Milliseconds 250
  $data = [System.Windows.Forms.Clipboard]::GetDataObject()
  $html = $data.GetData('HTML Format')
  $plain = $data.GetData([System.Windows.Forms.DataFormats]::UnicodeText)
  $utf8 = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText(
    $env:DOCFLOW_COPY_HTML,
    [string]$html,
    $utf8
  )
  [System.IO.File]::WriteAllText(
    $env:DOCFLOW_COPY_TEXT,
    [string]$plain,
    $utf8
  )
} finally {
  if ($tempBook -ne $null) { $tempBook.Close($false) }
  if ($sourceBook -ne $null) { $sourceBook.Close($false) }
  $excel.Quit()
  [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
  [GC]::Collect()
  [GC]::WaitForPendingFinalizers()
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
        if start_row < 1:
            return fields, headers, planned_rows, False

        workbook_headers = [
            _cell_text(sheet.cell(header_row, column).value) for column in columns
        ]
        if all(workbook_headers):
            headers = workbook_headers

        rows: list[dict[str, str]] = []
        for offset, planned in enumerate(planned_rows):
            physical_row = start_row + offset
            actual = {
                field: _cell_text(sheet.cell(physical_row, column).value)
                for field, column in zip(fields, columns, strict=True)
            }
            if actual.get("rfiNo") != str(planned.get("rfiNo") or ""):
                return fields, headers, planned_rows, False
            # Yeni formula hucrelerinin cached sonucu Excel acilmadan once bazi
            # okuyucularda bos olabilir. Bu durumda yalniz o hucre icin onayli
            # plan degerini kullan; diger tum veri fiziksel workbook'tan gelir.
            for field in fields:
                if not actual[field] and planned.get(field):
                    actual[field] = str(planned[field])
            rows.append(actual)
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


def _excel_clipboard_payload(
    plan: Plan,
    row_count: int,
) -> tuple[bytes, str] | None:
    editor = plan.meta.get("editor") or {}
    source = editor.get("copySource")
    if os.name != "nt" or not isinstance(source, dict) or row_count < 1:
        return None
    columns = [str(int(value)) for value in (source.get("columns") or [])]
    if not columns:
        return None

    encoded = base64.b64encode(
        EXCEL_CLIPBOARD_SCRIPT.encode("utf-16-le")
    ).decode("ascii")
    with tempfile.TemporaryDirectory(prefix="docflow-mail-") as temp_dir:
        html_path = pathlib.Path(temp_dir) / "excel-clipboard.html"
        text_path = pathlib.Path(temp_dir) / "excel-clipboard.txt"
        environment = os.environ.copy()
        environment.update(
            {
                "DOCFLOW_COPY_WORKBOOK": str(source.get("workbook") or ""),
                "DOCFLOW_COPY_SHEET": str(source.get("sheet") or ""),
                "DOCFLOW_COPY_HEADER_ROW": str(source.get("headerRow") or 9),
                "DOCFLOW_COPY_START_ROW": str(source.get("startRow") or 0),
                "DOCFLOW_COPY_ROW_COUNT": str(row_count),
                "DOCFLOW_COPY_COLUMNS": ",".join(columns),
                "DOCFLOW_COPY_HTML": str(html_path),
                "DOCFLOW_COPY_TEXT": str(text_path),
            }
        )
        try:
            subprocess.run(
                [
                    "powershell.exe",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Sta",
                    "-EncodedCommand",
                    encoded,
                ],
                check=True,
                capture_output=True,
                timeout=90,
                env=environment,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            html_bytes = html_path.read_bytes().rstrip(b"\0") + b"\0"
            plain = text_path.read_text(encoding="utf-8")
            if not html_bytes.startswith(b"Version:") or not plain:
                return None
            return html_bytes, plain
        except (OSError, subprocess.SubprocessError):
            return None


def _cf_html(fragment: str) -> bytes:
    start_marker = "<!--StartFragment-->"
    end_marker = "<!--EndFragment-->"
    document = f"<html><body>{start_marker}{fragment}{end_marker}</body></html>"
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
    start_fragment = start_html + len(
        document.split(start_marker, 1)[0].encode("utf-8")
    ) + len(start_marker.encode("utf-8"))
    end_fragment = start_html + len(
        document.split(end_marker, 1)[0].encode("utf-8")
    )
    end_html = start_html + len(document_bytes)
    header = header_template.format(
        start_html=start_html,
        end_html=end_html,
        start_fragment=start_fragment,
        end_fragment=end_fragment,
    ).encode("utf-8")
    return header + document_bytes + b"\0"


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
            excel_payload = _excel_clipboard_payload(plan, count)
            if excel_payload is not None:
                html_bytes, plain = excel_payload
                _set_windows_clipboard(html_bytes, plain)
                return count
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

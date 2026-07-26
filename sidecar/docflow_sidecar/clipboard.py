"""Onaylanmis plan tablosunu Outlook uyumlu HTML + duz metin olarak kopyala."""

from __future__ import annotations

import ctypes
import html
import os
import time
from ctypes import wintypes

from .contracts import Plan


def _payload(plan: Plan) -> tuple[str, str, int]:
    editor = plan.meta.get("editor") or {}
    if editor.get("kind") != "recordTable" or not editor.get("copyEnabled"):
        raise ValueError("bu planda kopyalanabilir tablo yok")
    fields = list(editor.get("copyFields") or [])
    headers = list(editor.get("copyHeaders") or [])
    rows = list(editor.get("rows") or [])
    if not fields or len(fields) != len(headers):
        raise ValueError("mail tablosu kolonlari gecersiz")

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
    fragment, plain, count = _payload(plan)
    if os.name == "nt":
        _set_windows_clipboard(_cf_html(fragment), plain)
    else:
        import tkinter

        root = tkinter.Tk()
        root.withdraw()
        root.clipboard_clear()
        root.clipboard_append(plain)
        root.update()
        root.destroy()
    return count

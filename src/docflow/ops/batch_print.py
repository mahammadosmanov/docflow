"""Windows mixed-file batch printing with an explicit preview queue."""

from __future__ import annotations

import ctypes
import copy
import json
import os
import pathlib
import threading
import time
import uuid
from ctypes import wintypes
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

from ..contracts import (
    BoolParam,
    ChoiceParam,
    FilesParam,
    IntParam,
    Operation,
    ParamSchema,
    Plan,
    Result,
    Step,
    Warning,
)
from ..registry import register
from ..progress import cancellation_requested
from ..rpc import notify

MAX_FILES = 250
MAX_COPIES = 99
PRINTER_ENUM_LOCAL = 0x00000002
PRINTER_ENUM_CONNECTIONS = 0x00000004
ASSOCSTR_COMMAND = 1
LPBYTE = ctypes.POINTER(wintypes.BYTE)
PRINTER_CHANGE_ADD_JOB = 0x00000100
PRINTER_CHANGE_SET_JOB = 0x00000200
PRINTER_CHANGE_DELETE_JOB = 0x00000400
PRINTER_CHANGE_WRITE_JOB = 0x00000800
PRINTER_CHANGE_JOB = 0x0000FF00
JOB_STATUS_DELETED = 0x00000100
JOB_STATUS_COMPLETE = 0x00001000
JOB_STATUS_PRINTED = 0x00000080
JOB_STATUS_ERROR = 0x00000002
JOB_STATUS_OFFLINE = 0x00000020
JOB_STATUS_PAPEROUT = 0x00000040
JOB_STATUS_BLOCKED_DEVQ = 0x00000200
JOB_STATUS_USER_INTERVENTION = 0x00000400
JOB_FAILURE_STATUS = (
    JOB_STATUS_ERROR
    | JOB_STATUS_OFFLINE
    | JOB_STATUS_PAPEROUT
    | JOB_STATUS_BLOCKED_DEVQ
    | JOB_STATUS_USER_INTERVENTION
)
JOB_SUCCESS_STATUS = JOB_STATUS_COMPLETE | JOB_STATUS_PRINTED
JOB_CONTROL_CANCEL = 3
WAIT_OBJECT_0 = 0
WAIT_TIMEOUT = 258
INFINITE_JOB_COUNT = 0xFFFFFFFF
SPOOL_POLL_SECONDS = 0.2
SPOOL_ACK_TIMEOUT_SECONDS = 30 * 60
SPOOL_FINISH_TIMEOUT_SECONDS = 4 * 60 * 60
DEFAULT_TIMEOUT_MINUTES = 30
MAX_TIMEOUT_MINUTES = 240
MAX_RETRIES = 3
CONTROLLED_FORMATS = {".pdf", ".png", ".jpg", ".jpeg", ".bmp", ".tif", ".tiff"}
PAPER_CHOICES = ("auto", "A4", "A3", "A5", "Letter", "Legal")
ORIENTATION_CHOICES = ("auto", "portrait", "landscape")
SCALING_CHOICES = ("fit", "actual", "shrink")
COLOR_CHOICES = ("printer", "color", "grayscale")
DUPLEX_CHOICES = ("printer", "simplex", "long", "short")
ERROR_ACTION_CHOICES = ("pause", "skip")


class PrintCancelled(RuntimeError):
    def __init__(self, message: str, *, uncertain: bool = False) -> None:
        super().__init__(message)
        self.uncertain = uncertain


class PrinterStateError(RuntimeError):
    pass


class PrintStateUncertain(RuntimeError):
    """Windows may have accepted or partially printed a job."""


class _PrinterInfo4(ctypes.Structure):
    _fields_ = [
        ("pPrinterName", wintypes.LPWSTR),
        ("pServerName", wintypes.LPWSTR),
        ("Attributes", wintypes.DWORD),
    ]


class _SystemTime(ctypes.Structure):
    _fields_ = [
        ("wYear", wintypes.WORD),
        ("wMonth", wintypes.WORD),
        ("wDayOfWeek", wintypes.WORD),
        ("wDay", wintypes.WORD),
        ("wHour", wintypes.WORD),
        ("wMinute", wintypes.WORD),
        ("wSecond", wintypes.WORD),
        ("wMilliseconds", wintypes.WORD),
    ]


class _JobInfo1(ctypes.Structure):
    _fields_ = [
        ("JobId", wintypes.DWORD),
        ("pPrinterName", wintypes.LPWSTR),
        ("pMachineName", wintypes.LPWSTR),
        ("pUserName", wintypes.LPWSTR),
        ("pDocument", wintypes.LPWSTR),
        ("pDatatype", wintypes.LPWSTR),
        ("pStatus", wintypes.LPWSTR),
        ("Status", wintypes.DWORD),
        ("Priority", wintypes.DWORD),
        ("Position", wintypes.DWORD),
        ("TotalPages", wintypes.DWORD),
        ("PagesPrinted", wintypes.DWORD),
        ("Submitted", _SystemTime),
    ]


@dataclass(frozen=True)
class _SpoolJob:
    job_id: int
    document: str
    status: int


def _job_matches_document(job: _SpoolJob, requested: str) -> bool:
    queued = job.document.casefold()
    path = pathlib.Path(requested)
    name = path.name.casefold()
    stem = path.stem.casefold()
    return bool(queued and (name in queued or (stem and stem in queued)))


class _PrintQueueMonitor:
    """Wait until one shell print request has fully cleared the spooler."""

    def __init__(self, printer: str) -> None:
        if os.name != "nt":
            raise OSError("Windows printing is required")

        win_dll = getattr(ctypes, "WinDLL")
        self._winspool = win_dll("winspool.drv", use_last_error=True)
        self._kernel32 = win_dll("kernel32.dll", use_last_error=True)
        self._configure_functions()
        self._printer = wintypes.HANDLE()
        if not self._winspool.OpenPrinterW(
            printer,
            ctypes.byref(self._printer),
            None,
        ):
            raise ctypes.WinError(ctypes.get_last_error())

        self._notification: int | None = None
        notification = self._winspool.FindFirstPrinterChangeNotification(
            self._printer,
            PRINTER_CHANGE_JOB,
            0,
            None,
        )
        invalid_handle = ctypes.c_void_p(-1).value
        if notification and notification != invalid_handle:
            self._notification = int(notification)
        self._baseline = set(self._jobs())

    def _configure_functions(self) -> None:
        self._winspool.OpenPrinterW.argtypes = [
            wintypes.LPWSTR,
            ctypes.POINTER(wintypes.HANDLE),
            wintypes.LPVOID,
        ]
        self._winspool.OpenPrinterW.restype = wintypes.BOOL
        self._winspool.ClosePrinter.argtypes = [wintypes.HANDLE]
        self._winspool.ClosePrinter.restype = wintypes.BOOL
        self._winspool.EnumJobsW.argtypes = [
            wintypes.HANDLE,
            wintypes.DWORD,
            wintypes.DWORD,
            wintypes.DWORD,
            LPBYTE,
            wintypes.DWORD,
            ctypes.POINTER(wintypes.DWORD),
            ctypes.POINTER(wintypes.DWORD),
        ]
        self._winspool.EnumJobsW.restype = wintypes.BOOL
        self._winspool.SetJobW.argtypes = [
            wintypes.HANDLE,
            wintypes.DWORD,
            wintypes.DWORD,
            LPBYTE,
            wintypes.DWORD,
        ]
        self._winspool.SetJobW.restype = wintypes.BOOL
        self._winspool.FindFirstPrinterChangeNotification.argtypes = [
            wintypes.HANDLE,
            wintypes.DWORD,
            wintypes.DWORD,
            wintypes.LPVOID,
        ]
        self._winspool.FindFirstPrinterChangeNotification.restype = wintypes.HANDLE
        self._winspool.FindNextPrinterChangeNotification.argtypes = [
            wintypes.HANDLE,
            ctypes.POINTER(wintypes.DWORD),
            wintypes.LPVOID,
            ctypes.POINTER(wintypes.LPVOID),
        ]
        self._winspool.FindNextPrinterChangeNotification.restype = wintypes.BOOL
        self._winspool.FindClosePrinterChangeNotification.argtypes = [
            wintypes.HANDLE,
        ]
        self._winspool.FindClosePrinterChangeNotification.restype = wintypes.BOOL
        self._kernel32.WaitForSingleObject.argtypes = [
            wintypes.HANDLE,
            wintypes.DWORD,
        ]
        self._kernel32.WaitForSingleObject.restype = wintypes.DWORD

    def _jobs(self) -> dict[int, _SpoolJob]:
        needed = wintypes.DWORD()
        returned = wintypes.DWORD()
        self._winspool.EnumJobsW(
            self._printer,
            0,
            INFINITE_JOB_COUNT,
            1,
            None,
            0,
            ctypes.byref(needed),
            ctypes.byref(returned),
        )
        if not needed.value:
            return {}

        buffer = ctypes.create_string_buffer(needed.value)
        if not self._winspool.EnumJobsW(
            self._printer,
            0,
            INFINITE_JOB_COUNT,
            1,
            ctypes.cast(buffer, LPBYTE),
            needed.value,
            ctypes.byref(needed),
            ctypes.byref(returned),
        ):
            raise ctypes.WinError(ctypes.get_last_error())

        records = ctypes.cast(
            buffer,
            ctypes.POINTER(_JobInfo1 * returned.value),
        ).contents
        return {
            int(record.JobId): _SpoolJob(
                int(record.JobId),
                record.pDocument or "",
                int(record.Status),
            )
            for record in records
        }

    def _wait_change(self, seconds: float) -> int:
        if self._notification is None:
            time.sleep(seconds)
            return 0
        wait_result = self._kernel32.WaitForSingleObject(
            self._notification,
            max(1, int(seconds * 1000)),
        )
        if wait_result == WAIT_TIMEOUT:
            return 0
        if wait_result != WAIT_OBJECT_0:
            raise ctypes.WinError(ctypes.get_last_error())
        change = wintypes.DWORD()
        if not self._winspool.FindNextPrinterChangeNotification(
            self._notification,
            ctypes.byref(change),
            None,
            None,
        ):
            raise ctypes.WinError(ctypes.get_last_error())
        return int(change.value)

    def _cancel_jobs(self, job_ids: set[int]) -> None:
        for job_id in job_ids:
            self._winspool.SetJobW(
                self._printer,
                job_id,
                0,
                None,
                JOB_CONTROL_CANCEL,
            )

    def wait_for_completion(
        self,
        document: str,
        *,
        job_id: int | None = None,
        timeout_seconds: float = SPOOL_FINISH_TIMEOUT_SECONDS,
        acknowledge_seconds: float = SPOOL_ACK_TIMEOUT_SECONDS,
    ) -> None:
        """Fail closed: no later file starts until this request is finished."""

        tracked: set[int]
        if job_id is not None:
            # StartDocW gives us the spool id. A very fast virtual printer can
            # finish and remove that id before the first poll; successful
            # StartDocW + EndDoc means that disappearance is completion.
            if job_id not in self._jobs():
                return
            tracked = {job_id}
        else:
            tracked = set()
            acknowledge_deadline = time.monotonic() + acknowledge_seconds

            while time.monotonic() < acknowledge_deadline:
                if cancellation_requested():
                    raise PrintCancelled(
                        "cancelled before Windows identified the spool job",
                        uncertain=True,
                    )
                current = self._jobs()
                added = {
                    current_job_id
                    for current_job_id, job in current.items()
                    if current_job_id not in self._baseline
                    and _job_matches_document(job, document)
                }
                if added:
                    tracked = added
                    break
                self._wait_change(SPOOL_POLL_SECONDS)

            if not tracked:
                raise PrintStateUncertain(
                    f"Windows did not accept the print job within "
                    f"{max(1, round(acknowledge_seconds / 60))} minutes: "
                    f"{document}"
                )

        finish_deadline = time.monotonic() + timeout_seconds
        while time.monotonic() < finish_deadline:
            if cancellation_requested():
                self._cancel_jobs(tracked)
                raise PrintCancelled("print job cancelled")
            current = self._jobs()
            active = [current[job_id] for job_id in tracked if job_id in current]
            if not active:
                return
            failed = [job for job in active if job.status & JOB_FAILURE_STATUS]
            if failed:
                self._cancel_jobs({job.job_id for job in failed})
                raise PrinterStateError(
                    f"printer reported status 0x{failed[0].status:08x}"
                )
            if any(job.status & JOB_STATUS_DELETED for job in active):
                raise PrinterStateError("Windows spool job was deleted")
            if all(job.status & JOB_SUCCESS_STATUS for job in active):
                return
            self._wait_change(SPOOL_POLL_SECONDS)

        self._cancel_jobs(tracked)
        raise PrintStateUncertain(
            f"Windows did not finish the print job within "
            f"{max(1, round(timeout_seconds / 60))} minutes: {document}"
        )

    def close(self) -> None:
        if self._notification is not None:
            self._winspool.FindClosePrinterChangeNotification(
                self._notification,
            )
            self._notification = None
        if self._printer:
            self._winspool.ClosePrinter(self._printer)
            self._printer = wintypes.HANDLE()

    def __enter__(self) -> _PrintQueueMonitor:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()


# The Windows shell print verb does not carry paper/orientation/scaling
# settings. PDF and image jobs therefore use a private GDI device context with
# a per-job DEVMODE; printer defaults are never changed for other applications.
DM_ORIENTATION = 0x00000001
DM_PAPERSIZE = 0x00000002
DM_COLOR = 0x00000800
DM_DUPLEX = 0x00001000
DM_COLLATE = 0x00008000
DMORIENT_PORTRAIT = 1
DMORIENT_LANDSCAPE = 2
DMCOLOR_MONOCHROME = 1
DMCOLOR_COLOR = 2
DMDUP_SIMPLEX = 1
DMDUP_VERTICAL = 2
DMDUP_HORIZONTAL = 3
DMCOLLATE_TRUE = 1
DM_OUT_BUFFER = 0x00000002
DM_IN_BUFFER = 0x00000008
PAPER_IDS = {"A4": 9, "A3": 8, "A5": 11, "Letter": 1, "Legal": 5}
HORZRES = 8
VERTRES = 10
LOGPIXELSX = 88
LOGPIXELSY = 90
HALFTONE = 4
DIB_RGB_COLORS = 0
SRCCOPY = 0x00CC0020
GDI_ERROR = 0xFFFFFFFF
BI_RGB = 0


class _PointL(ctypes.Structure):
    _fields_ = [("x", wintypes.LONG), ("y", wintypes.LONG)]


class _PrinterDevModeFields(ctypes.Structure):
    _fields_ = [
        ("dmOrientation", ctypes.c_short),
        ("dmPaperSize", ctypes.c_short),
        ("dmPaperLength", ctypes.c_short),
        ("dmPaperWidth", ctypes.c_short),
        ("dmScale", ctypes.c_short),
        ("dmCopies", ctypes.c_short),
        ("dmDefaultSource", ctypes.c_short),
        ("dmPrintQuality", ctypes.c_short),
    ]


class _DisplayDevModeFields(ctypes.Structure):
    _fields_ = [
        ("dmPosition", _PointL),
        ("dmDisplayOrientation", wintypes.DWORD),
        ("dmDisplayFixedOutput", wintypes.DWORD),
    ]


class _DevModeUnion(ctypes.Union):
    _anonymous_ = ("printer", "display")
    _fields_ = [
        ("printer", _PrinterDevModeFields),
        ("display", _DisplayDevModeFields),
    ]


class _DevModeW(ctypes.Structure):
    _anonymous_ = ("fields",)
    _fields_ = [
        ("dmDeviceName", wintypes.WCHAR * 32),
        ("dmSpecVersion", wintypes.WORD),
        ("dmDriverVersion", wintypes.WORD),
        ("dmSize", wintypes.WORD),
        ("dmDriverExtra", wintypes.WORD),
        ("dmFields", wintypes.DWORD),
        ("fields", _DevModeUnion),
        ("dmColor", ctypes.c_short),
        ("dmDuplex", ctypes.c_short),
        ("dmYResolution", ctypes.c_short),
        ("dmTTOption", ctypes.c_short),
        ("dmCollate", ctypes.c_short),
        ("dmFormName", wintypes.WCHAR * 32),
        ("dmLogPixels", wintypes.WORD),
        ("dmBitsPerPel", wintypes.DWORD),
        ("dmPelsWidth", wintypes.DWORD),
        ("dmPelsHeight", wintypes.DWORD),
        ("dmDisplayFlags", wintypes.DWORD),
        ("dmDisplayFrequency", wintypes.DWORD),
        ("dmICMMethod", wintypes.DWORD),
        ("dmICMIntent", wintypes.DWORD),
        ("dmMediaType", wintypes.DWORD),
        ("dmDitherType", wintypes.DWORD),
        ("dmReserved1", wintypes.DWORD),
        ("dmReserved2", wintypes.DWORD),
        ("dmPanningWidth", wintypes.DWORD),
        ("dmPanningHeight", wintypes.DWORD),
    ]


class _DocInfoW(ctypes.Structure):
    _fields_ = [
        ("cbSize", ctypes.c_int),
        ("lpszDocName", wintypes.LPCWSTR),
        ("lpszOutput", wintypes.LPCWSTR),
        ("lpszDatatype", wintypes.LPCWSTR),
        ("fwType", wintypes.DWORD),
    ]


class _BitmapInfoHeader(ctypes.Structure):
    _fields_ = [
        ("biSize", wintypes.DWORD),
        ("biWidth", wintypes.LONG),
        ("biHeight", wintypes.LONG),
        ("biPlanes", wintypes.WORD),
        ("biBitCount", wintypes.WORD),
        ("biCompression", wintypes.DWORD),
        ("biSizeImage", wintypes.DWORD),
        ("biXPelsPerMeter", wintypes.LONG),
        ("biYPelsPerMeter", wintypes.LONG),
        ("biClrUsed", wintypes.DWORD),
        ("biClrImportant", wintypes.DWORD),
    ]


class _RgbQuad(ctypes.Structure):
    _fields_ = [
        ("rgbBlue", wintypes.BYTE),
        ("rgbGreen", wintypes.BYTE),
        ("rgbRed", wintypes.BYTE),
        ("rgbReserved", wintypes.BYTE),
    ]


class _BitmapInfo(ctypes.Structure):
    _fields_ = [
        ("bmiHeader", _BitmapInfoHeader),
        ("bmiColors", _RgbQuad * 1),
    ]


def _configure_devmode(
    devmode: _DevModeW,
    settings: dict[str, Any],
    orientation: str,
) -> None:
    paper = str(settings.get("paper") or "A4")
    if paper in PAPER_IDS:
        devmode.dmFields |= DM_PAPERSIZE
        devmode.dmPaperSize = PAPER_IDS[paper]
    if orientation in {"portrait", "landscape"}:
        devmode.dmFields |= DM_ORIENTATION
        devmode.dmOrientation = (
            DMORIENT_LANDSCAPE if orientation == "landscape" else DMORIENT_PORTRAIT
        )
    color = str(settings.get("color") or "printer")
    if color != "printer":
        devmode.dmFields |= DM_COLOR
        devmode.dmColor = DMCOLOR_COLOR if color == "color" else DMCOLOR_MONOCHROME
    duplex = str(settings.get("duplex") or "printer")
    duplex_value = {
        "simplex": DMDUP_SIMPLEX,
        "long": DMDUP_VERTICAL,
        "short": DMDUP_HORIZONTAL,
    }.get(duplex)
    if duplex_value is not None:
        devmode.dmFields |= DM_DUPLEX
        devmode.dmDuplex = duplex_value
    devmode.dmFields |= DM_COLLATE
    devmode.dmCollate = DMCOLLATE_TRUE


def _open_printer_devmode(
    printer: str,
    settings: dict[str, Any],
    orientation: str,
) -> tuple[Any, Any, Any, Any, _DevModeW, Any]:
    win_dll = getattr(ctypes, "WinDLL")
    winspool = win_dll("winspool.drv", use_last_error=True)
    gdi32 = win_dll("gdi32.dll", use_last_error=True)
    printer_handle = wintypes.HANDLE()
    winspool.OpenPrinterW.argtypes = [
        wintypes.LPWSTR,
        ctypes.POINTER(wintypes.HANDLE),
        wintypes.LPVOID,
    ]
    winspool.OpenPrinterW.restype = wintypes.BOOL
    winspool.ClosePrinter.argtypes = [wintypes.HANDLE]
    winspool.ClosePrinter.restype = wintypes.BOOL
    winspool.DocumentPropertiesW.argtypes = [
        wintypes.HWND,
        wintypes.HANDLE,
        wintypes.LPWSTR,
        ctypes.POINTER(_DevModeW),
        ctypes.POINTER(_DevModeW),
        wintypes.DWORD,
    ]
    winspool.DocumentPropertiesW.restype = ctypes.c_long
    if not winspool.OpenPrinterW(printer, ctypes.byref(printer_handle), None):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        size = winspool.DocumentPropertiesW(
            None, printer_handle, printer, None, None, 0
        )
        if size <= 0:
            raise RuntimeError("printer driver did not provide DEVMODE")
        buffer = ctypes.create_string_buffer(size)
        pointer = ctypes.cast(buffer, ctypes.POINTER(_DevModeW))
        if winspool.DocumentPropertiesW(
            None,
            printer_handle,
            printer,
            pointer,
            None,
            DM_OUT_BUFFER,
        ) < 0:
            raise RuntimeError("printer driver rejected DEVMODE")
        devmode = pointer.contents
        _configure_devmode(devmode, settings, orientation)
        if winspool.DocumentPropertiesW(
            None,
            printer_handle,
            printer,
            pointer,
            pointer,
            DM_IN_BUFFER | DM_OUT_BUFFER,
        ) < 0:
            raise RuntimeError("printer driver rejected the requested settings")

        gdi32.CreateDCW.argtypes = [
            wintypes.LPCWSTR,
            wintypes.LPCWSTR,
            wintypes.LPCWSTR,
            ctypes.POINTER(_DevModeW),
        ]
        gdi32.CreateDCW.restype = wintypes.HDC
        hdc = gdi32.CreateDCW("WINSPOOL", printer, None, pointer)
        if not hdc:
            raise ctypes.WinError(ctypes.get_last_error())
        return winspool, gdi32, printer_handle, buffer, devmode, hdc
    except Exception:
        winspool.ClosePrinter(printer_handle)
        raise


def _page_orientation(page: Any, requested: str) -> str:
    if requested in {"portrait", "landscape"}:
        return requested
    return "landscape" if page.rect.width > page.rect.height else "portrait"


def _draw_pdf_page(
    gdi32: Any,
    hdc: Any,
    devmode_buffer: Any,
    page: Any,
    settings: dict[str, Any],
) -> Any:
    from PIL import Image, ImageOps
    import pymupdf as fitz

    pointer = ctypes.cast(devmode_buffer, ctypes.POINTER(_DevModeW))
    orientation = _page_orientation(
        page, str(settings.get("orientation") or "auto")
    )
    _configure_devmode(pointer.contents, settings, orientation)
    gdi32.ResetDCW.argtypes = [wintypes.HDC, ctypes.POINTER(_DevModeW)]
    gdi32.ResetDCW.restype = wintypes.HDC
    reset = gdi32.ResetDCW(hdc, pointer)
    if not reset:
        raise RuntimeError("printer driver rejected the page settings")
    hdc = reset

    gdi32.GetDeviceCaps.argtypes = [wintypes.HDC, ctypes.c_int]
    gdi32.GetDeviceCaps.restype = ctypes.c_int
    width = gdi32.GetDeviceCaps(hdc, HORZRES)
    height = gdi32.GetDeviceCaps(hdc, VERTRES)
    dpi_x = max(72, gdi32.GetDeviceCaps(hdc, LOGPIXELSX))
    dpi_y = max(72, gdi32.GetDeviceCaps(hdc, LOGPIXELSY))
    if width <= 0 or height <= 0:
        raise RuntimeError("printer reported an invalid printable area")

    actual_width = max(1.0, page.rect.width * dpi_x / 72.0)
    actual_height = max(1.0, page.rect.height * dpi_y / 72.0)
    fit_scale = min(width / actual_width, height / actual_height)
    scaling = str(settings.get("scaling") or "fit")
    scale = 1.0 if scaling == "actual" else fit_scale
    if scaling == "shrink":
        scale = min(1.0, fit_scale)
    destination_width = max(1, round(actual_width * scale))
    destination_height = max(1, round(actual_height * scale))
    destination_x = round((width - destination_width) / 2)
    destination_y = round((height - destination_height) / 2)

    render_zoom = min(
        300 / 72,
        max(
            1.0,
            destination_width / max(1.0, page.rect.width),
            destination_height / max(1.0, page.rect.height),
        ),
    )
    pixmap = page.get_pixmap(
        matrix=fitz.Matrix(render_zoom, render_zoom),
        colorspace=fitz.csRGB,
        alpha=False,
    )
    image = Image.frombytes(
        "RGB", (pixmap.width, pixmap.height), pixmap.samples
    )
    if str(settings.get("color") or "printer") == "grayscale":
        image = ImageOps.grayscale(image).convert("RGB")
    stride = ((image.width * 3 + 3) // 4) * 4
    dib = image.tobytes("raw", "BGR", stride, -1)
    bitmap = _BitmapInfo()
    bitmap.bmiHeader = _BitmapInfoHeader(
        ctypes.sizeof(_BitmapInfoHeader),
        image.width,
        image.height,
        1,
        24,
        BI_RGB,
        len(dib),
        0,
        0,
        0,
        0,
    )

    gdi32.StartPage.argtypes = [wintypes.HDC]
    gdi32.StartPage.restype = ctypes.c_int
    gdi32.EndPage.argtypes = [wintypes.HDC]
    gdi32.EndPage.restype = ctypes.c_int
    if gdi32.StartPage(hdc) <= 0:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        if cancellation_requested():
            raise PrintCancelled("cancelled while rendering the current page")
        gdi32.SetStretchBltMode(hdc, HALFTONE)
        gdi32.StretchDIBits.argtypes = [
            wintypes.HDC,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_int,
            wintypes.LPVOID,
            ctypes.POINTER(_BitmapInfo),
            wintypes.UINT,
            wintypes.DWORD,
        ]
        gdi32.StretchDIBits.restype = ctypes.c_int
        bits = ctypes.create_string_buffer(dib)
        drawn = gdi32.StretchDIBits(
            hdc,
            destination_x,
            destination_y,
            destination_width,
            destination_height,
            0,
            0,
            image.width,
            image.height,
            bits,
            ctypes.byref(bitmap),
            DIB_RGB_COLORS,
            SRCCOPY,
        )
        if drawn <= 0 or drawn == GDI_ERROR:
            raise ctypes.WinError(ctypes.get_last_error())
    except Exception:
        gdi32.EndPage(hdc)
        raise
    if gdi32.EndPage(hdc) <= 0:
        raise ctypes.WinError(ctypes.get_last_error())
    return hdc


def _submit_controlled_job(
    path: pathlib.Path,
    printer: str,
    settings: dict[str, Any],
) -> None:
    if os.name != "nt":
        raise OSError("Windows printing is required")
    import pymupdf as fitz

    orientation = str(settings.get("orientation") or "auto")
    winspool, gdi32, printer_handle, devmode_buffer, _devmode, hdc = (
        _open_printer_devmode(printer, settings, orientation)
    )
    document_name = f"DocFlow - {path.name}"
    doc_info = _DocInfoW(
        ctypes.sizeof(_DocInfoW), document_name, None, None, 0
    )
    started = False
    try:
        gdi32.StartDocW.argtypes = [wintypes.HDC, ctypes.POINTER(_DocInfoW)]
        gdi32.StartDocW.restype = ctypes.c_int
        gdi32.EndDoc.argtypes = [wintypes.HDC]
        gdi32.EndDoc.restype = ctypes.c_int
        gdi32.AbortDoc.argtypes = [wintypes.HDC]
        gdi32.AbortDoc.restype = ctypes.c_int
        with _PrintQueueMonitor(printer) as monitor:
            windows_job_id = gdi32.StartDocW(hdc, ctypes.byref(doc_info))
            if windows_job_id <= 0:
                raise ctypes.WinError(ctypes.get_last_error())
            started = True
            with fitz.open(path) as document:
                if document.page_count < 1:
                    raise ValueError("document has no printable pages")
                for page_number in range(document.page_count):
                    if cancellation_requested():
                        raise PrintCancelled("cancelled before the next page")
                    hdc = _draw_pdf_page(
                        gdi32,
                        hdc,
                        devmode_buffer,
                        document.load_page(page_number),
                        settings,
                    )
            if gdi32.EndDoc(hdc) <= 0:
                raise ctypes.WinError(ctypes.get_last_error())
            started = False
            timeout_minutes = int(
                settings.get("timeoutMinutes") or DEFAULT_TIMEOUT_MINUTES
            )
            monitor.wait_for_completion(
                document_name,
                job_id=windows_job_id,
                timeout_seconds=timeout_minutes * 60,
                acknowledge_seconds=min(timeout_minutes * 60, 10 * 60),
            )
    except Exception:
        if started:
            gdi32.AbortDoc(hdc)
        raise
    finally:
        gdi32.DeleteDC.argtypes = [wintypes.HDC]
        gdi32.DeleteDC.restype = wintypes.BOOL
        gdi32.DeleteDC(hdc)
        winspool.ClosePrinter(printer_handle)


def _printer_inventory() -> tuple[list[str], str]:
    """Return installed printer names with the default printer first."""

    if os.name != "nt":
        return [], ""
    try:
        win_dll = getattr(ctypes, "WinDLL")
        winspool = win_dll("winspool.drv", use_last_error=True)
        winspool.EnumPrintersW.argtypes = [
            wintypes.DWORD,
            wintypes.LPWSTR,
            wintypes.DWORD,
            LPBYTE,
            wintypes.DWORD,
            ctypes.POINTER(wintypes.DWORD),
            ctypes.POINTER(wintypes.DWORD),
        ]
        winspool.EnumPrintersW.restype = wintypes.BOOL
        winspool.GetDefaultPrinterW.argtypes = [
            wintypes.LPWSTR,
            ctypes.POINTER(wintypes.DWORD),
        ]
        winspool.GetDefaultPrinterW.restype = wintypes.BOOL
        needed = wintypes.DWORD()
        returned = wintypes.DWORD()
        flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS
        winspool.EnumPrintersW(
            flags,
            None,
            4,
            None,
            0,
            ctypes.byref(needed),
            ctypes.byref(returned),
        )
        names: list[str] = []
        if needed.value:
            buffer = ctypes.create_string_buffer(needed.value)
            ok = winspool.EnumPrintersW(
                flags,
                None,
                4,
                ctypes.cast(buffer, LPBYTE),
                needed.value,
                ctypes.byref(needed),
                ctypes.byref(returned),
            )
            if ok:
                printers = ctypes.cast(
                    buffer,
                    ctypes.POINTER(_PrinterInfo4 * returned.value),
                ).contents
                names = [
                    item.pPrinterName
                    for item in printers
                    if item.pPrinterName
                ]

        default = ""
        default_size = wintypes.DWORD()
        winspool.GetDefaultPrinterW(None, ctypes.byref(default_size))
        if default_size.value:
            default_buffer = ctypes.create_unicode_buffer(default_size.value)
            if winspool.GetDefaultPrinterW(
                default_buffer,
                ctypes.byref(default_size),
            ):
                default = default_buffer.value

        unique = sorted(set(names), key=str.casefold)
        if default in unique:
            unique.remove(default)
            unique.insert(0, default)
        return unique, default
    except (AttributeError, OSError, ValueError):
        return [], ""


def _association_command(extension: str, verb: str) -> str:
    """Read the Windows shell command for an extension/verb without executing it."""

    if os.name != "nt" or not extension:
        return ""
    try:
        win_dll = getattr(ctypes, "WinDLL")
        shlwapi = win_dll("shlwapi.dll", use_last_error=True)
        query = shlwapi.AssocQueryStringW
        query.argtypes = [
            wintypes.DWORD,
            wintypes.DWORD,
            wintypes.LPCWSTR,
            wintypes.LPCWSTR,
            wintypes.LPWSTR,
            ctypes.POINTER(wintypes.DWORD),
        ]
        query.restype = ctypes.c_long
        size = wintypes.DWORD()
        query(
            0,
            ASSOCSTR_COMMAND,
            extension,
            verb,
            None,
            ctypes.byref(size),
        )
        if not size.value:
            return ""
        buffer = ctypes.create_unicode_buffer(size.value)
        result = query(
            0,
            ASSOCSTR_COMMAND,
            extension,
            verb,
            buffer,
            ctypes.byref(size),
        )
        return buffer.value if result == 0 else ""
    except (AttributeError, OSError, ValueError):
        return ""


def _resolve_print_verb(
    path: pathlib.Path,
    printer: str,
    default_printer: str,
) -> str:
    extension = path.suffix.lower()
    if extension in CONTROLLED_FORMATS:
        return "docflow"
    if _association_command(extension, "printto"):
        return "printto"
    if printer == default_printer and _association_command(extension, "print"):
        return "print"
    return ""


def _submit_print_job(
    path: pathlib.Path,
    printer: str,
    verb: str,
    settings: dict[str, Any] | None = None,
) -> None:
    """Submit one job through the registered Windows print handler."""

    if os.name != "nt":
        raise OSError("Windows printing is required")
    if not path.is_file():
        raise FileNotFoundError(str(path))
    printers, default = _printer_inventory()
    if printer not in printers:
        raise RuntimeError(f"printer is no longer available: {printer}")
    if verb == "print" and printer != default:
        raise RuntimeError(
            "the default printer changed after preview; preview the queue again"
        )
    effective = settings or {}
    if verb == "docflow":
        _submit_controlled_job(path, printer, effective)
        return

    startfile = getattr(os, "startfile", None)
    if startfile is None:
        raise OSError("Windows Shell printing is unavailable")
    # Arm the spooler monitor before launching the associated application. If
    # Word/Excel/PDF asks the user to confirm something, this call waits there.
    # The next file is never launched until this job has left the Windows queue.
    with _PrintQueueMonitor(printer) as monitor:
        if verb == "printto":
            startfile(str(path), "printto", f'"{printer}"')
        elif verb == "print":
            startfile(str(path), "print")
        else:
            raise ValueError(f"unsupported print verb: {verb}")
        timeout_minutes = int(
            effective.get("timeoutMinutes") or DEFAULT_TIMEOUT_MINUTES
        )
        monitor.wait_for_completion(
            path.name,
            timeout_seconds=timeout_minutes * 60,
            acknowledge_seconds=min(timeout_minutes * 60, 10 * 60),
        )


def _ordered_unique_files(raw: Any) -> list[pathlib.Path]:
    if not isinstance(raw, list):
        return []
    seen: set[str] = set()
    files: list[pathlib.Path] = []
    for value in raw:
        path = pathlib.Path(str(value)).expanduser().absolute()
        key = os.path.normcase(str(path))
        if key in seen:
            continue
        seen.add(key)
        files.append(path)
    return files


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _queue_root() -> pathlib.Path:
    configured = os.environ.get("DOCFLOW_PRINT_QUEUE_DIR")
    if configured:
        return pathlib.Path(configured).expanduser()
    return pathlib.Path.home() / ".docflow" / "print-queues"


def _queue_path(queue_id: str) -> pathlib.Path:
    if not queue_id or not all(char.isalnum() or char == "-" for char in queue_id):
        raise ValueError("invalid print queue id")
    return _queue_root() / f"{queue_id}.json"


_QUEUE_WRITE_LOCK = threading.RLock()
_MEMORY_QUEUES: dict[str, dict[str, Any]] = {}
_MEMORY_QUEUE_ROOTS: dict[str, str] = {}


def _save_queue(state: dict[str, Any]) -> None:
    state["updatedAt"] = _utc_now()
    path = _queue_path(str(state["queueId"]))
    temporary = path.with_name(f".{path.name}.{uuid.uuid4().hex}.tmp")
    with _QUEUE_WRITE_LOCK:
        queue_id = str(state["queueId"])
        _MEMORY_QUEUES[queue_id] = copy.deepcopy(state)
        _MEMORY_QUEUE_ROOTS[queue_id] = str(path.parent)
        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            temporary.write_text(
                json.dumps(state, ensure_ascii=False, separators=(",", ":")),
                encoding="utf-8",
            )
            temporary.replace(path)
        except OSError:
            # Printing remains usable in a locked-down profile. Recovery is
            # still available until the sidecar exits via the in-memory copy.
            return
        finally:
            try:
                temporary.unlink()
            except FileNotFoundError:
                pass


def _load_queue(queue_id: str) -> dict[str, Any]:
    try:
        decoded = json.loads(_queue_path(queue_id).read_text(encoding="utf-8"))
    except FileNotFoundError:
        with _QUEUE_WRITE_LOCK:
            decoded = (
                copy.deepcopy(_MEMORY_QUEUES.get(queue_id))
                if _MEMORY_QUEUE_ROOTS.get(queue_id) == str(_queue_root())
                else None
            )
    if not isinstance(decoded, dict) or decoded.get("queueId") != queue_id:
        raise ValueError("invalid saved print queue")
    jobs = decoded.get("jobs")
    if not isinstance(jobs, list):
        raise ValueError("saved print queue has no jobs")
    decoded["jobs"] = [dict(job) for job in jobs if isinstance(job, dict)]
    return decoded


def _recover_interrupted_queue(state: dict[str, Any]) -> bool:
    changed = False
    for job in state.get("jobs", []):
        if job.get("status") == "printing":
            job["status"] = "uncertain"
            job["error"] = "application closed while this job was active"
            changed = True
    if state.get("status") == "running":
        state["status"] = "paused"
        changed = True
    if changed:
        state["currentJobId"] = None
        _save_queue(state)
    return changed


def _queue_counts(state: dict[str, Any]) -> dict[str, int]:
    counts = {
        "pending": 0,
        "printing": 0,
        "completed": 0,
        "failed": 0,
        "cancelled": 0,
        "uncertain": 0,
    }
    for job in state.get("jobs", []):
        status = str(job.get("status") or "pending")
        if status in counts:
            counts[status] += 1
    return counts


def _queue_summary(state: dict[str, Any]) -> dict[str, Any]:
    counts = _queue_counts(state)
    resumable = sum(
        counts[key] for key in ("pending", "failed", "cancelled", "uncertain")
    )
    return {
        "queueId": state.get("queueId"),
        "status": state.get("status"),
        "createdAt": state.get("createdAt"),
        "updatedAt": state.get("updatedAt"),
        "counts": counts,
        "resumable": resumable,
        "fileCount": len(
            {
                str(job.get("path") or "")
                for job in state.get("jobs", [])
                if job.get("path")
            }
        ),
    }


def latest_print_queue() -> dict[str, Any] | None:
    root = _queue_root()
    try:
        candidates = sorted(
            root.glob("*.json"), key=lambda item: item.stat().st_mtime_ns, reverse=True
        )
    except OSError:
        candidates = []
    seen: set[str] = set()
    for path in candidates:
        try:
            state = _load_queue(path.stem)
            seen.add(path.stem)
            _recover_interrupted_queue(state)
            summary = _queue_summary(state)
            if int(summary["resumable"]) > 0:
                return summary
        except (OSError, ValueError, TypeError, json.JSONDecodeError):
            continue
    # If the Windows profile is locked down, _save_queue keeps a process-local
    # recovery copy. Surface it in exactly the same way as a persisted queue.
    with _QUEUE_WRITE_LOCK:
        memory_states = sorted(
            (
                copy.deepcopy(state)
                for queue_id, state in _MEMORY_QUEUES.items()
                if queue_id not in seen
                and _MEMORY_QUEUE_ROOTS.get(queue_id) == str(root)
            ),
            key=lambda state: str(state.get("updatedAt") or ""),
            reverse=True,
        )
    for state in memory_states:
        try:
            _recover_interrupted_queue(state)
            summary = _queue_summary(state)
            if int(summary["resumable"]) > 0:
                return summary
        except (OSError, ValueError, TypeError):
            continue
    return None


def discard_print_queue(queue_id: str) -> dict[str, Any]:
    state = _load_queue(queue_id)
    _recover_interrupted_queue(state)
    for job in state.get("jobs", []):
        if job.get("status") in {"pending", "failed", "cancelled", "uncertain"}:
            job["status"] = "abandoned"
    state["status"] = "abandoned"
    state["currentJobId"] = None
    _save_queue(state)
    return _queue_summary(state)


def _file_signature(path: pathlib.Path) -> dict[str, int]:
    try:
        stat = path.stat()
    except OSError:
        return {}
    return {"size": stat.st_size, "mtimeNs": stat.st_mtime_ns}


def _int_setting(value: Any, default: int, minimum: int, maximum: int) -> int:
    try:
        parsed = int(value)
    except (TypeError, ValueError):
        return default
    return parsed if minimum <= parsed <= maximum else default


def _choice_setting(value: Any, choices: tuple[str, ...], default: str) -> str:
    parsed = str(value or "")
    return parsed if parsed in choices else default


def _print_columns() -> list[dict[str, Any]]:
    return [
        {
            "key": "enabled",
            "labelKey": "print.col.include",
            "kind": "bool",
            "width": 64,
        },
        {
            "key": "status",
            "labelKey": "print.col.status",
            "kind": "readonly",
            "width": 90,
            "bulk": False,
        },
        {
            "key": "file",
            "labelKey": "print.col.file",
            "kind": "readonly",
            "width": 260,
            "bulk": False,
        },
        {
            "key": "format",
            "labelKey": "print.col.format",
            "kind": "readonly",
            "width": 76,
            "bulk": False,
        },
        {
            "key": "printer",
            "labelKey": "print.col.printer",
            "kind": "choice",
            "width": 190,
        },
        {
            "key": "paper",
            "labelKey": "print.col.paper",
            "kind": "choice",
            "width": 88,
        },
        {
            "key": "orientation",
            "labelKey": "print.col.orientation",
            "kind": "choice",
            "width": 112,
        },
        {
            "key": "scaling",
            "labelKey": "print.col.scaling",
            "kind": "choice",
            "width": 104,
        },
        {
            "key": "color",
            "labelKey": "print.col.color",
            "kind": "choice",
            "width": 110,
        },
        {
            "key": "duplex",
            "labelKey": "print.col.duplex",
            "kind": "choice",
            "width": 116,
        },
        {
            "key": "copies",
            "labelKey": "print.col.copies",
            "kind": "int",
            "width": 76,
        },
    ]


def _print_editor(
    rows: list[dict[str, Any]], printers: list[str]
) -> dict[str, Any]:
    return {
        "kind": "recordTable",
        "layout": "compact",
        "titleKey": "print.editor.title",
        "rowCountKey": "print.editor.rowCount",
        "groupField": "format",
        "selectionGroupFields": ["format"],
        "columns": _print_columns(),
        "rows": copy.deepcopy(rows),
        "choices": {
            "printer": printers,
            "paper": list(PAPER_CHOICES),
            "orientation": list(ORIENTATION_CHOICES),
            "scaling": list(SCALING_CHOICES),
            "color": list(COLOR_CHOICES),
            "duplex": list(DUPLEX_CHOICES),
        },
        "choiceLabels": {
            "paper": {choice: f"choice.print.paper.{choice}" for choice in PAPER_CHOICES},
            "orientation": {
                choice: f"choice.print.orientation.{choice}"
                for choice in ORIENTATION_CHOICES
            },
            "scaling": {
                choice: f"choice.print.scaling.{choice}"
                for choice in SCALING_CHOICES
            },
            "color": {choice: f"choice.print.color.{choice}" for choice in COLOR_CHOICES},
            "duplex": {
                choice: f"choice.print.duplex.{choice}" for choice in DUPLEX_CHOICES
            },
        },
        "bulkEnabled": True,
        "selectionEnabled": True,
        "duplicateEnabled": False,
    }


def _jobs_from_rows(
    rows: list[dict[str, Any]], collate: bool
) -> list[dict[str, Any]]:
    selected = [row for row in rows if bool(row.get("enabled", True))]

    def physical_job(row: dict[str, Any], copy_index: int) -> dict[str, Any]:
        row_id = str(row.get("_rowId") or uuid.uuid4().hex)
        return {
            "jobId": str(row.get("_resumeJobId") or f"{row_id}-c{copy_index}"),
            "path": str(row["path"]),
            "printer": str(row["printer"]),
            "verb": str(row["verb"]),
            "format": str(row["format"]),
            "paper": str(row["paper"]),
            "orientation": str(row["orientation"]),
            "scaling": str(row["scaling"]),
            "color": str(row["color"]),
            "duplex": str(row["duplex"]),
            "timeoutMinutes": int(row["timeoutMinutes"]),
            "retries": int(row["retries"]),
            "onError": str(row["onError"]),
            "copy": copy_index,
            "signature": copy.deepcopy(row.get("signature") or {}),
        }

    jobs: list[dict[str, Any]] = []
    if collate:
        max_copies = max((int(row["copies"]) for row in selected), default=0)
        for copy_index in range(1, max_copies + 1):
            for row in selected:
                if int(row["copies"]) >= copy_index:
                    jobs.append(physical_job(row, copy_index))
    else:
        for row in selected:
            for copy_index in range(1, int(row["copies"]) + 1):
                jobs.append(physical_job(row, copy_index))
    return jobs


def _plan_from_print_rows(
    *,
    rows: list[dict[str, Any]],
    printers: list[str],
    collate: bool,
    queue_id: str,
    warnings: list[Warning],
    resume_queue_id: str = "",
) -> Plan:
    jobs = _jobs_from_rows(rows, collate)
    steps = [
        Step(
            "print",
            str(row["path"]),
            str(row["path"]),
            note_key="note.print.queued",
            meta={
                "printer": row["printer"],
                "verb": row["verb"],
                "format": row["format"],
                "copies": row["copies"],
                "paper": row["paper"],
                "orientation": row["orientation"],
                "scaling": row["scaling"],
                "color": row["color"],
                "duplex": row["duplex"],
            },
        )
        for row in rows
        if bool(row.get("enabled", True))
    ]
    effective_warnings = list(warnings)
    if steps and not any(item.key == "warn.print.nonUndoable" for item in effective_warnings):
        effective_warnings.append(Warning("warn.print.nonUndoable"))
    if not steps:
        effective_warnings.append(Warning("warn.print.noJobsSelected"))
    return Plan(
        op_id="files.batch_print",
        steps=steps,
        warnings=effective_warnings,
        meta={
            "queueId": queue_id,
            "resumeQueueId": resume_queue_id,
            "collate": collate,
            "jobs": jobs,
            "fileCount": len(steps),
            "jobCount": len(jobs),
            "nonUndoable": True,
            "editor": _print_editor(rows, printers),
        },
    )


@register("files.batch_print")
class BatchPrint(Operation):
    title_key = "op.files.batch_print.title"
    description_key = "op.files.batch_print.desc"

    def __init__(self) -> None:
        printers, default = _printer_inventory()
        self._printers = printers
        self._default_printer = default
        default_printer = default or (printers[0] if printers else None)
        self.schema = ParamSchema(
            [
                FilesParam(
                    "files",
                    "param.print.files",
                    help_key="param.print.files.help",
                ),
                ChoiceParam(
                    "printer",
                    "param.print.printer",
                    printers,
                    default=default_printer,
                    help_key="param.print.printer.help",
                ),
                ChoiceParam(
                    "paper",
                    "param.print.paper",
                    list(PAPER_CHOICES),
                    default="A4",
                    choice_labels={
                        choice: f"choice.print.paper.{choice}" for choice in PAPER_CHOICES
                    },
                    help_key="param.print.paper.help",
                ),
                ChoiceParam(
                    "orientation",
                    "param.print.orientation",
                    list(ORIENTATION_CHOICES),
                    default="auto",
                    choice_labels={
                        choice: f"choice.print.orientation.{choice}"
                        for choice in ORIENTATION_CHOICES
                    },
                ),
                ChoiceParam(
                    "scaling",
                    "param.print.scaling",
                    list(SCALING_CHOICES),
                    default="fit",
                    choice_labels={
                        choice: f"choice.print.scaling.{choice}"
                        for choice in SCALING_CHOICES
                    },
                ),
                ChoiceParam(
                    "color",
                    "param.print.color",
                    list(COLOR_CHOICES),
                    default="printer",
                    choice_labels={
                        choice: f"choice.print.color.{choice}" for choice in COLOR_CHOICES
                    },
                ),
                ChoiceParam(
                    "duplex",
                    "param.print.duplex",
                    list(DUPLEX_CHOICES),
                    default="printer",
                    choice_labels={
                        choice: f"choice.print.duplex.{choice}"
                        for choice in DUPLEX_CHOICES
                    },
                ),
                IntParam(
                    "copies",
                    "param.print.copies",
                    default=1,
                    help_key="param.print.copies.help",
                ),
                BoolParam(
                    "collate",
                    "param.print.collate",
                    default=True,
                    help_key="param.print.collate.help",
                ),
                IntParam(
                    "retries",
                    "param.print.retries",
                    default=0,
                    help_key="param.print.retries.help",
                ),
                ChoiceParam(
                    "on_error",
                    "param.print.onError",
                    list(ERROR_ACTION_CHOICES),
                    default="pause",
                    choice_labels={
                        "pause": "choice.print.onError.pause",
                        "skip": "choice.print.onError.skip",
                    },
                    help_key="param.print.onError.help",
                ),
                IntParam(
                    "timeout_minutes",
                    "param.print.timeout",
                    default=DEFAULT_TIMEOUT_MINUTES,
                    help_key="param.print.timeout.help",
                ),
            ]
        )

    def _row_from_file(
        self,
        path: pathlib.Path,
        index: int,
        params: dict[str, Any],
    ) -> tuple[dict[str, Any] | None, Warning | None]:
        if not path.is_file():
            return None, Warning("warn.print.fileMissing", path.name)
        printer = str(params["printer"])
        verb = _resolve_print_verb(path, printer, self._default_printer)
        if not verb:
            return None, Warning(
                "warn.print.noHandler", f"{path.name} ({path.suffix or '?'})"
            )
        file_format = (path.suffix.lstrip(".") or "file").upper()
        return (
            {
                "_rowId": f"file-{index + 1}",
                "enabled": True,
                "status": "ready",
                "file": path.name,
                "path": str(path),
                "format": file_format,
                "printer": printer,
                "paper": params["paper"],
                "orientation": params["orientation"],
                "scaling": params["scaling"],
                "color": params["color"],
                "duplex": params["duplex"],
                "copies": params["copies"],
                "retries": params["retries"],
                "onError": params["onError"],
                "timeoutMinutes": params["timeoutMinutes"],
                "verb": verb,
                "signature": _file_signature(path),
            },
            None,
        )

    def preview(self, params: dict) -> Plan:
        files = _ordered_unique_files(params.get("files"))
        printer = str(params.get("printer") or "")
        if not self._printers:
            return Plan(op_id=self.id, warnings=[Warning("warn.print.noPrinters")])
        if printer not in self._printers:
            return Plan(
                op_id=self.id,
                warnings=[Warning("warn.print.printerUnavailable", printer)],
            )
        if len(files) > MAX_FILES:
            return Plan(
                op_id=self.id,
                warnings=[Warning("warn.print.tooManyFiles", str(len(files)))],
            )
        raw_copies = params.get("copies")
        copies = _int_setting(
            1 if raw_copies in (None, "") else raw_copies,
            0,
            1,
            MAX_COPIES,
        )
        raw_retries = params.get("retries")
        retries = _int_setting(
            0 if raw_retries in (None, "") else raw_retries,
            -1,
            0,
            MAX_RETRIES,
        )
        raw_timeout = params.get("timeout_minutes")
        timeout_minutes = _int_setting(
            DEFAULT_TIMEOUT_MINUTES if raw_timeout in (None, "") else raw_timeout,
            0,
            1,
            MAX_TIMEOUT_MINUTES,
        )
        warnings: list[Warning] = []
        if not copies:
            warnings.append(Warning("warn.print.invalidCopies", str(params.get("copies"))))
        if retries < 0:
            warnings.append(Warning("warn.print.invalidRetries", str(params.get("retries"))))
        if not timeout_minutes:
            warnings.append(
                Warning("warn.print.invalidTimeout", str(params.get("timeout_minutes")))
            )
        if warnings:
            return Plan(op_id=self.id, warnings=warnings)
        normalized: dict[str, Any] = {
            "printer": printer,
            "paper": _choice_setting(params.get("paper"), PAPER_CHOICES, "A4"),
            "orientation": _choice_setting(
                params.get("orientation"), ORIENTATION_CHOICES, "auto"
            ),
            "scaling": _choice_setting(params.get("scaling"), SCALING_CHOICES, "fit"),
            "color": _choice_setting(params.get("color"), COLOR_CHOICES, "printer"),
            "duplex": _choice_setting(params.get("duplex"), DUPLEX_CHOICES, "printer"),
            "copies": copies,
            "retries": retries,
            "onError": _choice_setting(
                params.get("on_error"), ERROR_ACTION_CHOICES, "pause"
            ),
            "timeoutMinutes": timeout_minutes,
        }
        rows: list[dict[str, Any]] = []
        for index, path in enumerate(files):
            notify("progress", {"phase": "preview", "done": index, "total": len(files)})
            row, warning = self._row_from_file(path, index, normalized)
            if row is not None:
                rows.append(row)
                if row["verb"] != "docflow":
                    warnings.append(Warning("warn.print.settingsLimited", path.name))
            if warning is not None:
                warnings.append(warning)
        notify("progress", {"phase": "preview", "done": len(files), "total": len(files)})
        return _plan_from_print_rows(
            rows=rows,
            printers=self._printers,
            collate=bool(params.get("collate", True)),
            queue_id=str(uuid.uuid4()),
            warnings=warnings,
        )

    def _safe_editor_rows(
        self, plan: Plan, raw_rows: list[Any]
    ) -> list[dict[str, Any]]:
        editor = plan.meta.get("editor")
        originals = list(editor.get("rows") or []) if isinstance(editor, dict) else []
        original_by_id = {
            str(row.get("_rowId")): row for row in originals if isinstance(row, dict)
        }
        if len(raw_rows) != len(original_by_id):
            raise ValueError("print queue rows cannot be added or removed")
        rows: list[dict[str, Any]] = []
        seen: set[str] = set()
        for raw in raw_rows:
            if not isinstance(raw, dict):
                raise ValueError("invalid print row")
            row_id = str(raw.get("_rowId") or "")
            if row_id in seen or row_id not in original_by_id:
                raise ValueError("invalid print row identity")
            seen.add(row_id)
            original = original_by_id[row_id]
            path = pathlib.Path(str(original["path"]))
            printer = str(raw.get("printer") or "")
            if printer not in self._printers:
                raise ValueError(f"printer is unavailable: {printer}")
            copies = _int_setting(raw.get("copies"), 0, 1, MAX_COPIES)
            if not copies:
                raise ValueError("copies must be between 1 and 99")
            row = copy.deepcopy(original)
            row.update(
                {
                    "enabled": bool(raw.get("enabled", True)),
                    "printer": printer,
                    "paper": _choice_setting(raw.get("paper"), PAPER_CHOICES, "A4"),
                    "orientation": _choice_setting(
                        raw.get("orientation"), ORIENTATION_CHOICES, "auto"
                    ),
                    "scaling": _choice_setting(
                        raw.get("scaling"), SCALING_CHOICES, "fit"
                    ),
                    "color": _choice_setting(raw.get("color"), COLOR_CHOICES, "printer"),
                    "duplex": _choice_setting(raw.get("duplex"), DUPLEX_CHOICES, "printer"),
                    "copies": copies,
                }
            )
            row["verb"] = _resolve_print_verb(path, printer, self._default_printer)
            if not row["verb"]:
                raise ValueError(f"no print handler: {path.name}")
            rows.append(row)
        return rows

    def revise(self, plan: Plan, payload: dict) -> Plan:
        rows = self._safe_editor_rows(plan, list(payload.get("rows") or []))
        warnings = [
            warning
            for warning in plan.warnings
            if warning.key not in {"warn.print.nonUndoable", "warn.print.noJobsSelected"}
        ]
        return _plan_from_print_rows(
            rows=rows,
            printers=self._printers,
            collate=bool(plan.meta.get("collate", True)),
            queue_id=str(plan.meta.get("queueId") or uuid.uuid4()),
            resume_queue_id=str(plan.meta.get("resumeQueueId") or ""),
            warnings=warnings,
        )

    def resume(self, queue_id: str) -> Plan:
        state = _load_queue(queue_id)
        _recover_interrupted_queue(state)
        rows: list[dict[str, Any]] = []
        for index, job in enumerate(state["jobs"]):
            status = str(job.get("status") or "pending")
            path = pathlib.Path(str(job.get("path") or ""))
            rows.append(
                {
                    "_rowId": f"resume-{index + 1}",
                    "_resumeJobId": str(job.get("jobId") or f"job-{index + 1}"),
                    "enabled": status in {"pending", "failed", "cancelled"},
                    "status": status,
                    "file": path.name,
                    "path": str(path),
                    "format": str(job.get("format") or path.suffix.lstrip(".").upper()),
                    "printer": str(job.get("printer") or ""),
                    "paper": str(job.get("paper") or "A4"),
                    "orientation": str(job.get("orientation") or "auto"),
                    "scaling": str(job.get("scaling") or "fit"),
                    "color": str(job.get("color") or "printer"),
                    "duplex": str(job.get("duplex") or "printer"),
                    "copies": 1,
                    "retries": int(job.get("retries") or 0),
                    "onError": str(job.get("onError") or "pause"),
                    "timeoutMinutes": int(
                        job.get("timeoutMinutes") or DEFAULT_TIMEOUT_MINUTES
                    ),
                    "verb": str(job.get("verb") or ""),
                    "signature": copy.deepcopy(job.get("signature") or {}),
                }
            )
        return _plan_from_print_rows(
            rows=rows,
            printers=self._printers,
            collate=False,
            queue_id=queue_id,
            resume_queue_id=queue_id,
            warnings=[Warning("warn.print.resumingQueue", queue_id[:8])],
        )

    @staticmethod
    def _state_for_plan(plan: Plan, jobs: list[dict[str, Any]]) -> dict[str, Any]:
        queue_id = str(plan.meta.get("queueId") or uuid.uuid4())
        resume_queue_id = str(plan.meta.get("resumeQueueId") or "")
        if resume_queue_id:
            state = _load_queue(resume_queue_id)
            by_id = {
                str(job.get("jobId")): job
                for job in state.get("jobs", [])
                if isinstance(job, dict)
            }
            for planned in jobs:
                saved = by_id.get(str(planned.get("jobId")))
                if saved is None:
                    raise ValueError("saved print job no longer exists")
                saved.update(copy.deepcopy(planned))
                saved["status"] = "pending"
                saved["error"] = ""
            state["status"] = "ready"
            return state
        created = _utc_now()
        return {
            "version": 1,
            "queueId": queue_id,
            "createdAt": created,
            "updatedAt": created,
            "status": "ready",
            "currentJobId": None,
            "jobs": [
                {
                    **copy.deepcopy(job),
                    "status": "pending",
                    "attempts": 0,
                    "error": "",
                }
                for job in jobs
            ],
        }

    def apply(self, plan: Plan) -> Result:
        raw_jobs = plan.meta.get("jobs")
        if not isinstance(raw_jobs, list):
            return Result(failed=1, errors=[Warning("error.print.invalidPlan")])
        jobs: list[dict[str, Any]] = []
        for index, raw in enumerate(raw_jobs):
            if not isinstance(raw, dict):
                return Result(failed=1, errors=[Warning("error.print.invalidPlan")])
            job = copy.deepcopy(raw)
            job.setdefault("jobId", f"legacy-{index + 1}")
            job.setdefault("paper", "A4")
            job.setdefault("orientation", "auto")
            job.setdefault("scaling", "fit")
            job.setdefault("color", "printer")
            job.setdefault("duplex", "printer")
            job.setdefault("timeoutMinutes", DEFAULT_TIMEOUT_MINUTES)
            job.setdefault("retries", 0)
            job.setdefault("onError", "pause")
            job.setdefault("signature", {})
            jobs.append(job)
        try:
            state = self._state_for_plan(plan, jobs)
        except Exception as error:  # noqa: BLE001
            return Result(
                failed=1,
                errors=[Warning("error.print.invalidPlan", str(error))],
            )
        by_id = {str(job["jobId"]): job for job in state["jobs"]}
        state["status"] = "running"
        _save_queue(state)
        result = Result()
        paused = False
        for index, job in enumerate(jobs):
            saved = by_id[str(job["jobId"])]
            if cancellation_requested():
                paused = True
                result.errors.append(Warning("warn.print.cancelled"))
                break
            notify(
                "progress",
                {
                    "phase": "print_waiting",
                    "done": index,
                    "total": len(jobs),
                    "subject": pathlib.Path(str(job.get("path") or "")).name,
                },
            )
            attempts_allowed = 1 + _int_setting(
                job.get("retries"), 0, 0, MAX_RETRIES
            )
            completed = False
            for _attempt in range(attempts_allowed):
                saved["status"] = "printing"
                saved["attempts"] = int(saved.get("attempts") or 0) + 1
                saved["error"] = ""
                state["currentJobId"] = saved["jobId"]
                _save_queue(state)
                try:
                    path = pathlib.Path(str(job["path"]))
                    signature = job.get("signature")
                    if isinstance(signature, dict) and signature:
                        if _file_signature(path) != signature:
                            raise RuntimeError(
                                "file changed after the queue was previewed"
                            )
                    _submit_print_job(
                        path,
                        str(job["printer"]),
                        str(job["verb"]),
                        job,
                    )
                    saved["status"] = "completed"
                    saved["completedAt"] = _utc_now()
                    state["currentJobId"] = None
                    result.applied += 1
                    _save_queue(state)
                    completed = True
                    break
                except PrintCancelled as error:
                    saved["status"] = "uncertain" if error.uncertain else "cancelled"
                    saved["error"] = str(error)
                    state["currentJobId"] = None
                    state["status"] = "paused"
                    _save_queue(state)
                    result.errors.append(
                        Warning(
                            "warn.print.cancelledUncertain"
                            if error.uncertain
                            else "warn.print.cancelled",
                            pathlib.Path(str(job.get("path") or "")).name,
                        )
                    )
                    paused = True
                    break
                except PrintStateUncertain as error:
                    # Never retry automatically when Windows may still have
                    # printed all or part of the job. The user can explicitly
                    # select it in the recovered queue after checking paper.
                    saved["status"] = "uncertain"
                    saved["error"] = str(error)
                    state["currentJobId"] = None
                    state["status"] = "paused"
                    _save_queue(state)
                    result.errors.append(
                        Warning(
                            "warn.print.stateUncertain",
                            pathlib.Path(str(job.get("path") or "")).name,
                        )
                    )
                    paused = True
                    break
                except Exception as error:  # noqa: BLE001
                    saved["error"] = f"{type(error).__name__}: {error}"
                    state["currentJobId"] = None
                    if int(saved["attempts"]) < attempts_allowed and not cancellation_requested():
                        saved["status"] = "pending"
                        _save_queue(state)
                        continue
                    saved["status"] = "failed"
                    _save_queue(state)
                    result.failed += 1
                    result.errors.append(
                        Warning(
                            "error.print.failed",
                            f"{pathlib.Path(str(job.get('path') or '')).name}: "
                            f"{type(error).__name__}: {error}",
                        )
                    )
                    if str(job.get("onError") or "pause") == "pause":
                        result.errors.append(
                            Warning("error.print.queuePaused", str(len(jobs) - index - 1))
                        )
                        paused = True
                    break
            if paused:
                break
            if not completed and str(job.get("onError") or "pause") != "skip":
                break

        state["currentJobId"] = None
        counts = _queue_counts(state)
        resumable = sum(
            counts[key] for key in ("pending", "failed", "cancelled", "uncertain")
        )
        if resumable:
            state["status"] = "paused" if paused or counts["pending"] else "completed_with_errors"
        else:
            state["status"] = "completed"
        _save_queue(state)
        summary = _queue_summary(state)
        result.meta = {
            **summary,
            "resumeAvailable": bool(resumable),
        }
        notify(
            "progress",
            {"phase": "apply", "done": len(jobs) - counts["pending"], "total": len(jobs)},
        )
        return result

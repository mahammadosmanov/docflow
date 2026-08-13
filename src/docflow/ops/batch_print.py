"""Windows mixed-file batch printing with an explicit preview queue."""

from __future__ import annotations

import ctypes
import os
import pathlib
import time
from ctypes import wintypes
from dataclasses import dataclass
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
JOB_TERMINAL_STATUS = (
    JOB_STATUS_DELETED | JOB_STATUS_COMPLETE | JOB_STATUS_PRINTED
)
WAIT_OBJECT_0 = 0
WAIT_TIMEOUT = 258
INFINITE_JOB_COUNT = 0xFFFFFFFF
SPOOL_POLL_SECONDS = 0.2
SPOOL_ACK_TIMEOUT_SECONDS = 30 * 60
SPOOL_FINISH_TIMEOUT_SECONDS = 4 * 60 * 60


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

    def wait_for_completion(self, document: str) -> None:
        """Fail closed: no later file starts until this request is finished."""

        tracked: set[int] = set()
        acknowledge_deadline = time.monotonic() + SPOOL_ACK_TIMEOUT_SECONDS

        while time.monotonic() < acknowledge_deadline:
            current = self._jobs()
            added = {
                job_id
                for job_id, job in current.items()
                if job_id not in self._baseline
                and _job_matches_document(job, document)
            }
            if added:
                tracked = added
                break
            self._wait_change(SPOOL_POLL_SECONDS)

        if not tracked:
            raise TimeoutError(
                f"Windows did not accept the print job within "
                f"{SPOOL_ACK_TIMEOUT_SECONDS // 60} minutes: {document}"
            )

        finish_deadline = time.monotonic() + SPOOL_FINISH_TIMEOUT_SECONDS
        while time.monotonic() < finish_deadline:
            current = self._jobs()
            active = [current[job_id] for job_id in tracked if job_id in current]
            if not active:
                return
            if all(job.status & JOB_TERMINAL_STATUS for job in active):
                return
            self._wait_change(SPOOL_POLL_SECONDS)

        raise TimeoutError(
            f"Windows did not finish the print job within "
            f"{SPOOL_FINISH_TIMEOUT_SECONDS // 3600} hours: {document}"
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
    if _association_command(extension, "printto"):
        return "printto"
    if printer == default_printer and _association_command(extension, "print"):
        return "print"
    return ""


def _submit_print_job(path: pathlib.Path, printer: str, verb: str) -> None:
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
        monitor.wait_for_completion(path.name)


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


@register("files.batch_print")
class BatchPrint(Operation):
    title_key = "op.files.batch_print.title"
    description_key = "op.files.batch_print.desc"

    def __init__(self) -> None:
        printers, default = _printer_inventory()
        self._printers = printers
        self._default_printer = default
        self.schema = ParamSchema([
            FilesParam(
                "files",
                "param.print.files",
                help_key="param.print.files.help",
            ),
            ChoiceParam(
                "printer",
                "param.print.printer",
                printers,
                default=default or (printers[0] if printers else None),
                help_key="param.print.printer.help",
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
        ])

    def preview(self, params: dict) -> Plan:
        plan = Plan(op_id=self.id)
        files = _ordered_unique_files(params.get("files"))
        printer = str(params.get("printer") or "")
        try:
            raw_copies = params.get("copies")
            copies = 1 if raw_copies in (None, "") else int(raw_copies)
        except (TypeError, ValueError):
            copies = 0
        collate = bool(params.get("collate", True))

        if not self._printers:
            plan.warnings.append(Warning("warn.print.noPrinters"))
            return plan
        if printer not in self._printers:
            plan.warnings.append(Warning("warn.print.printerUnavailable", printer))
            return plan
        if copies < 1 or copies > MAX_COPIES:
            plan.warnings.append(Warning("warn.print.invalidCopies", str(copies)))
            return plan
        if len(files) > MAX_FILES:
            plan.warnings.append(Warning("warn.print.tooManyFiles", str(len(files))))
            return plan

        printable: list[dict[str, str]] = []
        for index, path in enumerate(files):
            notify(
                "progress",
                {"phase": "preview", "done": index, "total": len(files)},
            )
            if not path.is_file():
                plan.warnings.append(Warning("warn.print.fileMissing", path.name))
                continue
            verb = _resolve_print_verb(path, printer, self._default_printer)
            if not verb:
                plan.warnings.append(
                    Warning("warn.print.noHandler", f"{path.name} ({path.suffix or '?'})")
                )
                continue
            item = {
                "path": str(path),
                "printer": printer,
                "verb": verb,
                "format": (path.suffix.lstrip(".") or "file").upper(),
            }
            printable.append(item)
            plan.steps.append(Step(
                "print",
                str(path),
                str(path),
                note_key="note.print.queued",
                meta={
                    "printer": printer,
                    "verb": verb,
                    "format": item["format"],
                    "copies": copies,
                },
            ))

        jobs: list[dict[str, str | int]] = []
        if collate:
            for copy_index in range(1, copies + 1):
                for item in printable:
                    jobs.append({**item, "copy": copy_index})
        else:
            for item in printable:
                for copy_index in range(1, copies + 1):
                    jobs.append({**item, "copy": copy_index})

        if printable:
            plan.warnings.append(Warning("warn.print.nonUndoable"))
        plan.meta = {
            "printer": printer,
            "copies": copies,
            "collate": collate,
            "jobs": jobs,
            "fileCount": len(printable),
            "jobCount": len(jobs),
            "nonUndoable": True,
        }
        notify(
            "progress",
            {"phase": "preview", "done": len(files), "total": len(files)},
        )
        return plan

    def apply(self, plan: Plan) -> Result:
        result = Result()
        jobs = plan.meta.get("jobs")
        if not isinstance(jobs, list):
            return Result(
                failed=1,
                errors=[Warning("error.print.invalidPlan")],
            )
        for index, raw in enumerate(jobs):
            notify(
                "progress",
                {
                    "phase": "print_waiting",
                    "done": index,
                    "total": len(jobs),
                    "subject": pathlib.Path(str(raw.get("path", ""))).name
                    if isinstance(raw, dict)
                    else "",
                },
            )
            try:
                if not isinstance(raw, dict):
                    raise ValueError("invalid print job")
                path = pathlib.Path(str(raw["path"]))
                printer = str(raw["printer"])
                verb = str(raw["verb"])
                _submit_print_job(path, printer, verb)
                result.applied += 1
            except Exception as error:  # noqa: BLE001 - report exact queue failure
                remaining = len(jobs) - index - 1
                result.failed += 1 + remaining
                subject = str(raw.get("path") if isinstance(raw, dict) else raw)
                result.errors.append(
                    Warning(
                        "error.print.failed",
                        f"{pathlib.Path(subject).name}: {type(error).__name__}: {error}",
                    )
                )
                if remaining:
                    result.errors.append(
                        Warning("error.print.queueStopped", str(remaining))
                    )
                # Strict ordering is fail-closed. Continuing after an unknown
                # print state could place later files ahead of this one.
                break
        notify(
            "progress",
            {"phase": "apply", "done": len(jobs), "total": len(jobs)},
        )
        return result

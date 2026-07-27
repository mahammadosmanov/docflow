"""Windows mixed-file batch printing with an explicit preview queue."""

from __future__ import annotations

import ctypes
import os
import pathlib
import time
from ctypes import wintypes
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


class _PrinterInfo4(ctypes.Structure):
    _fields_ = [
        ("pPrinterName", wintypes.LPWSTR),
        ("pServerName", wintypes.LPWSTR),
        ("Attributes", wintypes.DWORD),
    ]


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
    if verb == "printto":
        startfile(str(path), "printto", f'"{printer}"')
    elif verb == "print":
        startfile(str(path), "print")
    else:
        raise ValueError(f"unsupported print verb: {verb}")
    # Shell print handlers are asynchronous. A small gap prevents a burst of
    # mixed Office/PDF/image launches from losing jobs on slower workstations.
    time.sleep(0.5)


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
                {"phase": "apply", "done": index, "total": len(jobs)},
            )
            try:
                if not isinstance(raw, dict):
                    raise ValueError("invalid print job")
                path = pathlib.Path(str(raw["path"]))
                printer = str(raw["printer"])
                verb = str(raw["verb"])
                _submit_print_job(path, printer, verb)
                result.applied += 1
            except Exception as error:  # noqa: BLE001 - one job must not stop the queue
                result.failed += 1
                subject = str(raw.get("path") if isinstance(raw, dict) else raw)
                result.errors.append(
                    Warning(
                        "error.print.failed",
                        f"{pathlib.Path(subject).name}: {type(error).__name__}: {error}",
                    )
                )
        notify(
            "progress",
            {"phase": "apply", "done": len(jobs), "total": len(jobs)},
        )
        return result

"""CIV/MEC RFI Log satiri olusturma.

Is kurallari preview'da hesaplanir. Apply yalnizca onaylanan kayitlari, onceden
belirlenen satirlara yazar; kaynak dosyanin preview'dan sonra degismedigini
SHA-256 ile dogrular. Expiry/Status/Validity sutunlarina dokunulmaz.
"""

from __future__ import annotations

import copy
import os
import pathlib
import re
import shutil
import uuid
import warnings as pywarnings
from dataclasses import dataclass
from datetime import date, datetime, time, timedelta
from typing import Any

from openpyxl import load_workbook

from ..contracts import (
    BoolParam,
    ChoiceParam,
    FileParam,
    IntParam,
    JournalEntry,
    Operation,
    ParamSchema,
    Plan,
    Result,
    Step,
    Warning,
)
from ..fs.apply import file_sha256, write_journal
from ..registry import register
from ..rpc import notify
from ..xlsx_ooxml import patch_workbook

pywarnings.filterwarnings(
    "ignore",
    message="Data Validation extension is not supported.*",
    category=UserWarning,
    module=r"openpyxl\..*",
)

RFI_RE = re.compile(r"-RFI-(CIV|MEC)-(\d+)$", re.IGNORECASE)
ITEM_RE = re.compile(r"^(CIV|MEC)-(\d+)\.", re.IGNORECASE)
NORTH_RE = re.compile(r"(?:^|[^A-Z0-9])N\d{1,3}(?:[^A-Z0-9]|$)", re.IGNORECASE)
SOUTH_RE = re.compile(r"(?:^|[^A-Z0-9])S\d{1,3}(?:[^A-Z0-9]|$)", re.IGNORECASE)
DAY_START = time(9, 0)
DAY_END = time(17, 30)
SLOT = timedelta(minutes=30)
MAX_RFI_COUNT = 100
LOOKUP_LAST_ROW = 205


def _default_workbook() -> str | None:
    configured = os.environ.get("DOCFLOW_RFI_LOG")
    if configured:
        return configured
    candidate = (
        pathlib.Path.home()
        / "OneDrive - AZFEN LLC"
        / "Desktop"
        / "SystemU"
        / "RFI Log.xlsx"
    )
    return str(candidate) if candidate.is_file() else None


@dataclass(frozen=True)
class Layout:
    sheet: str
    issue_col: int
    inspection_col: int
    expiry_col: int
    time_col: int
    azfen_col: int
    intec_col: int
    location_col: int
    subcontractor_col: int
    status_col: int
    validity_col: int


LAYOUTS = {
    "CIV": Layout("CIV", 8, 9, 10, 11, 12, 13, 14, 15, 16, 17),
    # MEC'te H = ROW No.; kullanici istemedigi icin bos ve dokunulmadan kalir.
    "MEC": Layout("MEC", 9, 10, 11, 12, 13, 14, 15, 16, 17, 18),
}

COPY_FIELDS = [
    "rfiNo",
    "itpNo",
    "itemNo",
    "description",
    "forms",
    "tagNo",
    "area",
    "dateOfIssue",
    "inspectionDate",
    "startTime",
    "azfenType",
    "intecType",
    "location",
    "subcontractor",
]

COPY_HEADERS = [
    "RFI No.",
    "ITP No.",
    "Item No.",
    "INSPECTION DESCRIPTION",
    "FORMS",
    "TAG NO",
    "AREA",
    "DATE OF ISSUE",
    "INSPECTION DATE",
    "INSPECTION START TIME",
    "INSP. TYPE AZFEN",
    "INSP. TYPE INTEC",
    "Inspection Location",
    "SUBCONTRACTOR",
]


def _today() -> date:
    return date.today()


def _signature(path: pathlib.Path) -> dict[str, Any]:
    stat = path.stat()
    return {
        "size": stat.st_size,
        "sha256": file_sha256(path),
    }


def _as_date(value: Any) -> date | None:
    if isinstance(value, datetime):
        return value.date()
    if isinstance(value, date):
        return value
    return None


def _as_time(value: Any) -> time | None:
    if isinstance(value, datetime):
        return value.time().replace(second=0, microsecond=0)
    if isinstance(value, time):
        return value.replace(second=0, microsecond=0)
    if isinstance(value, (int, float)):
        seconds = round(float(value) * 24 * 60 * 60)
        return time((seconds // 3600) % 24, (seconds % 3600) // 60)
    if isinstance(value, str):
        try:
            return datetime.strptime(value.strip(), "%H:%M").time()
        except ValueError:
            return None
    return None


def _date_text(value: date) -> str:
    return value.strftime("%d-%b-%y")


def _time_text(value: time) -> str:
    return value.strftime("%H:%M")


def _parse_date_text(value: str) -> date:
    for fmt in ("%d-%b-%y", "%Y-%m-%d", "%d.%m.%Y"):
        try:
            return datetime.strptime(value.strip(), fmt).date()
        except ValueError:
            continue
    raise ValueError(f"gecersiz tarih: {value}")


def _parse_time_text(value: str) -> time:
    try:
        return datetime.strptime(value.strip(), "%H:%M").time()
    except ValueError as exc:
        raise ValueError(f"gecersiz saat: {value}") from exc


def _canonical_itp(discipline: str, number: int) -> str:
    return f"AZE-SUN-INT-QMS-ITP-{discipline}-{number:04d}"


def _recent_unique(values: list[str], limit: int = 500) -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for value in reversed(values):
        clean = value.strip()
        if not clean or clean in seen:
            continue
        seen.add(clean)
        out.append(clean)
        if len(out) >= limit:
            break
    return out


def _infer_area(tag: str) -> str:
    north = bool(NORTH_RE.search(tag))
    south = bool(SOUTH_RE.search(tag))
    if north == south:
        return ""
    return "North Site" if north else "South Site"


def _read_source(path: pathlib.Path, discipline: str) -> dict[str, Any]:
    layout = LAYOUTS[discipline]
    with pywarnings.catch_warnings():
        pywarnings.filterwarnings(
            "ignore",
            message="Data Validation extension is not supported.*",
            category=UserWarning,
        )
        workbook = load_workbook(
            path,
            read_only=True,
            data_only=True,
            keep_links=True,
        )
    try:
        if layout.sheet not in workbook.sheetnames or "Sheet1" not in workbook.sheetnames:
            raise ValueError("RFI Log icin CIV/MEC/Sheet1 sayfalari bulunamadi")
        sheet = workbook[layout.sheet]
        lookup_sheet = workbook["Sheet1"]

        presets: dict[str, dict[str, str]] = {}
        for row in lookup_sheet.iter_rows(
            min_row=1,
            max_row=LOOKUP_LAST_ROW,
            min_col=11,
            max_col=17,
            values_only=True,
        ):
            item = str(row[0] or "").strip()
            match = ITEM_RE.match(item)
            if not match or match.group(1).upper() != discipline:
                continue
            presets[item] = {
                "description": str(row[1] or "").strip(),
                "forms": str(row[2] or "").strip(),
                "azfenType": str(row[4] or "").strip(),
                "intecType": str(row[6] or "").strip(),
            }

        max_number = 0
        last_row = 9
        latest_item = ""
        tags: list[str] = []
        areas: list[str] = []
        locations: list[str] = []
        subcontractors: list[str] = []
        occupied: dict[date, dict[str, time]] = {}

        for row_number, row in enumerate(
            sheet.iter_rows(
            min_row=10,
            max_row=sheet.max_row,
            min_col=1,
            max_col=layout.validity_col,
            values_only=True,
            ),
            start=10,
        ):
            rfi = str(row[0] or "").strip()
            match = RFI_RE.search(rfi)
            if not match or match.group(1).upper() != discipline:
                continue
            last_row = row_number
            max_number = max(max_number, int(match.group(2)))
            latest_item = str(row[2] or "").strip() or latest_item
            tags.append(str(row[5] or ""))
            areas.append(str(row[6] or ""))
            locations.append(str(row[layout.location_col - 1] or ""))
            subcontractors.append(str(row[layout.subcontractor_col - 1] or ""))
            inspection_day = _as_date(row[layout.inspection_col - 1])
            inspection_time = _as_time(row[layout.time_col - 1])
            if inspection_day and inspection_time:
                occupied.setdefault(inspection_day, {})[rfi] = inspection_time

        if not presets:
            raise ValueError(f"{discipline} icin Sheet1 esleme verisi bulunamadi")

        itp_to_items: dict[str, list[str]] = {}
        for item in presets:
            match = ITEM_RE.match(item)
            assert match is not None
            itp = _canonical_itp(discipline, int(match.group(2)))
            itp_to_items.setdefault(itp, []).append(item)
        for items in itp_to_items.values():
            items.sort(
                key=lambda value: [
                    int(part) if part.isdigit() else part
                    for part in re.split(r"(\d+)", value)
                ]
            )

        default_item = latest_item if latest_item in presets else next(iter(presets))
        item_match = ITEM_RE.match(default_item)
        assert item_match is not None
        default_itp = _canonical_itp(discipline, int(item_match.group(2)))
        return {
            "presets": presets,
            "itpToItems": itp_to_items,
            "defaultItem": default_item,
            "defaultItp": default_itp,
            "maxNumber": max_number,
            "lastRow": last_row,
            "maxRow": sheet.max_row,
            "tags": _recent_unique(tags),
            "areas": _recent_unique(areas),
            "locations": _recent_unique(locations),
            "subcontractors": _recent_unique(subcontractors),
            "occupied": occupied,
        }
    finally:
        workbook.close()


def _next_slot(
    occupied: dict[date, dict[str, time]],
    day: date,
) -> time:
    times = list(occupied.get(day, {}).values())
    if not times:
        return DAY_START
    last = max(times)
    return (datetime.combine(day, last) + SLOT).time()


def _allocate(
    occupied: dict[date, dict[str, time]],
    start_day: date,
    count: int,
    wrap: bool,
) -> tuple[list[tuple[date, time]], int]:
    working = {day: dict(values) for day, values in occupied.items()}
    slots: list[tuple[date, time]] = []
    overflowed = 0
    day = start_day
    for index in range(count):
        candidate = _next_slot(working, day)
        if candidate > DAY_END:
            overflowed += 1
            if wrap:
                while candidate > DAY_END:
                    day += timedelta(days=1)
                    candidate = _next_slot(working, day)
        marker = f"__planned_{index}"
        working.setdefault(day, {})[marker] = candidate
        slots.append((day, candidate))
    return slots, overflowed


def _columns() -> list[dict[str, Any]]:
    return [
        {"key": "rfiNo", "labelKey": "rfi.col.rfiNo", "kind": "readonly", "width": 236},
        {"key": "itpNo", "labelKey": "rfi.col.itpNo", "kind": "choice", "width": 236},
        {"key": "itemNo", "labelKey": "rfi.col.itemNo", "kind": "choice", "width": 128},
        {
            "key": "description",
            "labelKey": "rfi.col.description",
            "kind": "multiline",
            "width": 300,
        },
        {"key": "forms", "labelKey": "rfi.col.forms", "kind": "multiline", "width": 240},
        {"key": "tagNo", "labelKey": "rfi.col.tagNo", "kind": "autocomplete", "width": 150},
        {"key": "area", "labelKey": "rfi.col.area", "kind": "autocomplete", "width": 144},
        {"key": "dateOfIssue", "labelKey": "rfi.col.issueDate", "kind": "text", "width": 112},
        {
            "key": "inspectionDate",
            "labelKey": "rfi.col.inspectionDate",
            "kind": "text",
            "width": 120,
        },
        {"key": "startTime", "labelKey": "rfi.col.startTime", "kind": "text", "width": 100},
        {"key": "azfenType", "labelKey": "rfi.col.azfenType", "kind": "text", "width": 104},
        {"key": "intecType", "labelKey": "rfi.col.intecType", "kind": "text", "width": 104},
        {
            "key": "location",
            "labelKey": "rfi.col.location",
            "kind": "autocomplete",
            "width": 180,
        },
        {
            "key": "subcontractor",
            "labelKey": "rfi.col.subcontractor",
            "kind": "autocomplete",
            "width": 140,
        },
    ]


def _plan_meta(
    discipline: str,
    source: dict[str, Any],
    rows: list[dict[str, Any]],
) -> dict[str, Any]:
    return {
        "editor": {
            "kind": "recordTable",
            "titleKey": "rfi.editor.title",
            "groupField": "rfiNo",
            "columns": _columns(),
            "rows": copy.deepcopy(rows),
            "choices": {
                "itpNo": sorted(source["itpToItems"]),
            },
            "choicesByValue": {
                "itemNo": {
                    "dependsOn": "itpNo",
                    "values": source["itpToItems"],
                },
            },
            "autoFill": {
                "field": "itemNo",
                "values": source["presets"],
            },
            "autocomplete": {
                "tagNo": source["tags"],
                "area": source["areas"],
                "location": source["locations"],
                "subcontractor": source["subcontractors"],
            },
            "regexFill": [
                {
                    "source": "tagNo",
                    "target": "area",
                    "pattern": NORTH_RE.pattern,
                    "value": "North Site",
                },
                {
                    "source": "tagNo",
                    "target": "area",
                    "pattern": SOUTH_RE.pattern,
                    "value": "South Site",
                },
            ],
            "duplicateEnabled": True,
            "copyEnabled": True,
            "copyFields": COPY_FIELDS,
            "copyHeaders": COPY_HEADERS,
            "discipline": discipline,
        }
    }


def _record(
    *,
    index: int,
    rfi: str,
    source: dict[str, Any],
    issue_day: date,
    slot: tuple[date, time],
) -> dict[str, Any]:
    preset = source["presets"][source["defaultItem"]]
    return {
        "_rowId": f"g{index + 1}-r1",
        "rfiNo": rfi,
        "itpNo": source["defaultItp"],
        "itemNo": source["defaultItem"],
        "description": preset["description"],
        "forms": preset["forms"],
        "tagNo": "",
        "area": "",
        "dateOfIssue": _date_text(issue_day),
        "inspectionDate": _date_text(slot[0]),
        "startTime": _time_text(slot[1]),
        "azfenType": preset["azfenType"],
        "intecType": preset["intecType"],
        "location": "",
        "subcontractor": "AZFEN",
    }


def _warnings_for_rows(
    rows: list[dict[str, Any]],
    *,
    overflowed: int = 0,
    wrap: bool = True,
) -> list[Warning]:
    warnings: list[Warning] = []
    if overflowed:
        key = "warn.rfi.overflowMoved" if wrap else "warn.rfi.overflowKept"
        warnings.append(Warning(key, str(overflowed)))

    by_rfi: dict[str, list[dict[str, Any]]] = {}
    for row in rows:
        by_rfi.setdefault(str(row["rfiNo"]), []).append(row)
    for rfi, group in by_rfi.items():
        schedule = {
            (str(row.get("inspectionDate") or ""), str(row.get("startTime") or ""))
            for row in group
        }
        if len(schedule) > 1:
            warnings.append(Warning("warn.rfi.groupScheduleMismatch", rfi))
    return warnings


def _safe_rows(
    raw_rows: list[Any],
    *,
    base_rfis: list[str],
    source: dict[str, Any],
) -> list[dict[str, Any]]:
    allowed_fields = set(COPY_FIELDS) | {"_rowId"}
    allowed_rfis = set(base_rfis)
    rows: list[dict[str, Any]] = []
    for index, raw in enumerate(raw_rows):
        if not isinstance(raw, dict):
            raise ValueError("gecersiz RFI satiri")
        row = {key: raw.get(key, "") for key in allowed_fields}
        row["_rowId"] = str(row.get("_rowId") or f"row-{uuid.uuid4().hex[:10]}")
        row["rfiNo"] = str(row["rfiNo"]).strip()
        if row["rfiNo"] not in allowed_rfis:
            raise ValueError(f"plan disi RFI numarasi: {row['rfiNo']}")
        for key in COPY_FIELDS:
            row[key] = str(row.get(key) or "").strip()
        if not row["area"]:
            row["area"] = _infer_area(row["tagNo"])
        _parse_date_text(row["dateOfIssue"])
        _parse_date_text(row["inspectionDate"])
        _parse_time_text(row["startTime"])
        if row["itemNo"] not in source["presets"]:
            raise ValueError(f"bilinmeyen Item No.: {row['itemNo']}")
        if row["itpNo"] not in source["itpToItems"]:
            raise ValueError(f"bilinmeyen ITP No.: {row['itpNo']}")
        if row["itemNo"] not in source["itpToItems"][row["itpNo"]]:
            raise ValueError(
                f"Item No. secilen ITP'ye ait degil: {row['itpNo']} / {row['itemNo']}"
            )
        for key in (
            "description",
            "forms",
            "azfenType",
            "intecType",
            "subcontractor",
        ):
            if not row[key]:
                raise ValueError(f"zorunlu alan bos: {key} (satir {index + 1})")
        rows.append(row)

    if not rows:
        raise ValueError("en az bir RFI satiri gerekli")
    present = {row["rfiNo"] for row in rows}
    if present != allowed_rfis:
        missing = sorted(allowed_rfis - present)
        raise ValueError(f"RFI grubu silinemez: {', '.join(missing)}")
    return rows


def _assert_not_locked(path: pathlib.Path) -> None:
    if os.name != "nt":
        return
    try:
        import msvcrt

        with path.open("r+b") as stream:
            msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
            msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
    except (OSError, PermissionError) as exc:
        raise PermissionError(
            "RFI Log Excel'de acik veya yazmaya kapali. Dosyayi kapatip tekrar deneyin."
        ) from exc


def _formula_or_value(
    value: str,
    default_value: str,
    formula: str,
) -> tuple[str, str]:
    return ("formula", formula) if value == default_value else ("value", value)


def _write_records(
    workbook_path: pathlib.Path,
    temp_path: pathlib.Path,
    discipline: str,
    start_row: int,
    rows: list[dict[str, Any]],
    presets: dict[str, dict[str, str]],
) -> None:
    layout = LAYOUTS[discipline]
    updates: dict[tuple[int, int], tuple[str, Any]] = {}
    for index, record in enumerate(rows):
        row_number = start_row + index
        preset = presets[record["itemNo"]]
        values = {
            1: record["rfiNo"],
            2: record["itpNo"],
            3: record["itemNo"],
            6: record["tagNo"],
            7: record["area"],
            layout.issue_col: _parse_date_text(record["dateOfIssue"]),
            layout.inspection_col: _parse_date_text(record["inspectionDate"]),
            layout.time_col: _parse_time_text(record["startTime"]),
            layout.location_col: record["location"],
            layout.subcontractor_col: record["subcontractor"],
        }
        for column, value in values.items():
            updates[(row_number, column)] = ("value", value)

        if discipline == "CIV":
            description_col, forms_col, azfen_col, intec_col = 4, 5, 12, 13
        else:
            description_col, forms_col, azfen_col, intec_col = 4, 5, 13, 14

        updates[(row_number, description_col)] = _formula_or_value(
                record["description"],
                preset["description"],
                f'=IFERROR(VLOOKUP(C{row_number},\'Sheet1\'!$K$1:$L${LOOKUP_LAST_ROW},2,0),"")',
        )
        updates[(row_number, forms_col)] = _formula_or_value(
                record["forms"],
                preset["forms"],
                f'=IFERROR(VLOOKUP(D{row_number},\'Sheet1\'!$L$1:$M${LOOKUP_LAST_ROW},2,0),"")',
        )
        updates[(row_number, azfen_col)] = _formula_or_value(
                record["azfenType"],
                preset["azfenType"],
                f'=IFERROR(VLOOKUP(C{row_number},\'Sheet1\'!$N$1:$O${LOOKUP_LAST_ROW},2,0),"")',
        )
        updates[(row_number, intec_col)] = _formula_or_value(
                record["intecType"],
                preset["intecType"],
                f'=IFERROR(VLOOKUP(C{row_number},\'Sheet1\'!$P$1:$Q${LOOKUP_LAST_ROW},2,0),"")',
        )
    patch_workbook(workbook_path, temp_path, layout.sheet, updates)


def _verify_records(
    workbook_path: pathlib.Path,
    discipline: str,
    start_row: int,
    rows: list[dict[str, Any]],
) -> None:
    layout = LAYOUTS[discipline]
    with pywarnings.catch_warnings():
        pywarnings.filterwarnings(
            "ignore",
            message="Data Validation extension is not supported.*",
            category=UserWarning,
        )
        workbook = load_workbook(
            workbook_path,
            read_only=True,
            data_only=False,
            keep_links=True,
        )
    try:
        sheet = workbook[layout.sheet]
        for index, record in enumerate(rows):
            row_number = start_row + index
            checks = {
                1: record["rfiNo"],
                2: record["itpNo"],
                3: record["itemNo"],
                6: record["tagNo"],
                7: record["area"],
                layout.location_col: record["location"],
                layout.subcontractor_col: record["subcontractor"],
            }
            for column, expected in checks.items():
                actual = str(sheet.cell(row_number, column).value or "")
                if actual != str(expected or ""):
                    raise RuntimeError(
                        f"yazma dogrulamasi basarisiz: {layout.sheet}!"
                        f"{sheet.cell(row_number, column).coordinate}"
                    )
            # Dokunulmamasi gereken kolonlar acikca korunur.
            if layout.expiry_col in (
                layout.issue_col,
                layout.inspection_col,
                layout.time_col,
            ):
                raise AssertionError("gecersiz kolon eslemesi")
    finally:
        workbook.close()


@register("rfi.log_add")
class AddRfiLogRows(Operation):
    title_key = "op.rfi.log_add.title"
    description_key = "op.rfi.log_add.desc"
    schema = ParamSchema(
        [
            FileParam(
                "workbook",
                "param.rfi.workbook",
                default=_default_workbook(),
                help_key="param.rfi.workbook.help",
            ),
            ChoiceParam(
                "discipline",
                "param.rfi.discipline",
                choices=["CIV", "MEC"],
                default="CIV",
            ),
            IntParam(
                "count",
                "param.rfi.count",
                default=3,
                help_key="param.rfi.count.help",
            ),
            BoolParam(
                "overflow_next_day",
                "param.rfi.overflowNextDay",
                default=True,
                help_key="param.rfi.overflowNextDay.help",
            ),
        ]
    )

    def preview(self, params: dict) -> Plan:
        workbook_path = pathlib.Path(str(params["workbook"])).expanduser().resolve()
        discipline = str(params.get("discipline") or "CIV").upper()
        count = int(params.get("count") or 0)
        wrap = bool(params.get("overflow_next_day", True))
        if workbook_path.suffix.lower() != ".xlsx" or not workbook_path.is_file():
            raise ValueError("gecerli bir .xlsx RFI Log dosyasi secin")
        if discipline not in LAYOUTS:
            raise ValueError("yalnizca CIV ve MEC destekleniyor")
        if count < 1 or count > MAX_RFI_COUNT:
            raise ValueError(f"RFI adedi 1-{MAX_RFI_COUNT} arasinda olmali")

        notify("progress", {"phase": "preview", "done": 0, "total": 3})
        source = _read_source(workbook_path, discipline)
        notify("progress", {"phase": "preview", "done": 1, "total": 3})
        start_day = _today() + timedelta(days=1)
        slots, overflowed = _allocate(source["occupied"], start_day, count, wrap)
        issue_day = _today()
        rows = []
        base_rfis = []
        for index in range(count):
            number = source["maxNumber"] + index + 1
            rfi = f"AZE-SUN-INT-QMS-RFI-{discipline}-{number:04d}"
            base_rfis.append(rfi)
            rows.append(
                _record(
                    index=index,
                    rfi=rfi,
                    source=source,
                    issue_day=issue_day,
                    slot=slots[index],
                )
            )

        start_row = source["lastRow"] + 1
        if start_row + len(rows) - 1 > source["maxRow"]:
            raise ValueError("RFI Log'da yeni satirlar icin ayrilmis kapasite kalmadi")
        signature = _signature(workbook_path)
        backup_dir = workbook_path.parent / ".docflow-backups"
        backup_name = (
            f"{workbook_path.stem}_{datetime.now():%Y%m%d_%H%M%S}_"
            f"{uuid.uuid4().hex[:8]}{workbook_path.suffix}"
        )
        step = Step(
            "workbook_update",
            src=str(workbook_path),
            dst=str(backup_dir / backup_name),
            note_key="note.rfi.workbookUpdate",
            meta={
                "discipline": discipline,
                "startRow": start_row,
                "records": copy.deepcopy(rows),
                "sourceSignature": signature,
                "baseRfis": base_rfis,
                "presets": source["presets"],
                "sourceData": {
                    key: source[key]
                    for key in (
                        "presets",
                        "itpToItems",
                        "tags",
                        "areas",
                        "locations",
                        "subcontractors",
                    )
                },
            },
        )
        notify("progress", {"phase": "preview", "done": 3, "total": 3})
        return Plan(
            op_id=self.id,
            steps=[step],
            warnings=_warnings_for_rows(rows, overflowed=overflowed, wrap=wrap),
            meta=_plan_meta(discipline, source, rows),
        )

    def revise(self, plan: Plan, payload: dict) -> Plan:
        if len(plan.steps) != 1 or plan.steps[0].kind != "workbook_update":
            raise ValueError("gecersiz RFI Log plani")
        original = plan.steps[0]
        source_data = copy.deepcopy(original.meta["sourceData"])
        rows = _safe_rows(
            list(payload.get("rows") or []),
            base_rfis=list(original.meta["baseRfis"]),
            source=source_data,
        )
        start_row = int(original.meta["startRow"])
        # Kaynak dosyadaki ayrilmis tablo kapasitesini ilk preview'da dogruladigimiz
        # icin burada yalnizca kullanicinin ekledigi kopyalarin makul sinirini koru.
        if len(rows) > 500:
            raise ValueError("tek planda en fazla 500 fiziksel satir olabilir")
        revised_step = copy.deepcopy(original)
        revised_step.meta["records"] = copy.deepcopy(rows)
        revised = Plan(
            op_id=self.id,
            steps=[revised_step],
            warnings=_warnings_for_rows(rows),
            meta=_plan_meta(str(original.meta["discipline"]), source_data, rows),
        )
        revised.steps[0].meta["startRow"] = start_row
        return revised

    def apply(self, plan: Plan) -> Result:
        if len(plan.steps) != 1 or plan.steps[0].kind != "workbook_update":
            return Result(failed=1, errors=[Warning("error.step_failed", "gecersiz plan")])
        step = plan.steps[0]
        workbook_path = pathlib.Path(str(step.src))
        backup_path = pathlib.Path(str(step.dst))
        discipline = str(step.meta["discipline"])
        start_row = int(step.meta["startRow"])
        rows = list(step.meta["records"])
        signature = dict(step.meta["sourceSignature"])
        presets = dict(step.meta["presets"])
        result = Result()
        temp_path = workbook_path.with_name(
            f".{workbook_path.name}.{plan.plan_id[:8]}.docflow.tmp"
        )

        try:
            if _signature(workbook_path) != signature:
                raise RuntimeError(
                    "RFI Log onizlemeden sonra degismis. Yeni onizleme olusturun."
                )
            _assert_not_locked(workbook_path)
            backup_path.parent.mkdir(parents=True, exist_ok=True)
            if backup_path.exists():
                raise FileExistsError(str(backup_path))
            shutil.copy2(workbook_path, backup_path)

            restore_step = Step(
                "restore_backup",
                src=str(backup_path),
                dst=str(workbook_path),
                note_key="note.rfi.restoreBackup",
                meta={"originalSha256": signature["sha256"]},
            )
            entry = JournalEntry(op_id=self.id, steps=[restore_step])
            result.journal_id = entry.journal_id
            write_journal(entry)  # journal ONCE, workbook degisikligi SONRA

            notify("progress", {"phase": "apply", "done": 0, "total": 3})
            _write_records(
                workbook_path,
                temp_path,
                discipline,
                start_row,
                rows,
                presets,
            )
            notify("progress", {"phase": "apply", "done": 1, "total": 3})
            os.replace(temp_path, workbook_path)
            _verify_records(workbook_path, discipline, start_row, rows)
            notify("progress", {"phase": "apply", "done": 2, "total": 3})

            restore_step.meta["appliedSha256"] = file_sha256(workbook_path)
            write_journal(entry)
            result.applied = len(rows)
            notify("progress", {"phase": "apply", "done": 3, "total": 3})
            return result
        except Exception as exc:  # noqa: BLE001
            temp_path.unlink(missing_ok=True)
            # Atomik replace sonrasi dogrulama patlarsa orijinali hemen geri koy.
            try:
                if (
                    backup_path.is_file()
                    and workbook_path.is_file()
                    and file_sha256(workbook_path) != signature["sha256"]
                ):
                    restore_temp = workbook_path.with_name(
                        f".{workbook_path.name}.docflow-restore.tmp"
                    )
                    shutil.copy2(backup_path, restore_temp)
                    os.replace(restore_temp, workbook_path)
            except Exception:
                pass
            result.failed = 1
            result.errors.append(Warning("error.step_failed", str(exc)))
            return result

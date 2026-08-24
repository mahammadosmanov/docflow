"""Cekirdek sozlesmeler. Her ozellik bunlari implement eder — istisnasiz.

Bu dosya uygulamanin tamamini tasiyan soyutlama. Degistirmeden once dusun:
buradaki her ekleme, var olan ve gelecekteki TUM operation'lari etkiler.
"""

from __future__ import annotations

import uuid
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Literal


# --------------------------------------------------------------------------
# Parametre semasi — UI BUNDAN uretilir. Operation'a ozel form YAZILMAZ.
# --------------------------------------------------------------------------

class ParamKind(str, Enum):
    FOLDER = "folder"
    FILE = "file"
    FILES = "files"
    TEXT = "text"
    BOOL = "bool"
    CHOICE = "choice"
    INT = "int"
    LONG_TEXT = "longText"
    SECRET = "secret"
    SHEET_COUNTS = "sheetCounts"


@dataclass(frozen=True)
class Param:
    name: str
    kind: ParamKind
    label_key: str                        # i18n anahtari — DUZ METIN DEGIL
    default: Any = None
    required: bool = True
    choices: list[str] = field(default_factory=list)   # kind == CHOICE icin
    choice_labels: dict[str, str] = field(default_factory=dict)
    visible_when: dict[str, Any] = field(default_factory=dict)
    help_key: str | None = None

    def to_json(self) -> dict:
        return {
            "name": self.name,
            "kind": self.kind.value,
            "labelKey": self.label_key,
            "default": self.default,
            "required": self.required,
            "choices": self.choices,
            "choiceLabels": self.choice_labels,
            "visibleWhen": self.visible_when,
            "helpKey": self.help_key,
        }


def FolderParam(name: str, label_key: str, **kw: Any) -> Param:
    return Param(name, ParamKind.FOLDER, label_key, **kw)


def FileParam(name: str, label_key: str, **kw: Any) -> Param:
    return Param(name, ParamKind.FILE, label_key, **kw)


def FilesParam(name: str, label_key: str, **kw: Any) -> Param:
    return Param(name, ParamKind.FILES, label_key, default=[], **kw)


def BoolParam(
    name: str, label_key: str, default: bool = False, **kw: Any
) -> Param:
    return Param(name, ParamKind.BOOL, label_key, default=default, required=False, **kw)


def TextParam(name: str, label_key: str, **kw: Any) -> Param:
    return Param(name, ParamKind.TEXT, label_key, **kw)


def ChoiceParam(
    name: str,
    label_key: str,
    choices: list[str],
    choice_labels: dict[str, str] | None = None,
    **kw: Any,
) -> Param:
    return Param(
        name,
        ParamKind.CHOICE,
        label_key,
        choices=choices,
        choice_labels=choice_labels or {},
        **kw,
    )


def IntParam(name: str, label_key: str, **kw: Any) -> Param:
    return Param(name, ParamKind.INT, label_key, **kw)


def LongTextParam(name: str, label_key: str, **kw: Any) -> Param:
    return Param(name, ParamKind.LONG_TEXT, label_key, **kw)


def SecretParam(name: str, label_key: str, **kw: Any) -> Param:
    return Param(name, ParamKind.SECRET, label_key, **kw)


def SheetCountsParam(
    name: str,
    label_key: str,
    choices: list[str],
    **kw: Any,
) -> Param:
    return Param(
        name,
        ParamKind.SHEET_COUNTS,
        label_key,
        choices=choices,
        **kw,
    )


@dataclass(frozen=True)
class ParamSchema:
    params: list[Param]

    def to_json(self) -> list[dict]:
        return [p.to_json() for p in self.params]

    def validate(self, values: dict) -> list[str]:
        """i18n anahtari listesi dondurur — bos ise gecerli."""
        errors = []
        for p in self.params:
            value = values.get(p.name)
            if (
                p.kind == ParamKind.SHEET_COUNTS
                and p.required
                and (
                    not isinstance(value, dict)
                    or not any(
                        isinstance(count, int) and count > 0
                        for count in value.values()
                    )
                )
            ):
                errors.append(f"error.param.required.{p.name}")
            elif p.required and not value:
                errors.append(f"error.param.required.{p.name}")
        return errors


# --------------------------------------------------------------------------
# Plan / Step — preview() bunu uretir, apply() SADECE bunu uygular
# --------------------------------------------------------------------------

StepKind = Literal[
    "mkdir",
    "move",
    "copy",
    "rename",
    "write",
    "workbook_update",
    "restore_backup",
    "print",
    "pdf_first_page_replace",
    "cleanup_temp",
]
JournalState = Literal["pending", "applied", "failed"]


@dataclass
class Step:
    kind: StepKind
    src: str | None = None
    dst: str | None = None
    note_key: str | None = None           # UI'da gosterilecek aciklama anahtari
    meta: dict = field(default_factory=dict)
    state: JournalState | None = None

    def to_json(self) -> dict:
        payload = {
            "kind": self.kind, "src": self.src, "dst": self.dst,
            "noteKey": self.note_key, "meta": self.meta,
        }
        if self.state is not None:
            payload["state"] = self.state
        return payload


@dataclass
class Warning:
    key: str                              # i18n anahtari
    subject: str = ""                     # dosya adi vb.

    def to_json(self) -> dict:
        return {"key": self.key, "subject": self.subject}


@dataclass
class Plan:
    """preview() ciktisi. apply() BUNU alir ve baska hicbir sey hesaplamaz.

    Bu ayrim pazarlik konusu degil: apply() icinde yeniden hesaplama yapmak,
    kullanicinin onayladigi sey ile yapilan sey arasinda sessiz bir fark acar.
    """
    op_id: str
    steps: list[Step] = field(default_factory=list)
    warnings: list[Warning] = field(default_factory=list)
    meta: dict = field(default_factory=dict)
    plan_id: str = field(default_factory=lambda: str(uuid.uuid4()))

    def to_json(self) -> dict:
        return {
            "planId": self.plan_id,
            "opId": self.op_id,
            "steps": [s.to_json() for s in self.steps],
            "warnings": [w.to_json() for w in self.warnings],
            "meta": self.meta,
        }


@dataclass
class Result:
    applied: int = 0
    failed: int = 0
    journal_id: str = ""
    errors: list[Warning] = field(default_factory=list)
    undoable: bool = False
    meta: dict[str, Any] = field(default_factory=dict)

    def to_json(self) -> dict:
        return {
            "applied": self.applied, "failed": self.failed,
            "journalId": self.journal_id,
            "errors": [e.to_json() for e in self.errors],
            "undoable": self.undoable,
            "meta": self.meta,
        }


@dataclass
class JournalEntry:
    """Geri alma kaydi. Undo = steps'i TERS SIRADA ters cevir."""
    op_id: str
    steps: list[Step]
    pending_step: Step | None = None
    undoable: bool = False
    journal_id: str = field(default_factory=lambda: str(uuid.uuid4()))
    ts: str = field(default_factory=lambda: datetime.now(timezone.utc).isoformat())

    def to_json(self) -> dict:
        payload: dict[str, Any] = {
            "journalId": self.journal_id, "opId": self.op_id, "ts": self.ts,
            "steps": [s.to_json() for s in self.steps],
            "undoable": self.undoable,
        }
        if self.pending_step is not None:
            payload["pendingStep"] = self.pending_step.to_json()
        return payload


# --------------------------------------------------------------------------
# Operation — tek sozlesme
# --------------------------------------------------------------------------

class Operation(ABC):
    id: str
    title_key: str
    description_key: str
    schema: ParamSchema

    @abstractmethod
    def preview(self, params: dict) -> Plan:
        """HICBIR SEYI DEGISTIRMEZ. Sadece ne olacagini hesaplar."""

    @abstractmethod
    def apply(self, plan: Plan) -> Result:
        """SADECE plan.steps'i uygular. Yeniden hesaplama YOK."""

    def revise(self, plan: Plan, payload: dict) -> Plan:
        """Return a new plan after validating user edits to an editable preview."""
        raise ValueError("this plan is not editable")

    def to_json(self) -> dict:
        return {
            "id": self.id,
            "titleKey": self.title_key,
            "descriptionKey": self.description_key,
            "schema": self.schema.to_json(),
        }

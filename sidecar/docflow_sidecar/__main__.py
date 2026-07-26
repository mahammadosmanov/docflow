"""Sidecar giris noktasi:  python -m docflow_sidecar"""
from __future__ import annotations

from . import ops  # noqa: F401  — @register'lari tetikler
from . import review as _review
from .clipboard import copy_plan_table as _copy_plan_table
from .contracts import Plan, Step, Warning
from .fs.apply import apply_steps as _apply_steps
from .fs.apply import get_journal as _get_journal
from .fs.apply import list_history as _list_history
from .fs.apply import undo as _undo_steps
from .registry import all_operations, get
from .rpc import serve

_PLANS: dict[str, Plan] = {}


def _list_operations(_: dict) -> list[dict]:
    return [op.to_json() for op in all_operations()]


def _preview(p: dict) -> dict:
    op = get(p["op"])
    if errors := op.schema.validate(p.get("params") or {}):
        return {"planId": None, "opId": p["op"], "steps": [],
                "warnings": [{"key": e, "subject": ""} for e in errors]}
    plan = op.preview(p.get("params") or {})
    _PLANS[plan.plan_id] = plan
    return plan.to_json()


def _apply(p: dict) -> dict:
    plan = _PLANS.get(p["planId"])
    if plan is None:
        raise KeyError("plan bulunamadi veya suresi doldu — preview'i tekrarla")
    try:
        op = get(plan.op_id)
    except KeyError:
        # Kayitli operasyona bagli olmayan plan (or. rfi.review) — dogrudan uygula.
        return _apply_steps(plan).to_json()
    return op.apply(plan).to_json()


def _revise(p: dict) -> dict:
    plan = _PLANS.get(p["planId"])
    if plan is None:
        raise KeyError("plan bulunamadi veya suresi doldu - preview'i tekrarla")
    revised = get(plan.op_id).revise(plan, p.get("payload") or {})
    _PLANS[revised.plan_id] = revised
    return revised.to_json()


def _copy_table(p: dict) -> dict:
    plan = _PLANS.get(p["planId"])
    if plan is None:
        raise KeyError("plan bulunamadi veya suresi doldu - preview'i tekrarla")
    return {"count": _copy_plan_table(plan)}


def _undo(p: dict) -> dict:
    # destFolder verilirse dosyalar orijinal kaynaga degil secilen klasore doner.
    # steps verilirse (bulut kaydi) yerel journal'a ihtiyac yok.
    return _undo_steps(
        p.get("journalId") or "",
        p.get("destFolder") or None,
        p.get("steps"),
    ).to_json()


def _history(p: dict) -> list[dict]:
    return _list_history(int(p.get("limit") or 50))


def _journal_get(p: dict) -> dict:
    return _get_journal(p["journalId"])


def _review_scan(p: dict) -> list[dict]:
    return _review.scan(p["dest"])


def _review_preview(p: dict) -> dict:
    plan = _review.preview(p["dest"], p.get("decisions") or [])
    _PLANS[plan.plan_id] = plan
    return plan.to_json()


def _ping(_: dict) -> dict:
    from . import __version__
    return {"ok": True, "version": __version__}


def main() -> None:
    serve({
        "ping": _ping,
        "list_operations": _list_operations,
        "preview": _preview,
        "plan_revise": _revise,
        "plan_copy_table": _copy_table,
        "apply": _apply,
        "undo": _undo,
        "review_scan": _review_scan,
        "review_preview": _review_preview,
        "history_list": _history,
        "journal_get": _journal_get,
    })


if __name__ == "__main__":
    main()

"""Sidecar giris noktasi:  python -m docflow_sidecar"""
from __future__ import annotations

from . import ops  # noqa: F401  — @register'lari tetikler
from .contracts import Plan, Step, Warning
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
    return get(plan.op_id).apply(plan).to_json()


def _undo(p: dict) -> dict:
    return _undo_steps(p["journalId"]).to_json()


def _ping(_: dict) -> dict:
    from . import __version__
    return {"ok": True, "version": __version__}


def main() -> None:
    serve({
        "ping": _ping,
        "list_operations": _list_operations,
        "preview": _preview,
        "apply": _apply,
        "undo": _undo,
    })


if __name__ == "__main__":
    main()

"""Framework-independent document automation engine.

The Tauri shell reaches this facade through the small JSON-lines bridge. The
exact preview/apply/undo contract is independent of React, Tauri, and IPC.
"""

from __future__ import annotations

from collections.abc import Iterable

from . import ops  # noqa: F401  -- trigger @register decorators
from . import review
from .clipboard import copy_plan_table, preview_plan_table
from .contracts import Operation, Plan, Result, Warning
from .fs.apply import apply_steps, get_journal, list_history, undo
from .progress import ProgressCallback, reporting
from .registry import all_operations, get


class Engine:
    def __init__(self) -> None:
        self._plans: dict[str, Plan] = {}

    def list_operations(self) -> list[Operation]:
        return all_operations()

    def preview(
        self,
        op_id: str,
        params: dict,
        *,
        progress: ProgressCallback | None = None,
    ) -> Plan:
        op = get(op_id)
        if errors := op.schema.validate(params):
            return Plan(
                op_id=op_id,
                warnings=[Warning(key=key) for key in errors],
            )
        with reporting(progress):
            plan = op.preview(params)
        self._plans[plan.plan_id] = plan
        return plan

    def apply(
        self,
        plan_id: str,
        *,
        progress: ProgressCallback | None = None,
    ) -> Result:
        plan = self._plans.get(plan_id)
        if plan is None:
            raise KeyError("plan not found or expired; preview again")
        with reporting(progress):
            try:
                operation = get(plan.op_id)
            except KeyError:
                return apply_steps(plan)
            return operation.apply(plan)

    def revise(self, plan_id: str, payload: dict) -> Plan:
        plan = self._plans.get(plan_id)
        if plan is None:
            raise KeyError("plan not found or expired; preview again")
        revised = get(plan.op_id).revise(plan, payload)
        self._plans[revised.plan_id] = revised
        return revised

    def copy_plan_table(self, plan_id: str, sheet: str | None = None) -> int:
        plan = self._plans.get(plan_id)
        if plan is None:
            raise KeyError("plan not found or expired; preview again")
        return copy_plan_table(plan, sheet)

    def preview_plan_table(self, plan_id: str, sheet: str | None = None) -> dict:
        plan = self._plans.get(plan_id)
        if plan is None:
            raise KeyError("plan not found or expired; preview again")
        return preview_plan_table(plan, sheet)

    def review_scan(self, destination: str) -> list[dict]:
        return review.scan(destination)

    def review_preview(self, destination: str, decisions: list[dict]) -> Plan:
        plan = review.preview(destination, decisions)
        self._plans[plan.plan_id] = plan
        return plan

    def undo(
        self,
        journal_id: str = "",
        *,
        destination: str | None = None,
        steps: Iterable[dict] | None = None,
    ) -> Result:
        serialized = list(steps) if steps is not None else None
        return undo(journal_id, destination, serialized)

    def history(self, limit: int = 50) -> list[dict]:
        return list_history(limit)

    def journal(self, journal_id: str) -> dict:
        return get_journal(journal_id)

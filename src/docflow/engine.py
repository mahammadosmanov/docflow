"""Framework-independent document automation engine.

The Tauri shell reaches this facade through the small JSON-lines bridge. The
exact preview/apply/undo contract is independent of React, Tauri, and IPC.
"""

from __future__ import annotations

import threading
from typing import cast

from . import ops  # noqa: F401  -- trigger @register decorators
from . import review
from .clipboard import copy_plan_table, preview_plan_table
from .contracts import Operation, Plan, Result, Warning
from .fs.apply import (
    apply_steps,
    get_journal,
    journal_undoable,
    list_history,
    undo,
)
from .progress import ProgressCallback, cancellable, reporting
from .registry import all_operations, get


class Engine:
    def __init__(self) -> None:
        self._plans: dict[str, Plan] = {}
        self._cancellations: dict[str, threading.Event] = {}
        self._lock = threading.RLock()
        self._apply_lock = threading.Lock()

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
        with self._lock:
            self._plans[plan.plan_id] = plan
        return plan

    def apply(
        self,
        plan_id: str,
        *,
        progress: ProgressCallback | None = None,
    ) -> Result:
        with self._lock:
            plan = self._plans.get(plan_id)
        if plan is None:
            raise KeyError("plan not found or expired; preview again")
        if not self._apply_lock.acquire(blocking=False):
            raise RuntimeError("another operation is already running")
        cancellation = threading.Event()
        with self._lock:
            self._cancellations[plan_id] = cancellation
        try:
            with reporting(progress), cancellable(cancellation.is_set):
                try:
                    operation = get(plan.op_id)
                except KeyError:
                    return apply_steps(plan)
                return operation.apply(plan)
        finally:
            with self._lock:
                self._cancellations.pop(plan_id, None)
            self._apply_lock.release()

    def cancel(self, plan_id: str) -> bool:
        with self._lock:
            cancellation = self._cancellations.get(plan_id)
        if cancellation is None:
            return False
        cancellation.set()
        return True

    def latest_print_queue(self) -> dict | None:
        from .ops.batch_print import latest_print_queue

        return latest_print_queue()

    def discard_print_queue(self, queue_id: str) -> dict:
        from .ops.batch_print import discard_print_queue

        return discard_print_queue(queue_id)

    def resume_print_queue(self, queue_id: str) -> Plan:
        operation = get("files.batch_print")
        resume = getattr(operation, "resume", None)
        if not callable(resume):
            raise RuntimeError("print queue recovery is unavailable")
        plan = cast(Plan, resume(queue_id))
        with self._lock:
            self._plans[plan.plan_id] = plan
        return plan

    def revise(self, plan_id: str, payload: dict) -> Plan:
        with self._lock:
            plan = self._plans.get(plan_id)
        if plan is None:
            raise KeyError("plan not found or expired; preview again")
        revised = get(plan.op_id).revise(plan, payload)
        with self._lock:
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

    def undo(self, journal_id: str = "") -> Result:
        return undo(journal_id)

    def history(self, limit: int = 50) -> list[dict]:
        return list_history(limit)

    def journal(self, journal_id: str) -> dict:
        return get_journal(journal_id)

    def journal_undoable(self, journal_id: str) -> bool:
        return journal_undoable(get_journal(journal_id))

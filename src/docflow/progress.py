"""Context-local progress reporting for long document operations.

The Tauri application runs the engine in a Python sidecar. A scoped callback
lets the sidecar stream progress events without coupling the document layer to
the transport, while concurrent operations cannot receive one another's events.
"""

from __future__ import annotations

from collections.abc import Callable, Iterator
from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass


@dataclass(frozen=True)
class Progress:
    phase: str
    done: int
    total: int
    subject: str = ""

    @property
    def fraction(self) -> float:
        return 0.0 if self.total <= 0 else self.done / self.total


ProgressCallback = Callable[[Progress], None]
_callback: ContextVar[ProgressCallback | None] = ContextVar(
    "docflow_progress_callback", default=None
)


@contextmanager
def reporting(callback: ProgressCallback | None) -> Iterator[None]:
    token = _callback.set(callback)
    try:
        yield
    finally:
        _callback.reset(token)


def notify(phase: str, done: int, total: int, subject: str = "") -> None:
    callback = _callback.get()
    if callback is not None:
        callback(Progress(phase=phase, done=done, total=total, subject=subject))

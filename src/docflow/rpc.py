"""Compatibility adapter for operation progress notifications.

DocFlow 0.5.4 operations publish the original JSON-RPC shaped progress calls.
The Tauri migration uses a context-local callback instead, so this module keeps
the operation sources unchanged while routing their events safely.
"""

from __future__ import annotations

from typing import Any

from .progress import notify as _notify


def notify(method: str, params: dict[str, Any]) -> None:
    if method != "progress":
        return
    _notify(
        str(params.get("phase") or ""),
        int(params.get("done") or 0),
        int(params.get("total") or 0),
        str(params.get("subject") or ""),
    )

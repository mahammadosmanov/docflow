"""stdin/stdout uzerinde satir-ayrimli JSON-RPC.

>>> TEK EN ONEMLI KURAL <<<
stdout SADECE JSON-RPC icindir. Tek bir print() protokolu bozar ve hata
mesaji vermez — istemci sessizce parse hatasi alir. Log -> stderr.

Bu dosya baslangicta sys.stdout'u kilitler; kaza eseri print() cagiran kod
RuntimeError alir. Sessiz bozulmadan iyidir.
"""
from __future__ import annotations

import json
import logging
import sys
import traceback
from typing import Any, Callable

log = logging.getLogger("docflow")

_real_stdout = sys.stdout


class _StdoutGuard:
    """Kaza eseri print()'i yakalar. Kural 3'un bekcisi — bkz. CLAUDE.md."""

    def write(self, s: str) -> int:
        if s.strip():
            raise RuntimeError(
                "stdout'a yazildi. Bu JSON-RPC kanali. Log icin stderr kullan.\n"
                + "".join(traceback.format_stack()[:-1])
            )
        return 0

    def flush(self) -> None:
        pass


def emit(obj: dict) -> None:
    _real_stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
    _real_stdout.flush()


def notify(method: str, params: dict) -> None:
    """Istemciye tek yonlu bildirim (ornek: ilerleme)."""
    emit({"method": method, "params": params})


def serve(handlers: dict[str, Callable[[dict], Any]]) -> None:
    logging.basicConfig(stream=sys.stderr, level=logging.INFO)
    sys.stdout = _StdoutGuard()  # type: ignore[assignment]

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError as e:
            emit({"id": None, "error": {"code": -32700, "message": str(e)}})
            continue

        rid, method = req.get("id"), req.get("method")
        try:
            if method not in handlers:
                raise KeyError(f"unknown method: {method}")
            emit({"id": rid, "result": handlers[method](req.get("params") or {})})
        except Exception as e:  # noqa: BLE001 — sidecar asla olmemeli
            log.exception("handler failed: %s", method)
            emit({"id": rid, "error": {"code": -32000, "message": f"{type(e).__name__}: {e}"}})

from __future__ import annotations

import io
import json

from docflow_sidecar import rpc


class _BinarySink:
    def __init__(self) -> None:
        self.buffer = io.BytesIO()


def test_emit_uses_ascii_safe_json_and_restores_unicode(monkeypatch) -> None:
    sink = _BinarySink()
    monkeypatch.setattr(rpc, "_real_stdout", sink)

    expected = "Before concrete:\n•\tGrounding checked — sahə"
    rpc.emit({"id": 7, "result": {"description": expected}})
    payload = sink.buffer.getvalue()

    assert payload.isascii()
    assert b"\\u2022" in payload
    assert b"\\u0259" in payload
    assert json.loads(payload)["result"]["description"] == expected

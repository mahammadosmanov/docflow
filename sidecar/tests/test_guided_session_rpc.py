"""Strict JSON-RPC boundary tests for guided print sessions."""

from __future__ import annotations

import pytest

from docflow_sidecar import __main__ as sidecar_main


def test_guided_session_action_preserves_exact_revision(monkeypatch):
    seen: list[tuple[str, str, int]] = []
    monkeypatch.setattr(
        sidecar_main._batch_print,
        "update_guided_session",
        lambda session_id, action, revision: seen.append(
            (session_id, action, revision)
        )
        or {"ok": True},
    )

    result = sidecar_main._operation_session_action(
        {
            "op": "files.batch_print",
            "sessionId": "queue-1",
            "action": "stop",
            "revision": 4,
        }
    )

    assert result == {"ok": True}
    assert seen == [("queue-1", "stop", 4)]


@pytest.mark.parametrize("revision", [None, True, False, 1.9, "1", -1])
def test_guided_session_action_rejects_coerced_revisions(revision):
    with pytest.raises(ValueError, match="revision"):
        sidecar_main._operation_session_action(
            {
                "op": "files.batch_print",
                "sessionId": "queue-1",
                "action": "stop",
                "revision": revision,
            }
        )


def test_guided_session_recovery_flag_must_be_boolean():
    with pytest.raises(ValueError, match="recover"):
        sidecar_main._operation_session(
            {"op": "files.batch_print", "recover": "false"}
        )

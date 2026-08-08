"""Batch printing plans are deterministic and never print during tests."""

from __future__ import annotations

import json
import pathlib

import pytest

from docflow_sidecar.contracts import FilesParam, Plan
from docflow_sidecar.ops import batch_print


def _operation(monkeypatch) -> batch_print.BatchPrint:
    monkeypatch.setattr(
        batch_print,
        "_printer_inventory",
        lambda: (["Office Printer", "Backup Printer"], "Office Printer"),
    )
    monkeypatch.setattr(batch_print, "notify", lambda *_: None)
    monkeypatch.setattr(
        batch_print,
        "_resolve_print_verb",
        lambda path, printer, default: "printto"
        if path.suffix.lower() != ".unsupported"
        else "",
    )
    return batch_print.BatchPrint()


def test_files_param_serializes_as_multi_file_input():
    value = FilesParam(
        "files",
        "param.print.files",
        item_choices=["A4", "A3"],
        item_default="A4",
    ).to_json()

    assert value["kind"] == "files"
    assert value["default"] == []
    assert value["required"] is True
    assert value["itemChoices"] == ["A4", "A3"]
    assert value["itemDefault"] == "A4"


def test_preview_preserves_each_files_requested_paper_size(monkeypatch):
    first = pathlib.Path(r"C:\Print tests\first.pdf")
    second = pathlib.Path(r"C:\Print tests\second.docx")
    monkeypatch.setattr(pathlib.Path, "is_file", lambda _: True)

    plan = _operation(monkeypatch).preview(
        {
            "files": [
                {"path": str(first), "option": "A4"},
                {"path": str(second), "option": "A3"},
            ],
            "printer": "Office Printer",
            "copies": 1,
        }
    )

    assert [step.meta["paperSize"] for step in plan.steps] == ["A4", "A3"]
    assert [job["paperSize"] for job in plan.meta["jobs"]] == ["A4", "A3"]


def test_preview_preserves_order_and_deduplicates_guided_files(monkeypatch):
    first = pathlib.Path(r"C:\Print tests\first.pdf")
    second = pathlib.Path(r"C:\Print tests\second.docx")
    unsupported = pathlib.Path(r"C:\Print tests\notes.unsupported")
    missing = pathlib.Path(r"C:\Print tests\missing.xlsx")
    existing = {first, second, unsupported}
    monkeypatch.setattr(
        pathlib.Path,
        "is_file",
        lambda path: path in existing,
    )

    plan = _operation(monkeypatch).preview(
        {
            "files": [
                str(first),
                str(second),
                str(first),
                str(missing),
                str(unsupported),
            ],
            "printer": "Office Printer",
            "copies": 2,
            "collate": True,
        }
    )

    assert [step.src for step in plan.steps] == [
        str(first),
        str(second),
        str(unsupported),
    ]
    assert [job["path"] for job in plan.meta["jobs"]] == [
        str(first),
        str(second),
        str(unsupported),
    ]
    assert plan.meta["fileCount"] == 3
    assert plan.meta["jobCount"] == 3
    assert {warning.key for warning in plan.warnings} == {
        "warn.print.fileMissing",
        "warn.print.nonUndoable",
    }


def test_preview_guided_queue_carries_copies_as_an_instruction(monkeypatch):
    first = pathlib.Path(r"C:\Print tests\first.pdf")
    second = pathlib.Path(r"C:\Print tests\second.xlsx")
    monkeypatch.setattr(pathlib.Path, "is_file", lambda _: True)

    plan = _operation(monkeypatch).preview(
        {
            "files": [str(first), str(second)],
            "copies": 2,
        }
    )

    assert [job["path"] for job in plan.meta["jobs"]] == [
        str(first),
        str(second),
    ]
    assert [job["copies"] for job in plan.meta["jobs"]] == [2, 2]


def test_preview_guided_does_not_require_a_printer_or_shell_print_verb(monkeypatch):
    path = pathlib.Path(r"C:\Print tests\first.pdf")
    monkeypatch.setattr(batch_print, "_printer_inventory", lambda: ([], ""))
    monkeypatch.setattr(pathlib.Path, "is_file", lambda _: True)
    monkeypatch.setattr(batch_print, "_resolve_print_verb", lambda *_: "")
    operation = batch_print.BatchPrint()

    plan = operation.preview(
        {"files": [{"path": str(path), "option": "A2"}], "copies": 3}
    )

    assert [parameter.name for parameter in operation.schema.params] == [
        "files",
        "copies",
    ]
    assert [step.src for step in plan.steps] == [str(path)]
    assert plan.meta["jobs"] == [
        {"path": str(path), "format": "PDF", "paperSize": "A2", "copies": 3}
    ]
    assert "warn.print.noPrinters" not in {warning.key for warning in plan.warnings}
    assert "warn.print.noHandler" not in {warning.key for warning in plan.warnings}


def test_zero_copies_is_rejected(monkeypatch):
    plan = _operation(monkeypatch).preview(
        {
            "files": [r"C:\Print tests\first.pdf"],
            "printer": "Office Printer",
            "copies": 0,
        }
    )

    assert not plan.steps
    assert [warning.key for warning in plan.warnings] == [
        "warn.print.invalidCopies"
    ]


def test_spooler_monitor_waits_for_job_to_appear_and_finish(monkeypatch):
    monitor = object.__new__(batch_print._PrintQueueMonitor)
    monitor._baseline = {7}
    snapshots = iter(
        [
            {
                7: batch_print._SpoolJob(7, "other.pdf", 0),
                9: batch_print._SpoolJob(9, "unrelated.docx", 0),
            },
            {
                7: batch_print._SpoolJob(7, "other.pdf", 0),
                9: batch_print._SpoolJob(9, "unrelated.docx", 0),
                12: batch_print._SpoolJob(12, "first.pdf", 0),
            },
            {
                7: batch_print._SpoolJob(7, "other.pdf", 0),
                9: batch_print._SpoolJob(9, "unrelated.docx", 0),
            },
        ]
    )
    changes: list[float] = []
    monkeypatch.setattr(monitor, "_jobs", lambda: next(snapshots))
    monkeypatch.setattr(
        monitor,
        "_wait_change",
        lambda seconds: changes.append(seconds)
        or batch_print.PRINTER_CHANGE_ADD_JOB,
    )

    monitor.wait_for_completion("first.pdf")

    assert changes


def test_apply_starts_guided_session_without_submitting_print(monkeypatch):
    first = pathlib.Path(r"C:\Print tests\first.pdf")
    second = pathlib.Path(r"C:\Print tests\second.docx")
    operation = _operation(monkeypatch)
    started: list[list[dict]] = []
    monkeypatch.setattr(
        batch_print,
        "start_guided_session",
        lambda jobs: started.append(jobs) or {
            "sessionId": "queue-1",
            "revision": 1,
            "opId": operation.id,
            "status": "active",
            "current": 0,
            "total": len(jobs),
            "currentJob": jobs[0],
            "lastError": "",
        },
    )
    plan = Plan(
        op_id=operation.id,
        meta={
            "jobs": [
                {
                    "path": str(first),
                    "printer": "Office Printer",
                    "verb": "printto",
                },
                {
                    "path": str(second),
                    "printer": "Office Printer",
                    "verb": "printto",
                },
            ]
        },
    )

    result = operation.apply(plan)

    assert [job["path"] for job in started[0]] == [str(first), str(second)]
    assert result.applied == 0
    assert result.failed == 0
    assert result.journal_id == ""
    assert result.meta["guidedSession"]["sessionId"] == "queue-1"


def test_guided_session_persists_stop_continue_and_restart(tmp_path, monkeypatch):
    state_file = tmp_path / "print-session.json"
    opened: list[str] = []
    monkeypatch.setattr(batch_print, "PRINT_SESSION_FILE", state_file)
    monkeypatch.setattr(batch_print, "_open_document", lambda path: opened.append(str(path)))
    jobs = [
        {"path": r"C:\Docs\first.pdf", "paperSize": "A4", "copies": 1},
        {"path": r"C:\Docs\second.pdf", "paperSize": "A3", "copies": 2},
    ]

    started = batch_print.start_guided_session(jobs)
    session_id = started["sessionId"]
    assert opened == [r"C:\Docs\first.pdf"]
    assert batch_print.get_guided_session(session_id)["current"] == 0
    assert state_file.is_file()

    paused = batch_print.update_guided_session(
        session_id, "stop", started["revision"]
    )
    assert paused["status"] == "paused"
    resumed = batch_print.update_guided_session(
        session_id, "continue", paused["revision"]
    )
    assert resumed["status"] == "active"
    assert opened[-1] == r"C:\Docs\first.pdf"

    recovered = batch_print.get_guided_session(session_id, recover=True)
    assert recovered["status"] == "paused"
    resumed = batch_print.update_guided_session(
        session_id, "continue", recovered["revision"]
    )

    second = batch_print.update_guided_session(
        session_id, "complete", resumed["revision"]
    )
    assert second["current"] == 1
    assert second["currentJob"]["paperSize"] == "A3"
    assert opened[-1] == r"C:\Docs\second.pdf"

    paused_second = batch_print.update_guided_session(
        session_id, "stop", second["revision"]
    )
    restarted = batch_print.update_guided_session(
        session_id, "restart", paused_second["revision"]
    )
    assert restarted["current"] == 0
    assert opened[-1] == r"C:\Docs\first.pdf"

    complete = batch_print.update_guided_session(
        session_id, "complete", restarted["revision"]
    )
    complete = batch_print.update_guided_session(
        session_id, "complete", complete["revision"]
    )
    assert complete["status"] == "completed"
    assert complete["current"] == 2


def test_guided_session_survives_open_failure_for_retry(tmp_path, monkeypatch):
    state_file = tmp_path / "print-session.json"
    monkeypatch.setattr(batch_print, "PRINT_SESSION_FILE", state_file)
    monkeypatch.setattr(
        batch_print,
        "_open_document",
        lambda _: (_ for _ in ()).throw(OSError("no handler")),
    )

    session = batch_print.start_guided_session(
        [{"path": r"C:\Docs\first.pdf", "paperSize": "A4", "copies": 1}]
    )

    assert session["status"] == "paused"
    assert "no handler" in session["lastError"]
    restored = batch_print.get_guided_session(session["sessionId"])
    assert restored["current"] == 0
    assert restored["status"] == "paused"


def test_guided_session_rejects_stale_actions_and_queue_overwrite(
    tmp_path, monkeypatch
):
    state_file = tmp_path / "print-session.json"
    monkeypatch.setattr(batch_print, "PRINT_SESSION_FILE", state_file)
    monkeypatch.setattr(batch_print, "_open_document", lambda _: None)
    jobs = [
        {"path": r"C:\Docs\first.pdf", "paperSize": "A4", "copies": 1}
    ]

    started = batch_print.start_guided_session(jobs)
    with pytest.raises(RuntimeError, match="unfinished print session"):
        batch_print.start_guided_session(jobs)
    with pytest.raises(RuntimeError, match="not paused"):
        batch_print.update_guided_session(
            started["sessionId"], "continue", started["revision"]
        )

    stopped = batch_print.update_guided_session(
        started["sessionId"], "stop", started["revision"]
    )
    with pytest.raises(RuntimeError, match="not active"):
        batch_print.update_guided_session(
            started["sessionId"], "complete", stopped["revision"]
        )
    with pytest.raises(RuntimeError, match="print session changed"):
        batch_print.update_guided_session(
            started["sessionId"], "continue", started["revision"]
        )
    assert stopped["revision"] > started["revision"]
    assert not list(tmp_path.glob("*.tmp"))


@pytest.mark.parametrize(
    "change",
    [
        {"status": "completed", "current": 0},
        {"current": -1},
        {"opId": "other.operation"},
        {"jobs": [{"path": "relative.pdf", "paperSize": "A4", "copies": 1}]},
        {"jobs": [{"path": "x", "paperSize": "A9", "copies": 1}]},
        {"jobs": [{"path": "x", "paperSize": "A4", "copies": 0}]},
    ],
)
def test_guided_session_rejects_corrupt_persisted_state(
    tmp_path, monkeypatch, change
):
    state_file = tmp_path / "print-session.json"
    monkeypatch.setattr(batch_print, "PRINT_SESSION_FILE", state_file)
    state = {
        "sessionId": "queue-1",
        "revision": 2,
        "opId": "files.batch_print",
        "status": "paused",
        "current": 0,
        "jobs": [
            {"path": r"C:\Docs\first.pdf", "paperSize": "A4", "copies": 1}
        ],
    }
    state.update(change)
    state_file.write_text(json.dumps(state), encoding="utf-8")

    with pytest.raises(ValueError, match="invalid print session"):
        batch_print.get_guided_session("queue-1")
    with pytest.raises(ValueError, match="invalid print session"):
        batch_print.start_guided_session(
            [{"path": r"C:\Docs\new.pdf", "paperSize": "A4", "copies": 1}]
        )


def test_guided_session_rejects_more_than_max_files(tmp_path, monkeypatch):
    state_file = tmp_path / "print-session.json"
    monkeypatch.setattr(batch_print, "PRINT_SESSION_FILE", state_file)
    monkeypatch.setattr(batch_print, "_open_document", lambda _: None)
    jobs = [
        {
            "path": rf"C:\Docs\drawing-{index}.pdf",
            "paperSize": "A4",
            "copies": 1,
        }
        for index in range(batch_print.MAX_FILES + 1)
    ]

    with pytest.raises(ValueError, match="invalid guided print job"):
        batch_print.start_guided_session(jobs)
    with pytest.raises(ValueError, match="invalid guided print job"):
        batch_print.start_guided_session(["not-a-job"])  # type: ignore[list-item]
    accepted = batch_print.start_guided_session(jobs[: batch_print.MAX_FILES])
    assert accepted["total"] == batch_print.MAX_FILES

    state_file.write_text(
        json.dumps(
            {
                "sessionId": "queue-oversized",
                "revision": 1,
                "opId": "files.batch_print",
                "status": "paused",
                "current": 0,
                "jobs": jobs,
            }
        ),
        encoding="utf-8",
    )
    with pytest.raises(ValueError, match="invalid print session"):
        batch_print.get_guided_session("queue-oversized")

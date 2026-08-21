"""Batch printing plans are deterministic and never print during tests."""

from __future__ import annotations

import pathlib
import json

import pytest

from docflow.contracts import FilesParam, Plan
from docflow.ops import batch_print
from docflow.progress import cancellable


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
        lambda path, printer, default: (
            ""
            if path.suffix.lower() == ".unsupported"
            else "docflow"
            if path.suffix.lower() == ".pdf"
            else "printto"
        ),
    )
    return batch_print.BatchPrint()


def test_files_param_serializes_as_multi_file_input():
    value = FilesParam("files", "param.print.files").to_json()

    assert value["kind"] == "files"
    assert value["default"] == []
    assert value["required"] is True


def test_preview_preserves_order_deduplicates_and_collates(monkeypatch, tmp_path):
    first = tmp_path / "Print tests" / "first.pdf"
    second = tmp_path / "Print tests" / "second.docx"
    unsupported = tmp_path / "Print tests" / "notes.unsupported"
    missing = tmp_path / "Print tests" / "missing.xlsx"
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

    assert [step.src for step in plan.steps] == [str(first), str(second)]
    assert [job["path"] for job in plan.meta["jobs"]] == [
        str(first),
        str(second),
        str(first),
        str(second),
    ]
    assert plan.meta["fileCount"] == 2
    assert plan.meta["jobCount"] == 4
    assert {warning.key for warning in plan.warnings} == {
        "warn.print.fileMissing",
        "warn.print.noHandler",
        "warn.print.nonUndoable",
        "warn.print.settingsLimited",
    }


def test_preview_uncollated_prints_each_file_copies_together(monkeypatch, tmp_path):
    first = tmp_path / "Print tests" / "first.pdf"
    second = tmp_path / "Print tests" / "second.xlsx"
    monkeypatch.setattr(pathlib.Path, "is_file", lambda _: True)

    plan = _operation(monkeypatch).preview(
        {
            "files": [str(first), str(second)],
            "printer": "Backup Printer",
            "copies": 2,
            "collate": False,
        }
    )

    assert [job["path"] for job in plan.meta["jobs"]] == [
        str(first),
        str(first),
        str(second),
        str(second),
    ]


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


def test_spooler_monitor_accepts_fast_known_gdi_job(monkeypatch):
    monitor = object.__new__(batch_print._PrintQueueMonitor)
    monitor._baseline = set()
    monkeypatch.setattr(monitor, "_jobs", lambda: {})
    monkeypatch.setattr(
        monitor,
        "_wait_change",
        lambda _seconds: (_ for _ in ()).throw(
            AssertionError("completed jobs must not wait for acknowledgement")
        ),
    )

    monitor.wait_for_completion("DocFlow - first.pdf", job_id=42)


def test_spooler_timeout_cancels_known_job_and_marks_state_uncertain(
    monkeypatch,
):
    monitor = object.__new__(batch_print._PrintQueueMonitor)
    monitor._baseline = set()
    monkeypatch.setattr(
        monitor,
        "_jobs",
        lambda: {42: batch_print._SpoolJob(42, "first.pdf", 0)},
    )
    cancelled: list[set[int]] = []
    monkeypatch.setattr(monitor, "_cancel_jobs", cancelled.append)

    with pytest.raises(batch_print.PrintStateUncertain):
        monitor.wait_for_completion(
            "first.pdf",
            job_id=42,
            timeout_seconds=0,
        )

    assert cancelled == [{42}]


def test_apply_pauses_and_remembers_remaining_files(monkeypatch, tmp_path):
    monkeypatch.setenv("DOCFLOW_PRINT_QUEUE_DIR", str(tmp_path / "queues"))
    first = pathlib.Path(r"C:\Print tests\first.pdf")
    second = pathlib.Path(r"C:\Print tests\second.docx")
    operation = _operation(monkeypatch)
    submitted: list[tuple[str, str, str]] = []

    def fake_submit(path, printer, verb, settings):
        del settings
        submitted.append((str(path), printer, verb))
        if path == first:
            raise RuntimeError("simulated queue failure")

    monkeypatch.setattr(batch_print, "_submit_print_job", fake_submit)
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

    assert submitted == [
        (str(first), "Office Printer", "printto"),
    ]
    assert result.applied == 0
    assert result.failed == 1
    assert result.journal_id == ""
    assert result.meta["resumeAvailable"] is True
    assert result.meta["counts"]["pending"] == 1
    assert [error.key for error in result.errors] == [
        "error.print.failed",
        "error.print.queuePaused",
    ]


def test_revise_supports_per_file_settings_and_selection(monkeypatch, tmp_path):
    first = tmp_path / "first.pdf"
    second = tmp_path / "second.pdf"
    first.write_bytes(b"first")
    second.write_bytes(b"second")
    operation = _operation(monkeypatch)
    plan = operation.preview(
        {
            "files": [str(first), str(second)],
            "printer": "Office Printer",
            "copies": 2,
            "paper": "A4",
            "orientation": "auto",
            "scaling": "fit",
            "color": "printer",
            "duplex": "printer",
            "retries": 1,
            "on_error": "pause",
            "timeout_minutes": 30,
            "collate": True,
        }
    )

    rows = plan.meta["editor"]["rows"]
    rows[0]["enabled"] = False
    rows[1]["paper"] = "A3"
    rows[1]["orientation"] = "landscape"
    rows[1]["color"] = "grayscale"
    revised = operation.revise(plan, {"rows": rows})

    assert [step.src for step in revised.steps] == [str(second)]
    assert len(revised.meta["jobs"]) == 2
    assert {job["paper"] for job in revised.meta["jobs"]} == {"A3"}
    assert {job["orientation"] for job in revised.meta["jobs"]} == {
        "landscape"
    }
    assert {job["color"] for job in revised.meta["jobs"]} == {"grayscale"}


def test_saved_queue_resumes_failed_and_pending_without_completed(
    monkeypatch, tmp_path
):
    queue_dir = tmp_path / "queues"
    monkeypatch.setenv("DOCFLOW_PRINT_QUEUE_DIR", str(queue_dir))
    files = [tmp_path / name for name in ("one.pdf", "two.pdf", "three.pdf")]
    for path in files:
        path.write_bytes(path.name.encode())
    operation = _operation(monkeypatch)
    plan = operation.preview(
        {
            "files": [str(path) for path in files],
            "printer": "Office Printer",
            "copies": 1,
            "retries": 0,
            "on_error": "pause",
            "timeout_minutes": 30,
        }
    )
    submitted: list[str] = []

    def fake_submit(path, printer, verb, settings):
        del printer, verb, settings
        submitted.append(path.name)
        if path == files[1]:
            raise RuntimeError("paper jam")

    monkeypatch.setattr(batch_print, "_submit_print_job", fake_submit)
    result = operation.apply(plan)

    assert submitted == ["one.pdf", "two.pdf"]
    assert result.applied == 1
    assert result.failed == 1
    queue_id = str(result.meta["queueId"])
    saved = json.loads((queue_dir / f"{queue_id}.json").read_text())
    assert [job["status"] for job in saved["jobs"]] == [
        "completed",
        "failed",
        "pending",
    ]

    resumed = operation.resume(queue_id)
    resume_rows = resumed.meta["editor"]["rows"]
    assert [row["enabled"] for row in resume_rows] == [False, True, True]
    assert [job["path"] for job in resumed.meta["jobs"]] == [
        str(files[1]),
        str(files[2]),
    ]
    discarded = batch_print.discard_print_queue(queue_id)
    assert discarded["resumable"] == 0
    assert batch_print.latest_print_queue() is None


def test_cancel_before_next_job_keeps_everything_pending(monkeypatch, tmp_path):
    monkeypatch.setenv("DOCFLOW_PRINT_QUEUE_DIR", str(tmp_path / "queues"))
    operation = _operation(monkeypatch)
    plan = Plan(
        op_id=operation.id,
        meta={
            "jobs": [
                {
                    "path": str(tmp_path / "one.pdf"),
                    "printer": "Office Printer",
                    "verb": "docflow",
                }
            ]
        },
    )
    with cancellable(lambda: True):
        result = operation.apply(plan)

    assert result.applied == 0
    assert result.failed == 0
    assert result.meta["counts"]["pending"] == 1
    assert result.meta["resumeAvailable"] is True
    assert [warning.key for warning in result.errors] == [
        "warn.print.cancelled"
    ]


def test_uncertain_job_is_never_retried_automatically(monkeypatch, tmp_path):
    monkeypatch.setenv("DOCFLOW_PRINT_QUEUE_DIR", str(tmp_path / "queues"))
    operation = _operation(monkeypatch)
    attempts = 0

    def fake_submit(*_args):
        nonlocal attempts
        attempts += 1
        raise batch_print.PrintStateUncertain("spool state unknown")

    monkeypatch.setattr(batch_print, "_submit_print_job", fake_submit)
    plan = Plan(
        op_id=operation.id,
        meta={
            "jobs": [
                {
                    "path": str(tmp_path / "one.pdf"),
                    "printer": "Office Printer",
                    "verb": "docflow",
                    "retries": 3,
                }
            ]
        },
    )

    result = operation.apply(plan)

    assert attempts == 1
    assert result.failed == 0
    assert result.meta["counts"]["uncertain"] == 1
    assert result.meta["resumeAvailable"] is True
    assert [warning.key for warning in result.errors] == [
        "warn.print.stateUncertain"
    ]


def test_devmode_settings_do_not_change_printer_defaults():
    devmode = batch_print._DevModeW()
    batch_print._configure_devmode(
        devmode,
        {
            "paper": "A3",
            "color": "grayscale",
            "duplex": "short",
        },
        "landscape",
    )

    assert devmode.dmPaperSize == batch_print.PAPER_IDS["A3"]
    assert devmode.dmOrientation == batch_print.DMORIENT_LANDSCAPE
    assert devmode.dmColor == batch_print.DMCOLOR_MONOCHROME
    assert devmode.dmDuplex == batch_print.DMDUP_HORIZONTAL

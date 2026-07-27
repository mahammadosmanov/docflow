"""Batch printing plans are deterministic and never print during tests."""

from __future__ import annotations

import pathlib

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
    value = FilesParam("files", "param.print.files").to_json()

    assert value["kind"] == "files"
    assert value["default"] == []
    assert value["required"] is True


def test_preview_preserves_order_deduplicates_and_collates(monkeypatch):
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
    }


def test_preview_uncollated_prints_each_file_copies_together(monkeypatch):
    first = pathlib.Path(r"C:\Print tests\first.pdf")
    second = pathlib.Path(r"C:\Print tests\second.xlsx")
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


def test_apply_uses_exact_preview_jobs_and_continues_after_failure(
    monkeypatch,
):
    first = pathlib.Path(r"C:\Print tests\first.pdf")
    second = pathlib.Path(r"C:\Print tests\second.docx")
    operation = _operation(monkeypatch)
    submitted: list[tuple[str, str, str]] = []

    def fake_submit(path, printer, verb):
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
        (str(second), "Office Printer", "printto"),
    ]
    assert result.applied == 1
    assert result.failed == 1
    assert result.journal_id == ""
    assert result.errors[0].key == "error.print.failed"

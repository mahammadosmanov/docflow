"""apply/undo — dosya sistemine dokunan tek yer, en siki test edilen yer."""
import json
import pathlib
import pytest
from docflow.contracts import JournalEntry, Plan, Step
from docflow.fs import apply as fsapply
from docflow.progress import cancellable


@pytest.fixture(autouse=True)
def tmp_journal(tmp_path, monkeypatch):
    monkeypatch.setattr(fsapply, "JOURNAL_DIR", tmp_path / "journal")


def _plan(tmp_path):
    src = tmp_path / "in" / "scan.pdf"
    src.parent.mkdir(parents=True)
    src.write_bytes(b"%PDF-1.4 test")
    folder = tmp_path / "out" / "AZE-SUN-INT-QMS-RFI-MEC-0107"
    return src, folder, Plan(op_id="rfi.process_scans", steps=[
        Step("mkdir", dst=str(folder)),
        Step("move", str(src), str(folder / "AZE-SUN-INT-QMS-RFI-MEC-0107_R0.pdf")),
    ])


def test_apply_then_undo_is_lossless(tmp_path):
    src, folder, plan = _plan(tmp_path)
    r = fsapply.apply_steps(plan)
    assert r.failed == 0 and not src.exists()
    assert (folder / "AZE-SUN-INT-QMS-RFI-MEC-0107_R0.pdf").exists()

    u = fsapply.undo(r.journal_id)
    assert u.failed == 0
    assert src.exists() and src.read_bytes() == b"%PDF-1.4 test"
    assert not folder.exists()


def test_never_overwrites(tmp_path):
    src, folder, plan = _plan(tmp_path)
    folder.mkdir(parents=True)
    (folder / "AZE-SUN-INT-QMS-RFI-MEC-0107_R0.pdf").write_bytes(b"ONCEDEN VAR")

    r = fsapply.apply_steps(plan)
    assert r.failed == 2
    assert r.applied == 0
    assert (folder / "AZE-SUN-INT-QMS-RFI-MEC-0107_R0.pdf").read_bytes() == b"ONCEDEN VAR"
    assert src.exists()          # kaynak korundu


def test_copy_race_never_overwrites_a_destination_created_after_preflight(
    tmp_path,
    monkeypatch,
):
    source = tmp_path / "source.pdf"
    source.write_bytes(b"NEW")
    destination = tmp_path / "destination.pdf"
    original_exists = pathlib.Path.exists
    raced = False

    def create_destination_after_preflight(path: pathlib.Path) -> bool:
        nonlocal raced
        if path == destination and not raced:
            raced = True
            destination.write_bytes(b"KEEP")
            return False
        return original_exists(path)

    monkeypatch.setattr(
        pathlib.Path,
        "exists",
        create_destination_after_preflight,
    )
    result = fsapply.apply_steps(
        Plan(op_id="test", steps=[Step("copy", str(source), str(destination))])
    )

    assert result.applied == 0
    assert result.failed == 1
    assert destination.read_bytes() == b"KEEP"
    assert source.read_bytes() == b"NEW"


@pytest.mark.parametrize("kind", ["move", "rename"])
def test_move_race_never_overwrites_a_destination_created_after_preflight(
    tmp_path,
    monkeypatch,
    kind,
):
    source = tmp_path / "source.pdf"
    source.write_bytes(b"NEW")
    destination = tmp_path / "destination.pdf"
    original_exists = pathlib.Path.exists
    raced = False

    def create_destination_after_preflight(path: pathlib.Path) -> bool:
        nonlocal raced
        if path == destination and not raced:
            raced = True
            destination.write_bytes(b"KEEP")
            return False
        return original_exists(path)

    def overwrite_like_posix(source_path: str, destination_path: str) -> None:
        pathlib.Path(source_path).replace(destination_path)

    monkeypatch.setattr(
        pathlib.Path,
        "exists",
        create_destination_after_preflight,
    )
    monkeypatch.setattr(fsapply.shutil, "move", overwrite_like_posix)
    result = fsapply.apply_steps(
        Plan(op_id="test", steps=[Step(kind, str(source), str(destination))])
    )

    assert result.applied == 0
    assert result.failed == 1
    assert destination.read_bytes() == b"KEEP"
    assert source.read_bytes() == b"NEW"


@pytest.mark.parametrize("kind", ["copy", "move", "rename"])
def test_file_identity_is_durable_while_the_step_is_pending(
    tmp_path,
    monkeypatch,
    kind,
):
    source = tmp_path / "source.pdf"
    source.write_bytes(b"EXPECTED")
    destination = tmp_path / "destination.pdf"
    journal_dir = tmp_path / "journal"
    monkeypatch.setattr(fsapply, "JOURNAL_DIR", journal_dir)
    expected_hash = fsapply.file_sha256(source)
    observed: dict[str, object] = {}
    original_do = fsapply._do

    def inspect_pending(step: Step) -> None:
        journal_file = next(journal_dir.glob("*.json"))
        payload = json.loads(journal_file.read_text(encoding="utf-8"))
        observed["stepHash"] = step.meta.get("undoSha256")
        observed["pending"] = payload.get("pendingStep")
        original_do(step)

    monkeypatch.setattr(fsapply, "_do", inspect_pending)
    result = fsapply.apply_steps(
        Plan(op_id="test", steps=[Step(kind, str(source), str(destination))])
    )

    assert result.failed == 0
    assert observed["stepHash"] == expected_hash
    pending = observed["pending"]
    assert isinstance(pending, dict)
    assert pending["state"] == "pending"
    assert pending["meta"]["undoSha256"] == expected_hash


def test_undo_never_touches_destination_of_failed_step(tmp_path):
    src = tmp_path / "source.pdf"
    src.write_bytes(b"NEW")
    destination = tmp_path / "existing.pdf"
    destination.write_bytes(b"KEEP")
    plan = Plan(
        op_id="test",
        steps=[Step("copy", str(src), str(destination))],
    )

    result = fsapply.apply_steps(plan)
    assert result.failed == 1

    with pytest.raises(RuntimeError, match="journal is not safely undoable"):
        fsapply.undo(result.journal_id)
    assert destination.read_bytes() == b"KEEP"
    assert src.read_bytes() == b"NEW"


def test_all_failed_journal_is_explicitly_non_undoable(tmp_path):
    source = tmp_path / "source.pdf"
    source.write_bytes(b"NEW")
    destination = tmp_path / "existing.pdf"
    destination.write_bytes(b"KEEP")

    result = fsapply.apply_steps(
        Plan(op_id="test", steps=[Step("copy", str(source), str(destination))])
    )
    journal = fsapply.get_journal(result.journal_id)

    assert result.undoable is False
    assert result.to_json()["undoable"] is False
    assert [step["state"] for step in journal["steps"]] == ["failed"]
    assert journal["undoable"] is False
    assert fsapply.journal_undoable(journal) is False
    with pytest.raises(RuntimeError, match="journal is not safely undoable"):
        fsapply.undo(result.journal_id)
    assert destination.read_bytes() == b"KEEP"


def test_mixed_journal_undo_reverses_only_applied_steps(tmp_path):
    first_source = tmp_path / "first.pdf"
    first_source.write_bytes(b"FIRST")
    failed_source = tmp_path / "failed.pdf"
    failed_source.write_bytes(b"FAILED")
    last_source = tmp_path / "last.pdf"
    last_source.write_bytes(b"LAST")
    first_output = tmp_path / "first-output.pdf"
    existing_output = tmp_path / "existing.pdf"
    existing_output.write_bytes(b"KEEP")
    last_output = tmp_path / "last-output.pdf"

    result = fsapply.apply_steps(
        Plan(
            op_id="test",
            steps=[
                Step("copy", str(first_source), str(first_output)),
                Step("copy", str(failed_source), str(existing_output)),
                Step("copy", str(last_source), str(last_output)),
            ],
        )
    )
    journal = fsapply.get_journal(result.journal_id)

    assert result.undoable is True
    assert result.to_json()["undoable"] is True
    assert [step["state"] for step in journal["steps"]] == [
        "applied",
        "failed",
        "applied",
    ]
    assert journal["undoable"] is True
    undone = fsapply.undo(result.journal_id)

    assert undone.applied == 2
    assert undone.failed == 0
    assert not first_output.exists()
    assert existing_output.read_bytes() == b"KEEP"
    assert not last_output.exists()


def test_undo_never_overwrites_recreated_source(tmp_path):
    src, folder, plan = _plan(tmp_path)
    result = fsapply.apply_steps(plan)
    destination = folder / "AZE-SUN-INT-QMS-RFI-MEC-0107_R0.pdf"
    src.write_bytes(b"CREATED LATER")

    undone = fsapply.undo(result.journal_id)

    assert undone.failed == 1
    assert src.read_bytes() == b"CREATED LATER"
    assert destination.read_bytes() == b"%PDF-1.4 test"


def test_undo_never_deletes_modified_copy(tmp_path):
    src = tmp_path / "source.pdf"
    src.write_bytes(b"ORIGINAL")
    destination = tmp_path / "copy.pdf"
    plan = Plan(
        op_id="test",
        steps=[Step("copy", str(src), str(destination))],
    )
    result = fsapply.apply_steps(plan)
    destination.write_bytes(b"USER EDIT")

    undone = fsapply.undo(result.journal_id)

    assert undone.failed == 1
    assert src.read_bytes() == b"ORIGINAL"
    assert destination.read_bytes() == b"USER EDIT"


def test_copy_undo_never_deletes_a_replacement_created_after_hash_check(
    tmp_path,
    monkeypatch,
):
    source = tmp_path / "source.pdf"
    source.write_bytes(b"APPLIED")
    destination = tmp_path / "output.pdf"
    result = fsapply.apply_steps(
        Plan(
            op_id="test",
            steps=[Step("copy", str(source), str(destination))],
        )
    )
    original_sha256 = fsapply._sha256
    original_rename = fsapply.os.rename
    swapped = False

    def replace_before_claim(source_path, destination_path):
        nonlocal swapped
        source_path = pathlib.Path(source_path)
        if source_path == destination and not swapped:
            destination.unlink()
            destination.write_bytes(b"USER REPLACEMENT")
            swapped = True
        return original_rename(source_path, destination_path)

    def replace_after_hash(path: pathlib.Path) -> str:
        nonlocal swapped
        digest = original_sha256(path)
        if path == destination and not swapped:
            destination.unlink()
            destination.write_bytes(b"USER REPLACEMENT")
            swapped = True
        return digest

    monkeypatch.setattr(fsapply, "_sha256", replace_after_hash)
    monkeypatch.setattr(fsapply.os, "rename", replace_before_claim)

    undone = fsapply.undo(result.journal_id)

    assert swapped is True
    assert undone.applied == 0
    assert undone.failed == 1
    assert destination.read_bytes() == b"USER REPLACEMENT"


def test_restore_undo_never_overwrites_a_replacement_created_after_hash_check(
    tmp_path,
    monkeypatch,
):
    monkeypatch.setattr(fsapply, "JOURNAL_DIR", tmp_path / "journal")
    backup = tmp_path / "workbook.backup.xlsx"
    backup.write_bytes(b"ORIGINAL")
    workbook = tmp_path / "workbook.xlsx"
    workbook.write_bytes(b"APPLIED")
    step = Step(
        "restore_backup",
        str(backup),
        str(workbook),
        meta={
            "appliedSha256": fsapply.file_sha256(workbook),
            "originalSha256": fsapply.file_sha256(backup),
        },
        state="applied",
    )
    entry = JournalEntry(op_id="test", steps=[step])
    fsapply.write_journal(entry)
    original_sha256 = fsapply._sha256
    original_rename = fsapply.os.rename
    swapped = False

    def replace_before_claim(source_path, destination_path):
        nonlocal swapped
        source_path = pathlib.Path(source_path)
        if source_path == workbook and not swapped:
            workbook.unlink()
            workbook.write_bytes(b"USER REPLACEMENT")
            swapped = True
        return original_rename(source_path, destination_path)

    def replace_after_hash(path: pathlib.Path) -> str:
        nonlocal swapped
        digest = original_sha256(path)
        if path == workbook and not swapped:
            workbook.unlink()
            workbook.write_bytes(b"USER REPLACEMENT")
            swapped = True
        return digest

    monkeypatch.setattr(fsapply, "_sha256", replace_after_hash)
    monkeypatch.setattr(fsapply.os, "rename", replace_before_claim)

    undone = fsapply.undo(entry.journal_id)

    assert swapped is True
    assert undone.applied == 0
    assert undone.failed == 1
    assert workbook.read_bytes() == b"USER REPLACEMENT"
    assert backup.read_bytes() == b"ORIGINAL"


def test_one_bad_step_does_not_abort_the_rest(tmp_path):
    src, folder, plan = _plan(tmp_path)
    plan.steps.insert(1, Step("move", str(tmp_path / "yok.pdf"), str(folder / "yok.pdf")))
    r = fsapply.apply_steps(plan)
    assert r.failed == 1 and r.applied == 2
    assert (folder / "AZE-SUN-INT-QMS-RFI-MEC-0107_R0.pdf").exists()


def test_undo_only_removes_empty_dirs(tmp_path):
    src, folder, plan = _plan(tmp_path)
    r = fsapply.apply_steps(plan)
    (folder / "kullanici_ekledi.txt").write_text("dokunma")
    fsapply.undo(r.journal_id)
    assert folder.exists() and (folder / "kullanici_ekledi.txt").exists()


def test_undo_never_removes_a_replaced_directory_entry(tmp_path):
    folder = tmp_path / "created"
    result = fsapply.apply_steps(
        Plan(op_id="test", steps=[Step("mkdir", dst=str(folder))])
    )
    folder.rmdir()
    folder.mkdir()

    undone = fsapply.undo(result.journal_id)

    assert undone.applied == 0
    assert undone.failed == 1
    assert folder.is_dir()


def test_undo_never_removes_a_directory_replaced_after_identity_check(
    tmp_path,
    monkeypatch,
):
    folder = tmp_path / "created"
    result = fsapply.apply_steps(
        Plan(op_id="test", steps=[Step("mkdir", dst=str(folder))])
    )
    original_identity = fsapply.directory_entry_identity
    raced = False

    def replace_after_identity(path: pathlib.Path) -> tuple[int, int]:
        nonlocal raced
        identity = original_identity(path)
        if path == folder and not raced:
            raced = True
            folder.rmdir()
            folder.mkdir()
        return identity

    monkeypatch.setattr(
        fsapply,
        "directory_entry_identity",
        replace_after_identity,
    )
    undone = fsapply.undo(result.journal_id)

    assert undone.applied == 0
    assert undone.failed == 1
    assert folder.is_dir()


def test_get_journal_rejects_a_shortened_capability_id(tmp_path):
    entry = JournalEntry(op_id="test", steps=[])
    fsapply.write_journal(entry)

    with pytest.raises(ValueError, match="canonical journal ID"):
        fsapply.get_journal(entry.journal_id[:8])


def test_journal_validator_rejects_a_noncanonical_embedded_id(tmp_path):
    folder = tmp_path / "created"
    result = fsapply.apply_steps(
        Plan(op_id="test", steps=[Step("mkdir", dst=str(folder))])
    )
    journal = fsapply.get_journal(result.journal_id)
    journal["journalId"] = result.journal_id.upper()

    assert fsapply.journal_undoable(journal) is False


def test_undo_preflights_the_complete_journal_before_any_reversal(tmp_path):
    source = tmp_path / "source.pdf"
    source.write_bytes(b"ORIGINAL")
    target = tmp_path / "target.pdf"
    target.write_bytes(b"ORIGINAL")
    malformed_directory = tmp_path / "created"
    malformed_directory.mkdir()
    entry = JournalEntry(
        op_id="test",
        steps=[
            Step(
                "copy",
                src=str(source),
                dst=str(target),
                meta={"undoSha256": fsapply.file_sha256(target)},
            ),
            Step("mkdir", dst=str(malformed_directory)),
        ],
    )
    fsapply.write_journal(entry)

    with pytest.raises(RuntimeError, match="journal is not safely undoable"):
        fsapply.undo(entry.journal_id)

    assert target.read_bytes() == b"ORIGINAL"
    assert malformed_directory.is_dir()


def test_cancel_keeps_unstarted_steps_and_reports_pending(tmp_path):
    src, folder, plan = _plan(tmp_path)
    with cancellable(lambda: True):
        result = fsapply.apply_steps(plan)

    assert result.applied == 0
    assert result.failed == 0
    assert result.meta == {"cancelled": True, "pending": 2}
    assert [warning.key for warning in result.errors] == [
        "warn.operation.cancelled"
    ]
    assert src.exists()
    assert not folder.exists()

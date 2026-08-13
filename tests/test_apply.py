"""apply/undo — dosya sistemine dokunan tek yer, en siki test edilen yer."""
import pathlib
import pytest
from docflow.contracts import Plan, Step
from docflow.fs import apply as fsapply


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
    assert r.failed == 1
    assert (folder / "AZE-SUN-INT-QMS-RFI-MEC-0107_R0.pdf").read_bytes() == b"ONCEDEN VAR"
    assert src.exists()          # kaynak korundu


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

    undone = fsapply.undo(result.journal_id)
    assert undone.failed == 0
    assert undone.applied == 0
    assert destination.read_bytes() == b"KEEP"
    assert src.read_bytes() == b"NEW"


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

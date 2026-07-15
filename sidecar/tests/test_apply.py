"""apply/undo — dosya sistemine dokunan tek yer, en siki test edilen yer."""
import pathlib
import pytest
from docflow_sidecar.contracts import Plan, Step
from docflow_sidecar.fs import apply as fsapply


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

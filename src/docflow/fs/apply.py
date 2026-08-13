"""Plan uygulama + journal. Bu uygulamadaki EN TEHLIKELI dosya.

Kullanicinin belgelerini tasiyor. Bir veri kaybi projeyi oldurur. Kurallar:

  - apply_steps() plan.steps DISINDA hicbir sey yapmaz. Hesaplama YOK.
  - Her adim journal'a yazilir. Undo = ters sirada ters cevir.
  - Uzerine yazma YOK. Hedef varsa adim basarisiz olur ve raporlanir.
  - Journal, adim uygulanmadan ONCE ``pendingStep`` olarak yazilir. Yalnizca
    basarili adimlar geri alma listesindeki ``steps`` alanina girer. Boylece
    basarisiz bir adimin undo'su kullanicinin onceden var olan hedefine dokunmaz.
"""
from __future__ import annotations

import hashlib
import json
import os
import pathlib
import shutil

from ..contracts import JournalEntry, Plan, Result, Step, Warning
from ..progress import notify

JOURNAL_DIR = pathlib.Path.home() / ".docflow" / "journal"


def _copy_contents(
    source: pathlib.Path,
    destination: pathlib.Path,
    *,
    overwrite: bool = False,
) -> None:
    mode = "wb" if overwrite else "xb"
    with source.open("rb") as incoming, destination.open(mode) as outgoing:
        shutil.copyfileobj(incoming, outgoing, length=1024 * 1024)


def _sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _journal_path(entry: JournalEntry) -> pathlib.Path:
    JOURNAL_DIR.mkdir(parents=True, exist_ok=True)
    return JOURNAL_DIR / f"{entry.ts.replace(':', '-')}_{entry.journal_id[:8]}.json"


def _write_journal(entry: JournalEntry) -> None:
    _journal_path(entry).write_text(
        json.dumps(entry.to_json(), ensure_ascii=False, indent=2), encoding="utf-8"
    )


def write_journal(entry: JournalEntry) -> None:
    """Persist journals created by operation-specific atomic workflows."""
    _write_journal(entry)


def file_sha256(path: pathlib.Path) -> str:
    return _sha256(path)


def _do(step: Step) -> None:
    if step.kind == "mkdir":
        assert step.dst is not None
        pathlib.Path(step.dst).mkdir(parents=True, exist_ok=True)
        return

    assert step.src is not None and step.dst is not None
    src, dst = pathlib.Path(step.src), pathlib.Path(step.dst)
    if dst.exists():
        raise FileExistsError(str(dst))          # uzerine YAZMA
    dst.parent.mkdir(parents=True, exist_ok=True)

    if step.kind in ("move", "rename"):
        shutil.move(str(src), str(dst))
    elif step.kind == "copy":
        shutil.copy2(str(src), str(dst))
    else:
        raise ValueError(f"bilinmeyen adim: {step.kind}")


def _undo(step: Step, dest_folder: str | None = None) -> None:
    if step.kind == "restore_backup":
        assert step.src is not None and step.dst is not None
        backup = pathlib.Path(step.src)
        workbook = pathlib.Path(step.dst)
        if not backup.is_file():
            raise FileNotFoundError(str(backup))
        if dest_folder is not None:
            target = pathlib.Path(dest_folder) / workbook.name
            if target.exists():
                raise FileExistsError(str(target))
            target.parent.mkdir(parents=True, exist_ok=True)
            _copy_contents(backup, target)
            return

        expected = str(step.meta.get("appliedSha256") or "")
        if expected and workbook.is_file() and file_sha256(workbook) != expected:
            raise RuntimeError(
                "workbook changed after apply; undo stopped to preserve newer edits"
            )
        temp = workbook.with_name(f".{workbook.name}.docflow-undo.tmp")
        _copy_contents(backup, temp, overwrite=True)
        os.replace(temp, workbook)
        return

    # dest_folder verilirse: dosyalari orijinal kaynaga DEGIL, secilen klasore
    # geri getir. Dosya adi orijinal kaynak adini korur. Uzerine ASLA yazma.
    if step.kind == "mkdir":
        if dest_folder is not None:
            return                                 # yonlendirmede klasorlere dokunma
        assert step.dst is not None
        p = pathlib.Path(step.dst)
        if p.is_dir() and not any(p.iterdir()):   # sadece BOS klasoru sil
            p.rmdir()
        return

    if dest_folder is not None:
        assert step.src is not None and step.dst is not None
        target = pathlib.Path(dest_folder) / pathlib.Path(step.src).name
        if target.exists():
            raise FileExistsError(str(target))     # uzerine YAZMA
        source = pathlib.Path(step.dst)
        if not source.exists():
            raise FileNotFoundError(str(source))
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(source), str(target))       # move/rename/copy -> hepsi klasore
        return

    if step.kind in ("move", "rename"):
        assert step.src is not None and step.dst is not None
        source = pathlib.Path(step.dst)
        target = pathlib.Path(step.src)
        if target.exists():
            raise FileExistsError(str(target))     # kullanicinin yeni dosyasini ezme
        if not source.exists():
            raise FileNotFoundError(str(source))
        move_expected = step.meta.get("undoSha256")
        if isinstance(move_expected, str) and _sha256(source) != move_expected:
            raise RuntimeError(f"changed since apply: {source}")
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(source), str(target))
    elif step.kind == "copy":
        assert step.src is not None and step.dst is not None
        source = pathlib.Path(step.src)
        target = pathlib.Path(step.dst)
        if not target.exists():
            raise FileNotFoundError(str(target))
        copy_expected = step.meta.get("undoSha256")
        if isinstance(copy_expected, str):
            unchanged = _sha256(target) == copy_expected
        else:
            # Legacy journals predate stored fingerprints. A surviving source
            # still gives us a safe equality check; otherwise refuse deletion.
            unchanged = source.is_file() and _sha256(source) == _sha256(target)
        if not unchanged:
            raise RuntimeError(f"changed since apply: {target}")
        target.unlink()


def apply_steps(plan: Plan) -> Result:
    entry = JournalEntry(op_id=plan.op_id, steps=[])
    result = Result(journal_id=entry.journal_id)

    for i, step in enumerate(plan.steps):
        notify("apply", i, len(plan.steps))
        entry.pending_step = step
        _write_journal(entry)                    # ONCE pending, SONRA is
        try:
            _do(step)
            result.applied += 1
        except Exception as e:  # noqa: BLE001
            result.failed += 1
            result.errors.append(Warning("error.step_failed", f"{step.src or step.dst}: {e}"))
        else:
            if step.kind in ("move", "copy", "rename") and step.dst is not None:
                try:
                    step.meta["undoSha256"] = _sha256(pathlib.Path(step.dst))
                except OSError:
                    # A fingerprint is an additional undo guard. The original
                    # collision checks still keep move/rename non-overwriting.
                    pass
            entry.steps.append(step)
        finally:
            entry.pending_step = None
            _write_journal(entry)                # undo sadece basarili adimlari gorur

    return result


def _steps_from_json(raw: list[dict]) -> list[Step]:
    return [Step(**{k: v for k, v in s.items() if k != "noteKey"} | {"note_key": s.get("noteKey")})
            for s in raw]


def get_journal(journal_id: str) -> dict:
    """Journal kaydini oku (steps dahil). Bulut gecmisine yollamak icin de kullanilir."""
    hits = list(JOURNAL_DIR.glob(f"*_{journal_id[:8]}.json"))
    if not hits:
        raise KeyError(f"journal bulunamadi: {journal_id}")
    data: dict = json.loads(hits[0].read_text(encoding="utf-8"))
    return data


def undo(
    journal_id: str = "",
    dest_folder: str | None = None,
    steps: list[dict] | None = None,
) -> Result:
    """Geri al. steps verilirse ONU kullanir (bulut kaydindan, baska makinede de
    calisir); yoksa yerel journal'dan okur. dest_folder ile secilen klasore.
    """
    if steps is None:
        data = get_journal(journal_id)
        steps = data.get("steps", [])
        journal_id = data.get("journalId", journal_id)

    step_objs = _steps_from_json(steps)
    result = Result(journal_id=journal_id)
    for step in reversed(step_objs):             # TERS sira
        try:
            _undo(step, dest_folder)
            result.applied += 1
        except Exception as e:  # noqa: BLE001
            result.failed += 1
            result.errors.append(Warning("error.undo_failed", f"{step.dst}: {e}"))
    return result

def list_history(limit: int = 50) -> list[dict]:
    """Son journal kayitlarini (en yeni once) ozet olarak dondur — salt-okunur.

    Journal dosya adi ts ile basladigindan leksikografik siralama = kronolojik.
    """
    if not JOURNAL_DIR.is_dir():
        return []
    out: list[dict] = []
    for file in sorted(JOURNAL_DIR.glob("*.json"), reverse=True)[:limit]:
        try:
            data = json.loads(file.read_text(encoding="utf-8"))
        except Exception:  # noqa: BLE001 — bozuk tek kayit listeyi bozmaz
            continue
        steps = data.get("steps", [])
        moves = sum(
            1
            for step in steps
            if step.get("kind") in ("move", "copy", "rename", "restore_backup")
        )
        out.append(
            {
                "journalId": data.get("journalId", ""),
                "opId": data.get("opId", ""),
                "ts": data.get("ts", ""),
                "total": len(steps),
                "moves": moves,
            }
        )
    return out

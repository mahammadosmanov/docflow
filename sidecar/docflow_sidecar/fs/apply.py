"""Plan uygulama + journal. Bu uygulamadaki EN TEHLIKELI dosya.

Kullanicinin belgelerini tasiyor. Bir veri kaybi projeyi oldurur. Kurallar:

  - apply_steps() plan.steps DISINDA hicbir sey yapmaz. Hesaplama YOK.
  - Her adim journal'a yazilir. Undo = ters sirada ters cevir.
  - Uzerine yazma YOK. Hedef varsa adim basarisiz olur ve raporlanir.
  - Journal, adim uygulanmadan ONCE yazilir. Cokme aninda yarim is kalirsa
    journal'da fazladan kayit olur (zararsiz); tersi veri kaybidir.
"""
from __future__ import annotations

import json
import pathlib
import shutil

from ..contracts import JournalEntry, Plan, Result, Step, Warning
from ..rpc import notify

JOURNAL_DIR = pathlib.Path.home() / ".docflow" / "journal"


def _journal_path(entry: JournalEntry) -> pathlib.Path:
    JOURNAL_DIR.mkdir(parents=True, exist_ok=True)
    return JOURNAL_DIR / f"{entry.ts.replace(':', '-')}_{entry.journal_id[:8]}.json"


def _write_journal(entry: JournalEntry) -> None:
    _journal_path(entry).write_text(
        json.dumps(entry.to_json(), ensure_ascii=False, indent=2), encoding="utf-8"
    )


def _do(step: Step) -> None:
    if step.kind == "mkdir":
        pathlib.Path(step.dst).mkdir(parents=True, exist_ok=True)
        return

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
    # dest_folder verilirse: dosyalari orijinal kaynaga DEGIL, secilen klasore
    # geri getir. Dosya adi orijinal kaynak adini korur. Uzerine ASLA yazma.
    if step.kind == "mkdir":
        if dest_folder is not None:
            return                                 # yonlendirmede klasorlere dokunma
        p = pathlib.Path(step.dst)
        if p.is_dir() and not any(p.iterdir()):   # sadece BOS klasoru sil
            p.rmdir()
        return

    if dest_folder is not None:
        assert step.src is not None and step.dst is not None
        target = pathlib.Path(dest_folder) / pathlib.Path(step.src).name
        if target.exists():
            raise FileExistsError(str(target))     # uzerine YAZMA
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(step.dst, str(target))         # move/rename/copy -> hepsi klasore
        return

    if step.kind in ("move", "rename"):
        shutil.move(step.dst, step.src)
    elif step.kind == "copy":
        pathlib.Path(step.dst).unlink(missing_ok=True)


def apply_steps(plan: Plan) -> Result:
    entry = JournalEntry(op_id=plan.op_id, steps=[])
    result = Result(journal_id=entry.journal_id)

    for i, step in enumerate(plan.steps):
        notify("progress", {"phase": "apply", "done": i, "total": len(plan.steps)})
        entry.steps.append(step)
        _write_journal(entry)                    # ONCE journal, SONRA is
        try:
            _do(step)
            result.applied += 1
        except Exception as e:  # noqa: BLE001
            result.failed += 1
            result.errors.append(Warning("error.step_failed", f"{step.src or step.dst}: {e}"))

    return result


def undo(journal_id: str, dest_folder: str | None = None) -> Result:
    hits = list(JOURNAL_DIR.glob(f"*_{journal_id[:8]}.json"))
    if not hits:
        raise KeyError(f"journal bulunamadi: {journal_id}")

    data = json.loads(hits[0].read_text(encoding="utf-8"))
    steps = [Step(**{k: v for k, v in s.items() if k != "noteKey"} | {"note_key": s.get("noteKey")})
             for s in data["steps"]]

    result = Result(journal_id=journal_id)
    for step in reversed(steps):                 # TERS sira
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
    for f in sorted(JOURNAL_DIR.glob("*.json"), reverse=True)[:limit]:
        try:
            d = json.loads(f.read_text(encoding="utf-8"))
        except Exception:  # noqa: BLE001 — bozuk tek kayit listeyi bozmaz
            continue
        steps = d.get("steps", [])
        moves = sum(1 for s in steps if s.get("kind") in ("move", "copy", "rename"))
        out.append({
            "journalId": d.get("journalId", ""),
            "opId": d.get("opId", ""),
            "ts": d.get("ts", ""),
            "total": len(steps),
            "moves": moves,
        })
    return out

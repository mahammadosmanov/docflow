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

import copy
import errno
import hashlib
import json
import os
import pathlib
import shutil
import stat
import uuid
from typing import Callable, Literal, cast

from ..contracts import JournalEntry, Plan, Result, Step, Warning
from ..progress import cancellation_requested, notify

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
    entry.undoable = journal_entry_undoable(entry)
    _journal_path(entry).write_text(
        json.dumps(entry.to_json(), ensure_ascii=False, indent=2), encoding="utf-8"
    )


def write_journal(entry: JournalEntry) -> None:
    """Persist journals created by operation-specific atomic workflows."""
    _write_journal(entry)


def file_sha256(path: pathlib.Path) -> str:
    return _sha256(path)


def directory_entry_identity(directory: pathlib.Path) -> tuple[int, int]:
    """Return a stable identity only for a real, non-redirected directory."""

    entry = directory.lstat()
    attributes = int(getattr(entry, "st_file_attributes", 0))
    reparse_point = int(getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0))
    is_junction = bool(getattr(directory, "is_junction", lambda: False)())
    if (
        not stat.S_ISDIR(entry.st_mode)
        or directory.is_symlink()
        or is_junction
        or bool(attributes & reparse_point)
    ):
        raise ValueError("directory entry is redirected")
    return int(entry.st_dev), int(entry.st_ino)


_DIRECTORY_OWNER_XATTR = b"user.docflow.owner"


def create_directory_ownership_token(directory: pathlib.Path) -> str | None:
    """Mark a POSIX directory entry with a random ownership token."""
    if os.name == "nt":
        return None
    token = uuid.uuid4().hex
    setter = cast(Callable[..., None] | None, getattr(os, "setxattr", None))
    if setter is None:
        raise RuntimeError("directory ownership markers are unavailable")
    try:
        setter(
            directory,
            _DIRECTORY_OWNER_XATTR,
            token.encode("ascii"),
            follow_symlinks=False,
        )
    except (AttributeError, OSError) as error:
        raise RuntimeError("directory ownership markers are unavailable") from error
    return token


def directory_entry_owned(
    directory: pathlib.Path,
    expected_identity: tuple[int, int],
    expected_token: str | None,
) -> bool:
    if directory_entry_identity(directory) != expected_identity:
        return False
    if os.name == "nt":
        return expected_token is None
    if not isinstance(expected_token, str) or not expected_token:
        return False
    getter = cast(Callable[..., bytes] | None, getattr(os, "getxattr", None))
    if getter is None:
        return False
    try:
        value = getter(
            directory,
            _DIRECTORY_OWNER_XATTR,
            follow_symlinks=False,
        )
    except (AttributeError, OSError):
        return False
    return value == expected_token.encode("ascii")


def _file_entry_identity(path: pathlib.Path) -> tuple[int, int]:
    entry = path.lstat()
    attributes = int(getattr(entry, "st_file_attributes", 0))
    reparse_point = int(getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0))
    if (
        not stat.S_ISREG(entry.st_mode)
        or path.is_symlink()
        or bool(attributes & reparse_point)
    ):
        raise ValueError(f"file entry is redirected: {path}")
    return int(entry.st_dev), int(entry.st_ino)


def remove_owned_empty_directory(
    directory: pathlib.Path,
    expected_identity: tuple[int, int],
    expected_token: str | None,
) -> Literal["removed", "nonempty", "changed"]:
    if not directory_entry_owned(directory, expected_identity, expected_token):
        return "changed"

    quarantine = directory.with_name(
        f".{directory.name}.docflow-remove-{uuid.uuid4().hex}"
    )
    if os.path.lexists(quarantine):
        raise FileExistsError(str(quarantine))
    os.rename(directory, quarantine)

    def restore() -> None:
        if os.path.lexists(directory):
            raise RuntimeError(
                f"directory changed during cleanup; preserved at {quarantine}"
            )
        os.rename(quarantine, directory)

    try:
        if not directory_entry_owned(quarantine, expected_identity, expected_token):
            restore()
            return "changed"
        if any(quarantine.iterdir()):
            restore()
            return "nonempty"
        quarantine.rmdir()
        return "removed"
    except Exception:
        if os.path.lexists(quarantine) and not os.path.lexists(directory):
            restore()
        raise


def _copy_exclusive(source: pathlib.Path, destination: pathlib.Path) -> None:
    with source.open("rb") as incoming, destination.open("xb") as outgoing:
        shutil.copyfileobj(incoming, outgoing, length=1024 * 1024)


def _move_exclusive(source: pathlib.Path, destination: pathlib.Path) -> None:
    try:
        os.link(source, destination)
    except OSError as error:
        if error.errno != errno.EXDEV:
            raise
        _copy_exclusive(source, destination)
    source.unlink()


def _claim_file(path: pathlib.Path, expected_sha256: str) -> pathlib.Path:
    """Move a path aside atomically, then validate the claimed file entry."""
    quarantine = path.with_name(f".{uuid.uuid4().hex}.dfu")
    if os.path.lexists(quarantine):
        raise FileExistsError(str(quarantine))
    os.rename(path, quarantine)

    def restore() -> None:
        if os.path.lexists(path):
            raise RuntimeError(
                f"file changed during undo; preserved at {quarantine}"
            )
        os.rename(quarantine, path)

    try:
        _file_entry_identity(quarantine)
        if _sha256(quarantine) != expected_sha256:
            restore()
            raise RuntimeError(f"changed since apply: {path}")
        return quarantine
    except Exception:
        if os.path.lexists(quarantine) and not os.path.lexists(path):
            restore()
        raise


def _do(step: Step) -> None:
    if step.kind == "mkdir":
        assert step.dst is not None
        directory = pathlib.Path(step.dst)
        directory.mkdir(parents=True, exist_ok=False)
        device, inode = directory_entry_identity(directory)
        owner_token = create_directory_ownership_token(directory)
        step.meta["directoryDevice"] = device
        step.meta["directoryInode"] = inode
        if owner_token is not None:
            step.meta["directoryOwnerToken"] = owner_token
        return

    assert step.src is not None and step.dst is not None
    src, dst = pathlib.Path(step.src), pathlib.Path(step.dst)
    if dst.exists():
        raise FileExistsError(str(dst))          # uzerine YAZMA
    dst.parent.mkdir(parents=True, exist_ok=True)

    if step.kind in ("move", "rename"):
        _move_exclusive(src, dst)
    elif step.kind == "copy":
        _copy_exclusive(src, dst)
    else:
        raise ValueError(f"bilinmeyen adim: {step.kind}")


def _undo(step: Step) -> None:
    if step.kind == "restore_backup":
        assert step.src is not None and step.dst is not None
        backup = pathlib.Path(step.src)
        workbook = pathlib.Path(step.dst)
        if not backup.is_file():
            raise FileNotFoundError(str(backup))
        applied_expected = str(step.meta.get("appliedSha256") or "")
        original_expected = str(step.meta.get("originalSha256") or "")
        claimed = _claim_file(workbook, applied_expected)
        try:
            if _sha256(backup) != original_expected:
                raise RuntimeError("backup changed since apply")
            _copy_exclusive(backup, workbook)
            if _sha256(workbook) != original_expected:
                raise RuntimeError("restored workbook identity mismatch")
        except Exception:
            if os.path.lexists(workbook):
                failed_restore = workbook.with_name(
                    f".{uuid.uuid4().hex}.failed-restore"
                )
                os.rename(workbook, failed_restore)
            if os.path.lexists(claimed) and not os.path.lexists(workbook):
                os.rename(claimed, workbook)
            raise
        claimed.unlink()
        return

    if step.kind == "mkdir":
        assert step.dst is not None
        p = pathlib.Path(step.dst)
        expected_device = step.meta.get("directoryDevice")
        expected_inode = step.meta.get("directoryInode")
        expected_token = step.meta.get("directoryOwnerToken")
        if (
            not isinstance(expected_device, int)
            or isinstance(expected_device, bool)
            or not isinstance(expected_inode, int)
            or isinstance(expected_inode, bool)
            or (os.name != "nt" and not isinstance(expected_token, str))
            or (os.name == "nt" and expected_token is not None)
        ):
            raise RuntimeError(f"missing directory identity: {p}")
        removal = remove_owned_empty_directory(
            p,
            (expected_device, expected_inode),
            expected_token if isinstance(expected_token, str) else None,
        )
        if removal == "changed":
            raise RuntimeError(f"directory entry changed since apply: {p}")
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
        if not isinstance(move_expected, str):
            raise RuntimeError(f"missing content identity: {source}")
        claimed = _claim_file(source, move_expected)
        try:
            target.parent.mkdir(parents=True, exist_ok=True)
            _move_exclusive(claimed, target)
        except Exception:
            if os.path.lexists(claimed) and not os.path.lexists(source):
                os.rename(claimed, source)
            raise
    elif step.kind == "copy":
        assert step.src is not None and step.dst is not None
        source = pathlib.Path(step.src)
        target = pathlib.Path(step.dst)
        if not target.exists():
            raise FileNotFoundError(str(target))
        copy_expected = step.meta.get("undoSha256")
        if not isinstance(copy_expected, str):
            raise RuntimeError(f"missing content identity: {target}")
        claimed = _claim_file(target, copy_expected)
        claimed.unlink()


def apply_steps(plan: Plan) -> Result:
    entry = JournalEntry(op_id=plan.op_id, steps=[])
    result = Result(journal_id=entry.journal_id)
    _write_journal(entry)

    for i, step in enumerate(plan.steps):
        if cancellation_requested():
            result.meta = {
                "cancelled": True,
                "pending": len(plan.steps) - i,
            }
            result.errors.append(Warning("warn.operation.cancelled"))
            break
        notify("apply", i, len(plan.steps))
        journal_step = copy.deepcopy(step)
        journal_step.state = "pending"
        if journal_step.kind in ("move", "copy", "rename"):
            try:
                assert journal_step.src is not None
                journal_step.meta["undoSha256"] = _sha256(
                    pathlib.Path(journal_step.src)
                )
            except Exception as error:  # noqa: BLE001
                journal_step.state = "failed"
                entry.steps.append(journal_step)
                result.failed += 1
                result.errors.append(
                    Warning(
                        "error.step_failed",
                        f"{journal_step.src or journal_step.dst}: {error}",
                    )
                )
                _write_journal(entry)
                continue
        entry.pending_step = journal_step
        _write_journal(entry)                    # ONCE pending, SONRA is
        try:
            _do(journal_step)
            if (
                journal_step.kind in ("move", "copy", "rename")
                and journal_step.dst is not None
                and _sha256(pathlib.Path(journal_step.dst))
                != journal_step.meta["undoSha256"]
            ):
                raise RuntimeError("destination identity changed during apply")
            result.applied += 1
        except Exception as e:  # noqa: BLE001
            journal_step.state = "failed"
            entry.steps.append(journal_step)
            result.failed += 1
            result.errors.append(
                Warning(
                    "error.step_failed",
                    f"{journal_step.src or journal_step.dst}: {e}",
                )
            )
        else:
            journal_step.state = "applied"
            entry.steps.append(journal_step)
        finally:
            entry.pending_step = None
            _write_journal(entry)                # undo sadece basarili adimlari gorur

    result.undoable = entry.undoable
    return result


def _steps_from_json(raw: list[dict]) -> list[Step]:
    return [Step(**{k: v for k, v in s.items() if k != "noteKey"} | {"note_key": s.get("noteKey")})
            for s in raw]


def _sha256_identity(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def _undo_step_safe(step: Step) -> bool:
    if step.kind == "mkdir":
        device = step.meta.get("directoryDevice")
        inode = step.meta.get("directoryInode")
        owner_token = step.meta.get("directoryOwnerToken")
        return (
            step.dst is not None
            and isinstance(device, int)
            and not isinstance(device, bool)
            and isinstance(inode, int)
            and not isinstance(inode, bool)
            and (
                (os.name == "nt" and owner_token is None)
                or (os.name != "nt" and isinstance(owner_token, str))
            )
        )
    if step.kind in {"move", "rename", "copy"}:
        return (
            step.src is not None
            and step.dst is not None
            and _sha256_identity(step.meta.get("undoSha256"))
        )
    if step.kind == "restore_backup":
        return (
            step.src is not None
            and step.dst is not None
            and _sha256_identity(step.meta.get("appliedSha256"))
            and _sha256_identity(step.meta.get("originalSha256"))
        )
    return False


def journal_undoable(data: dict) -> bool:
    if data.get("undoable") is not True:
        return False
    journal_id = data.get("journalId")
    try:
        canonical_id = str(uuid.UUID(str(journal_id)))
    except (AttributeError, TypeError, ValueError):
        return False
    if journal_id != canonical_id:
        return False
    if data.get("pendingStep") is not None:
        return False
    raw_steps = data.get("steps")
    if not isinstance(raw_steps, list) or not raw_steps:
        return False
    try:
        steps = _steps_from_json(raw_steps)
    except (TypeError, ValueError):
        return False
    if any(step.state not in {"applied", "failed"} for step in steps):
        return False
    applied_steps = [step for step in steps if step.state == "applied"]
    return bool(applied_steps) and all(
        _undo_step_safe(step) for step in applied_steps
    )


def journal_entry_undoable(entry: JournalEntry) -> bool:
    if entry.pending_step is not None:
        return False
    if any(step.state not in {"applied", "failed"} for step in entry.steps):
        return False
    applied_steps = [step for step in entry.steps if step.state == "applied"]
    return bool(applied_steps) and all(
        _undo_step_safe(step) for step in applied_steps
    )


def get_journal(journal_id: str) -> dict:
    """Journal kaydini oku (steps dahil). Bulut gecmisine yollamak icin de kullanilir."""
    try:
        canonical = str(uuid.UUID(journal_id))
    except (AttributeError, TypeError, ValueError) as error:
        raise ValueError("canonical journal ID required") from error
    if canonical != journal_id:
        raise ValueError("canonical journal ID required")
    for path in JOURNAL_DIR.glob(f"*_{canonical[:8]}.json"):
        decoded: object = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(decoded, dict):
            continue
        data = decoded
        if data.get("journalId") == canonical:
            return data
    raise KeyError(f"journal bulunamadi: {canonical}")


def undo(journal_id: str = "") -> Result:
    """Reverse an exact local journal capability."""

    data = get_journal(journal_id)
    raw_steps = data.get("steps", [])
    steps = list(raw_steps) if isinstance(raw_steps, list) else []
    journal_id = str(data.get("journalId", journal_id))

    if not journal_undoable(data):
        raise RuntimeError("journal is not safely undoable")

    step_objs = [
        step for step in _steps_from_json(steps) if step.state == "applied"
    ]
    result = Result(journal_id=journal_id)
    for step in reversed(step_objs):             # TERS sira
        try:
            _undo(step)
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
        applied_steps = [
            step for step in steps if step.get("state") == "applied"
        ]
        moves = sum(
            1
            for step in applied_steps
            if step.get("kind") in ("move", "copy", "rename", "restore_backup")
        )
        out.append(
            {
                "journalId": data.get("journalId", ""),
                "opId": data.get("opId", ""),
                "ts": data.get("ts", ""),
                "total": len(applied_steps),
                "moves": moves,
            }
        )
    return out

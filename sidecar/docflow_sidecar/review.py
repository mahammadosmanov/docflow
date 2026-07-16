"""_review/ klasorunu yonet: dosya bazinda PUT (hedefe koy) / DELETE (cope at).

Guvenlik (sahibi: "kucuk bir hata bile cok tehlikeli"):
  - DELETE = geri donuslu cope (~/.docflow/trash/) TASIMA; asla hard-delete.
  - PUT = review dosyasini {dest}/{code}.pdf yap; hedefte dosya varsa onu once
    cope al (takas). Uzerine ASLA yazma.
  - Her ikisi de Plan uretir -> apply/journal/undo. "Preview olmadan apply yok"
    korunur: kullanici kararlari plandir, apply sadece plani uygular.
"""
from __future__ import annotations

import pathlib
from datetime import datetime, timezone

from .contracts import Plan, Step, Warning
from .extract import extract_first_page
from .fs.naming import target_for

REVIEW_DIR = "_review"
TRASH_DIR = pathlib.Path.home() / ".docflow" / "trash"


def scan(dest: str) -> list[dict]:
    """_review/ icindeki PDF'leri kodlariyla listele (SALT-OKUNUR, hicbir sey degismez)."""
    d = pathlib.Path(dest)
    rd = d / REVIEW_DIR
    out: list[dict] = []
    if not rd.is_dir():
        return out
    for f in sorted(rd.iterdir()):
        if not f.is_file() or f.name.startswith(".") or f.suffix.lower() != ".pdf":
            continue
        code, conf = "", 0.0
        try:
            m, _ = extract_first_page(str(f))
            if m.ok:
                code, conf = m.code, m.confidence
        except Exception:  # noqa: BLE001 — tek bozuk dosya listelemeyi durdurmaz
            pass
        tgt = target_for(d, code) if code else None
        out.append({
            "name": f.name,
            "path": str(f),
            "code": code,
            "confidence": conf,
            "target": (str(tgt) if tgt else None),
            "targetExists": bool(tgt and tgt.exists()),
        })
    return out


def _trash_path(name: str) -> pathlib.Path:
    ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%f")
    return TRASH_DIR / f"{ts}_{name}"


def preview(dest: str, decisions: list[dict]) -> Plan:
    """decisions: [{path, action: 'put'|'delete', code}]. apply/journal/undo edilebilir Plan."""
    d = pathlib.Path(dest)
    plan = Plan(op_id="rfi.review")

    for dec in decisions:
        src = pathlib.Path(dec.get("path", ""))
        action = dec.get("action")

        if action == "delete":
            # Geri donuslu cope; undo geri getirir.
            plan.steps.append(Step(
                "move", str(src), str(_trash_path(src.name)),
                note_key="note.deleted", meta={"action": "delete"},
            ))

        elif action == "put":
            code = (dec.get("code") or "").strip()
            if not code:
                plan.warnings.append(Warning("warn.no_code", src.name))
                continue
            target = target_for(d, code)
            if target.exists():
                # Yerinden edilen mevcut dosya -> cope (once bosalt, sonra yerlestir).
                plan.steps.append(Step(
                    "move", str(target), str(_trash_path(target.name)),
                    note_key="note.displaced", meta={"action": "put", "code": code},
                ))
            plan.steps.append(Step(
                "move", str(src), str(target),
                note_key="note.renamed", meta={"action": "put", "code": code},
            ))

        else:
            plan.warnings.append(Warning("warn.unreadable", src.name))

    return plan

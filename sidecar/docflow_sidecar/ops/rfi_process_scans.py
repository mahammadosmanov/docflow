"""MVP operation: taramalari RFI koduna gore adlandir + klasorle.

>>> BU DOSYA REFERANS IMPLEMENTASYONDUR. <<<
Her yeni operation bu sekli kopyalar. Once bunu oku, sonra yenisini yaz.

Dikkat edilecek dort sey:
  1. preview() dosya sistemine DOKUNMAZ — sadece okur ve Plan uretir.
  2. apply() HESAP YAPMAZ — sadece plan.steps'i uygular.
  3. Kod bulunamayan dosya _review/ altina gider. Asla tahmin, asla sessiz atlama.
  4. Kullaniciya giden her metin bir i18n anahtari.
"""

from __future__ import annotations

import pathlib

from ..contracts import (
    BoolParam,
    FolderParam,
    Operation,
    ParamSchema,
    Plan,
    Result,
    Step,
    Warning,
)
from ..extract import extract_first_page
from ..fs.apply import apply_steps
from ..registry import register
from ..rpc import notify

REVIEW_DIR = "_review"

# Bu esigin altindaki eslesmeler _review'a gider. fuzzy = 0.7-0.9 araligi.
# Dusurmeden once gercek taramalarda hata oranini ol.
MIN_CONFIDENCE = 0.80


@register("rfi.process_scans")
class ProcessScans(Operation):
    title_key = "op.rfi.process_scans.title"
    description_key = "op.rfi.process_scans.desc"
    schema = ParamSchema([
        FolderParam("source", "param.source", help_key="param.source.help"),
        FolderParam("dest", "param.dest", help_key="param.dest.help"),
        BoolParam("keep_original", "param.keep_original", default=False),
    ])

    def preview(self, params: dict) -> Plan:
        source = pathlib.Path(params["source"])
        dest = pathlib.Path(params["dest"])
        copy_mode = bool(params.get("keep_original"))

        plan = Plan(op_id=self.id)
        # Uzanti buyuk/kucuk harf duyarsiz: glob("*.pdf") "SCAN.PDF"i KACIRIR
        # (Python fnmatch, dosya sistemi case-insensitive olsa bile case-sensitive).
        # Gizli/nokta dosyalari (._foo.pdf gibi macOS AppleDouble) atla.
        pdfs = sorted(
            p for p in source.iterdir()
            if p.is_file() and p.suffix.lower() == ".pdf" and not p.name.startswith(".")
        )

        kind = "copy" if copy_mode else "move"

        # 1. GECIS: her dosyayi OCR et. Okunamayan/kod yok/dusuk guven -> hemen
        # _review. Kod bulunanlari koda gore GRUPLA (revizyon ele almak icin).
        groups: dict[str, list[tuple[pathlib.Path, float, float, str]]] = {}
        for i, pdf in enumerate(pdfs):
            notify("progress", {"phase": "preview", "done": i, "total": len(pdfs)})

            try:
                m, method = extract_first_page(str(pdf))
            except Exception as e:  # noqa: BLE001 — tek bozuk dosya tarama durdurmaz
                plan.warnings.append(Warning("warn.unreadable", pdf.name))
                plan.steps.append(Step("move", str(pdf), str(dest / REVIEW_DIR / pdf.name),
                                       note_key="note.unreadable", meta={"error": str(e)}))
                continue

            if not m.ok or m.confidence < MIN_CONFIDENCE:
                key = "warn.no_code" if not m.ok else "warn.low_confidence"
                plan.warnings.append(Warning(key, pdf.name))
                plan.steps.append(Step(
                    "move", str(pdf), str(dest / REVIEW_DIR / pdf.name),
                    note_key="note.needs_review",
                    meta={"confidence": m.confidence, "method": m.method, "source": method},
                ))
                continue

            try:
                mtime = pdf.stat().st_mtime
            except OSError:
                mtime = 0.0
            groups.setdefault(m.code, []).append((pdf, mtime, m.confidence, method))

        # 2. GECIS: her RFI numarasi icin EN YENI (mtime) -> {code}.pdf; eskiler
        # -> _review. Isim = RFI numarasi, suffix YOK; ayni ada iki dosya olamaz.
        for code, files in groups.items():
            files.sort(key=lambda t: t[1], reverse=True)   # en yeni once
            winner = files[0]
            plan.steps.append(Step(
                kind, str(winner[0]), str(dest / f"{code}.pdf"),
                note_key="note.renamed",
                meta={"code": code, "confidence": winner[2], "source": winner[3]},
            ))
            for older, _mt, _conf, _method in files[1:]:
                plan.warnings.append(Warning("warn.duplicate", older.name))
                plan.steps.append(Step(
                    kind, str(older), str(dest / REVIEW_DIR / older.name),
                    note_key="note.needs_review",
                    meta={"code": code, "reason": "older_revision", "kept": winner[0].name},
                ))

        if not pdfs:
            plan.warnings.append(Warning("warn.no_pdfs", str(source)))

        return plan

    def apply(self, plan: Plan) -> Result:
        return apply_steps(plan)

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

        # Dosya adi = RFI numarasi, baska hicbir sey (sahibi karari): AZE-...-0064.pdf
        # Ayni RFI numarasi ikinci kez gelirse ayni ad iki dosyaya verilemez;
        # suffix (_R0/_R1) YOK, uzerine yazma YOK, tahmin YOK -> _review'a alinir.
        seen: dict[str, str] = {}  # code -> ilk goruldugu dosya adi

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

            if m.code in seen:
                # Ayni numara ikinci kez -> ada koyulacak yer yok. _review'a al.
                plan.warnings.append(Warning("warn.duplicate", pdf.name))
                plan.steps.append(Step(
                    "copy" if copy_mode else "move",
                    str(pdf), str(dest / REVIEW_DIR / pdf.name),
                    note_key="note.needs_review",
                    meta={"code": m.code, "reason": "duplicate", "firstFile": seen[m.code]},
                ))
                continue
            seen[m.code] = pdf.name

            # DUZ yerlesim: {dest}/{code}.pdf — klasor yok, suffix yok.
            target = dest / f"{m.code}.pdf"
            plan.steps.append(Step(
                "copy" if copy_mode else "move", str(pdf), str(target),
                note_key="note.renamed",
                meta={"code": m.code, "confidence": m.confidence,
                      "method": m.method, "source": method},
            ))

        if not pdfs:
            plan.warnings.append(Warning("warn.no_pdfs", str(source)))

        return plan

    def apply(self, plan: Plan) -> Result:
        return apply_steps(plan)

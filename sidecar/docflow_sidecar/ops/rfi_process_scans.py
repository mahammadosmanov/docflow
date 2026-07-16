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
from ..extract import OcrEngineMissing, extract_first_page
from ..fs.apply import apply_steps
from ..fs.naming import folder_for, target_for
from ..registry import register
from ..rpc import notify

REVIEW_DIR = "_review"


def _same_file(a: pathlib.Path, b: pathlib.Path) -> bool:
    """a ile b AYNI dosya mi? (zaten islenmis dosyayi tekrar islememek icin)

    os.path.samefile SADECE ikisi de varken calisir; hedef yoksa FileNotFoundError
    atar. Bu yuzden once varlik kontrolu. Yol karsilastirmasi yerine samefile:
    macOS'ta dosya sistemi case-insensitive ve "A.pdf" ile "a.pdf" ayni dosyadir,
    ama string olarak esit degildir.
    """
    try:
        return b.exists() and a.samefile(b)
    except OSError:
        return False

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
            except OcrEngineMissing:
                # KURULUM hatasi — dosya hatasi DEGIL. Her dosya icin tekrarlanir,
                # yani "hepsi okunamadi" gibi gorunur. Plani BOSALT ve tek net
                # uyariyla don: eksik bir binary yuzunden kullanicinin butun
                # klasorunu _review'e tasiyan bir plan uretmek KABUL EDILEMEZ.
                # (Tam olarak bu oldu: 20 dosyanin 20'si "move -> _review".)
                return Plan(op_id=self.id, warnings=[Warning("warn.no_ocr", "")])
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

        # 2. GECIS: her RFI numarasi icin EN YENI (mtime) -> hedef; eskiler
        # -> _review. Ad = "RFI No. {code}.pdf"; ayni ada iki dosya olamaz.
        for code, files in groups.items():
            files.sort(key=lambda t: t[1], reverse=True)   # en yeni once
            winner = files[0]
            wpdf = winner[0]

            # Hedef HER ZAMAN {dest}/{code}/RFI No. {code}.pdf
            folder = folder_for(dest, code)
            target = target_for(dest, code)
            folder_exists = folder.is_dir()

            if _same_file(wpdf, target):
                # Dosya ZATEN dogru adda ve dogru yerde — yapacak is yok.
                # Bu kontrol olmadan (ozellikle source == dest iken) asagidaki
                # "hedef var" dali devreye girer ve dosyayi KENDI hedefi var diye
                # _review'e tasir: ikinci calistirmada duzgun arsiv dagilir.
                continue

            if target.exists():
                # Hedef ZATEN VAR -> uzerine ASLA yazma (veri kaybi olur). Bu RFI
                # zaten islenmis; yeni tarama _review'a gider, kullanici karar verir.
                plan.warnings.append(Warning("warn.exists", wpdf.name))
                plan.steps.append(Step(
                    kind, str(wpdf), str(dest / REVIEW_DIR / wpdf.name),
                    note_key="note.needs_review",
                    meta={"code": code, "reason": "target_exists", "target": str(target)},
                ))
            else:
                # Klasor yoksa ONCE onu olustur. Adim SIRASI onemli: apply
                # sirayla, undo TERS sirayla calisir — yani geri alirken once
                # dosya cikar, sonra (bos kalan) klasor silinir.
                if not folder_exists:
                    plan.steps.append(Step(
                        "mkdir", None, str(folder),
                        note_key="note.folder_created", meta={"code": code},
                    ))
                plan.steps.append(Step(
                    kind, str(wpdf), str(target),
                    note_key="note.renamed",
                    meta={"code": code, "confidence": winner[2], "source": winner[3],
                          "intoFolder": True, "folderCreated": not folder_exists},
                ))

            for older, _mt, _conf, _method in files[1:]:
                plan.warnings.append(Warning("warn.duplicate", older.name))
                plan.steps.append(Step(
                    kind, str(older), str(dest / REVIEW_DIR / older.name),
                    note_key="note.needs_review",
                    meta={"code": code, "reason": "older_revision", "kept": wpdf.name},
                ))

        if not pdfs:
            plan.warnings.append(Warning("warn.no_pdfs", str(source)))

        return plan

    def apply(self, plan: Plan) -> Result:
        return apply_steps(plan)

"""Stage signed RFI first pages without ever modifying the archive.

The selected archive is a read-only source of truth. Preview identifies each
new scan, finds exactly one archive PDF with the canonical RFI filename, and
maps it to the same relative path under a separate review root. Apply builds a
new PDF there, replacing only page one. Ambiguity always produces a warning and
no actionable step.
"""

from __future__ import annotations

import errno
import hashlib
import json
import os
import pathlib
import shutil
import tempfile
import time as time_module
from typing import Any

import fitz

from ..contracts import (
    BoolParam,
    FolderParam,
    JournalEntry,
    Operation,
    ParamSchema,
    Plan,
    Result,
    Step,
    Warning,
)
from ..extract import OcrEngineMissing, diagnostics, extract_first_page
from ..fs.apply import file_sha256, write_journal
from ..fs.naming import filename_for
from ..progress import cancellation_requested, notify
from ..registry import register
from .rfi_process_scans import MIN_CONFIDENCE


def _inside(path: pathlib.Path, root: pathlib.Path) -> bool:
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def _overlap(first: pathlib.Path, second: pathlib.Path) -> bool:
    return _inside(first, second) or _inside(second, first)


def _folder(raw: object, label: str) -> pathlib.Path:
    path = pathlib.Path(str(raw)).expanduser().resolve()
    if not path.is_dir():
        raise ValueError(f"{label} is not an existing folder: {path}")
    return path


def _validate_roots(
    signed_root: pathlib.Path,
    archive_root: pathlib.Path,
    review_root: pathlib.Path,
) -> None:
    # Output must be completely separate. This also catches the same folders
    # selected through aliases or symlinks because all roots are resolved.
    if _overlap(review_root, archive_root) or _overlap(review_root, signed_root):
        raise ValueError("error.rfi.reviewRootMustBeSeparate")


def _pdfs(root: pathlib.Path, recursive: bool) -> list[pathlib.Path]:
    iterator = root.rglob("*") if recursive else root.iterdir()
    return sorted(
        path
        for path in iterator
        if path.is_file()
        and path.suffix.lower() == ".pdf"
        and not path.name.startswith(".")
    )


def _archive_index(
    archive_root: pathlib.Path,
    codes: set[str],
) -> tuple[dict[str, list[pathlib.Path]], list[str]]:
    wanted = {filename_for(code).casefold(): code for code in codes}
    matches: dict[str, list[pathlib.Path]] = {code: [] for code in codes}
    errors: list[str] = []

    def on_error(error: OSError) -> None:
        errors.append(str(error))

    for dir_path, _dir_names, file_names in os.walk(
        archive_root, followlinks=False, onerror=on_error
    ):
        for name in file_names:
            code = wanted.get(name.casefold())
            if code is None:
                continue
            candidate = pathlib.Path(dir_path) / name
            try:
                resolved = candidate.resolve(strict=True)
            except OSError as error:
                errors.append(f"{candidate}: {error}")
                continue
            if not resolved.is_file() or not _inside(resolved, archive_root):
                errors.append(f"archive entry leaves selected root: {candidate}")
                continue
            matches[code].append(candidate)
    return matches, errors


def _page_facts(path: pathlib.Path) -> dict[str, Any]:
    with fitz.open(path) as document:
        if document.page_count < 1:
            raise ValueError("PDF has no pages")
        page = document[0]
        return {
            "pageCount": document.page_count,
            "firstPageWidth": round(float(page.rect.width), 3),
            "firstPageHeight": round(float(page.rect.height), 3),
            "firstPageRotation": int(page.rotation),
        }


def _page_layout_differs(first: dict[str, Any], second: dict[str, Any]) -> bool:
    return (
        abs(float(first["firstPageWidth"]) - float(second["firstPageWidth"]))
        > 1.0
        or abs(
            float(first["firstPageHeight"]) - float(second["firstPageHeight"])
        )
        > 1.0
        or int(first["firstPageRotation"]) != int(second["firstPageRotation"])
    )


def _missing_directory_steps(
    parent: pathlib.Path,
    review_root: pathlib.Path,
    planned: set[pathlib.Path],
) -> list[Step] | None:
    """Return one-level mkdir steps, or None for a blocked/symlinked path."""
    relative = parent.relative_to(review_root)
    current = review_root
    additions: list[Step] = []
    for part in relative.parts:
        current = current / part
        if current in planned:
            continue
        if current.exists():
            # A symlink would preserve bytes but not the selected folder layout.
            # Reject it instead of writing through an unexpected indirection.
            if not current.is_dir() or current.is_symlink():
                return None
            continue
        additions.append(
            Step(
                "mkdir",
                dst=str(current),
                note_key="note.folder_created",
                meta={"reviewRoot": str(review_root)},
            )
        )
        planned.add(current)
    return additions


def _preservation_fingerprint(document: fitz.Document) -> str:
    """Fingerprint document-level data that page replacement must preserve."""
    attachments: list[dict[str, str]] = []
    for name in sorted(document.embfile_names()):
        attachments.append(
            {
                "name": name,
                "sha256": hashlib.sha256(document.embfile_get(name)).hexdigest(),
            }
        )
    payload = {
        "metadata": document.metadata,
        "attachments": attachments,
        "toc": document.get_toc(),
        "pageLabels": document.get_page_labels(),
    }
    encoded = json.dumps(payload, ensure_ascii=False, sort_keys=True).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _restore_navigation(
    document: fitz.Document,
    toc: list[list[Any]],
    page_labels: list[dict[str, Any]],
) -> None:
    """Restore archive bookmarks and page labels after page insertion.

    PyMuPDF correctly shifts bookmarks for pages after the deleted page, but a
    bookmark to page one becomes invalid and inserted pages add a label range.
    Rebuilding from the original navigation keeps the copied PDF's behaviour.
    """
    if toc:
        document.set_toc([item[:3] for item in toc], collapse=0)
        for index, item in enumerate(toc):
            if len(item) < 4 or not isinstance(item[3], dict):
                continue
            destination = dict(item[3])
            destination.pop("xref", None)
            document.set_toc_item(index, dest_dict=destination)
    # Even an empty list matters: insert_pdf creates implicit label ranges.
    # Clearing them restores an archive that intentionally had no labels.
    document.set_page_labels(page_labels)


def _is_temporary_access_error(error: OSError) -> bool:
    return (
        isinstance(error, PermissionError)
        or error.errno in {errno.EACCES, errno.EPERM, errno.EBUSY}
        or getattr(error, "winerror", None) in {5, 32, 33}
    )


def _unlink_with_retry(path: pathlib.Path, attempts: int = 40) -> bool:
    """Remove a temp entry after short-lived OneDrive/AV locks are released."""
    for attempt in range(attempts):
        try:
            path.unlink(missing_ok=True)
            return True
        except OSError as error:
            if not _is_temporary_access_error(error) or attempt >= attempts - 1:
                return False
            time_module.sleep(0.15)
    return False


def _publish_windows_without_overwrite(
    temp_path: pathlib.Path,
    destination: pathlib.Path,
    attempts: int = 40,
) -> None:
    """Publish on Windows despite transient sync/indexer locks.

    Windows ``os.rename`` refuses an existing destination, so it preserves the
    operation's strict no-overwrite contract. OneDrive can, however, hold a
    sharing lock that blocks rename while still allowing a hard link or an
    exclusive copy. Falling back after the retry window prevents a completed
    PDF from being discarded while leaving only its planned folders behind.
    """
    for attempt in range(attempts):
        if destination.exists():
            raise FileExistsError(str(destination))
        try:
            os.rename(temp_path, destination)
            return
        except FileExistsError:
            raise
        except OSError as error:
            # A destination created by another process always wins, even if the
            # Windows error was reported as a generic sharing/access failure.
            if destination.exists():
                raise FileExistsError(str(destination)) from error
            if not _is_temporary_access_error(error):
                raise
            if attempt >= attempts - 1:
                break
            time_module.sleep(0.15)

    _publish_by_link_or_copy(temp_path, destination)


def _publish_by_link_or_copy(
    temp_path: pathlib.Path,
    destination: pathlib.Path,
) -> None:
    """Publish without overwrite when rename is unavailable or temporarily locked."""
    try:
        # Temp and destination share a directory, so a hard link is an atomic,
        # no-clobber publish on normal NTFS/APFS/ext filesystems. If a sync
        # service keeps the temp name locked, the destination remains a valid
        # completed PDF and the stale name can be cleaned on the next preview.
        os.link(temp_path, destination)
        _unlink_with_retry(temp_path)
        return
    except FileExistsError:
        raise
    except OSError as error:
        if destination.exists():
            raise FileExistsError(str(destination)) from error
        if error.errno not in {
            errno.EPERM,
            errno.EACCES,
            errno.ENOTSUP,
            errno.EOPNOTSUPP,
            errno.EXDEV,
        }:
            raise

    # Synced/removable filesystems may not support hard links. Exclusive
    # creation still guarantees that an existing review file is never replaced.
    created = False
    try:
        with temp_path.open("rb") as incoming, destination.open("xb") as outgoing:
            created = True
            shutil.copyfileobj(incoming, outgoing, length=1024 * 1024)
            outgoing.flush()
            os.fsync(outgoing.fileno())
    except Exception:
        if created and not _unlink_with_retry(destination):
            raise RuntimeError(
                f"incomplete review PDF could not be cleaned: {destination}"
            )
        raise
    _unlink_with_retry(temp_path)


def _publish_without_overwrite(temp_path: pathlib.Path, destination: pathlib.Path) -> None:
    """Install a completed PDF while preserving the no-overwrite contract."""
    if os.name == "nt":
        _publish_windows_without_overwrite(temp_path, destination)
        return
    _publish_by_link_or_copy(temp_path, destination)


def _temp_name_matches(temp_name: str, destination_name: str) -> bool:
    return (
        temp_name.startswith(f".{destination_name}.docflow-")
        and temp_name.endswith(".tmp")
    )


def _stale_hardlink_temps(
    destination: pathlib.Path,
    review_root: pathlib.Path,
) -> list[pathlib.Path]:
    """Find only our stale names that are hard links to a completed output."""
    try:
        destination_inside_review = _inside(
            destination.resolve(strict=True), review_root
        )
    except OSError:
        return []
    if (
        not destination.is_file()
        or destination.is_symlink()
        or not destination_inside_review
    ):
        return []
    stale: list[pathlib.Path] = []
    try:
        candidates = list(destination.parent.iterdir())
    except OSError:
        return []
    for candidate in candidates:
        if not _temp_name_matches(candidate.name, destination.name):
            continue
        try:
            if (
                candidate.is_symlink()
                or not candidate.is_file()
                or not _inside(candidate.resolve(strict=True), review_root)
                or not os.path.samefile(candidate, destination)
            ):
                continue
        except OSError:
            continue
        stale.append(candidate)
    return sorted(stale)


def _build_replacement(
    archive_pdf: pathlib.Path,
    signed_pdf: pathlib.Path,
    temp_path: pathlib.Path,
    expected_page_count: int,
) -> str:
    with fitz.open(archive_pdf) as output, fitz.open(signed_pdf) as signed:
        if output.page_count != expected_page_count or signed.page_count < 1:
            raise RuntimeError("PDF changed after preview; preview again")
        preservation = _preservation_fingerprint(output)
        original_toc = output.get_toc(simple=False)
        original_page_labels = output.get_page_labels()
        signed_page = signed[0]
        signed_layout = (
            round(float(signed_page.rect.width), 3),
            round(float(signed_page.rect.height), 3),
            int(signed_page.rotation),
        )
        output.delete_page(0)
        output.insert_pdf(
            signed,
            from_page=0,
            to_page=0,
            start_at=0,
            links=True,
            annots=True,
            widgets=True,
        )
        _restore_navigation(output, original_toc, original_page_labels)
        output.save(
            temp_path,
            garbage=0,
            deflate=True,
            encryption=fitz.PDF_ENCRYPT_KEEP,
            preserve_metadata=True,
        )

    with fitz.open(temp_path) as completed:
        if completed.page_count != expected_page_count:
            raise RuntimeError("output page count validation failed")
        first = completed[0]
        completed_layout = (
            round(float(first.rect.width), 3),
            round(float(first.rect.height), 3),
            int(first.rotation),
        )
        if completed_layout != signed_layout:
            raise RuntimeError("signed first-page layout validation failed")
        if _preservation_fingerprint(completed) != preservation:
            raise RuntimeError("document metadata or attachments were not preserved")

    # Windows rejects fsync() on a read-only descriptor with EBADF. Opening the
    # already-validated file read/write does not modify it, but gives both NTFS
    # and POSIX platforms a descriptor that can be flushed before publication.
    with temp_path.open("r+b") as stream:
        os.fsync(stream.fileno())
    return file_sha256(temp_path)


@register("rfi.stage_signed_first_pages")
class StageSignedFirstPages(Operation):
    title_key = "op.rfi.stage_signed_first_pages.title"
    description_key = "op.rfi.stage_signed_first_pages.desc"
    schema = ParamSchema(
        [
            FolderParam(
                "signed_scans",
                "param.rfi.signedScans",
                help_key="param.rfi.signedScans.help",
            ),
            FolderParam(
                "archive_root",
                "param.rfi.archiveRoot",
                help_key="param.rfi.archiveRoot.help",
            ),
            FolderParam(
                "review_root",
                "param.rfi.reviewRoot",
                help_key="param.rfi.reviewRoot.help",
            ),
            BoolParam(
                "scan_subfolders",
                "param.rfi.scanSubfolders",
                default=False,
                help_key="param.rfi.scanSubfolders.help",
            ),
        ]
    )

    def preview(self, params: dict) -> Plan:
        signed_root = _folder(params["signed_scans"], "signed scans")
        archive_root = _folder(params["archive_root"], "archive root")
        review_root = _folder(params["review_root"], "review root")
        try:
            _validate_roots(signed_root, archive_root, review_root)
        except ValueError:
            return Plan(
                op_id=self.id,
                warnings=[Warning("error.rfi.reviewRootMustBeSeparate")],
            )

        pdfs = _pdfs(signed_root, bool(params.get("scan_subfolders", False)))
        plan = Plan(
            op_id=self.id,
            meta={
                "archiveReadOnly": True,
                "preservesArchiveLayout": True,
                "usesOnlySignedFirstPage": True,
            },
        )
        if not pdfs:
            plan.warnings.append(Warning("warn.rfi.noSignedPdfs", str(signed_root)))
            return plan

        groups: dict[str, list[tuple[pathlib.Path, float, str]]] = {}
        for index, pdf in enumerate(pdfs):
            notify("preview", index, len(pdfs), pdf.name)
            try:
                match, method = extract_first_page(str(pdf))
            except OcrEngineMissing:
                return Plan(
                    op_id=self.id,
                    warnings=[Warning("warn.no_ocr", diagnostics())],
                    meta=plan.meta,
                )
            except Exception as error:  # one damaged scan must not stop detection
                plan.warnings.append(
                    Warning(
                        "warn.rfi.signedUnreadable",
                        f"{pdf.name} — {type(error).__name__}: {error}"[:300],
                    )
                )
                continue

            if not match.ok:
                plan.warnings.append(Warning("warn.no_code", pdf.name))
                continue
            if match.confidence < MIN_CONFIDENCE:
                plan.warnings.append(Warning("warn.low_confidence", pdf.name))
                continue
            groups.setdefault(match.code, []).append(
                (pdf, match.confidence, method)
            )

        unique: dict[str, tuple[pathlib.Path, float, str]] = {}
        for code, scans in sorted(groups.items()):
            if len(scans) != 1:
                names = ", ".join(scan[0].name for scan in scans)
                plan.warnings.append(
                    Warning("warn.rfi.signedDuplicate", f"{code} — {names}"[:300])
                )
                continue
            unique[code] = scans[0]

        matches, archive_errors = _archive_index(archive_root, set(unique))
        if archive_errors:
            return Plan(
                op_id=self.id,
                warnings=[
                    Warning("warn.rfi.archiveUnreadable", archive_errors[0][:300])
                ],
                meta=plan.meta,
            )

        planned_directories: set[pathlib.Path] = set()
        for code, (signed_pdf, confidence, method) in sorted(unique.items()):
            archive_matches = matches[code]
            if not archive_matches:
                plan.warnings.append(Warning("warn.rfi.archiveMissing", code))
                continue
            if len(archive_matches) > 1:
                relative_names = ", ".join(
                    str(path.relative_to(archive_root)) for path in archive_matches
                )
                plan.warnings.append(
                    Warning(
                        "warn.rfi.archiveAmbiguous",
                        f"{code} — {relative_names}"[:300],
                    )
                )
                continue

            archive_pdf = archive_matches[0]
            relative = archive_pdf.relative_to(archive_root)
            destination = review_root / relative
            if destination.exists():
                stale_temps = _stale_hardlink_temps(destination, review_root)
                try:
                    destination_hash = (
                        file_sha256(destination) if stale_temps else ""
                    )
                except OSError:
                    stale_temps = []
                    destination_hash = ""
                if stale_temps:
                    for stale_temp in stale_temps:
                        plan.steps.append(
                            Step(
                                "cleanup_temp",
                                src=str(stale_temp),
                                dst=str(destination),
                                note_key="note.rfi.cleanupStaleTemp",
                                meta={
                                    "reviewRoot": str(review_root),
                                    "destinationSha256": destination_hash,
                                },
                            )
                        )
                    plan.warnings.append(
                        Warning("warn.rfi.staleTempFound", str(relative))
                    )
                else:
                    plan.warnings.append(
                        Warning("warn.rfi.reviewExists", str(relative))
                    )
                continue
            if not _inside(destination.resolve(strict=False), review_root):
                plan.warnings.append(
                    Warning("warn.rfi.reviewPathBlocked", str(relative))
                )
                continue

            try:
                archive_facts = _page_facts(archive_pdf)
                signed_facts = _page_facts(signed_pdf)
                archive_hash = file_sha256(archive_pdf)
                signed_hash = file_sha256(signed_pdf)
            except Exception as error:
                plan.warnings.append(
                    Warning(
                        "warn.rfi.signedUnreadable",
                        f"{code} — {type(error).__name__}: {error}"[:300],
                    )
                )
                continue

            additions = _missing_directory_steps(
                destination.parent, review_root, planned_directories
            )
            if additions is None:
                plan.warnings.append(
                    Warning("warn.rfi.reviewPathBlocked", str(relative))
                )
                continue
            plan.steps.extend(additions)

            if int(signed_facts["pageCount"]) > 1:
                plan.warnings.append(
                    Warning("warn.rfi.signedExtraPages", signed_pdf.name)
                )
            if _page_layout_differs(archive_facts, signed_facts):
                plan.warnings.append(
                    Warning("warn.rfi.firstPageLayoutDiffers", code)
                )

            plan.steps.append(
                Step(
                    "pdf_first_page_replace",
                    src=str(archive_pdf),
                    dst=str(destination),
                    note_key="note.rfi.signedFirstPageStaged",
                    meta={
                        "code": code,
                        "signedPdf": str(signed_pdf),
                        "signedRoot": str(signed_root),
                        "archiveRoot": str(archive_root),
                        "reviewRoot": str(review_root),
                        "relativePath": str(relative),
                        "confidence": confidence,
                        "method": method,
                        "archiveSha256": archive_hash,
                        "signedSha256": signed_hash,
                        "archivePageCount": archive_facts["pageCount"],
                        "signedPageCount": signed_facts["pageCount"],
                        "archiveFirstPage": archive_facts,
                        "signedFirstPage": signed_facts,
                    },
                )
            )

        notify("preview", len(pdfs), len(pdfs))
        return plan

    def _apply_mkdir(self, step: Step) -> None:
        if step.dst is None:
            raise ValueError("mkdir destination is missing")
        review_root = _folder(step.meta.get("reviewRoot"), "review root")
        destination = pathlib.Path(step.dst)
        if not _inside(destination.resolve(strict=False), review_root):
            raise ValueError("mkdir leaves the approved review root")
        destination.mkdir(exist_ok=False)

    def _apply_pdf(self, step: Step) -> Step:
        if step.src is None or step.dst is None:
            raise ValueError("PDF replacement paths are missing")
        archive_root = _folder(step.meta.get("archiveRoot"), "archive root")
        signed_root = _folder(step.meta.get("signedRoot"), "signed scans")
        review_root = _folder(step.meta.get("reviewRoot"), "review root")
        _validate_roots(signed_root, archive_root, review_root)

        archive_pdf = pathlib.Path(step.src).resolve(strict=True)
        signed_pdf = pathlib.Path(str(step.meta.get("signedPdf"))).resolve(strict=True)
        destination = pathlib.Path(step.dst)
        if not _inside(archive_pdf, archive_root):
            raise ValueError("archive PDF leaves the approved archive root")
        if not _inside(signed_pdf, signed_root):
            raise ValueError("signed PDF leaves the approved scan root")
        if not archive_pdf.is_file() or not signed_pdf.is_file():
            raise FileNotFoundError("an approved input PDF is missing")
        if destination.exists():
            raise FileExistsError(str(destination))
        if not destination.parent.is_dir() or destination.parent.is_symlink():
            raise ValueError("approved output folder is missing or redirected")
        if not _inside(destination.resolve(strict=False), review_root):
            raise ValueError("output leaves the approved review root")

        approved_archive_hash = str(step.meta.get("archiveSha256") or "")
        approved_signed_hash = str(step.meta.get("signedSha256") or "")
        if file_sha256(archive_pdf) != approved_archive_hash:
            raise RuntimeError("archive PDF changed after preview; preview again")
        if file_sha256(signed_pdf) != approved_signed_hash:
            raise RuntimeError("signed PDF changed after preview; preview again")

        descriptor, raw_temp = tempfile.mkstemp(
            prefix=f".{destination.name}.docflow-",
            suffix=".tmp",
            dir=destination.parent,
        )
        os.close(descriptor)
        temp_path = pathlib.Path(raw_temp)
        if not _unlink_with_retry(temp_path):
            raise PermissionError(f"temporary file is locked: {temp_path}")
        try:
            output_hash = _build_replacement(
                archive_pdf,
                signed_pdf,
                temp_path,
                int(step.meta["archivePageCount"]),
            )
            # Inputs are read again immediately before publish. No output based
            # on stale preview bytes is installed.
            if file_sha256(archive_pdf) != approved_archive_hash:
                raise RuntimeError("archive PDF changed during apply; preview again")
            if file_sha256(signed_pdf) != approved_signed_hash:
                raise RuntimeError("signed PDF changed during apply; preview again")
            _publish_without_overwrite(temp_path, destination)
            if file_sha256(destination) != output_hash:
                raise RuntimeError("published PDF validation failed")
        finally:
            # Cleanup must never turn an already validated/published PDF into
            # a false failed result. The Windows publish path normally moves
            # the temp entry away; this is a final retry for exceptional exits.
            _unlink_with_retry(temp_path)

        journal_step = Step(
            "copy",
            src=str(archive_pdf),
            dst=str(destination),
            note_key=step.note_key,
            meta=dict(step.meta),
        )
        journal_step.meta["undoSha256"] = output_hash
        return journal_step

    def _apply_cleanup_temp(self, step: Step) -> None:
        if step.src is None or step.dst is None:
            raise ValueError("temp cleanup paths are missing")
        review_root = _folder(step.meta.get("reviewRoot"), "review root")
        temp_path = pathlib.Path(step.src)
        destination = pathlib.Path(step.dst)
        if temp_path.is_symlink() or destination.is_symlink():
            raise ValueError("temp cleanup path is redirected")
        temp_path = temp_path.resolve(strict=True)
        destination = destination.resolve(strict=True)
        if (
            not _inside(temp_path, review_root)
            or not _inside(destination, review_root)
            or temp_path.parent != destination.parent
            or not _temp_name_matches(temp_path.name, destination.name)
            or not temp_path.is_file()
            or not destination.is_file()
            or not os.path.samefile(temp_path, destination)
        ):
            raise ValueError("temp cleanup did not match a completed review PDF")

        approved_hash = str(step.meta.get("destinationSha256") or "")
        if not approved_hash or file_sha256(destination) != approved_hash:
            raise RuntimeError("review PDF changed after preview; cleanup stopped")
        if not _unlink_with_retry(temp_path):
            raise PermissionError(f"temporary file is still locked: {temp_path}")
        if not destination.is_file() or file_sha256(destination) != approved_hash:
            raise RuntimeError("review PDF changed during temp cleanup")

    @staticmethod
    def _remove_empty_created_directories(
        created_directories: list[Step],
        entry: JournalEntry,
        result: Result,
    ) -> None:
        """Do not leave an empty review tree after a PDF or cancellation failure."""
        for step in reversed(created_directories):
            if step.dst is None:
                continue
            directory = pathlib.Path(step.dst)
            try:
                if directory.is_symlink():
                    continue
                directory.rmdir()
            except OSError:
                # Non-empty, concurrently changed, or already unavailable.
                # In each case preserving the path is the safest outcome.
                continue
            try:
                entry.steps.remove(step)
            except ValueError:
                pass
            result.applied = max(0, result.applied - 1)

    def apply(self, plan: Plan) -> Result:
        if plan.op_id != self.id:
            raise ValueError("operation plan mismatch")
        entry = JournalEntry(op_id=plan.op_id, steps=[])
        result = Result(journal_id=entry.journal_id)
        created_directories: list[Step] = []

        for index, step in enumerate(plan.steps):
            if cancellation_requested():
                result.meta = {
                    "cancelled": True,
                    "pending": len(plan.steps) - index,
                }
                result.errors.append(Warning("warn.operation.cancelled"))
                break
            notify("apply", index, len(plan.steps), pathlib.Path(step.dst or "").name)
            entry.pending_step = step
            write_journal(entry)
            try:
                if step.kind == "mkdir":
                    self._apply_mkdir(step)
                    journal_step: Step | None = step
                elif step.kind == "pdf_first_page_replace":
                    journal_step = self._apply_pdf(step)
                elif step.kind == "cleanup_temp":
                    self._apply_cleanup_temp(step)
                    journal_step = None
                else:
                    raise ValueError(f"unsupported approved step: {step.kind}")
            except Exception as error:  # continue other independent RFIs
                result.failed += 1
                result.errors.append(
                    Warning(
                        "error.step_failed",
                        f"{step.src or step.dst}: {type(error).__name__}: {error}",
                    )
                )
            else:
                result.applied += 1
                if journal_step is not None:
                    entry.steps.append(journal_step)
                    if step.kind == "mkdir":
                        created_directories.append(journal_step)
            finally:
                entry.pending_step = None
                write_journal(entry)

        self._remove_empty_created_directories(
            created_directories, entry, result
        )
        write_journal(entry)
        notify("apply", len(plan.steps), len(plan.steps))
        return result

# ADR 0003: Exact-page staging for combined Worley scans

- Date: 2026-09-13
- Status: Accepted

## Context

Worley may sign only the first page of many RFIs. The operator can therefore
scan hundreds of signed first pages into one PDF instead of creating one input
PDF per RFI. The earlier staging operation inspected only page one of each file,
which forced the operator to split a combined scan manually.

Splitting source files during preview would create hidden mutations and new
temporary-file cleanup risks. Re-identifying pages during apply would allow the
approved preview and executed result to diverge.

## Decision

Signed-page staging analyzes every page of a combined input PDF independently,
up to a fixed 20,000-page total. Exact canonical RFI recognition and the existing
confidence threshold still apply. Preview stores the selected zero-based page
index together with the whole source fingerprint, archive fingerprint, exact
archive candidate, and review destination in the immutable Plan.

Apply revalidates the fingerprints, copies the signed scan and archive PDF into
private operation storage, and imports only the approved source page as page one
of the review copy. The original scan and archive are never split, moved, or
modified. Existing review outputs are never overwritten.

Pages that are unreadable, low-confidence, duplicated for one RFI, contain an
unsupported form widget, or map to a missing or ambiguous archive file remain
visible warnings and produce no output. Adapters without page-level support keep
the prior first-page-only behavior and warn about extra pages.

## Consequences

- One scanner job can prepare many independent RFI review copies without manual
  PDF splitting.
- Each review row identifies its source filename and one-based page number.
- One combined source fingerprint protects every planned page from later edits.
- OCR effort grows with page count and remains bounded. Page-level progress was
  added in 0.13.1 and bounded thumbnails are governed by ADR 0004.
- Native Windows OCR and OneDrive-backed review behavior still require acceptance
  on the exact packaged Windows candidate.

## Rollback

The original source and archive remain untouched. Undo removes only unchanged
journaled review outputs and operation-created empty folders. Reverting this
feature restores first-page-only recognition without requiring data migration.

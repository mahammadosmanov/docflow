# ADR 0013: Safe multi-RFI incoming file grouping

- Date: 2026-09-15
- Status: Accepted

## Context

Incoming RFI packages mix PDFs, Office documents, EML, MSG, and filenames that
may omit the RFI number. One item, especially a mail, can belong to several RFIs.
Manual duplication is slow, but a false match or in-place reorganization could
lose or misfile source evidence.

## Options considered

1. Keep manual folder creation and copying.
2. Move files automatically from filename guesses.
3. Inspect bounded exact evidence, preview every fan-out copy, keep the source
   read-only, and route uncertainty to a separate review folder.

## Decision

Use option 3. Only exact canonical ASCII RFI identifiers are accepted. Ordinary
files use a canonical filename match when present; mail is always content-scanned
for additional RFIs, and files without a name match use bounded type-appropriate
content inspection. Analysis is bounded and parallel, while approved filesystem
effects remain ordered, no-clobber, journaled, and undoable.

The operation reads only top-level regular files. Existing source subfolders are
ignored so prior filing is not duplicated. It copies each approved file into
every matching canonical RFI folder beneath a separate destination. Unmatched or
unreadable files are copied to `_review` without changing the originals.

## Consequences

- Multi-RFI mail fan-out is explicit in the generic record-table Preview and can
  be disabled per row, selected group, or all rows.
- Malformed packages, missing OCR, unsafe paths, symlinks, and collisions fail
  closed or remain visible for manual review.
- Native Windows acceptance remains required for representative Outlook/Office
  storage and OneDrive locking behavior.

## Rollback

Remove the registry entry and native router while leaving journals and already
created output folders intact. Existing copies remain ordinary operator data and
must not be deleted automatically.

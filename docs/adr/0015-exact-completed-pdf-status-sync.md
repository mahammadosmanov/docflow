# ADR 0015: Exact completed-PDF status synchronization

- Status: Accepted
- Date: 2026-09-16

## Context

An exact `RFI No. <canonical RFI>.pdf` stored anywhere below the RFI System is
the operational evidence that the RFI is complete. Manually locating these PDFs
and changing RFI Log `Inspection Status` cells is repetitive. Folding this rule
into Worley synchronization would unnecessarily require a Follow Up workbook and
would mix the Completed lifecycle state with the later Worley-signed state.

## Decision

Add a separate `rfi.complete_status_sync` operation. Preview performs a bounded,
parallel, recursive, read-only filename walk and groups exact canonical matches
deterministically. It maps those RFIs only to existing CIV/MEC/ELE/PRC RFI Log
rows and proposes `Completed` for literal blank or `Ongoing` Inspection Status
cells. Repeated physical rows are updated together. Conflicting terminal or
unknown values, formulas, merges, missing cells/rows, malformed names, symlinks,
and incomplete traversal authorize no change.

Apply consumes only the approved Plan and reuses the existing fingerprint,
no-clobber backup, guarded OOXML patch/delta verification, journal, and Undo
path. Its typed workbook step permits only the `Completed` replacement. Follow
Up, REMARKS, signed sheets, Worley data, and source PDFs are outside its authority.

## Consequences

- Operators see the exact PDF evidence, current status, proposed status, and
  approved workbook cells before Apply.
- Filename discovery avoids unnecessary PDF rendering/OCR; a durable content
  cache is not needed because safe correctness still requires a complete current
  directory walk.
- The operation cannot silently correct inconsistent lifecycle data. Those rows
  remain explicit manual review.
- Native Windows/OneDrive and Excel acceptance remains a release gate.

## Rollback

Undo restores the exact retained workbook backup when no later edit exists.
Application rollback removes the registered operation without changing the RFI
System or automatically deleting retained journals/backups.

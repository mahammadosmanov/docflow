# ADR 0041: Separate publish-status operation

- Status: Accepted
- Date: 2026-10-24

## Context

The publish flow receives a PDF and a DOCX per RFI from a requester, then marks
the RFI `Ongoing` in the RFI Log, syncs Follow Up, and prepares the publish
mail. Marking `Ongoing` is the same guarded single-cell workbook edit as
`rfi.complete_status_sync`, but that operation is fixed to `Completed` by
ADR 0015 and its revise path rejects any other replacement value.

## Decision

Add a separate `rfi.publish_status` operation. It maps exact RFI codes found in
the mail's attachment names (`publish_intake`) to existing RFI Log rows and
proposes `Ongoing` only for literal blank Inspection Status cells. Any non-blank
status, missing row, merged/formula cell, or incomplete PDF+DOCX pair authorizes
no change.

It reuses the existing fingerprint, no-clobber backup, guarded OOXML patch and
delta verification, journal, and Undo machinery through a new
`SyncWorkbookKind` variant with its own revise validation. The existing
`RfiLogStatus` kind, `rfi.complete_status_sync`, and their tests are not edited.

## Consequences

- `Completed` sync behaves exactly as before; a regression there is impossible
  through this change except by a shared-code mistake, which the existing suite
  covers.
- Two near-duplicate preview builders exist. Accepted: the safety guarantees
  differ (blank-only vs blank-or-Ongoing) and must not drift together.
- Recipient lists, the publish mail draft, and Word correction are separate
  decisions and are not part of this ADR.

## Rollback

Remove the operation, its registry entry, and the new `SyncWorkbookKind`
variant. No stored settings, journal, or workbook format changes.

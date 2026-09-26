# ADR 0026: Mixed RFI status remarks on CIV and PCU Follow Up sheets

- Date: 2026-09-25
- Status: Superseded by [ADR 0028](0028-remove-follow-up-status-remarks.md)

## Context

The Follow Up workbook stores multiple RFI identifiers in one status cell. A
single aggregate status such as `In Progress` hides which members of a mixed
group are already `Completed`. The current workbook has existing `Remarks`
columns on the CIV and PCU sheets for this distinction.

## Decision

Extend `rfi.follow_up_sync` to write a per-RFI status summary in the existing
row `Remarks` cell only on the exact `CIV` and `PCU` sheets when a shared RFI
cell contains at least two resolvable RFIs and at least one source status is
`Completed`. List every RFI from each qualifying group with its individual
mapped source status, for example `CIV-1222 Completed, CIV-1223 In Progress`.
When several qualifying groups share one physical row, combine their entries
into the same row remark. Preserve existing remark text and maintain one
replaceable `RFI Status Sync:` line so repeated syncs update it without adding
duplicates. Do not change remarks on MEC, Road, or Signed By sheets.

The exact remarks cell and its old and proposed literal text are part of the
approved Plan and guarded Apply. If the cell is physically absent, Preview pins
that absence and Apply may materialize only that blank cell inside the existing
worksheet row and header-defined Remarks column. It never creates a row or
worksheet. Formula-backed, merged, ambiguous, or otherwise unsafe targets keep
the affected status update in manual review. Existing workbook backup, journal,
delta verification, and Undo rules remain mandatory.

## Alternatives

- Keep only the aggregate status: no new behavior, but the per-RFI completion
  difference remains hidden.
- Write each RFI status to a separate worksheet cell: would alter the template
  layout and is outside the approved cell mutation model.
- Replace the whole Remarks cell: simpler, but would erase operator notes.

## Consequences and rollback

Operators can see each RFI's status where a mixed group contains a completed
member. Other sheet remarks remain unchanged. Undo restores the exact prior
workbook; source rollback uses the previous immutable code release.

## Propagation

- `docs/PRD.md`: DF-033 and core product contract.
- `docs/PRODUCT.md`: Follow Up sync capability boundary.
- `docs/RFI_DOMAIN.md`: mixed-group remarks rules.
- `docs/TRACEABILITY.md`: implementation and focused evidence owner.
- `frontend/src/locales/*.arb`: preview label and manual-review reason in all
  four supported languages.
- `src-tauri/src/rfi_follow_up.rs`: classify, plan, validate, and guard remark
  cell updates.
- `src-tauri/tests/rfi_follow_up_native.rs`: mixed groups, multiple groups on
  one row, idempotent remark replacement, preservation, and unsafe targets.
- `src-tauri/src/ooxml.rs`: create a pinned blank cell only in an existing row
  when the approved Follow Up Remarks target was physically absent.

This decision extended the Follow Up projection accepted under ADR 0008. ADR
0028 supersedes the generated-remark behavior; its history remains here.

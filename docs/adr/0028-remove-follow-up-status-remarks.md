# ADR 0028: Remove generated Follow Up status remarks

- Date: 2026-09-26
- Status: Superseded by [ADR 0047](0047-follow-up-generated-row-remarks.md)
- Supersedes: [ADR 0026](0026-mixed-rfi-status-remarks.md)

## Context

Follow Up Sync added generated per-RFI status summaries to CIV and PCU
worksheet `Remarks` cells for mixed-status groups. The owner requested removing
that behavior. Status synchronization remains useful, while editing remarks is
outside the intended Follow Up Sync result.

## Decision

Follow Up Sync updates only its approved RFI number and status cells. It does
not add, replace, or clear content in any Follow Up worksheet `Remarks` cell,
including generated `RFI Status Sync:` lines from earlier versions. It does not
preview remark fields, include remark cells in an approved Plan, or create a
blank remark cell. RFI Log `REMARKS` remain read-only evidence for Worley
Signed/Not Signed mapping.

## Alternatives

- Keep the generated remarks: rejected at the owner's request.
- Delete old generated lines during sync: rejected because it would mutate
  operator workbook content without an explicit request to clean existing data.

## Consequences and rollback

Mixed RFI groups continue to receive their aggregate lifecycle status. The
individual statuses are no longer copied into worksheet remarks. Existing
operator text and previously generated remark lines remain unchanged by Sync.
Source rollback restores the prior immutable code version.

## Propagation

- `docs/PRD.md`: mark DF-033 retired and state that worksheet Remarks cells are
  outside Follow Up Sync authority.
- `docs/PRODUCT.md` and `docs/FEATURES.md`: remove generated-remark capability.
- `docs/RFI_DOMAIN.md`: define the exact writable Follow Up cells and retain
  RFI Log REMARKS as read-only Worley evidence.
- `docs/TRACEABILITY.md`: mark DF-033 retired and point to the no-write
  regression.
- `frontend/src/locales/app_{en,tr,ru,az}.arb`: remove generated-remark preview
  labels and unsafe-target reasons.
- `src-tauri/src/rfi_follow_up.rs`: remove remark fields from preview and Plan,
  and remove remark-cell guarded updates.
- `src-tauri/tests/rfi_follow_up_native.rs`: prove mixed groups retain status
  synchronization without previewing or applying remark-cell changes.

No earlier decision is voided. ADR 0028 supersedes ADR 0026 while preserving
the grouped status projection accepted under ADR 0008.

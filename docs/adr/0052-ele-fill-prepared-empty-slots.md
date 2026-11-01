# ADR 0052: Fill prepared empty ELE Follow Up slots

- Date: 2026-11-01
- Status: Accepted
- Supersedes: ADR 0051's restriction against filling an empty ELE RFI cell

## Context

Operators need Follow Up Sync to project supported ELE RFI Log records into
blank RFI/status pairs in the prepared ELE rows. ADR 0051 enabled only status
updates for identifiers already present, which made the newly supported route
behave more narrowly than the existing lifecycle routes.

## Decision

- Keep only the two ELE routes approved by ADR 0051: ITP `ELE-0005` / item
  `ELE-5.2.6` to the `2.5, 2.6` group, and ITP `ELE-0006` / item
  `ELE-6.2.5.3` to the `2.1, 2.4, 2.5.1, 2.5.3` group.
- Match one exact source `PCS-` Tag No to the prepared row's `Block` and the
  route's exact RFI/status columns.
- Update a matching existing RFI, or fill a safe empty RFI/status pair in an
  existing prepared row. Never create rows or sections, and never route `1.6`
  or another ELE ITP/item pair.
- Keep formula, merge, ambiguity, source-context, Preview, guarded Apply,
  backup, journal, and Undo safeguards.

## Consequences

Supported ELE identifiers can be added to a matching blank prepared slot, then
their lifecycle status is projected from the RFI Log. Unsupported pairs and
unmatched contexts remain unchanged. Road retains its separate approved
part-row creation behavior.

## Rollback

Restore ADR 0051's no-fill rule and the previous ELE guard in Follow Up
classification. Applied workbook changes remain reversible through Undo.

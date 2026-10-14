# ADR 0038: CIV Follow Up Tag No token coverage

- Date: 2026-10-14
- Status: Accepted

## Context

CIV RFI Log records can list several area tags in one `Tag No` cell, such as `S14,S15`, when the same RFI belongs in separate prepared Follow Up rows. Exact multi-tag fan-out already exists, but partial prepared coverage did not identify which individual tag lacked a destination.

## Decision

- Split each CIV `Tag No` list into individual exact tag contexts, and independently match each to an existing prepared CIV area row.
- Fan out to every uniquely matching distinct prepared row. Never infer or create a missing row.
- When only some source tags have a unique prepared row, allow independently safe matches to sync and add an unselectable Manual Review diagnostic naming each matched tag and its row and each unmatched tag.
- Duplicate or ambiguous prepared destinations remain Manual Review; partial coverage never weakens exact-match or guarded-Apply rules.
- Preserve the existing CIV `PCS-` exclusion, including removal of such RFIs from existing Follow Up groups. This decision does not add routes or change ITP/item scope.
- The source RFI Log remains read-only; the operator corrects an inaccurate source tag there.

## Consequences

Operators can distinguish missing CIV Follow Up rows from source-log tag errors without losing valid independently mapped rows. The existing zero-match template-scope rule, status conflict checks, and no-row-creation policy remain unchanged. [RFI_DOMAIN.md](../RFI_DOMAIN.md) is the canonical domain contract.

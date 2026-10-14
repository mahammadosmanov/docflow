# ADR 0035: Intentional Follow Up scope exclusions

- Date: 2026-10-14
- Status: Accepted

## Context

The current RFI Log contains ITP/item combinations that do not belong in the Follow Up workbook. Their lack of a route is intentional, not a missing mapping defect. Adding destinations for them would place RFIs in a workflow where they are not wanted.

## Decision

Keep these source ITP/item pairs outside Follow Up; do not add route mappings for them:

| Source ITP | Excluded item clauses |
|---|---|
| CIV-0002 | 2.1.4, 2.1.6 |
| CIV-0005 | 5.1.4, 5.1.5, 5.1.9, 5.3.4, 5.3.5 |
| CIV-0006 | 6.1.8, 6.2.1, 6.2.2, 6.2.3, 6.2.4, 6.2.5, 6.2.9 |
| CIV-0007 | 7.1.5, 7.2.3, 7.2.4, 7.3.9 |
| MEC-0001 | 1.1.4, 1.2.5 |
| MEC-0004 | 4.3.1 |

These exclusions are approved business scope, not work waiting to be mapped. Absence from the route table alone is not authorization to add a route. Any proposed inclusion or exclusion requires explicit operator approval and an update to this ADR and `docs/RFI_DOMAIN.md` before route behavior changes.

## Consequences

Other unsupported ITP/item pairs remain unresolved until their scope is explicitly decided. Existing exact-context, ambiguity, and no-row-creation safeguards remain unchanged.

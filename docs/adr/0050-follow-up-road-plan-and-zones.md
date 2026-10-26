# ADR 0050: Follow Up keeps every planned Road and its zone

- Status: Accepted
- Date: 2026-10-26
- Amends: [ADR 0048](0048-follow-up-road-part-rows.md)

## Context

The project has 21 Roads. Follow Up must show all of them from the start, with no RFI
needed, and every row needs its ZONE. Earlier versions only added rows for Roads that
an RFI named, leaving Roads 8, 9, and 10 and several ZONE cells empty.

## Decision

- Every Road 1 to 21 keeps at least one Follow Up row. A Road with no row gets a plain
  `Road NN` row in number order; parts, renames, and further parts follow ADR 0048 and
  appear only when the Log names them.
- ZONE is fixed by the owner: Roads 1-5 North, 6-13 Center, 14-20 South, 21 North.
  New rows get their zone; an existing ZONE cell that is present and empty is filled.
  A ZONE cell that already holds text is never changed.
- Roads outside 1-21 named by the Log are still added (without a zone) and flagged.

## Consequences

The first sync after this change may add up to all missing Road rows at once, each shown
as a ticked row in Preview. The zone table is source code, not a setting; changing it is
a code change.

## Rollback

Remove the planned-Road merge and the zone fill; written rows remain ordinary rows and
Undo restores the exact original bytes.

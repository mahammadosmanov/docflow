# ADR 0056: Follow Up moves placements the RFI Log contradicts and repairs mistyped RFI cells

- Date: 2026-11-21
- Status: Accepted
- Amends: [ADR 0036](0036-follow-up-context-scope-and-diagnostics.md) placement review rules

## Context

On the operator's current workbooks, the remaining Manual Review rows were
mostly RFIs typed into the wrong Follow Up cell: an excavation RFI in the
site-clearing column, one row's RFI repeated in the next row, two PCU RFIs
swapped between columns, or `CIV-0015` typed where `CIV-0315` belongs. The RFI
Log states each RFI's ITP item and Tag No, so these cells contradict it.
Until now DocFlow only reported them and never moved or deleted an RFI.

On 2026-11-21 the operator decided: when DocFlow is sure an RFI is in the wrong
place, remove it there and put it in the right place; when a cell is mistyped
(for example `CIV-13741461`), fix it.

## Decision

- An RFI's matching cells are pinned when its route matches exactly one
  prepared cell, or an exact fan-out in which every source tag or context
  matched, and its Log status is known.
- A copy of a pinned RFI in any other safe, non-Signed-By lifecycle cell is
  proposed for removal from that cell, and the RFI is added to its matching
  cell under the normal compatibility rules. Both cells' statuses are
  recomputed. Removals are found in a first pass, so the result does not
  depend on the order RFIs are processed in.
- Ambiguous or partly unmatched records, and other cells that hold a formula
  or merge, keep the `placedElsewhere` / `mappingAmbiguous` Manual Review row.
- A Follow Up RFI cell that does not parse is rewritten in the standard form
  only when a fixed repair (two four-digit numbers typed together, or `\`, `;`,
  `&`, `+`, `|`, commas, spaces and doubled slashes read as separators) gives
  RFIs that all exist in the Log. Otherwise it stays `invalidRfi`.
- Everything goes through preview, approval, Apply, journal and Undo as usual.
  The RFI Log is never changed.

## Consequences

Wrong-cell copies and run-together numbers no longer need fixing by hand, and
Manual Review is left with only the cases DocFlow cannot decide.
[RFI_DOMAIN.md](../RFI_DOMAIN.md) is the canonical rule text.

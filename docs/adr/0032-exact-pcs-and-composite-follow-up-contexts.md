# ADR 0032: Exact PCS and composite Follow Up contexts

- Date: 2026-09-26
- Status: Accepted
- Extends: [ADR 0008](0008-exact-rfi-log-follow-up-projection.md),
  [ADR 0029](0029-exact-civ-multi-area-follow-up.md), and
  [ADR 0031](0031-exact-mec-context-follow-up.md)

> Superseded for MEC destination-key fields, Inspection Location matching, and
> MEC Area fan-out by [ADR 0037](0037-mec-follow-up-section-block-matching.md).
> CIV/PCU scope and composite-tag rules remain in force.

## Context

The current RFI Log uses `PCS-` tags for some PCU records, while older route
logic recognized only `PCU-`. Some source Tag No values include an exact PCS
identifier followed by slash/comma-separated section labels. MEC Follow Up
rows can also encode an exact inspection location as a slash-separated list,
and a MEC source row can name several exact Area values while keeping one tag
and one inspection location. Treating these explicit values as one opaque
string leaves valid existing rows unsynchronized.

## Decision

For CIV ITP `0002` clauses `1.8`, `2.4`, and `2.5`, a source Tag No beginning
with exact `PCS-` or `PCU-` selects the existing PCU route. When Tag No includes
one PCS identifier followed by explicit section labels, only existing PCU rows
with that block and one of those exact area labels match. Multiple PCS
identifiers in one Tag No remain ambiguous.

MEC matching compares exact normalized `(Tag No, Area, Inspection Location)`
tokens. A target context cell may contain slash/comma-separated exact values.
A single source MEC row may fan out across multiple exact Area values only when
Tag No and Inspection Location each have exactly one value and every target
row matches a distinct exact tuple. The mapper does not build a Cartesian
product across multiple fields. Missing rows remain outside the current
template scope; no rows, sections, formulas, or cells are created.

The MEC source location header accepts both `INSPECTION LOCATION` and
`INSP. LOC./AREA`.

## Alternatives

- Keep the full composite text as one value: rejected because exact components
  already identify rows in the supplied template.
- Match by substring or first candidate: rejected because it can route an RFI
  to a different block or area.
- Create missing sections or rows: rejected because absent template scope does
  not authorize inventing a destination.

## Consequences and rollback

Exact PCS and MEC contexts can reach their unique existing cells. Composite
values with multiple PCS identifiers, ambiguous destinations, conflicting
source status, or incomplete tuple fields remain manual review. Reverting this
change restores the earlier narrow matching behavior. Workbook changes remain
subject to the existing immutable Plan, guarded Apply, backup, journal, and
Undo flow.

## Propagation

- `docs/PRD.md`, `docs/PRODUCT.md`, `docs/FEATURES.md`, and `docs/RFI_DOMAIN.md`:
  define exact context matching and unchanged template boundaries.
- `docs/TRACEABILITY.md`: link PCS routing, MEC multi-area fan-out, and target
  composite-value tests to DF-016.
- `src-tauri/src/rfi_follow_up.rs`: route exact PCS values and compare explicit
  context tokens.
- `src-tauri/tests/rfi_follow_up_native.rs`: cover PCS route selection, MEC
  explicit multi-area fan-out, and composite target locations.

No UI or locale changes are needed.

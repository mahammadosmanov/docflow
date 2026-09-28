# ADR 0029: Exact multi-area CIV Follow Up mapping

- Date: 2026-09-26
- Status: Accepted
- Extends: [ADR 0008](0008-exact-rfi-log-follow-up-projection.md)

## Context

Some CIV RFI Log records name multiple exact area tags in one `Tag No` cell.
The current Follow Up template has one distinct row for each of those tags.
Treating those rows as an undifferentiated ambiguity leaves valid source data
unsynced even though each destination is identified by a different exact tag.

## Decision

A CIV source record may be projected to multiple existing CIV Follow Up rows
only when every candidate row has a distinct non-empty area tag present in the
source record's normalized `Tag No` values. Each target row must be explained
by a different exact tag. The same RFI identifier and status are applied to
each corresponding area row, including when one or more of those rows already
contain that RFI.

Repeated rows for the same tag, non-CIV multi-row mappings, unsafe targets, and
an RFI already present outside the exact candidate set remain manual review.
No rows, sections, or cells are created; lifecycle status aggregation and
remarks exclusion remain unchanged.

## Alternatives

- Continue to treat all multi-row matches as ambiguous: rejected because
  distinct source area tags identify separate existing destinations.
- Fan out to every row matching any broad source context: rejected because it
  could duplicate an RFI into unrelated areas.

## Consequences and rollback

Exact distinct CIV area tags can now populate all corresponding existing rows.
Ambiguous duplicate-tag layouts remain unchanged for operator review. Reverting
the code restores the prior single-destination behavior; workbook edits remain
reversible through the existing journal and Undo.

## Propagation

- `docs/PRD.md`: update DF-016 with exact distinct-tag fan-out.
- `docs/RFI_DOMAIN.md` and `docs/FEATURES.md`: define the narrow multi-area
  rule and retain duplicate-tag manual review.
- `docs/TRACEABILITY.md`: add exact multi-area and duplicate-tag regressions.
- `src-tauri/src/rfi_follow_up.rs`: allow only distinct exact CIV tag targets.
- `src-tauri/tests/rfi_follow_up_native.rs`: cover safe fan-out and duplicate
  same-tag refusal.

No existing decision is voided. This narrows the meaning of an ambiguous CIV
multi-area match without creating a generic cartesian mapping rule.

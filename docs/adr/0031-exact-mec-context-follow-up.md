# ADR 0031: Exact multi-context MEC Follow Up mapping

- Date: 2026-09-26
- Status: Accepted
- Extends: [ADR 0008](0008-exact-rfi-log-follow-up-projection.md) and
  [ADR 0029](0029-exact-civ-multi-area-follow-up.md)

## Context

The supplied RFI Log repeats MEC identifiers across rows for the same ITP item,
status, and Worley evidence, with each row naming a different exact tag, area,
and inspection location. The Follow Up template has one destination row for
each route context. Treating all such rows as an ambiguity leaves valid source
records unsynchronized. Guessing across partially specified or duplicate
contexts could update the wrong area.

## Decision

A repeated MEC RFI may map to multiple existing MEC Follow Up rows only when
all source records agree on normalized ITP, item, lifecycle status, and Worley
state. Each source record must have one exact tag and inspection location.
`Area` may contain several explicit values in one row; each value forms one
exact `(Tag No, Area, Inspection Location)` tuple with the single tag and
location. Every candidate destination must match one distinct tuple. A target
location cell may contain slash/comma-delimited exact values such as
`N10/N11`; each token is compared exactly. Duplicate destination tuples,
conflicting source values, missing context, multiple tag or location values,
and unrelated pre-existing placements remain manual review.

No section, row, or cell is created. All other multi-row mappings retain manual
review. Existing formula, merge, protection, exact-cell, and Remarks safeguards
remain in force.

Regular lifecycle routing and FINAL Worley projection reconcile exact
destinations independently. A repeated MEC RFI may update only prepared rows
whose full tuple is represented in the source. When only some of several source
contexts have prepared rows, each exact matched destination may proceed and the
uncovered contexts remain manual review. No row or section is created. Duplicate
destination tuples, conflicting or incomplete source values, ambiguous contexts,
and unrelated existing placements remain manual review.

For FINAL Worley projection, a code already present in one exact destination
does not suppress updates to other distinct prepared contexts. Each candidate
row is reconciled independently. Ambiguous contexts or conflicting source
status cannot authorize status changes in a shared cell. A status formula is a
read-only no-op only when its existing RFI is already present and its cached
value matches the projected status; otherwise the target remains manual review.

This extends the distinct-context rule to lifecycle and FINAL Worley
destinations without changing the tuple contract or permitting a cartesian
match.

## Alternatives

- Leave every repeated MEC identifier for manual review: rejected because the
  three exact route fields identify each existing destination in the supplied
  workbook.
- Match on only tag, area, or location: rejected because any one field can
  identify multiple destinations.
- Build a cartesian mapping: rejected because it can copy an RFI into a route
  not present in the RFI Log.

## Consequences and rollback

The repeated MEC entries in the current workbook can sync to their separate
exact target rows. Duplicate or incomplete contexts remain visible for manual
review. Reverting the code restores the prior all-multiple-rows manual review.
Workbook edits remain guarded and journaled for Undo.

## Propagation

- `docs/PRD.md`: define the exact-context MEC exception under DF-016.
- `docs/RFI_DOMAIN.md` and `docs/FEATURES.md`: document context agreement,
  exact matching, and manual-review cases.
- `docs/TRACEABILITY.md` and `docs/RELEASE_SAFETY.md`: include distinct-context
  fan-out and duplicate-context regressions.
- `src-tauri/src/rfi_follow_up.rs`: retain per-source route contexts and fan
  out only across distinct exact MEC tuples, including one explicit multi-area
  field.
  `src-tauri/tests/rfi_follow_up_native.rs`: cover safe fan-out and duplicate
  tuple refusal, destination-specific FINAL fan-out, missing-context review,
  unrelated placements, shared-cell conflict refusal, and cached-formula
  satisfied no-op versus unsafe formula refusal.

No existing decision is voided; this defines a narrow MEC case alongside the
existing exact CIV multi-area rule. No UI or locale changes are needed.

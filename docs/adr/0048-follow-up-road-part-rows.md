# ADR 0048: Follow Up Road part rows

- Status: Accepted
- Date: 2026-10-24
- Extends: [ADR 0046](0046-follow-up-road-parts-and-deleted-rfis.md), supersedes the
  "no row creation" limit of [ADR 0008](0008-exact-rfi-log-follow-up-projection.md) for the
  `Road` sheet only.

## Context

Roads are split into parts while work progresses. The owner wants Follow Up to follow
the RFI Log automatically: `Road 18` becomes `Road 18 (Part 1)` when the Log names its
parts, missing part rows appear, and new rows look exactly like their neighbours,
including formulas.

## Decision

Preview derives the required Road rows from the Log's Road tags (CIV ITP-0004 records
that are not cancelled/rejected):

- A Road named only without a part keeps (or gets) one partless row `Road NN`.
- A partless row whose Road is named with parts is relabelled `Road NN (Part 1)`.
- Every explicitly named part without a row is inserted: after the nearest lower part,
  or before the first part when it is lower than all existing ones. A new Road is
  placed after the highest lower-numbered Road, or first.
- Rows are never removed or renumbered; an empty part stays.
- A new row is cloned from its neighbour like Excel's Insert Row: styles, row height,
  formulas (translated, shared formulas expanded first), with RFI and status cells
  empty. The zone cell is copied only for another part of the same Road; for a new Road
  it stays empty for the operator.

All references follow the insertion the way Excel shifts them: formulas on every sheet,
ranges (a range ending on the row above the insertion that spans several rows grows),
merged cells, conditional formatting, data validation, auto-filter, print area, defined
names, charts, and the dimension. A stale `calcChain` is removed. Apply builds this
structured baseline in memory from the approved operation list, writes the approved cell
updates onto it, and verifies the published file equals baseline plus exactly those
updates. Classification runs on the structured baseline, so every reviewed coordinate is
the final one. Structure operations are shown as unselectable review rows, validated to
the `Road` sheet, column B labels, and zone copy only, and are applied only together with
at least one approved cell update or remark. They are refused for signed packages, and
the package is refused if another sheet's shared formula points into the Road sheet.

Mass-deletion brake (ADR 0046 addition): when more than 50 RFIs, and more than a fifth of
Follow Up's RFIs, are absent from the Log, removal is paused and a warning names the
count, because the wrong or an outdated RFI Log is the likely cause.

## Consequences

Road upkeep is automatic and reviewable. Excel cannot be run in the development
environment, so the row insertion is verified structurally (against the real workbook
layout) and must be certified on Windows/Excel. Undo restores the exact original bytes.

## Rollback

Stop planning structure operations. Written rows remain ordinary rows and Undo restores
the original file.

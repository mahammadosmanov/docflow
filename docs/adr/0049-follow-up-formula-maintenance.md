# ADR 0049: Follow Up formula maintenance and per-cell remarks

- Status: Accepted
- Date: 2026-10-25
- Amends: [ADR 0047](0047-follow-up-generated-row-remarks.md)

## Context

A Windows run wrote Completed RFIs into a remark although only a different cell of the
row held a mixed group. The owner also found Road rows whose right-hand formula cells
(`Y`, `Z`) were empty and expects Follow Up sync to maintain the workbook, not only
mirror the Log, without adding anything unnecessary.

## Decision

1. Remarks are decided cell by cell. Only RFI cells that hold two or more RFIs with at
   least one not Completed contribute; the remark lists those cells' RFIs (Completed
   first, then In Progress) and nothing else from the row. No such cell: a generated
   remark is cleared.
2. On `CIV`, `PCU`, and `Road`, a formula column is a column to the right of the
   lifecycle RFI/status pairs where at least three rows and 60% of the span carry a
   formula. A populated data row inside that span whose formula cell exists but is
   completely empty gets the column's formula, translated from the nearest formula
   above it. Cells holding any value or formula, absent cells, blank rows, and rows
   outside the span are never touched. The repair is an unselected-by-default-safe,
   individually reviewable row; Apply writes only a plain formula without cached value
   (Excel computes it on open, already requested by ADR 0045) and verifies it.

## Consequences

Follow Up sync now also repairs missing formulas. A typed value in a formula column is
left alone and surfaces nothing; it must be fixed by hand.

## Rollback

Stop producing formula repair rows. Written formulas are ordinary cells and Undo
restores the exact original bytes.

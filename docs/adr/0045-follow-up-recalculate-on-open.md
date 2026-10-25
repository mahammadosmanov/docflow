# ADR 0045: Follow Up Apply requests recalculation on open

- Status: Accepted
- Date: 2026-10-24

## Context

Follow Up sheets carry percentage indicators computed by formulas, for example
`Road!Y5 = (COUNTIF(C5:P5,"Completed")+COUNTIF(C5:P5,"NA"))/6` and
`Road!Z3 = AVERAGE(Y5:Y48)`. Apply patches the literal RFI and status cells in the
worksheet XML. Excel does not mark XML-patched cells dirty, so the cached formula
values stay stale and the operator saw unchanged percentages even after opening
the file or pressing F9.

## Decision

Follow Up Apply additionally sets `fullCalcOnLoad="1"` on the existing `<calcPr>`
element of `xl/workbook.xml`. This is the only change allowed in that package
member; Apply's delta verification compares the whole workbook part against the
source with exactly that attribute added and rejects any other difference. A
workbook without `calcPr` is left byte-identical. Formulas, cached values, and
every other part are untouched, and the operator's Excel recalculates everything
once on open.

Other operations (RFI Log status, system sync, publish status) keep their existing
byte-identical workbook-part rule.

## Consequences

Excel recalculates the whole workbook the next time it opens the file and will ask
to save on close. Stale percentage indicators refresh without manual work. Undo
restores the exact original bytes, including the original `calcPr`.

## Rollback

Switch Follow Up back to `patch_workbook_sheets_guarded_text` and
`verify_guarded_workbook_delta_text`.

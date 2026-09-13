# ADR 0001: Guarded current RFI workbook synchronization

- Date: 2026-09-13
- Status: Accepted

## Context

The first `rfi.system_sync` implementation assumed that RFI Log REMARKS lived in
columns 20/21 and that Follow Up used the retired `Signed by CIV` / `Signed by
MEC` Final-ITP layout. In the current workbooks, columns 20/21 are shared-formula
Folder Link fields, REMARKS is discovered from the exact header, and the writable
Follow Up authority is the `Signed By Worley CIV` and `Signed By Worley MEC`
control tables. Replacing the formula cells caused Excel to remove shared-formula
and calculation-chain records during recovery.

## Options considered

1. Keep fixed coordinates and update them whenever a workbook changes.
2. Rewrite workbooks through a general spreadsheet library.
3. Discover exact headers, authorize only existing literal cells, and verify a
   closed OOXML package delta before publishing.

## Decision

Use option 3. RFI Log requires one exact `RFI No.` and `REMARKS` header on each
discipline sheet. Follow Up requires the CIV `RFI No.`/`STATUS` table and MEC
`RFI No.`/`Worley Status` control table on uniquely named `Signed By Worley`
sheets. Formula-bearing, merged, absent, stale, unexpected, or ambiguous cells
are manual-review outcomes and never authorized. Existing remarks are preserved
and the exact `Signed by Worley` line is appended only when absent.

The writer may change only approved cell value representations. Runtime delta
verification requires every other XML event and every non-target ZIP member,
including shared formulas, `calcChain.xml`, VBA, links, styles, and extensions,
to remain unchanged.

## Consequences

- Workbook column movement is tolerated when exact unambiguous headers remain.
- A changed or unfamiliar template fails closed and requires a new preview or a
  reviewed mapping change.
- Production workbooks remain external test inputs and are never committed.
- Microsoft Excel open/recalculate on Windows remains a separate release gate;
  package-level acceptance does not claim Office certification.

## Rollback

Reinstall the previous immutable release for application rollback. Restore any
document only through the operation's verified backup/journal undo; never use a
source-code rollback as document recovery.

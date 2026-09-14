# ADR 0008: Exact RFI Log to Follow Up projection

- Date: 2026-09-14
- Status: Accepted

## Context

The operator maintains RFI status in RFI Log and then repeats selected CIV/MEC
facts in a structurally different Inspection Status Follow Up workbook. Follow
Up contains shared RFI cells, compact historical identifiers, formulas, FINAL
Worley tables, and intentionally absent ITP sections. Generic row matching or a
normal spreadsheet rewrite can corrupt formulas, macros, and calc-chain state.

## Decision

Extend `rfi.follow_up_sync` instead of adding a second overlapping operation.
Preview discovers source columns from exact headers, accepts only the existing
CIV/MEC domain, and routes records only to an existing unique CIV, MEC, PCU, or
Road cell. It expands only exact same-discipline compact RFI forms and bounded
ranges. Blank, Ongoing, and Completed become Pending, In Progress, and Completed;
mixed active groups become In Progress. Cancelled and Rejected tokens are
removed before aggregation.

Signed By Worley processing is limited to the existing CIV/MEC FINAL tables.
`Signed` requires an exact `Signed by Worley` line in REMARKS; otherwise the
known FINAL record is `Not Signed`. ELE, PRC, POT test, STAT, Changelog, absent
ITP/item sections, formulas, merged cells, protected cells, and ambiguous
multi-area/multi-section mappings are outside mutation authority.

The approved Plan contains the exact old and replacement value for every RFI
and status cell. Apply revalidates both input fingerprints, writes a private
copy with targeted OOXML text patches, verifies the closed package delta, and
then uses the existing retained-backup, journal, recovery, and Undo protocol.

## Consequences

Safe uniquely mapped work becomes automatic, while unsupported or ambiguous
work remains unchanged and is counted in preview warnings. No worksheet,
section, row, formula, or validation is synthesized. Supporting a future Follow
Up template requires an explicit reviewed mapping and new regression evidence.

## Rollback

Workbook changes are reversed through the operation journal and retained exact
backup. Code rollback uses the previous immutable version; it is never a
substitute for document Undo.

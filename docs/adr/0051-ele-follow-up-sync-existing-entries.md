# ADR 0051: ELE Follow Up sync for existing entries

- Date: 2026-10-31
- Status: Accepted

## Context

The Inspection Status Follow Up workbook now has a prepared `ELE` sheet. The
current Follow Up operation excludes ELE and its RFI Log records. The supplied
workbooks contain two item groups whose RFI numbers are already present in
prepared ELE rows, plus other source items and a blank `1.6` section.

## Decision

Extend `rfi.follow_up_sync` to process only these ELE source pairs:

| Source ITP | Source item | Follow Up section group |
|---|---|---|
| `ELE-0005` | `ELE-5.2.6` | `2.5, 2.6` |
| `ELE-0006` | `ELE-6.2.5.3` | `2.1, 2.4, 2.5.1, 2.5.3` |

Match one exact source `Tag No` beginning `PCS-` to the target row's `Block`.
Update lifecycle status only when the same RFI already exists in its routed
target column. Never populate an empty RFI cell. Do not route `1.6` or any other
ELE ITP/item pair, create rows or sections, or change worksheet Remarks.
Unsupported ELE source records stay outside Follow Up review and existing target
cells remain unchanged.

Keep the existing Preview, immutable Plan, guarded Apply, backup, journal, and
Undo controls. Ambiguous tags, duplicate target blocks, unsafe cells, and
conflicting source records cannot authorize a status write.

## Consequences

The existing ELE entries can receive RFI Log status updates through the current
operation. New or unlisted ELE identifiers remain untouched until a separate
domain decision approves them.

## Rollback

Workbook changes use the existing retained-backup and Undo flow. Code rollback
uses the previous immutable version.

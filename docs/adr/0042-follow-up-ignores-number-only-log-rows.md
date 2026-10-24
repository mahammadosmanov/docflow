# ADR 0042: Follow Up ignores number-only RFI Log rows

- Status: Accepted
- Date: 2026-10-24

## Context

The operator pre-numbers rows in the RFI Log. A CIV sheet held about 18,000 rows
containing only an RFI No. Follow Up read each as an RFI with an unknown route and
listed it in Manual Review, burying the actionable rows and nearly reaching the
20,000-row source limit.

## Decision

A source row whose Inspection Status, Remarks, ITP No, Item No, Tag No, Area, and
Inspection Location are all blank is not an RFI record. It is skipped before
routing and produces no Manual Review row, no proposal, and no workbook change.

A row with any of those fields filled, including a status with no ITP, is still a
record and still appears in Manual Review when it cannot be routed.

## Consequences

Pre-numbered placeholders no longer affect Follow Up. A genuinely empty row can no
longer be reported as a missing mapping, which is intended: it carries no claim to
reconcile.

## Rollback

Remove the blank-row check in `rfi_follow_up/source.rs`; no stored format changes.

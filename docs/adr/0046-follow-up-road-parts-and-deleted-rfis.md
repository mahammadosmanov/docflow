# ADR 0046: Follow Up Road parts and deleted RFIs

- Status: Accepted
- Date: 2026-10-24

## Context

Roads are split into parts while work progresses (`Road 16 (Part 1)`). Follow Up
holds one row per part, with every part tracking the same RFI when a Log record
names the Road without a part. The previous exact tag comparison matched only
identical text, so partless and multi-part Log tags never reached part rows. Also,
an RFI removed from the Log stayed in Follow Up cells because only Cancelled and
Rejected were treated as removals.

## Decision

1. Road matching understands the part. A Log tag without a part belongs to every
   part row of that Road, including partless rows and parts added later. A tag
   with a part belongs only to that part. Lists such as
   `Road 15 (Part 1), (Part 2)` and `Road 15 (Part 1),Road 15 (Part 2)` belong to
   each listed part. Text that is not a Road tag keeps the previous exact
   comparison. A named Road or part with no Follow Up row is a partial-context
   Manual Review item while the matching rows still synchronize.
2. The RFI Log is the source of truth. An exact RFI number that appears nowhere in
   the Log's CIV or MEC RFI No. columns, not even as a bare number, is treated as
   deleted: Preview proposes removing it from its Follow Up cell, recalculating
   the cell status, under the normal approval, journal and Undo. A number that is
   present in the Log without record data is not deleted. Placeholder numbers
   (2000 and above) are never removed.

Row creation, deletion, and renaming of Road parts, and Remarks, are separate
decisions.

## Consequences

Part-less Road RFIs no longer land in Manual Review. A Follow Up RFI that was
deleted from the Log is cleaned automatically after approval. Choosing the wrong
RFI Log removes entries, so removals appear as ordinary reviewable rows.

## Rollback

Restore the exact Road comparison and the Cancelled/Rejected-only removal rule.

# ADR 0060. Retire selected RFI operations and background automation

- Date: 2026-11-28
- Status: Accepted
- Release: 2.0.0

## Context

The operator requested removal of Outlook Reply All, Analyze RFI System & Sync
Worley Signatures, Prepare RFIs for Publishing, Mark Completed RFIs, Action
Queue, and unattended RFI System sync.

## Decision

Removing navigation alone would leave stale clients able to request the
operations. Remove their active commands and dispatch routes as well as
navigation and the Action Queue worker. Keep validated recovery for old journals
and shared code still used by remaining operations. Historical operation IDs
continue identifying their original journal records.

Keep Batch Print and its independent queue, RFI Log Add, Follow Up Sync,
signed-page staging, and the read-only RFI Log status check in Fast RFI Scan.
Windows startup remains an independent per-user preference.

## Rollback and migration

The upgrade leaves existing documents, journals and stored preferences in place.
Disabled features reject new previews. Reinstall the prior signed release if an
operator needs to restore an older workflow.

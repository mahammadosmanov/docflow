# ADR 0030: Bounded S5 retained-file capacity

- Date: 2026-09-26
- Status: Accepted

## Context

Follow Up Apply reserves three retained files per operation. The previous
64-file ceiling allowed only 21 complete Apply reservations before a retry
failed on file count, even when the retained-byte quota still had ample room.

## Decision

Raise the shared S5 retained-file ceiling from 64 to 256. Keep the 8 GiB
logical-byte ceiling, no-follow regular-file checks, overflow checks, and
fail-before-mutation reservation behavior unchanged. The total projected file
count across the workbook directory and `.docflow-backups` must stay within 256.

## Alternatives

- Remove retained backups automatically: rejected because committed journal
  backups support exact guarded Undo.
- Remove the bounded limits: rejected because recovery storage must stay
  finite and preflighted before Apply.

## Consequences and rollback

The count ceiling now accommodates at least 85 complete three-file Follow Up
Apply reservations when no other S5 files are present; the 8 GiB ceiling may
still bind first for large workbooks. Any true quota exhaustion still fails
before modifying the workbook and requires reviewing retained recovery files.
Rollback restores the 64-file limit without changing existing journal formats.

## Propagation

- `src-tauri/src/journal.rs`: set the shared S5 retained-file bound to 256.
- `docs/PRD.md`: document the 256-file / 8 GiB recovery bound.
- `docs/RELEASE_SAFETY.md`: retain quota and no-mutation-on-exhaustion checks.
- `docs/TRACEABILITY.md`: include bounded retained-storage coverage.

No journal format or existing recovery authority changes.

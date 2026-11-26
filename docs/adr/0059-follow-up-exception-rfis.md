# ADR 0059: Follow Up sync leaves exception RFIs alone

- Date: 2026-11-26
- Status: Accepted
- Amends: ADR 0056 (placement and removal), ADR 0035 (scope exclusions)

## Context

Some old RFIs are recorded wrongly in the RFI Log and cannot be corrected there
any more. Follow Up sync reads the Log as truth, so for these RFIs it proposes
wrong status changes, removes them as "deleted from the Log", or places them in
the wrong row. The operator asked for a way to mark such RFIs so sync does not
touch them. On 2026-11-26 the operator also decided that the list must be kept
locally and in the cloud, that a shared cell must still sync its other RFIs, and
that skipped exceptions need no warning.

## Decision

- `rfi.follow_up_sync` has an optional `exception_rfis` long-text parameter. One
  code per line, comma or semicolon; full (`AZE-SUN-INT-QMS-RFI-CIV-1374`) or
  short (`CIV-1374`) form. `-CODE` removes a code from the stored list.
- The effective list is `(local ∪ cloud ∪ added) − removed`. It is saved to the
  settings section `followUpExceptions` and to the Supabase table
  `follow_up_exceptions` (`docs/supabase_follow_up_exceptions.sql`). Offline, the
  local list applies; additions reach the cloud on the next online run. A removal
  needs the cloud, so it cannot come back from there.
- An exception RFI is removed from the Log records before classification and
  Road row planning. Sync therefore never places, restores, moves, removes or
  reports it, and never counts it as deleted from the Log.
- A Follow Up cell that holds only exception RFIs is not touched. In a cell that
  also holds other RFIs, the exception codes keep their place, the other RFIs
  sync normally, and the status is computed from the other RFIs only.
- No warning or row is shown for a skipped exception.
- The exception list is stored in the plan, so Apply-time revision classifies
  with the same list. The legacy (non full-schema) Log path refuses a non-empty
  list instead of syncing without it.
- The list is a preference. It is not history and never authorizes a mutation
  or Undo; preview → approval → apply → journal → undo is unchanged.

## Consequences

- A wrong or forgotten exception hides that RFI from sync. A removal is explicit.
- The Supabase SQL file must be run once; without it the cloud copy is skipped.
- The status of a shared cell ignores its exception RFIs.

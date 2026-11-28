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

- `rfi.follow_up_sync` has an optional `exception_rfis` long-text parameter.
  Enter one RFI per line or separate codes with commas or semicolons. The
  field accepts full and short forms. These codes add protection only to the
  current plan. Preview reads stored exceptions and makes no changes to the
  local or shared lists.
- The protected set combines the current Supabase `follow_up_exceptions` rows,
  inherited local `followUpExceptions` codes, and codes entered for this run.
  If the shared list is unavailable, Preview warns that it is using only the
  local and one-run codes.
- Approved users manage the shared list in Settings through one explicit add
  or remove per canonical code. The native command confirms that code's
  resulting state. Shared edits use the existing table and access policies
  (`docs/supabase_follow_up_exceptions.sql`) and leave other codes untouched.
- Older settings may contain a mix of previously shared codes and codes added
  while offline. Keep them as local protections because their provenance is
  unknown. Settings displays them separately and can remove them from this
  computer. Old entries never upload automatically. New shared edits do not
  create local copies.
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
- Shared edits require the existing Supabase table and a working connection.
- The status of a shared cell ignores its exception RFIs.

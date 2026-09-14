# ADR 0011: Actionable signed-page warning review

- Date: 2026-09-14
- Status: Accepted

## Context

Signed-page warnings previously described OCR or archive failures but did not
place the affected input PDF in the operator's Review area. Subjects were often
too terse to identify a page in a combined scan. Making an implicit copy during
Apply would violate immutable-plan and review-root boundaries, while moving or
splitting the scan would alter the only signed source evidence.

## Decision

Preview records structured source PDF, page when known, exact RFI when known,
reason, and Review destination evidence. It adds at most one ordinary Copy step
per affected source PDF to `<review root>/_review/<signed-relative-path>`. The
step pins the source SHA-256. Apply revalidates the hash, follows the ordinary
write-ahead journal and Undo path, refuses symlinks and destination collisions,
and never moves, splits, renames, or modifies the source.

Several warning pages in one combined scan remain several reason records attached
to one full-source copy. An existing or structurally blocked Review destination
is visible in Preview and is never overwritten. Informational cleanup of a
verified same-file stale temporary name does not create a warning copy.

## Consequences

- Every actionable warning has a recoverable file in a predictable Review tree.
- The immutable Plan, not Apply-time discovery, determines every copy.
- A source changed after Preview fails closed and requires a new Preview.
- Native Windows and OneDrive acceptance remains an external release gate.

## Rollback

Remove the warning Review Copy steps and structured presentation metadata. Existing
journaled copies remain ordinary undoable effects; no source migration is needed.

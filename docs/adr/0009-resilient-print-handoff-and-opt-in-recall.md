# ADR 0009: Resilient print handoff and opt-in setting recall

- Date: 2026-09-14
- Status: Accepted

## Context

Some Windows physical-printer drivers, including the operator's EPSON L15160
Series environment, remove a successfully spooled job before DocFlow's first
status poll. The previous implementation treated a missing record and transient
offline, paper-out, blocked, or user-intervention flags as immediate uncertainty,
which stopped an otherwise valid queue within seconds.

Batch Print also restored every remembered value automatically. That made a new
visit look like the previous procedure and could retain a stale file selection,
even though task paths must not be reusable preferences.

## Options considered

1. Keep strict status polling and require the operator to resume every false
   uncertainty. This preserves the old behavior but makes the physical printer
   workflow unreliable.
2. Stop observing the spooler entirely after `EndDoc`. This avoids false errors
   but discards useful cancellation, timeout, and explicit fault evidence.
3. Treat normal post-`EndDoc` queue release as successful handoff, wait through
   recoverable status flags, retain explicit terminal faults as uncertain, and
   make prior safe-setting recall an explicit UI action.

## Decision

Choose option 3. A controlled print job is considered handed off when Windows
successfully accepts `EndDoc` and then either reports completion or no longer
retains the job record. Error, deleting, and deleted flags remain uncertain and
are never automatically retried. Offline, paper-out, blocked, and user-action
flags remain pending until completion, cancellation, or the approved timeout.
Single status-read/open failures receive a bounded three-second retry window;
that retry observes the same job and never resubmits it.

For native file-association handlers, Windows may accept `ShellExecute` while
the application or EPSON driver never exposes a uniquely identifiable spool
record. After the bounded acquisition window this becomes
`completed_unconfirmed`: DocFlow journals the accepted submission, forbids a
retry, warns the operator, and continues the remaining serial queue. An explicit
spool fault or uncertain cancellation still pauses and is not converted to this
state. This separation avoids both the repeated false stop and duplicate paper.

This includes the two-call `GetJob` race: if the driver releases the record
between the size probe and buffered read, the already accepted handoff is
complete. If the record grows, DocFlow retries only the read with a larger
buffer. Any other query error stays uncertain.

Batch Print's file parameter is non-remembered. Safe scalar settings are stored
only when Preview is requested. A new visit uses schema defaults and offers an
explicit one-click restore of the previous safe settings. Named presets remain
the long-lived reusable-procedure mechanism.

## Consequences

- Fast-purging Windows drivers no longer create an immediate false failure.
- Accepted native-handler jobs without a final receipt no longer stop every
  later file; their warning remains visible in the completed result.
- Temporary device conditions can recover without losing queue order.
- Physical output still cannot be certified by software; explicit terminal
  faults remain visible and non-retryable.
- Previous files are never silently selected. Restoring settings cannot resume
  or authorize an old queue.
- EPSON L15160 behavior still requires native Windows operator acceptance.

## Rollback

Reinstall the previous immutable release to restore application behavior. Saved
print queues remain compatible. Do not roll back by editing queue JSON or by
automatically retrying an uncertain submitted job.

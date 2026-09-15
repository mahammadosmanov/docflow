# DocFlow project memory

- The authoritative repository is `/Users/rohtolos/Documents/GitHub/docflow`.
- Print submission is serial. Never automatically resubmit an uncertain job;
  it may already have produced paper.
- Windows printer status bits can be transient or stale. Generic error,
  offline, paper-out, blocked, and user-intervention states require bounded
  observation; deleting/deleted states are immediately uncertain.
- Native Office handlers may replace the source filename in the spool title.
  Correlate exactly one new job, but fail closed when concurrent jobs make the
  result ambiguous.
- Native Windows and physical-printer acceptance are separate from source tests
  and Windows cross-build/package verification.

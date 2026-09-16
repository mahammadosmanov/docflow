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
- Completion Status Sync accepts only exact canonical `RFI No. ...pdf` names,
  recursively scans without following symlinks, and changes only literal blank
  or `Ongoing` RFI Log status cells after preview. Terminal, unknown, formula,
  merged, missing, and absent-row cases remain manual review.
- Completion evidence and Worley signing are separate authorities. Completion
  Status Sync never changes Follow Up or Worley fields and never opens or
  mutates the source PDFs.
- Replica publication is one-way: one explicit source, two targets, and a
  separate retained-backup root. It never deletes target-only files.
- Unattended publication is standing authorization only for the exact profile
  successfully applied once. Any unsafe error pauses it; changing the profile
  requires another explicit Preview and Apply.
- Windows startup uses the current-user Run key and grants no extra filesystem
  authority. Native Windows/OneDrive/GlobalProtect acceptance remains external.

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
- Worley signing authority is intentionally narrow: only the exact
  case-insensitive `RFI No. <canonical RFI>.pdf` and only actual ink in the
  bottom-right page-one OTHERS name/signature cell. Never fall back to another
  PDF or infer signing from contractor signatures, dates, notes, or other marks.
  Every detector change must use a new scan-index profile.
- RFI Log to Follow Up authority covers existing CIV, MEC, PCU, Road, and
  Signed By Worley CIV/MEC input cells. Lifecycle status and exact FINAL
  Signed/Not Signed values may change; report sheets and formulas recalculate
  naturally but are never rewritten.
- Missing FINAL identifiers may use only one uniquely matched, already-prepared
  literal RFI/status pair. Never append a row, synthesize an absent section,
  replace a formula, or guess an ambiguous location.
- The current PCU template has a separate FINAL `Signed by Worley` cell in
  column R keyed by the existing FINAL RFI in column P. Sync it from the exact
  RFI Log REMARKS line, but keep POT test, calculated remarks, and manual
  overrides outside mutation authority. Insert a missing PCU FINAL lifecycle
  value first and offer its Worley status on the next preview.
- RFI Log to Outlook continuation requires the exact committed `rfi.log_add`
  journal, owner/session, and Plan digest. Reuse the approved table values in a
  separate Outlook preview; never recalculate identifiers, invent recipients or
  message prose, or add send authority.
- PDF orientation uses 250/350-DPI OSD consensus, then a conservative
  four-direction English OCR score only for unresolved pages. The fallback must
  keep minimum word, average-confidence, winner-ratio, and margin gates; sparse
  drawings or close 0/180-degree results remain manual review. Every detector
  change requires a new scan-index profile.

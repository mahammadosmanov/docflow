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
  four-direction English OCR score only for unresolved pages. If the 250-DPI
  score remains uncertain, a 350-DPI pass may decide with the same strict gates
  or with matching OSD and OCR evidence at both resolutions plus separate
  minimum word, average-confidence, winner-ratio, and margin gates. Sparse
  drawings or conflicting results remain manual review. Every detector change
  requires a new scan-index profile.
- Long orientation scans may exceed the 30-minute path-grant TTL. Authorize the
  exact selected roots before scanning, then bind the cached Plan to fresh
  exact-root grants after scanning; do not extend authority to another path.
- The owner granted standing authorization to back up every completed DocFlow
  slice to the canonical private GitHub repository. After applicable checks,
  commit and push the source, create the immutable version tag and GitHub
  Release, and upload only commit-bound verified assets. This authority never
  includes secrets, operator documents, deployment, spending, or production
  data mutation.
- Windows refuses workbook replacement while any ordinary preview or
  fingerprint `File` handle remains open, even though Unix permits the same
  sequence. Explicitly drop those handles before guarded backup/replacement;
  keep Windows fixture handles scoped the same way as production.
- Preserve native Windows `io::Error` codes across capability-layer boundaries
  used by retry policy. Converting `os error 32` to a string disables the
  existing outer sharing-violation retry even when the message still displays
  the original code.
- Windows may permanently reject replace-by-rename for an existing replica
  target even with delete-sharing handles and bounded retries. For this
  operation, use the existing same-handle verified overwrite: validate the old
  hash, retain backup and pending journal first, write/flush/verify, then remove
  the staged copy.
- Quick-XML represents `<t></t>` with no text child. The guarded Follow Up
  verifier must accept that shape only when the approved replacement is exactly
  empty; otherwise Cancelled/Rejected removal can write a safe private output
  and then reject it before publication.
- A strict RFI regex match proves only that OCR produced a plausible shape; it
  does not prove the digits are correct. Process Scans image OCR requires one
  conflict-free code across at least three DPIs and two exact reads. In an
  established archive, a detected code must also have an existing exact RFI
  folder or remain in Review.
- Searchable PDF text is not visible-page truth. It may corroborate or veto the
  visible OCR consensus, but never authorize RFI naming alone; conflicting or
  multiple exact codes remain Review.
- Describe evidence only inside its proven boundary: heuristic scores are not
  accuracy probabilities, spool handoff is not physical paper output, and an
  unknown external result must never be relabelled as success.
- Corpus folder and filename agreement is reference evidence, not OCR ground
  truth. Every OCR/reference mismatch must stop the audit for visual review;
  a mismatch can reveal a misfiled production PDF rather than an OCR error.
- RFI identity OCR may use the bounded raw-grayscale PSM 11 fallback only after
  the adaptive cascade fails. It still needs one conflict-free code across at
  least three DPIs and two exact reads; a single competing code keeps Review.

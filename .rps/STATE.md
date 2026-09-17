# DocFlow project state

Updated: 2026-09-17

## Current release

- Version: 1.6.1
- Branch: `review/rust-tauri-migration`
- Implementation commit: `989113b`
- Objective: prevent false Worley-signature classification by requiring the
  exact canonical RFI PDF and using only the bottom-right OTHERS signature cell.

## Verified

- Durable replica hashes are reused only for the same owner, operation, source
  root, profile, relative path, identity, size, and modified time; Apply still
  live-revalidates source and target hashes.
- Pathless support diagnostics include printer inventory and the latest
  caller-owned queue's route, settings, attempts, outcome, and available error.
- Automatic replica runs defer for recently modified source files and stop
  before configured file-effect or total-MiB mass-change limits are exceeded.
- Excel automation no longer writes global calculation properties before a
  workbook exists, and pathless diagnostics retain COM stage/HRESULT evidence.
- Excel private PDFs receive bounded envelope checks; Windows' native decoder
  owns load and non-zero page-count validation before `StartDoc`.
- The incomplete-queue action explicitly identifies failed-only retry; completed
  and completed-unconfirmed jobs remain disabled in resumed plans.
- Operator acceptance passed on Windows with both supplied DOCX files and every
  selected worksheet from both supplied MDR workbooks; no application error or
  missing paper output was reported.
- Selected-mail archive authority is exact owner/session-bound, single-use, and
  fingerprint-revalidated; generated MSG writes are bounded, no-clobber,
  journaled, and hash-guarded for Undo.
- RFI System Sync accepts no arbitrary-PDF fallback. Its new scan profile
  measures only residual ink inside the page-one OTHERS name/signature cell and
  invalidates all prior detector cache entries.
- The supplied signed CIV-1041 and unsigned MEC-0737 PDFs pass the external
  regression without entering Git; exact-filename, outside-cell, and missing-PDF
  synthetic regressions also pass.
- Rust, Node, React, frontend, version, documentation, source, and license checks
  pass. Windows x64 Rust check and test-target compilation pass as recorded in
  `docs/VERIFICATION.md`.

## External gate

- Continue with the SharePoint/OneDrive, GlobalProtect/network, local, and
  backup roots. Confirm startup, publication,
  one-minute cadence, settle timing, mass-change brakes, interruption, and Undo.
  Export a support report after representative EPSON
  L15160 Series success, partial output, empty-paper signal, and uncertain jobs;
  compare it with actual physical output before changing retry policy.
- On classic Outlook for Windows, archive one non-sensitive selected mail into
  an isolated folder. Verify readable MSG content/attachments, mailbox
  immutability, selection-drift refusal, collision refusal, and guarded Undo.
- On Windows, Preview isolated copies containing OTHERS-signed, OTHERS-empty,
  outside-cell-only, faint-signature, and missing-exact-PDF cases. Confirm only
  authoritative OTHERS ink is actionable before using a production workbook.

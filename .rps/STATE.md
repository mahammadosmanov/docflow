# DocFlow project state

Updated: 2026-09-19

## Current release

- Version: 1.9.0
- Branch: `review/rust-tauri-migration`
- Implementation commit: `68f4610`
- Objective: continue a successful guarded RFI Log assignment into an official
  unsent Outlook Reply All draft without recalculating identifiers or granting
  send authority.

## Verified

- The reply continuation is offered only after a non-partial committed
  `rfi.log_add` journal and requires the exact owner, session, Plan digest, and
  journal identity. CIV/MEC/ELE/PRC tables retain their approved values and
  canonical order in plain text and escaped HTML.
- The Outlook reply remains a separate preview/Apply operation. It neither adds
  recipients nor sends mail, and the established selection-fingerprint and
  unsent-draft journal/Undo controls remain in force.
- Rust 1.97.1 formatting, Clippy, Cargo check, 353 serial unit tests, 220
  integration tests, 23 Node tests, 54 React tests, frontend build, and all
  source/docs/version/license audits pass. Four operator-file tests remain
  intentionally ignored. Native Windows Outlook acceptance remains external.
- The 1.9.0 Windows x64 installer, portable executable, and OCR-inclusive ZIP
  were cross-built from `68f4610`. Release checksums, ZIP integrity, package
  shape, bundled OCR checksums, and GUI-subsystem checks pass. Execution on
  native Windows remains external.

- The supplied current Follow Up workbook was inspected read-only. Its existing
  lifecycle layouts remain compatible; the new PCU FINAL Worley column is now
  discovered only from its exact header and paired existing FINAL RFI.
- Focused Follow Up tests cover PCU Signed/Not Signed projection, protected POT
  exclusion, main-before-Worley insertion order, and exact-cell mutation.
- Full Rust, Node, React, source, documentation, version, and license gates pass
  for 1.8.0. The Windows x64 installer, portable EXE, and OCR-inclusive ZIP were
  cross-built from `4f290bc`; checksums, ZIP integrity, package shape, and GUI
  subsystem checks pass. Native Windows/Excel/OneDrive execution remains an
  external acceptance gate.

- A read-only inventory of the consented Ventoy RFI System found 3,590 PDFs
  under CIV/MEC/ELE/PRC. A deterministic sample showed the former single
  150-DPI pass was the main recognition bottleneck; 250/350-DPI passes and
  conservative four-direction OCR recovered clear form orientations while
  rejecting sparse drawings and close 0/180-degree results.
- Orientation cascade unit tests and all six PDF-orientation integration tests
  pass. The detector profile changed, so old cached decisions cannot cross the
  algorithm boundary.
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
- Follow Up Sync recalculates existing CIV/MEC lifecycle groups, removes
  Cancelled/Rejected identifiers, and synchronizes exact FINAL Worley
  Signed/Not Signed values. A missing FINAL identifier is added only to one
  uniquely mapped, already-prepared literal RFI/status pair.
- Formula, merged, protected, absent, non-FINAL, and ambiguous Signed By targets
  remain visible manual review and are not authorized for Apply.
- The available prior RFI Log/Follow Up copies passed bounded preview/revision
  and closed OOXML-delta verification without modifying the originals.
- Rust, Node, React, frontend, version, documentation, source, and license checks
  pass. The Windows x64 cross-build, package audit, checksums, and GUI-subsystem
  checks pass as recorded in `docs/VERIFICATION.md`.

## External gate

- On classic Outlook for Windows, select a non-sensitive RFI request, complete
  an isolated RFI Log Apply, prepare the reply, and verify exact table rows,
  Reply All recipients, display-without-send behavior, selection-drift refusal,
  durable draft journaling, and guarded Undo.

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
- On Windows, run Follow Up Sync on isolated copies of the current RFI Log and
  Follow Up. Confirm lifecycle groups and FINAL Signed/Not Signed values,
  prepared missing FINAL rows, Excel repair-free open/recalculate, backup, and
  Undo before production use.

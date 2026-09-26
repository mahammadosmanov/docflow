# DocFlow project state

Updated: 2026-09-26

## Current candidate

- Version: 1.10.4
- Branch: `rohtolos/follow-up-unmatched-skip`
- Objective: skip supported RFI Log routes absent from the Follow Up template
  while retaining manual review for non-unique row matches.
- Release state: all local source gates pass on macOS with Rust 1.97.1. The
  supplied workbook preview was read-only. PR #34 is open. Linux CI still fails
  when `ld` crashes with signal 7 during test linking, including after Cargo
  jobs were bounded to one. Windows workflow `36221590137` passed build,
  installer smoke, and manifest checks on feature commit `c07282d`; its private
  draft installer SHA-256 is
  `845642ca220ecfa0722f81218a300163482ec4feffc2e3027e4e282fe995455c`.
  Review, immutable tag, and stable GitHub Release remain pending.

## Current slice

- Recognized CIV/MEC routes with zero rows in the current Follow Up template are
  skipped without an unmapped-record warning, including FINAL routes without a
  prepared row. Multiple matching rows still go to manual review; unique rows
  continue through the existing guarded sync.
- Read-only preview of the supplied workbooks reduced the unmapped warning from
  173 to 7: 166 no-match routes are skipped, 7 multi-row matches remain manual.
  The preview also found 361 ready lifecycle updates, 1 ready Worley update, and
  42 Follow Up entries missing from RFI Log source data. Synthetic regression
  tests cover standard/FINAL no-match skips and ambiguous-row review. Rust tests,
  format, Clippy, Cargo check, frontend tests/build, version/docs/source and
  license audits pass locally; ignored tests still require their operator
  fixtures. Linux CI linker failures are tracked above.

## Previous candidate

- Version: 1.10.3
- Branch: `rohtolos/follow-up-mixed-status-remarks`
- Objective: add guarded per-RFI status remarks for mixed completed groups on
  CIV and PCU Follow Up sheets.
- Release state: source gates pass on Rust 1.97.1; Windows package acceptance,
  PR review, push, tag, and GitHub Release remain pending.
- Mixed groups with at least two RFIs and one `Completed` source status preview
  every member's mapped status in the row's Remarks cell on CIV and PCU only.
  Existing operator text is retained, generated lines replace themselves on
  later syncs, absent blank cells are pinned and created only inside existing
  rows, and unresolved source statuses go to manual review.

## Previous candidate

- Version: 1.10.2
- Branch: `rohtolos/process-scans-batch-pdfs`
- Objective: reduce repeated PDF decoding during Process Scans page OCR without
  changing recognition passes, confidence gates, or ambiguity handling.
- Release state: `v1.10.2` passed Windows Release workflow `36125434510` for
  exact commit `1397a074ac807359662d12bed041b9e807dd9268`; installer, checksum,
  manifest, and smoke evidence are published. Installer SHA-256:
  `2dbc375a68848a30e72f942123e482b07183f01b227305194b097e1c29b2a840`.

## Previous slice

- Process Scans opens a single platform PDF document per page OCR sequence and
  reuses it for text extraction and every DPI render. OCR decisions and output
  ordering remain unchanged. Exact operator OCR returned CIV-1286 before and
  after the change (15.79 s baseline, 15.91 s after); this single-page sample is
  dominated by OCR itself, so stacked-PDF wall-clock improvement is not yet
  measured. Each page now opens the source PDF once instead of once per OCR DPI.

- Process Scans now scans page order inside a multi-page PDF. Strict exact RFI
  starts form groups; plain no-candidate pages continue the group; uncertain,
  unreadable, leading, and repeated-code groups are Review outputs. Page-only
  reconstruction preserves approved page content and refuses unsupported PDF
  structures. The combined source moves to app trash only after outputs succeed
  when Keep original is off.
- CIV/MEC/ELE/PRC output paths always include the discipline folder, with the
  existing organized or flat naming choice beneath it. Other disciplines retain
  the existing layout.
- Focused Process Scans integration: 24 passed, 0 failed, 1 ignored. Pinned Rust
  1.97.1 all-feature Cargo suite (368 unit tests and all runnable integrations),
  Clippy, check, and format pass. Node (23), React (54), frontend build,
  version/docs/source/license audits pass. Windows workflow built the installer,
  passed Tesseract OCR and clean install/startup/package/uninstall smoke, and
  published checksummed evidence. Office and printer acceptance remain outside
  source and package evidence.
- Windows 1.10.0 workflow run `36109512980` passed setup and Tesseract install,
  then failed the full Rust suite because the new Process Scans assertions
  searched for `/CIV/` and `/MEC/` in Windows paths. No installer was produced.
  Version 1.10.1 makes the assertions separator-independent. Windows workflow
  `36111425818` passed all tests, installer build, clean install/startup/package
  smoke, installed-tree audit, uninstall, artifact manifest, and release upload.
- PR CI exposed three Linux-only Rust warnings: macOS/Windows credential
  constants compiled unused on Linux, and libc's platform-specific `S_IFREG`
  type made a portable conversion look redundant. The constants are now
  target-gated and the required conversion has a narrow lint annotation. Local
  pinned Rust gates pass. The next CI run cleared Clippy and exposed the
  matching platform-specific import in `auth_http.rs`; that import is now
  target-gated, and another remote run is pending.
- PDF structure inspection and cache lookup run in deterministic two-file
  parallel batches; page OCR remains capped at four workers. Cached-page
  progress is coalesced, and PDF inspection has a visible progress stage.
- The focused orientation tests passed, including mixed cached/new files,
  first-run, cache reuse, forced rescan, safe Apply, and Undo. Node, UI,
  source/docs/version/license audits, frontend build, Rust formatting,
  warnings-denied Clippy, 367 serial Rust unit tests, all runnable integration
  tests, and Cargo check passed. Native Windows throughput and operator
  acceptance remain unmeasured.
- The 1.9.11 Windows CI run 35827485034 failed only the new
  `inspectionWorkers == 2` test assertion: its two-CPU runner correctly used
  one worker to reserve a CPU. The 1.9.12 test derives the expected worker
  count from the runner; the production algorithm is unchanged. The 1.9.11
  release is source-only with no installer. All 1.9.12 source gates passed:
  23 Node tests, 54 UI tests, frontend build, version/docs/source/license
  audits, Rust formatting, warnings-denied Clippy, 367 serial Rust unit
  tests, runnable integration tests, and Cargo check. Windows build and
  package acceptance are pending.

## Verified

- The ELE operator pages that produced Windows uncertainty were inspected
  visually and passed read-only macOS orientation checks, including a visibly
  sideways page that correctly needs a 90-degree rotation. Synthetic cover
  rotations at 90 and 180 degrees were classified correctly; the 270-degree
  case remained Review rather than receiving a false automatic decision.
- The orientation detector now performs a second four-direction OCR pass at
  350 DPI only after 250 DPI remains uncertain. A weak decision requires
  agreeing OSD and OCR at both resolutions plus word, average, ratio, and
  margin gates. The scan cache profile is now v3, so prior orientation results
  are not reused. Native Windows operator acceptance remains pending.
- The full ELE directory Preview finished in 177.48 seconds using only the
  read-only source and a temporary review root. The affected ELE RFI PDF now
  produces an orientation correction Plan without a warning for that PDF.
- Source gates for 1.9.9: 23 Node and 54 React tests, frontend build,
  source/docs/version audits, license audit, Rust 1.97.1 formatting,
  warnings-denied Clippy, 366 serial unit tests, integration tests, and Cargo
  check passed. Native Windows operator acceptance remains pending.

- A consented read-only audit covered 1,861 exact RFI cover references. The
  shared raw-grayscale sparse-text fallback raised deterministic sample
  reference coverage from 70.83% to 95.00% without weakening conflict vetoes.
- The full corpus produced 1,771 reference agreements, 88 safe Review outcomes,
  no runtime errors, and two visually resolved source-reference errors where
  OCR matched the visible page. Folder/filename agreement is not ground truth.
- Rust 1.97.1 formatting, warnings-denied Clippy, Cargo check, 365 serial unit
  tests, 221 integration tests, 23 Node tests, 54 React tests, frontend build,
  and source/version/docs/license audits pass. Five operator tests remain ignored.

- Searchable PDF text no longer bypasses visible-page OCR. It may corroborate
  one exact consensus code or veto automation when it conflicts or contains
  multiple exact codes.
- Signed-page staging accepts only exact conflict-free RFI identities. OCR UI
  labels now describe heuristic evidence rather than an accuracy probability,
  and mail extraction no longer exposes a provider-generated confidence claim.
- The CIV-1286 operator PDF passed the new read-only path as exact CIV-1286.
  Rust 1.97.1 formatting, Clippy, Cargo check, 363 serial unit tests, 221
  integration tests, 23 Node tests, 54 React tests, frontend build, and all
  source/docs/version/license audits pass. Five operator tests remain ignored.

- The operator-supplied CIV-1286 PDF was inspected read-only and passed the new
  four-pass, multi-resolution OCR consensus as exact CIV-1286. The file remains
  outside Git and is covered by an ignored operator regression hook.
- Consensus rejects any competing code such as 4286, requires support across
  three DPIs with at least two exact reads, and reports image OCR at 95% rather
  than a misleading 100%.
- When the selected destination already contains exact RFI folders, an OCR code
  without an existing folder is routed to Review and cannot create a new
  folder. Empty/new destinations retain the established folder-creation flow.

- A focused regression reproduces the former Apply failure by clearing a
  literal Follow Up cell through the production guarded writer and closed
  package-delta verifier. The fixed verifier accepts an empty text node only
  when the approved replacement is exactly empty.
- All 34 RFI Log to Follow Up integration tests pass. Cancelled/Rejected
  removal, formula/merge/protection refusal, exact FINAL Worley state, prepared
  missing FINAL handling, retained recovery, and Undo remain covered.

- The canonical branch was pushed through the 1.9.0 verification commit; local
  and remote branch tips matched before the 1.9.1 metadata change.
- Every version in `releases.json` through 1.9.0 has an immutable Git tag and a
  published GitHub Release. Verified available Windows assets were uploaded for
  0.6.0, 0.7.0-rc.1, 1.8.0, and 1.9.0; source-only historical releases do not
  claim a missing binary.
- The 1.9.1 source checkpoint, tag, and GitHub Release are preserved. Its
  Windows run 35494732675 exposed three Windows-only test failures before
  packaging; no installer was published from that failed run.
- The 1.9.2 source checkpoint and tag are preserved. Windows run 35495891429
  passed the workbook handle-lifetime regressions, then exposed a replica
  rename sharing violation whose native code was hidden from the outer bounded
  retry; no installer was published from that failed run.
- The 1.9.3 source checkpoint, tag, and source-only GitHub Release are
  preserved. Windows run 35496880556 proved the outer retry now runs for its
  full bound, but name replacement remained permanently unavailable for the
  replica fixture; no installer was published from that failed run.

- The reply continuation is offered only after a non-partial committed
  `rfi.log_add` journal and requires the exact owner, session, Plan digest, and
  journal identity. CIV/MEC/ELE/PRC tables retain their approved values and
  canonical order in plain text and escaped HTML.
- The Outlook reply remains a separate preview/Apply operation. It neither adds
  recipients nor sends mail, and the established selection-fingerprint and
  unsent-draft journal/Undo controls remain in force.
- Rust 1.97.1 formatting, Clippy, Cargo check, 360 serial unit tests, 221
  integration tests, 23 Node tests, 54 React tests, frontend build, and all
  source/docs/version/license audits pass. Five operator-file tests remain
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

# DocFlow project state

Updated: 2026-10-31

## Windows installer delivery (1.37.1)

- Windows tag build for 1.37.0 stopped in Node tests because the shared
  `frontend/src/test/appTestSupport.ts` helper existed only as a locally ignored
  file. `.gitignore` now explicitly keeps the lowercase `test/` support tree.
- The test helper is tracked and clean-source imports resolve. Version 1.37.1
  is reserved for this compatible build fix. Windows CI packaging and host
  install/smoke evidence are pending until the tagged workflow succeeds.
- The current Mac cross-build attempt failed against pre-existing uncommitted
  Windows Excel contract edits. Those edits were left untouched.

## ELE Follow Up existing-entry sync (1.37.0)

- Full Sync can update existing ELE RFIs only for the accepted pairs
  `ELE-0005 / ELE-5.2.6 -> 2.6` and
  `ELE-0006 / ELE-6.2.5.3 -> 2.5.3`. A source PCS Tag No must match the
  target Block, and the RFI must already exist in the corresponding target
  cell. No empty RFI slots, rows, or sections are populated; 1.6 and all other
  ELE item pairs remain excluded.
- Accepted decision: `docs/adr/0051-ele-follow-up-sync-existing-entries.md`;
  domain rules are in `docs/RFI_DOMAIN.md`.
- Verification: source regression, all 97 Follow Up native tests, full Cargo
  suite, actual workbook read-only Plan preview, docs/version/source and
  license audits, frontend tests/build, Clippy, and formatting passed.
  Windows-native Excel/Office behavior is not verified from this macOS run.

## MEC Follow Up Section/Block mapping (1.18.0)

- MEC and Signed By Worley MEC now match source `Tag No` to target `Block`
  and split source `Area` values into distinct target `Section` contexts.
  `Inspection Location` is ignored. Each distinct existing pair maps once;
  ambiguous duplicate pairs stay in Manual Review, and partial warnings name
  matched/unmatched Tag No and Area values. No Follow Up rows are created.
- Accepted decision: `docs/adr/0037-mec-follow-up-section-block-matching.md`;
  canonical rules are in `docs/RFI_DOMAIN.md`.
- Verification: all 690 Rust tests passed (20 ignored), 24 Node tests,
  73 frontend tests, Clippy, formatting, Cargo check, docs/version/source and
  license audits passed. Windows x64 cross-build package audit and checksums
  passed; host Tesseract checksum remained unchanged.
- Installer:
  `artifacts/windows-1.18.0/DocFlow_1.18.0_x64-setup.exe`
  (SHA-256 `4698b8609151d838d565c3045ba75fbf4b3a2986ef5be0c978b07a6078b4eeb2`).
  Native Windows install, startup, and user workbook acceptance are not
  verified from this macOS cross-build.

## Main integration and Windows cross-build (1.17.0)

- Integrated the current main-line workspace navigation update as DocFlow
  1.17.0. Home promotes Fast RFI Scan; authenticated routes stay reachable in a
  desktop sidebar or narrow-window bottom navigation. No workflow authority
  changed.
- Added release/changelog/product documentation and raised runtime manifests to
  1.17.0. Moved a static regex out of a loop and collapsed nested `if let`
  checks to clear pre-existing Clippy failures.
- Verification: Node (24), Vitest (73), Rust (690 passed, 20 ignored), frontend
  build, Rust fmt/clippy/check, version/docs/source/license audits passed.
  Browser MockBridge smoke verified sign-in, Home, History, Recovery, and a
  390px viewport with no document-width overflow.
- Windows x64 cross-build produced NSIS, portable EXE, and portable-with-OCR
  ZIP in `artifacts/windows-1.17.0/`; package audit and SHA-256 manifest passed.
  Host OCR resource checksum was identical before/after staging.
- Native Windows install, packaged executable smoke, GUI startup, Office,
  OneDrive, and printer acceptance remain unverified. Cross-build emitted
  unsigned/experimental-toolchain warnings; no Windows release was published.

## Local build storage cleanup and Rust quality gate fixes (1.13.7)

- Removed `src-tauri/target/` with `cargo clean`; Cargo reported 49,247 files and 33.5 GiB removed. Available filesystem space increased from 23 GiB to 46 GiB.
- Removed ignored `artifacts/` packages and verification records (about 1.2 GB).
- Preserved the 86 MB Windows Tesseract runtime source under ignored `src-tauri/resources/windows-tesseract/`; updated `tools/build_windows_cross.sh` to use it.
- Disabled Cargo incremental compilation for `dev` and `test` profiles to prevent stale incremental caches from accumulating.
- Added the missing `clause` fields to Follow Up Sync fixtures and fixed Clippy lints in OCR polling and header-clause extraction.
- Node suite (24), frontend suite (55), frontend build, 378 Rust unit tests, applicable Rust integration tests, Rust 1.97.1 format/Clippy/check, and version/docs/source/license audits pass.
- Operator-data-dependent Rust tests remain ignored as designed. Windows package install/smoke and external certification remain pending.
- Next: commit the 1.13.7 gate fixes, push the reviewed branch for exact-source CI, then complete platform release gates before publishing.

## Core process speedup trial (branch: codex/speed-up-try)

- Objective: maximize process throughput across all DocFlow features with zero-regression safety.
- Concurrency: replaced per-invocation Rayon thread pool construction/destruction in `parallel.rs` with lazy-static persistent worker pools (`LazyLock`).
- OCR latency: reduced child-process polling sleep from 20ms to 1ms in Tesseract orientation, direction, and whitelist execution, cutting process-wait overhead by up to 20x while keeping periodic size limits.
- Cascade short-circuiting: added early consensus break in `ocr.rs::read_page` after attempt 3 (3 distinct DPIs), skipping redundant attempt 4 when consensus is already satisfied.
- Fast cover blank screen: added top-34% band text cue filter to bypass PGM disk write and Tesseract execution on empty bands.
- Archive discovery: eliminated redundant `stat` syscalls in `stage_signed_pages.rs` by reading `entry.file_type()` directly from `DirEntry`.
- Workbook ingestion & matching: added `read_workbook_and_discovery` in `ooxml.rs` to parse discovery and sheets in a single pass; pre-indexed slot candidate lookups by `(sheet, column)` and `canonical_rfi` in `rfi_follow_up.rs` to replace $O(N \times M)$ scans with $O(1)$ lookups.
- Verification: full Cargo test suite passed in 42.31s (down from 53.06s, >20% wall-time reduction); 24 Node tests, 55 Vitest frontend tests, source and license audits passed cleanly.

## Laya review-triage feasibility pilot

- Ran Laya 0.3.20's cached English checkpoint with Hugging Face and Transformers
  offline mode forced. On 12 synthetic OCR examples, top-choice agreement was
  5/12; it suggested `cover` for 7 non-cover examples, including all 4
  uncertain examples.
- A second offline pass sampled the first two pages from 12 canonical RFI PDFs,
  selected at the first/middle/last code in CIV/MEC/PRC/ELE. OCR was filtered to
  relevant page terms and exact RFI codes, email addresses, and phone numbers
  were redacted before inference. The source PDFs were read-only; OCR text was
  not persisted.
- Expected labels used page position plus visible OCR evidence: one request
  heading, a known client/project mark, and one RFI identity for `cover`; pages
  without a confirmed cover signal were `continuation` or `uncertain`. The 24
  page sample had 7 confirmed covers, 12 continuations, and 5 uncertain pages.
  Agreement was 8/24. The model suggested `cover` for 3 continuations and all 5
  uncertain pages; only 1/12 continuations was labelled correctly.
- Both runs emitted Laya's invalid-temperature warning; confidence values were
  ignored. This bounded screen rejects the cached English checkpoint for the
  proposed scan-review suggestion. The sample is not a human-reviewed,
  representative acceptance corpus, and other Laya checkpoints were not tested.
  Do not integrate this checkpoint into Process Scans or Fast Cover Scans.

## Jev review-triage second-signal pilot

- `jev_decide` model `jev-1.13.0` matched all 12 generic synthetic cover,
  continuation, and uncertain examples.
- It also matched the expected class for all 6 unique OCR-signal patterns from
  the 24-page local sample, representing all 24 pages. Only page role, visible
  request-heading/known-mark booleans, and distinct RFI-ID counts were sent;
  no raw OCR, images, exact identifiers, or paths were sent.
- These checks show Jev followed an explicit rule over text examples and
  pre-extracted flags. They do not measure OCR cue extraction or cover
  recognition from real scan content. Any app integration would need a separate
  bounded Rust network design review; React/webview cannot call the service.

## Fast Cover Scans read and output fix (1.12.2 source candidate)

- The supplied 83-page scan's first-page cover is recognized. Earlier Preview
  produced no groups because page separation rejected a self-contained XML
  `/Metadata` stream and an empty `/Outlines` tree (`/Count 0`, no `/First` or
  `/Last`) as unsupported catalog features.
- Page extraction now preserves only self-contained XML metadata and drops an
  outline root only when it is explicitly empty. Active outlines, referenced
  metadata dictionaries, and other unsupported catalog features still fail
  closed. Preview of the disposable sample copy grouped all 83 pages under one
  confirmed RFI in 51,766 ms with four workers; it had zero Review groups and
  zero unreadable warnings. First-page OCR took 1,467 ms. The user PDF stayed
  unchanged.
- Fast Cover Scans now creates a missing exact CIV/MEC/ELE/PRC RFI folder even
  when the destination already contains other exact RFI folders. Existing
  files remain no-clobber; normal Process Scans keeps its previous archive
  guard. Focused Apply/Undo preserves a pre-existing RFI folder and removes
  only the new empty folder and its created output.
- Local Rust 1.97.1 format, Clippy, Cargo check, and full Cargo tests pass;
  Process Scans passes 32 tests with 2 operator-data tests ignored. 24 Node,
  55 React tests, frontend build, version/docs/source/package/license audits
  also pass. The Vite chunk-size advisory remains non-failing. PR quality and
  Windows installer gates remain pending for the final source commit.

## Windows release checksum binding (1.12.1 released)

- Post-publication download of immutable `v1.12.0` assets showed its manifest
  `checksumSha256` (`3811fd2a20831a25b2081fc56bbff525b22deaa2a411d4a3e0754d7862fb0f5e`)
  differs from the published `.sha256` asset digest
  (`c11291e37083f572094a93df35fc346c7391a110ec09cffa3a81312581f3d24f`). The
  sidecar content contains the correct installer SHA-256; the mismatch came
  from regenerating it with PowerShell after building the manifest.
- `windows-release.yml` now reuses the package sidecar and checks its exact
  installer hash. Added a Windows workflow contract regression test.
- Local checks pass: 24 Node tests, 55 React tests, frontend build, version,
  documentation, source, and license audits. PR quality run `36479039132` and
  Windows Release run `36480484218` passed for source commit
  `52787702a96f2c396b990aaf6324d805a5394f5b`.
- `v1.12.1` published: installer is 32,273,513 bytes, SHA-256
  `93ec0a7dc64e59403fac1f52c42861bf5838cccb1afc504e4024d5a388c01875`.
  Windows install/package/startup/uninstall smoke passed. Eight downloaded
  evidence checks confirm the manifest checksum field matches the published
  sidecar file and digest. Release:
  https://github.com/mahammadosmanov/docflow/releases/tag/v1.12.1.
- `v1.12.0` and its assets remain immutable; its manifest/sidecar hash
  discrepancy is documented in `docs/VERIFICATION.md`.

## Fast Cover Scans (1.12.0 released)

- Separate operation: `rfi.process_scans_fast_covers`; normal `rfi.process_scans`
  keeps its existing detector and defaults. Both share the existing immutable
  Preview, exact cached Plan, grants, journal, Apply, and Undo.
- For multi-page PDFs, the production detector renders at 150 DPI and OCRs the
  upper 34% band in four right-angle directions. It confirms only one strict
  RFI identity plus `REQUEST FOR INSPECTION` and a known `SUNRISE PROJECT`,
  `INTEC`, or `LIGHTSOURCEBP` mark. Searchable text alone never confirms a
  cover. PSM 6/12 retries and one 300-DPI rescue are bounded to promising pages;
  unresolved heading evidence begins a Review group. Pages without cover cues
  stay attached to the current confirmed RFI. Single-page PDFs use normal OCR.
- Fast Cover Scans defaults page orientation correction off, avoiding the
  separate full-page OSD/OCR pass; users can enable correction for output pages.
  Normal Process Scans retains its existing orientation default.
- Corpus check: 13 RFI PDFs (CIV/MEC/PRC/ELE), 95 original pages, plus 39
  temporary PDF display-rotation variants. All 13/13 first-page covers were
  detected, with zero false continuation covers and zero continuation pages
  sent to Review. All 39/39 rotated cover variants were detected. This checks
  PDF display rotation; physically rotated raster pixels and other templates
  were not separately measured.
- OCR-stage comparison on the supplied 4-page scan: standard per-page OCR took
  184,082 ms; the fast detector took 3,447 ms (about 53x on this sample),
  excluding orientation correction and full application overhead. The corpus
  regression ran in 251.28 seconds sequentially; production page analysis uses
  bounded parallel workers.
- A five-page synthetic integration test verifies continuation grouping,
  uncertain-cover Review boundaries, and that normal Process Scans uses its
  original behavior. Its filesystem Apply/Undo path is journaled and restored
  in the test workspace. A production-operation Preview of the supplied CIV-1063
  PDF produced one group containing all four pages in 4,279 ms with 4 workers;
  the source was copied to a disposable test workspace and remained unchanged.
- Local 1.12.0 checks: Rust 1.97.1 format, warnings-denied Clippy, Cargo check,
  and the full Rust suite pass (622 passed, 11 ignored); 23 Node and 55 React
  tests pass; frontend build and version/docs/source/license audits pass.
  Vite reports a non-failing 641.22 kB JavaScript chunk warning.
- PR #36 quality run `36473292966` passed all checks after CI began clearing
  Cargo test artifacts before `cargo check`; the first two Rust jobs had failed
  at `tauri-macros` linking with `ld` signal 7 / Bus error.
- Windows Release run `36474607152` passed in 28m29s for exact commit
  `6c70cbb95a0fbb37e2c95c5d35102a39819f3c8a`. The clean NSIS install/startup/
  package/uninstall smoke passed. Installer is 32,270,304 bytes with SHA-256
  `9f2ebf195830842ce87d3d1cd6ff6d66b05d78b9991099f7147c35f19cbe430e`;
  smoke reports 2,862 ms install, 26 ms cold start, and 22,589,440-byte peak
  working set. Immutable release: https://github.com/mahammadosmanov/docflow/releases/tag/v1.12.0.
- Physical raster rotations, unsupported cover templates, and processing the
  operator's real scan corpus on Windows remain separate acceptance checks.

## Confirmed-cover Process Scans grouping (1.11.9 fix)

- Objective: keep combined-scan continuation pages with their current RFI until
  the next confirmed cover.
- Fix: only a strict, canonical RFI identity with confidence `>= 0.8` starts a
  group. Uncertain, low-confidence, non-matching, and unreadable pages stay in
  the active group. Pages before the first cover and duplicate starts remain
  Review.
- Regression: 23 continuation pages between covers stay in the first RFI;
  the confirmed cover on page 24 starts the next group. Coverage includes
  ambiguous, low-confidence, repaired, and unreadable continuation evidence,
  plus leading-page Review.
- Local evidence: Process Scans 28 passed/1 ignored; full Rust suite 620
  passed/7 operator-data tests ignored; 23 Node and 55 frontend tests pass;
  frontend build, Rust 1.97.1 fmt/Clippy/check, version/docs/source/license
  audits pass. No operator PDF was used.
- Version: 1.11.9 (`fix`), source commit `d5b0792`. Pushed to
  `rohtolos/follow-up-unmatched-skip`; PR #34 Node/React and Rust format/Clippy/
  test jobs passed. Immutable tag `v1.11.9` is pushed.
- Windows Release run `36416196927` passed build, package audit, clean NSIS
  install/startup/uninstall smoke, and published `v1.11.9`:
  https://github.com/mahammadosmanov/docflow/releases/tag/v1.11.9. Manifest and
  smoke evidence bind to exact commit `d5b0792c3a34f963a499e3cc2ad15ad397f89437`.
  Installer size is 32,218,263 bytes; SHA-256 is
  `a22f7e33b97dbad20da09195e5fa726266ca3a518b6b11db1ae959204f0dbaf3`.
- Windows clean install/startup/package/uninstall smoke passed. Native Windows
  OCR and combined-scan cover detection with operator scans remain a separate
  acceptance gate; no operator PDF was used in this fix.
- Next: install v1.11.9 on Windows and confirm a combined scan splits only at
  confirmed RFI covers, with all intervening pages attached to that RFI.

## Failed Follow Up Apply Undo (1.11.8 fix)

- Objective: repair Apply recovery when the original Follow Up workbook has
  been displaced to a retained-original leaf and the canonical name is absent.
- Root cause: failed-Apply journals were rejected by normal Undo admission;
  recovery listed the unresolved record but offered no way to restore the exact
  original.
- Fix: Recovery Center and the existing result Undo action now admit a separate
  failed-Apply restore mode. Rust revalidates journal owner/root authority,
  file identity, bytes, and security metadata, records restore intent, then
  performs a same-parent no-clobber rename only to an absent canonical path.
  Recovery only finalizes an exact completed restore. Collisions and mismatches
  remain untouched; backup and private output stay retained.
- The owner restored their workbook before this fix; this task did not read or
  modify either operator workbook.
- Local evidence: focused restore, collision, interrupted-restore recovery,
  and Recovery Center tests pass; 55 frontend tests, frontend build, Node
  tooling tests, version/docs/source/license audits, Rust 1.97.1 fmt/Clippy,
  and full Rust tests pass. Seven operator-data tests remain ignored.
- Version: 1.11.8 (`fix`), commit `84632c6b70c6c04f7d1a98e9649312425ec14e92`.
  Pushed to `rohtolos/follow-up-unmatched-skip`; PR #34 Node/React and Rust
  format/Clippy/test jobs passed. Immutable tag `v1.11.8` is published.
- Windows Release run `36408268229` passed build, clean NSIS install/startup/
  package/uninstall smoke, package audit, and release publication in 34m30s.
  Manifest and smoke evidence bind to the exact commit. Installer size is
  32,221,929 bytes; SHA-256 is
  `17a65109969ba65110ec565fad51a3681ded71cbea829d6fb4e078e1b4bf9d49`.
- Microsoft Office/Excel acceptance remains separate. The operator's restored
  workbook was not accessed or modified by this work.
- Next: install v1.11.8 and validate the explicit restore on an isolated copy;
  use the published release at
  https://github.com/mahammadosmanov/docflow/releases/tag/v1.11.8.

## Follow Up false manual-review fix

- Version: 1.11.4
- Objective: stop correct formula-driven Worley `Signed` summaries from
  appearing as lifecycle-status manual reviews.
- Root cause: the full-workbook planner compared every target status to RFI Log
  lifecycle status before separating Signed By layouts. A formula result such
  as `Signed` differed from source `Completed`, so a read-only formula cell was
  incorrectly sent to manual review.
- Fix: compare lifecycle values only on non-Signed layouts. Keep the separate
  final Worley comparison and formula safety checks. Exclude Signed By summary
  copies when checking whether an RFI is already mapped to another lifecycle
  section.
- Exact supplied workbook pair was previewed read-only before and after the fix.
  Manual-review rows fell from 130 to 3. The 127 false positives came from
  lifecycle-versus-Worley status comparisons and Signed By formula summaries
  counted as alternate lifecycle mappings. Three multi-row FINAL Worley
  mappings remain ambiguous; one unique update is ready. Original workbooks
  were not modified.
- Regression reproduced the false positive before the fix and passed after it.
  Full local Rust, Node, UI, frontend build, formatting, Clippy, Cargo check,
  version, docs, source, and license checks pass. Seven operator-data tests
  remain ignored. Windows release run `36304760149` passed installer build,
  clean NSIS install/startup/package/uninstall smoke, and asset-manifest checks.
  Release `v1.11.4` is published. Manual Office/Excel acceptance remains
  outside this workflow.
- Next: install v1.11.4 and confirm the three ambiguous FINAL Worley rows remain
  available for operator review.

## Parallel feature-delivery workflow improvement

- Version: 1.11.5 (`fix`)
- Local gates passed: version, docs/source audits, 23 Node tooling tests,
  54 frontend tests, production build, Rust 1.97.1 fmt/Clippy/tests/check, and
  license audit. Windows packaging has not run.
- PR CI Node job passed (31–33s); separate Rust format/lint passed in 4m30s.
  Combined Rust jobs twice hit `rust-lld` Bus error. Splitting them fixed that,
  then exposed one platform-only native-PDF unit test failing on Linux because
  Linux intentionally has no page renderer. Its end-to-end test is now gated to
  macOS/Windows; full macOS Rust suite passes (369 tests). Awaiting final CI.
- Added `docs/FEATURE_DEVELOPMENT.md` and linked it from the documentation map.
- Feature issue/PR templates request a first slice, comparable timing evidence,
  focused-check timing, and applicable platform acceptance.
- Quality CI runs independent Node/React and Rust jobs concurrently; Rust
  registry/Git dependencies use a lockfile-keyed cache while build concurrency
  remains bounded to one.
- The branch is rebased onto v1.11.4 and the source changes are ready for
  release validation.

## Previous release baseline: 1.11.3

- Process Scans sparse drawing orientation fix was released as v1.11.3. Its
  Windows build and clean installer smoke test passed. The Linux quality run
  failed because the runner exhausted disk space while linking Rust tests.

## Previous release baseline: 1.11.1

- Process Scans orientation regression now normalizes path separators for
  Windows and Unix. The tagged 1.11.1 release remains immutable.

## Previous release baseline

- Version: 1.10.8
- Branch: `rohtolos/follow-up-unmatched-skip`
- Objective: match the supplied RFI Log to exact existing Follow Up contexts,
  including MEC inspection locations, PCS blocks, and explicit multi-area rows.
- Release state: all 48 Follow Up native tests pass. Latest supplied-workbook
  plan has 66 approved cell pairs and 47 ambiguous MEC rows for operator review.
  A guarded review copy was created; original workbook hashes are unchanged.
  Full release checks remain pending.

## 1.10.8 slice

- MEC source discovery now accepts the `INSP. LOC./AREA` header variant. This
  fixed the exact MEC-0791 context match without relaxing target matching.
- Native regression confirms MEC-0791 maps to its existing Follow Up row.
- Supplied workbook scan recognizes both `INSPECTION LOCATION` and
  `INSP. LOC./AREA`; exact PCS routes use block identity, composite PCS tags
  match explicit area tokens within the uniquely identified CIV zone, and MEC
  multi-area records map only when each target tuple is distinct.
- Latest plan: 50 lifecycle and 16 Worley updates; 47 source RFIs have multiple
  MEC candidates and remain manual. No rows or sections are invented.
- Review copy: `Inspection Status Follow Up - Detailed Sync - Review Pending
  v2.xlsm`; manual review list: `Follow Up - Detailed Sync Manual Review v2.csv`.
  The earlier `Safe Sync` copy and `Ongoing Missing` CSV are superseded.
- Verification: supplied-workbook guarded plan and 48 native Follow Up tests
  pass; full Rust suite, 54 frontend tests, frontend build, pinned Rust 1.97.1
  Clippy, Cargo check, format, version/docs/source/license audits pass. Current
  Rust 1.98 Clippy reports nine existing `chunks_exact_to_as_chunks` lints in
  `ocr.rs`, `rfi_file_group.rs`, and `pdf.rs`; pinned release Clippy is clean.
- Windows Excel acceptance remains pending.

## 1.10.6 carried slice

- Follow Up Sync maps repeated MEC RFIs to separate existing rows when each
  exact tag, area, and inspection-location tuple is unique.
- S5 retained files now cap at 256 while the 8 GiB bound remains unchanged.
- Commit `bc0a15be15c3b2886ab3dc5bd8bcb15499542fbc` is pushed; Windows run
  `36245298684` passed its recorded gates. Linux CI linker failure and Windows
  Excel acceptance remain pending.

## Previous current candidate (1.10.6)

- Objective: synchronize exact multi-row CIV/MEC Follow Up mappings and raise
  the bounded retained S5 file capacity after operator Apply failures.
- Supplied prior workbook preview produced 28 approved row pairs and no
  warnings; this is historical evidence and predates the broader workbook scan.
- Release draft:
  `https://github.com/mahammadosmanov/docflow/releases/tag/untagged-d7eb46481ca84ed17eb3`.

## 1.10.6 slice

- Repeated MEC records retain every exact `(tag, area, inspection location)`
  source context only when ITP, item, lifecycle status, and Worley evidence
  agree. Distinct target tuples map independently; duplicate or incomplete
  contexts remain manual review. CIV exact distinct-tag fan-out is preserved.
- S5 retained files now cap at 256, with the 8 GiB bound unchanged. A native
  Apply test seeds 64 prior files to verify the previous count limit no longer
  blocks a reservation.
- Updated current workbook pair preview: 28 approved cell-update rows across
  CIV (12), MEC (3), PCU (12), and Road (1); 27 lifecycle rows and 1 Worley
  row; zero warnings. Temporary-copy guarded OOXML delta check passes.
- Release workflow and Windows installer acceptance remain pending.

## 1.10.5 carried behavior

- Follow Up Sync now previews and applies only RFI number and status cells. It
  never adds, replaces, or clears Follow Up worksheet Remarks. RFI Log REMARKS
  remain read-only Worley evidence. Older generated `RFI Status Sync:` text is
  left untouched. ADR 0028 supersedes ADR 0026. Regression coverage confirms
  mixed groups still receive aggregate statuses without remark-cell writes.
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

- Version: 1.10.5
- Objective: remove generated mixed-status Remarks writes from Follow Up Sync.
- Release state: source branch has PR #34; its latest Linux check failed when
  `ld` crashed with signal 7 during test linking. Windows 1.10.5 draft build
  status remains historical and does not verify 1.10.6.

## Previous candidate

- Version: 1.10.4
- Branch: `rohtolos/follow-up-unmatched-skip`
- Objective: skip supported source routes absent from the Follow Up template
  while retaining manual review for non-unique row matches.
- The read-only supplied-workbook preview reduced the unmapped warning from
  173 to 7: 166 no-match routes were skipped and 7 multi-row matches stayed in
  manual review. It found 361 ready lifecycle updates, 1 ready Worley update,
  and 42 Follow Up RFIs with no source record. Windows draft installer was
  built from feature commit `c07282d` and passed clean install/startup/uninstall
  smoke; SHA-256 is
  `845642ca220ecfa0722f81218a300163482ec4feffc2e3027e4e282fe995455c`.

## Previous candidate

- Version: 1.10.3
- Branch: `rohtolos/follow-up-mixed-status-remarks`
- Objective: add guarded per-RFI status remarks for mixed completed groups on
  CIV and PCU Follow Up sheets.
- Release state: source gates pass on Rust 1.97.1; Windows package acceptance,
  PR review, push, tag, and GitHub Release remain pending.
- This generated-remarks behavior was retired by ADR 0028 in version 1.10.5.
  Mixed groups with at least two RFIs and one `Completed` source status preview
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
- Normal Process Scans retains the conservative existing-folder rule: an OCR
  code without a matching folder goes to Review when the destination already
  contains exact RFI folders. Fast Cover Scans now creates the exact missing
  discipline/RFI folder for each confirmed cover.

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

## Feature delivery workflow (1.11.6)

- CI run `36333564684` passed Node/React and Rust format/Clippy but found Linux
  failures in PDF/OCR integration tests that call platform renderers unavailable
  on Linux. The Rust core test job did not complete.
- Added macOS/Windows cfgs to renderer-dependent integration tests. On macOS,
  `cargo +1.97.1 test --manifest-path src-tauri/Cargo.toml --test
  rfi_system_sync_native` passes (6 passed, 3 operator-data tests ignored).
- Classified this CI fix as `fix` and advanced the version to 1.11.6.
- A Linux run also exposed a signed-page planning test asserting native-rendered
  image previews. It now exercises the platform-independent plan with previews
  disabled; the focused planning test passes on macOS.
- Classified this test correction as `fix` and advanced the version to 1.11.7.
- Next: rerun all applicable local source gates, push the fix, require green CI,
  then tag/publish only after the exact commit is validated. Windows Office,
  OneDrive, OCR, and printer acceptance remains a separate manual gate.

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

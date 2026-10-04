# Changelog

All notable DocFlow changes are recorded here. The project follows Semantic
Versioning; `releases.json` is the machine-readable release catalog.

## 1.13.7 - 2026-10-04

### Improved

- Limit local Rust cache growth, remove generated local artifacts, and preserve Windows OCR build inputs.
- Restore Rust test compilation for Follow Up Sync fixtures and satisfy current Clippy lints.
- Apply Rust 1.97.1 formatting to affected Rust sources.

## 1.13.5 - 2026-10-02

### Fixed

- Dynamic header clause discovery for Follow Up synchronization:
  - Detect milestone clause numbers dynamically from worksheet header rows (e.g. `(1.6)`, `(2.1, 2.2, 2.3)`, `(3.4)`, `(3.5)`, `(3.1)`, `(3.2)`) instead of relying on brittle hardcoded column indices.
  - Automatically adapt to inserted columns (e.g. quantity columns shifting `MEC` and `Signed By Worley MEC` columns right by +2), eliminating manual review errors.
  - Exclude hidden side-table control columns (`rfi_column > 14`) in `Signed By Worley MEC` from candidate layout targets.

## 1.13.4 - 2026-10-02

### Fixed

- Large workbook stability and limits:
  - Increase OOXML `MAX_ENTRY_BYTES` from 64MB to 128MB to safely support large workbooks with 180,000+ changelog entries without triggering false `limitExceeded` errors.
  - End-to-end verified lifecycle status synchronization for updated RFIs across full workbook schemas.

## 1.13.3 - 2026-10-02

### Fixed

- Follow Up sync multi-row RFI status recognition:
  - Reorder duplicate vs `alreadySynced` checks in legacy row classification so already synchronized multi-row RFIs are never marked as `Manual review only`.
  - Add flexible case-insensitive and whitespace-trimmed worksheet discovery in `supports_full_sync_schema`.

## 1.13.2 - 2026-10-02

### Fixed

- Strict Follow Up milestone and item clause cell isolation:
  - Enforce identical ITP and item clause contract before appending or merging RFIs into existing cells (`slot.codes.push`), preventing cross-milestone combinations (e.g. tracker `MEC-0370/0565` and module `MEC-0530` are strictly quarantined into their respective columns).
  - Add regression coverage ensuring distinct item clauses remain isolated and never merge into the same cell.

## 1.13.1 - 2026-10-02

### Fixed

- Follow Up synchronization accuracy and placement:
  - Support flexible hyphenless and underscore RFI code parsing in Follow Up (e.g. `MEC0267` correctly parsed to `MEC-0267` without clobbering survey cells).
  - Restrict `Signed By Worley MEC` candidate placement strictly to records signed by Worley, preventing 16 false "Not Signed" row additions.
  - Support exact Road name tag matching without spurious Zone text conflict, automatically populating empty milestone cells for Road 14.
  - Protect 2000-series placeholder RFIs from false `sourceMissing` warnings.
## 1.13.0 - 2026-09-30

### Removed

- Retired three document operations to streamline DocFlow into ten high-speed local document workflows:
  - `rfi.outlook_reply_draft` (Prepare Outlook Reply All draft)
  - `rfi.outlook_archive_selected` (Archive selected Outlook email)
  - `files.replica_publish` (Publish RFI Systems)

## 1.12.5 - 2026-09-30

### Added

- Add real-time cancellation support during operation preview across all document features:
  - Wire cancellation tokens into `preview_process_scans`, `preview_rfi_file_group`, `preview_rfi_log`, `preview_rfi_complete_status_sync`, `preview_stage_signed_pages`, and `preview_pdf_orientation`.
  - Enable operation ID cancellation dispatch in `BackendRouter::cancel` (`engine/cancel` accepts `{ opId }`).
  - Add dedicated Cancel button in `BusyState` during the `previewing` stage in the frontend.
  - Support `Escape` shortcut to cancel long-running previews immediately.

## 1.12.4 - 2026-09-30

### Changed

- Short-circuit Fast Cover Scans continuation page evaluation: when direction 0 shows clear upright non-cover words, bypass redundant 90°, 180°, and 270° rotations.
- Optimize RFI Log workbook reading by parsing only requested discipline and lookup worksheets (`CIV`, `MEC`, `ELE`, `PRC`, `Sheet1`), skipping unused calculation and summary sheets.

## 1.12.3 - 2026-09-30

### Changed

- Maximize process speeds across OCR, workbook operations, archive discovery, and concurrency:
  - Cache persistent Rayon worker pools with `LazyLock` in `parallel.rs`.
  - Accelerate Tesseract process polling from 20ms to 1ms with periodic output bounds checking.
  - Short-circuit the Process Scans OCR cascade after 3 distinct DPI passes when consensus is reached.
  - Screen blank/empty top bands in Fast Cover Scans to bypass redundant PGM writes and OCR.
  - Use `entry.file_type()` in archive and signed scans traversal to avoid redundant `stat` syscalls.
  - Parse OOXML discovery and worksheets in a single pass via `read_workbook_and_discovery`.
  - Pre-index Follow Up candidate slots by `(sheet, column)` and `canonical_rfi` for $O(1)$ lookups.

## 1.12.2 - 2026-09-29

### Fixed

- Fast Cover Scans creates missing per-RFI folders even when the destination
  already contains other RFI folders. Page extraction preserves self-contained
  XML metadata and safely omits only empty outline trees, so the supplied scan
  can be split without sending the entire PDF to `_review`.

## 1.12.1 - 2026-09-29

### Fixed

- Publish the Windows installer checksum sidecar used by its artifact manifest;
  verify it matches the installer instead of regenerating it with different
  line endings.

## 1.12.0 - 2026-09-28

### Added

- Added a separate Fast Cover Scans operation for multi-page RFI PDFs. It checks
  known cover cues in four directions, keeps continuation pages with the current
  RFI, and sends unresolved possible covers to `_review`. Normal Process Scans
  remains unchanged. Page rotation correction defaults off in the fast flow to
  avoid a second full-page OCR pass.

## 1.11.9 - 2026-09-28

### Fixed

- Process Scans keeps uncertain and unreadable continuation pages with the
  current RFI. Only a strict, canonical, high-confidence RFI cover starts the
  next group; leading pages and duplicate RFI starts remain in `_review`.

## 1.11.8 - 2026-09-28

### Fixed

- Failed Follow Up Sync Apply can now restore its exact displaced original
  through explicit Undo. Recovery never retries the rename; collisions preserve
  both the current file and retained evidence.

## 1.11.7 - 2026-09-27

### Fixed

- Keep signed-page preview assertions out of platform-independent planning
  tests; native rendered previews remain covered by supported platform checks.

## 1.11.6 - 2026-09-27

### Fixed

- Keep PDF renderer and OCR integration tests on supported macOS and Windows
  runners; Linux continues to run platform-independent Rust tests.

## 1.11.5 - 2026-09-27

### Changed

- Standardized feature work around small slices, comparable performance
  measurements, fast focused checks, and explicit platform acceptance.
- Frontend, Rust format/lint, and Rust test jobs now run concurrently; Rust
  dependency downloads use a lockfile-keyed cache while compilation remains
  bounded.

## 1.11.4 - 2026-09-27

### Fixed

- Follow Up Sync no longer compares formula-driven Worley `Signed` status with
  RFI lifecycle values such as `Completed`. Correctly matched formula summaries
  no longer create false manual-review entries.

## 1.11.3 - 2026-09-27

### Fixed

- Process Scans now cross-checks the top-page RFI header at 250 and 350 DPI
  when map/drawing text conflicts with OSD. It auto-selects only a strong,
  agreeing header direction; otherwise the page remains in `_review`.

## 1.11.2 - 2026-09-27

### Fixed

- Follow Up Sync excludes every CIV RFI Log record whose `Tag No` starts with
  `PCS-`, including composite PCS/area values. It removes these RFIs from
  existing groups and clears the status when no supported RFIs remain.

## 1.11.1 - 2026-09-27

### Fixed

- Process Scans orientation review regression now normalizes path separators,
  so the test checks correctly on Windows and Unix.

## 1.11.0 - 2026-09-26

### Added

- Process Scans now corrects confidently detected page rotations by default.
  Uncertain PDFs go unchanged to `_review`; source files remain unchanged.

## 1.10.8 - 2026-09-26

### Fixed

- Follow Up Sync now recognizes the MEC `INSP. LOC./AREA` source header, so
  exact inspection-location matches such as MEC-0791 can reach their existing
  target row.
- Follow Up Sync now treats exact `PCS-` tags as PCU routes, uses explicit
  block-area suffixes to find the PCU row, and resolves MEC composite target
  locations and single-row multi-area contexts against distinct existing rows.

## 1.10.7 - 2026-09-26

### Fixed

- Follow Up Sync now recognizes the MEC `INSP. LOC./AREA` source header, so
  exact inspection-location matches such as MEC-0791 can reach their existing
  target row.
- Follow Up Sync now treats exact `PCS-` tags as PCU routes, uses explicit
  block-area suffixes to find the PCU row, and resolves MEC composite target
  locations and single-row multi-area contexts against distinct existing rows.

## 1.10.6 - 2026-09-26

### Fixed

- Follow Up Sync now maps repeated MEC RFIs to separate existing rows when
  each exact tag, area, and inspection-location tuple is unique. Duplicate or
  conflicting route contexts remain in manual review.
- Follow Up Apply now allows up to 256 retained recovery files, while keeping
  the 8 GiB limit and refusing before mutation when either bound is exceeded.

## 1.10.5 - 2026-09-26

### Fixed

- Follow Up Sync no longer adds generated mixed-status text to worksheet
  Remarks cells. RFI Log REMARKS remain read-only evidence for Worley status.

## 1.10.4 - 2026-09-26

### Fixed

- Follow Up sync quietly skips supported RFI routes and FINAL items that have
  no matching row in the current template; multiple matches still require
  manual review. On the supplied workbook pair this reduces unmapped warnings
  from 173 to 7.

## 1.10.3 - 2026-09-25

### Fixed

- Follow Up sync now records individual statuses for mixed completed RFI groups
  in the existing CIV and PCU row Remarks cells, while preserving other notes.

## 1.10.2 - 2026-09-25

### Improved

- Process Scans reuses one decoded PDF document across each page's text check
  and OCR-resolution renders, reducing repeated PDF loading for stacked scans.

## 1.10.1 - 2026-09-25

### Fixed

- Process Scans discipline-folder regressions now check path components across
  Windows and Unix separators, so the Windows release gate tests the same
  folder structure on both platforms.

## 1.10.0 - 2026-09-25

### Improved

- Process Scans now separates ordered multi-RFI scans from one PDF into exact
  page-only PDFs, names them through the existing RFI rules, and files CIV,
  MEC, ELE, and PRC results under matching discipline folders. Uncertain pages
  go to Review; the source scan remains recoverable and Undo restores it.

## 1.9.13 - 2026-09-25

### Fixed

- Follow Up Sync now recognizes the PCU `Signed by Worley` header when Excel
  stores its words on separate lines, so the keyed FINAL status in column R
  appears in Preview and can be applied through the existing guarded plan.

## 1.9.12 - 2026-09-23

### Fixed

- The orientation parallelism regression test now respects the runtime's
  available CPU count. Windows CI runners with only two available CPUs
  correctly reserve one and use one PDF-inspection worker; capable machines
  still use two. This corrects the 1.9.11 Windows test gate without changing
  orientation decisions or production worker limits.

## 1.9.11 - 2026-09-23

### Improved

- Large RFI orientation previews now inspect PDF structure and scan-cache
  fingerprints in bounded two-file parallel batches before page OCR. A new
  progress stage shows how many PDFs have been inspected.
- Repeated previews emit fewer progress events while replaying cached pages.
  Exact page decisions, source checks, scan checkpoints, Plan order, serial
  Apply, and no-clobber review output remain unchanged.

## 1.9.10 - 2026-09-23

### Fixed

- Long PDF orientation previews, including large PRC trees, no longer fail at
  the end with `path_grant_not_found_or_expired` when scanning exceeds the
  30-minute path-grant lifetime. The completed plan is bound to fresh grants
  for the same selected source and review folders; Apply still revalidates it.

## 1.9.9 - 2026-09-23

### Fixed

- Sparse covers and scanned forms now receive a second four-direction OCR pass
  at 350 DPI when the 250-DPI result remains uncertain. Weak results are
  accepted only when both OSD resolutions and both OCR passes agree and meet
  minimum word, confidence, margin, and ratio gates.
- Remaining orientation warnings include compact OSD and OCR evidence for the
  affected page, so manual review can identify why a PDF was withheld.

### Safety

- The orientation scan profile changed, invalidating old cached decisions.
  Ambiguous pages still prevent a partial corrected PDF; the source RFI System
  remains untouched.

## 1.9.8 - 2026-09-22

### Fixed

- RFI identity OCR now runs a bounded raw-grayscale sparse-text fallback at
  250/300/350/400/450 DPI only after the existing adaptive cascade cannot form
  an exact consensus. Clear older scans are recognized more often without
  weakening the three-resolution, two-exact-read, or conflict-veto rules.
- Ambiguous OCR diagnostics now identify searchable-text-layer evidence instead
  of hiding whether that evidence was absent, empty, or conflicting.

### Safety

- A reusable read-only RFI System audit compares visible OCR with exact matching
  folder/filename references, rejects report output inside the source corpus,
  and exits non-zero for every reference mismatch. Reference agreement is not
  described as ground-truth precision; every mismatch requires visual review.

## 1.9.7 - 2026-09-22

### Fixed

- Searchable PDF text can no longer authorize an RFI identity without matching
  visible-page OCR consensus. A stale conflicting text layer, multiple exact
  codes in one OCR pass, or multiple exact text-layer codes now routes the file
  to Review instead of silently naming or filing it.
- Signed-page staging now accepts only exact conflict-free RFI identities;
  repaired and fuzzy OCR tokens remain manual review.

### Safety

- OCR percentages are labelled as heuristic evidence scores, not accuracy
  probabilities or confirmations. Email extraction no longer displays a
  provider-generated percentage as factual confidence.
- The accepted evidence-truthfulness decision records that platform handoff is
  not physical completion, heuristic evidence is not proof, and unknown or
  conflicting evidence must fail closed.

## 1.9.6 - 2026-09-22

### Fixed

- Process Scans no longer treats the first regex-shaped OCR result as 100%
  reliable. Automatic naming now requires one non-conflicting RFI code across
  at least three render resolutions, including at least two exact OCR reads.
- An existing RFI archive no longer creates a new folder for an OCR-only code
  that has no matching folder. The scan is routed to Review instead.

### Safety

- Any competing RFI candidate, insufficient multi-resolution support, repaired
  or fuzzy-only result remains non-actionable. Confirmed image OCR is displayed
  as 95%, not a misleading 100%. The supplied CIV-1286 scan is retained as an
  external regression and passes the new consensus path.

## 1.9.5 - 2026-09-22

### Fixed

- RFI Log to Follow Up Apply now accepts an approved literal cell becoming
  empty, including removal of the last Cancelled or Rejected RFI in a lifecycle
  group. The closed OOXML verifier no longer rejects DocFlow's own valid empty
  inline-string output.

### Safety

- Empty output is accepted only when the approved replacement is exactly empty.
  Extra XML children, formulas, values, attributes, styles, merged cells,
  protected targets, and every non-approved package change remain rejected.

## 1.9.4 - 2026-09-20

### Fixed

- Replica publication on Windows now uses the same verified in-place overwrite
  primitive already used by guarded Undo. It no longer depends on replacing an
  existing destination name, which Windows can reject indefinitely even after
  all DocFlow handles are closed.
- Replica rollback now verifies the original target hash instead of comparing
  the retained backup with the new source hash.

### Safety

- Apply still validates the exact old target hash on the same writable handle,
  writes only from the already hash-verified staging copy, flushes and verifies
  the result, then removes the staging file. The retained backup and pending
  journal exist before the first target byte changes.

## 1.9.3 - 2026-09-20

### Fixed

- Windows capability-relative rename errors now retain their native OS code.
  Transient sharing violations can reach the existing bounded replacement retry
  instead of stopping replica publication after the short inner retry.

### Safety

- Retry remains bounded and applies only to the existing Windows access-error
  allowlist. Source/target hash validation, backup-first publication, journal,
  verification, fail-fast behavior, and Undo are unchanged.

## 1.9.2 - 2026-09-20

### Fixed

- RFI System workbook Apply now closes every preview and fingerprint handle
  before replacing the approved workbook. Windows no longer rejects the
  operation with `os error 32` because DocFlow itself still holds the file.

### Safety

- Preview, source fingerprints, retained backup, exact approved Plan, journal,
  and Undo rules remain unchanged. Windows tests explicitly release fixture
  handles before Apply and report replica failure details when a platform step
  fails.

## 1.9.1 - 2026-09-20

### Fixed

- Backfilled immutable Git tags and GitHub Releases for every version recorded
  in the release catalog. Existing verified `0.6.0`, `0.7.0-rc.1`, `1.8.0`, and
  `1.9.0` Windows packages are retained as release assets.
- Completed DocFlow changes now require a verified commit, canonical branch
  push, immutable version tag, and GitHub Release instead of remaining on one
  workstation.

### Safety

- Source-only historical releases are labelled by their available Assets; no
  missing installer is represented as verified or reconstructed from an
  unrelated build. Secrets, operator documents, caches, deployment, and
  production data remain outside standing backup authority.

## 1.9.0 - 2026-09-19

### Added

- A successful RFI Log Apply now offers **Prepare Outlook reply**. Rust rebuilds
  every affected discipline's exact approved Excel table and opens a separate
  unsent Reply All preview for the currently selected classic Outlook mail.

### Safety

- The handoff accepts only the same owner/session-bound RFI Plan, never
  recalculates numbers, and is unavailable after failed or partial workbook
  results. Outlook selection is revalidated; recipients are not added and mail
  is never sent. Workbook and draft remain separate preview/apply/journal/undo
  operations.

## 1.8.0 - 2026-09-18

### Added

- RFI Log to Follow Up Sync now recognizes the current PCU FINAL `Signed by
  Worley` column and projects exact `Signed` / `Not Signed` state from the
  matching FINAL RFI's exact `Signed by Worley` REMARKS line.

### Safety

- The PCU Worley cell is updated only when its existing FINAL RFI maps exactly.
  A newly inserted PCU FINAL RFI receives its lifecycle value first; Worley
  state is offered on the next preview instead of creating a partially selected
  multi-cell dependency.
- `POT test`, `STAT`, `Changelog`, MEC calculated remarks, manual overrides,
  formulas, and unrelated workbook content remain outside mutation authority.
  Guarded backup, closed OOXML-delta verification, journal, and Undo remain
  mandatory.

## 1.7.1 - 2026-09-18

### Fixed

- Correct RFI PDF Page Orientation now uses a 250/350-DPI Tesseract OSD cascade
  instead of one 150-DPI attempt. Agreeing passes can combine evidence, and
  difficult form pages receive a bounded four-direction English OCR comparison.
- The orientation scan-index profile changed, so earlier detector results are
  not reused after this recognition update.

### Safety

- Four-direction OCR acts only when it has enough recognized words, a strong
  average score, and a clear lead over the runner-up. Sparse drawings and close
  0/180-degree results remain manual review instead of being guessed.
- The selected RFI System remains read-only. Apply still uses only approved,
  hash-pinned rotations to create no-clobber, fidelity-checked review copies.

## 1.7.0 - 2026-09-18

### Added

- RFI Log to Follow Up Sync now projects both lifecycle state and exact FINAL
  Worley `Signed`/`Not Signed` state in one guarded preview.
- Missing FINAL CIV/MEC identifiers can be added to one uniquely matched,
  already-prepared literal RFI/status pair. The preview shows the signature
  source value instead of presenting it as an inspection status.

### Safety

- Signed-row creation is limited to the approved FINAL ITP/item mappings and
  exact location match. Formula-bearing, merged, absent, protected, or
  ambiguous destinations remain unselected manual-review rows.
- `POT test`, `STAT`, `Changelog`, ELE/PRC, formulas, summaries, and missing
  workbook sections remain outside mutation authority. Apply still uses exact
  workbook fingerprints, retained backup, closed OOXML delta verification,
  journal, and Undo.

## 1.6.1 - 2026-09-17

### Fixed

- RFI System Sync now treats only the exact case-insensitive
  `RFI No. <canonical RFI>.pdf` filename as Worley-signature evidence. It no
  longer falls back to an unrelated PDF in the RFI folder.
- Worley detection now inspects only the inner name/signature area of the
  bottom-right `OTHERS` cell on page one. Printed table rules, contractor
  signatures, dates, notes, and marks elsewhere cannot classify an RFI as
  Worley-signed.

### Safety

- The detector fails closed when the exact PDF is missing, unreadable, empty,
  or below the conservative `OTHERS`-ink threshold. Its algorithm profile was
  changed, so earlier cached signature classifications are not reused.
- Regression evidence includes the operator-supplied signed CIV-1041 and
  unsigned MEC-0737 PDFs without adding either private document to Git.

## 1.6.0 - 2026-09-17

### Added

- Archive Selected Outlook Mail previews one exact classic Outlook message and
  creates a native Unicode MSG staging copy. The deterministic Windows-safe
  filename and destination are visible before Apply; the existing RFI grouping
  operation can then copy the MSG into every exact related RFI folder.

### Safety

- Apply revalidates the exact sole MailItem selection and bounded fingerprint,
  consumes owner/session-bound authority once, validates Outlook's private MSG
  export, and performs one capability-scoped no-clobber write. It never sends,
  moves, deletes, marks, or changes mailbox content.
- The created MSG is journaled. Undo removes only its exact unchanged hash;
  collisions, selection drift, invalid exports, locks, and uncertain writes fail
  closed or remain recoverable.

## 1.5.2 - 2026-09-17

### Fixed

- Excel worksheet printing no longer rejects a non-empty PDF merely because
  DocFlow's mutation-oriented PDF parser does not support one of Excel's valid
  PDF structures. A bounded PDF envelope check now hands the private bytes to
  Windows' native PDF decoder, which validates the document and page count
  before any printer job starts.
- The unfinished-queue action now explicitly says that it retries only
  unprinted jobs. Starting a new batch remains a new approval and can print
  previously completed files again.

### Safety

- Empty, oversized, incomplete, substituted, or non-PDF exports remain blocked.
  Windows native PDF load and non-zero page-count checks run before `StartDoc`;
  failures remain retryable without printer side effects and include a pathless
  decoder stage in support diagnostics.

## 1.5.1 - 2026-09-17

### Fixed

- Excel worksheet printing no longer writes the application-wide Calculation
  and CalculateBeforeSave properties before any workbook is open. Some desktop
  Excel versions reject that sequence with COM error 1004, causing every sheet
  in an otherwise safe workbook to fail before printing.
- Pathless printer diagnostics now retain the exact Excel COM member stage and
  HRESULT for a failed job while the UI keeps the localized operator message.

### Safety

- Workbooks still open read-only with macros, events, alerts, and link updates
  disabled. Safety preflight, source pinning, private staging, PDF validation,
  controlled serial submission, and no-duplicate retry rules are unchanged.

## 1.5.0 - 2026-09-17

### Added

- Automatic RFI System publication can check as often as once per minute while
  DocFlow is open.
- Each approved automatic profile now includes a source-file settle window and
  configurable maximum changed-file and total-MiB circuit breakers.

### Safety

- A recently modified source file defers the run without publishing or revoking
  standing approval. A mass change beyond either approved limit pauses automation
  before Apply and remains available for manual Preview.
- Mail extraction remains suggestion-only. Irregular requests and attached-file
  correctness require operator review; numbering still comes only from RFI Log.

## 1.4.0 - 2026-09-17

### Added

- Settings support diagnostics now includes a bounded, pathless snapshot of the
  latest print queue owned by the signed-in operator. It records printer
  inventory state, selected printer, formats, controlled/native routes,
  requested options, attempts, outcomes, counts, and redacted error details.
- The same local Preview, Copy, and Download controls produce the printer report;
  no new service, upload, or paid dependency is used.

### Safety

- Printer diagnostics exclude document names, paths, contents, source identity,
  credentials, and tokens. Recursive redaction and exact canonical clipboard
  validation still apply.
- Queue evidence never certifies physical paper output and never authorizes an
  automatic retry of an uncertain or completed-unconfirmed job.

## 1.3.0 - 2026-09-17

### Added

- Replica publication now stores successful SHA-256 comparison work in the
  durable local scan index. Later runs reuse only exact unchanged file
  fingerprints across the same owner, root, relative path, operation, and hash
  algorithm, and Preview shows reused versus freshly hashed counts.
- Hash checkpoints are written as each file completes, so an interrupted large
  RFI System comparison resumes without reading every completed file again.

### Safety

- Cached hashes never authorize publication. Apply still reopens and validates
  the approved live source and target hashes before backup, copy, or replacement.
- Invalid, changed, replaced, unreadable, or differently scoped cache entries
  are ignored and hashed again.

## 1.2.0 - 2026-09-16

### Added

- Added one-way RFI System publication from one operator-selected source to two
  targets. Scope is limited to CIV/MEC/ELE/PRC and explicitly named root files;
  every copy or replacement is visible before the first Apply.
- Added optional unattended publication. One successful explicit Apply grants
  revocable standing authorization only to that exact source, targets, backup,
  scope, and interval. Due runs continue while DocFlow is open; unsafe failures
  pause the profile. Settings shows the live running, waiting, disabled, or
  safety-paused state without exposing system paths.
- Added optional per-user Windows startup without administrator permission.

### Safety

- Changed target files receive retained no-clobber backups whose hashes are
  verified before serial atomic publication. Source and target hashes are
  revalidated, results are journaled and Undo is guarded. Target-only files are
  reported and never deleted; backups never expire automatically.
- Missing selected source files, symlinks, overlapping roots, incomplete scans,
  unstable files, locks, drift, and collisions fail closed.

## 1.1.0 - 2026-09-16

### Added

- Added `Mark completed RFIs from the RFI System`. It recursively scans every
  nested folder for exact `RFI No. AZE-SUN-INT-QMS-RFI-XXX-0000.pdf` filenames
  with bounded multi-core discovery, then previews the matching RFI Log
  `Inspection Status` cells before setting approved rows to `Completed`.
- Preview exposes the PDF path, duplicate count, current status, proposed
  status, and exact workbook-cell comparison. Blank and `Ongoing` literal cells
  are eligible; formulas, merged/missing cells, `Cancelled`, `Rejected`, unknown
  values, invalid PDF names, and missing RFI Log rows remain manual review.

### Safety

- The RFI System stays read-only. Apply uses the existing fingerprint, backup,
  closed OOXML-delta verification, journal, and guarded Undo path; it never
  recalculates discovery or changes Follow Up/Worley data.

## 1.0.2 - 2026-09-15

### Fixed

- EPSON and similar Windows drivers may report a transient generic error or a
  stale error beside completion. Batch Print now keeps observing the same job
  until completion, release, cancellation, or timeout instead of stopping
  immediately; destructive deletion states still pause and no uncertain job is
  automatically submitted twice.
- Word, PowerPoint, and other native print handlers are now correlated to the
  single new spool job even when the handler replaces the filename with an
  application-generated title. Transient queue-enumeration failures receive the
  existing bounded observation grace before the result becomes unconfirmed.

## 1.0.1 - 2026-09-15

### Fixed

- Group Files by RFI now preserves Word paragraph, table-cell, and explicit line
  boundaries while reading Office XML. Checklist RFI numbers split onto the next
  visual line are detected without accidentally joining the following table cell
  to the four-digit number.

## 1.0.0 - 2026-09-15

### Removed

- Removed RFI Work Tracking, its Today/Resume/Needs Review queues, Record RFI
  Work, and Export RFI Work Audit from the UI, operation registry, and callable
  native command surface.
- Removed the Device Readiness dashboard panel and its background frontend/native
  query. Workflow-specific OCR, printer, recovery, workbook, OneDrive, and
  source-validation checks remain active where they are required.
- Existing local work-event files are preserved as inactive legacy data; this
  release never deletes user history automatically.

### Breaking change

- The registered operation roster is now nine operations. Retired operation and
  command identifiers return unknown-operation/method errors.

## 0.28.1 - 2026-09-15

### Fixed

- Recognizes exact canonical RFI codes split across up to two OCR/content line
  breaks around hyphens, including `AZE-SUN-INT-QMS-RFI-` followed by
  `MEC-0765` on the next line. The shared recognizer improves Process Scans and
  other OCR-backed RFI workflows without accepting decorated identifiers.
- Group Files by RFI now requires at least one readable matching EML or MSG for
  each RFI. When mail is missing, it creates no folder and copies none of that
  RFI's related files; Preview identifies the RFI and affected files instead.
- Mail eligibility is independent per RFI, so a multi-RFI document can still be
  filed for the RFIs whose email evidence exists while the others remain blocked.

## 0.28.0 - 2026-09-15

### Added

- Added `Group Files by RFI`: it scans top-level incoming files, detects exact
  RFI identifiers from filenames or bounded PDF/Office/EML/MSG content, and
  previews one copy into every related canonical RFI folder.
- Multi-RFI mail is copied to each matching folder. Unmatched or unreadable
  files are preserved and copied to destination `_review` for manual handling.
- The compact generic Preview supports per-row, selected-group, and all-row
  inclusion controls and shows detection method, source file, and confidence.

### Safety and performance

- Source files and existing source subfolders remain untouched; source and
  destination must be separate, symbolic links fail closed, and existing
  targets are never overwritten.
- Independent file inspection uses the bounded multi-core Rust worker pool with
  visible discovery, analysis, and planning stages. Apply and Undo remain
  ordered, journaled, and deterministic.
- Decorated, non-ASCII, extended, malformed, and ambiguous RFI identifiers are
  rejected instead of being silently cleaned or persisted.

## 0.27.0 - 2026-09-15

### Added

- Every registered operation now reports a localized, named Preview stage with
  completed/total counts and the current file, folder, page, workbook, printer,
  or record where available.

### Changed

- RFI System discovery and Worley first-page signature detection now use bounded
  multi-core analysis. Each completed PDF still receives an immediate serial
  SQLite checkpoint, and final Plan order remains deterministic.
- Process Scans now reports each completed parallel OCR item. Recursive signed
  input discovery, signed-PDF inspection, PDF-orientation discovery, and large
  RFI work-event reads now use the shared bounded worker pool.
- Print submission, workbook mutation, Apply publication, journal, recovery,
  and Undo remain intentionally ordered and single-writer.

## 0.26.2 - 2026-09-14

### Changed

- Removed an unused direct Tokio dependency; Tauri continues to provide the
  asynchronous runtime used by the application.
- Removed a duplicated PDF-orientation paragraph from the architecture canon.

## 0.26.1 - 2026-09-14

### Changed

- Moved shared, domain-independent interface controls out of the main application
  module without changing operation behavior, styling, accessibility, or bundle
  loading.

## 0.26.0 - 2026-09-14

### Added

- Stage signed RFI first pages now shows live discovery, page inspection, OCR,
  classification, archive search, and comparison-preparation stages instead of
  appearing idle until the end.
- Warning PDFs have an explicit Review-copy preview with their exact source file,
  affected page and RFI when known, diagnostic reason, destination, and copy
  status. Apply copies each affected source PDF once beneath `_review`, preserves
  its signed-input subfolder path, revalidates its previewed hash, and never
  overwrites or modifies the source.

### Changed

- The signed-page preview puts approved RFIs first and adds a sticky navigator
  for approved rows, page comparisons, Review copies, and warnings.
- First-page comparison now uses larger higher-resolution thumbnails, explicit
  Before/After labels, source filenames, previous/next RFI controls, and an
  enlarged view. Only the approved first-page replacement is described; remaining
  archive pages are explicitly shown as unchanged.

## 0.25.0 - 2026-09-14

### Added

- Stage signed RFI first pages now searches the selected read-only RFI System
  root through every nested folder using a bounded multi-core breadth scan. The
  approved Plan records worker, directory, and entry counts while exact filename
  matching, symlink refusal, archive immutability, and deterministic output order
  remain unchanged.

### Changed

- New signed-scan folders are recursive by default, so nested scanner batches
  are included without an extra click. The operator can still deliberately
  disable this for a top-level-only input run; archive lookup always remains
  fully recursive.

## 0.24.2 - 2026-09-14

### Fixed

- Batch Print no longer stops after each file when Windows accepts a native-app
  print request but the EPSON driver exposes no identifiable spool record. The
  job is recorded as sent without a final receipt, is never submitted twice,
  and the queue automatically continues; explicit printer faults and uncertain
  cancellation still pause for review.

## 0.24.1 - 2026-09-14

### Fixed

- Fixed an EPSON/Windows spooler race where a successfully printed job could be
  released between the two `GetJob` reads and incorrectly stop Batch Print as
  unconfirmed after every file. Normal release now continues to the next file;
  real read errors and explicit fault/deletion states remain uncertain.

## 0.24.0 - 2026-09-14

### Added

- OCR-backed scan previews, Worley signed-page rows, page-orientation decisions,
  and Review items now show their recognition score out of 100. The tooltip
  makes clear that the score is review evidence, not automatic approval.

### Fixed

- Plan/editor content, long warning blocks, dialogs, and compact bulk controls
  now remain scrollable and reachable on small or short displays; Apply and
  Cancel remain outside the scrolling plan content.

## 0.23.0 - 2026-09-14

### Added

- Added a durable local incremental scan index for RFI System Sync and PDF page
  orientation. Each completed PDF is checkpointed independently, so interrupted
  jobs reuse finished work instead of restarting a large RFI archive from zero.
- Preview now reports found, reused, freshly scanned, failed, and earlier
  interrupted counts. A task-local Force full rescan switch bypasses the index
  once and is never remembered automatically.

### Changed

- Unchanged files with the same owner, root, operation profile, relative path,
  filesystem identity, size, and modification time skip repeated PDF rendering
  and OCR. New, changed, transiently failed, and force-selected files scan again.

### Safety

- Scan records, paths, fingerprints, and cached analysis stay in the local app
  database and are not uploaded to Supabase. Existing mutation journals remain
  the operation log and sole Undo authority.
- Cached results optimize Preview only. They cannot authorize a document or
  workbook change; Apply still revalidates the exact approved source hash,
  capability, destination, and workbook guards.

## 0.22.0 - 2026-09-14

### Added

- Added an explicit one-click action to restore the last previewed safe Batch
  Print settings. New visits start from clean defaults, and named presets remain
  available for intentionally reusable procedures.

### Fixed

- Fixed false Windows print failures when a physical-printer driver releases a
  successfully submitted spool job before DocFlow's first status poll.
- Offline, paper-out, blocked, and user-intervention states now wait for recovery
  or the approved timeout instead of stopping the queue on the first poll.
- Brief printer-open or status-read failures now receive a bounded observation
  retry without resubmitting the document.

### Safety

- Previous file selections are never remembered or restored. Only safe scalar
  settings can be recalled explicitly.
- Explicit error/deleting/deleted spool states remain uncertain and are never
  automatically retried; cancellation and queue recovery semantics are preserved.

## 0.21.0 - 2026-09-14

### Added

- Expanded RFI Log to Follow Up synchronization across the existing CIV, MEC,
  PCU, and Road tables, including safe insertion into uniquely mapped cells and
  grouped RFI status aggregation.
- Added exact parsing for historical compact RFI groups and ranges, plus a
  current-workbook guarded-delta test that patches only a private temporary copy.

### Changed

- Blank RFI Log status maps to `Pending`, `Ongoing` maps to `In Progress`, and
  `Completed` maps to `Completed`; a mixed shared cell becomes `In Progress`.
- `Signed By Worley CIV/MEC` status synchronization now follows the exact
  `Signed by Worley` REMARKS line and remains limited to existing FINAL tables.

### Safety

- Cancelled and Rejected RFI tokens are removed from grouped Follow Up cells;
  remaining status is recalculated, or both cells are cleared when none remain.
- `POT test`, `STAT`, `Changelog`, ELE/PRC, absent ITP sections, formulas,
  merged cells, ambiguous multi-location mappings, and unrelated OOXML package
  parts are never changed.
- Apply still revalidates both workbook fingerprints, creates a verified
  non-overwriting backup, executes only previewed literal-cell replacements,
  verifies the closed package delta, journals the result, and supports Undo.

## 0.20.0 - 2026-09-14

### Added

- Added multi-core page-by-page orientation analysis for every PDF below the RFI
  system's CIV, MEC, ELE, and PRC folders, with content-based Tesseract OSD
  detection and preserved relative folder layout in a separate review root.

### Changed

- Process Scans and combined Worley page recognition now use the same bounded
  multi-core Rust analysis pool when the native OCR adapter is active. Ordered
  Apply, journaling, Undo, Office work, and print submission remain serial.

### Safety

- The production RFI system remains read-only. Low-confidence, unreadable,
  encrypted, signed, malformed, and colliding PDFs fail closed instead of being
  partially rotated.
- Apply uses only approved page rotations, verifies the source hash, appends only
  page dictionary changes to a private copy, validates unrelated PDF structures,
  publishes without overwrite, journals the result, and supports guarded Undo.

## 0.19.0 - 2026-09-13

### Added

- Added a complete ordered onboarding map, stable product requirement IDs,
  feature catalog, RFI domain rules, safety/state contract, generic editor
  contract, code/test traceability, known-issue ledger, Windows runbook, and a
  concise compatibility guide for other coding agents.
- Added a documentation audit that verifies every required guide exists, local
  map links resolve, all 14 requirements have traceability owners, and the nine
  documented operations match the Rust registry exactly.

### Changed

- The command protocol now documents the active closed typed Tauri boundary
  before its retained historical sidecar description.
- The repository agent guide now routes every model through the canonical
  documentation map and distinguishes OCR normalization from strict persisted
  ASCII RFI identity.

### Safety

- Missing or stale onboarding/operation documentation now fails a canonical
  source gate instead of silently misleading a future maintainer or AI model.

## 0.18.3 - 2026-09-13

### Fixed

- The release tool now updates and verifies the current version shown in the
  README, product definition, and roadmap as well as every runtime manifest.
- Corrected the stale roadmap heading that still described 0.13 as current.

### Safety

- A release check now fails when any human-facing current-version marker drifts
  from `VERSION`, preventing a package and its operator or AI documentation from
  silently describing different baselines.

## 0.18.2 - 2026-09-13

### Fixed

- macOS startup now reads an existing secure session without allowing Keychain
  authentication UI to block the application on its splash screen.
- If a session cannot be read without interaction, DocFlow fails closed to the
  sign-in screen while leaving the Keychain item untouched.

### Safety

- Tokens remain inside the native OS credential store and never cross into the
  webview. Startup does not copy them into settings or weaken credential access.
- Credential creation, update, and deletion behavior is unchanged; only the
  passive startup read is non-interactive.

## 0.18.1 - 2026-09-13

### Fixed

- Device readiness now counts interrupted work through a bounded, read-only
  journal inspection instead of running recovery during dashboard startup.
- A finalized RFI Follow Up recovery notice can now be listed without reopening
  its former workbook root, so a stale path identity no longer hides the
  Recovery Center behind a raw path-grant error.

### Safety

- Startup inspection does not acquire mutation authority, rewrite a journal,
  reopen document roots, or retry an uncertain workbook operation.
- Pending and finalized recovery records remain visible and are never silently
  discarded or treated as safe to apply again.

## 0.18.0 - 2026-09-13

### Added

- Guarded RFI System and Follow Up updates now expose an expandable exact cell
  comparison before Apply: workbook, sheet/cell, RFI, current value, and new value.

### Safety

- The comparison reads only expected and replacement values already embedded in
  approved workbook steps. It does not reopen Excel or recalculate mutations.
- Invalid coordinates, oversized text, unknown workbook kinds, and non-canonical
  RFI links fail closed; display is capped at 1,000 approved cell changes.

### Fixed

- Corrected the release-safety document's stale operation count from eight to nine.

## 0.17.0 - 2026-09-13

### Added

- Warning-heavy previews now provide compact text search, warning-type filtering,
  visible result counts, and a bounded scrolling list.
- Exact Worley RFI warnings can open their related old/new first-page comparison
  without making the operator search the page selector manually.

### Safety

- Filtering is presentation-only: it never removes warnings from the approved
  Plan or changes Apply eligibility.
- Page focus requires an exact ASCII canonical RFI code already present in the
  signed-page editor. Corrupted or decorated identifiers cannot create a link,
  and focus never selects an archive or revises executable steps.

## 0.16.0 - 2026-09-13

### Added

- Worley signed-page staging now shows the current archive first page beside the
  exact new signed page selected by the approved Plan.
- Ambiguous archive choices update the old-page image together with the selected
  archive candidate.

### Safety

- Rust generates read-only grayscale PNG thumbnails only for the pinned Plan
  pages; source scans and archives are never changed during preview.
- Preview is capped at 20 RFI rows, 256 KiB per PNG, 4 MiB total encoded data,
  and 240 by 320 pixels. Missing or unsupported previews require manual review
  and never affect Apply authority.
- React accepts only bounded PNG-signature data values and receives no local file
  URL or added filesystem authority.

## 0.15.0 - 2026-09-13

### Added

- Added a collapsed exact-effects panel to schema-driven editable previews.
- Operators can expand it before Apply to inspect every approved non-directory
  effect with its complete source and destination paths.

### Safety

- The panel reads only immutable Plan steps already approved by Rust; opening it
  performs no filesystem read, recalculation, or mutation.
- The list is height-bounded and scrollable so large plans do not hide the
  editor or Apply controls.

## 0.14.0 - 2026-09-13

### Added

- Added a compact Apply-impact summary to every ordinary and editable preview.
- The summary separately counts new outputs, updated files, moves or renames,
  print jobs, Outlook drafts, cleanup actions, and warnings.

### Safety

- Impact counts are derived only from the immutable Rust-approved Plan and do
  not create, remove, or authorize effects.
- The summary explicitly reminds the operator that preview has not changed any
  document and Apply can execute only the approved preview.

## 0.13.1 - 2026-09-13

### Fixed

- Combined Worley scan preview now reports the page currently being analyzed
  and responds to cancellation between pages instead of waiting for the entire
  PDF to finish.

### Safety

- Cancellation during page recognition remains preview-only and creates no
  review output, journal, source split, or archive change.

## 0.13.0 - 2026-09-13

### Added

- Added page-by-page RFI recognition for a single combined Worley scan, so each
  confidently identified page becomes its own review row and exact staging step.
- Added page numbers to signed-scan review labels and four-language guidance for
  combined scans.

### Safety

- Preview pins the whole signed PDF fingerprint and exact page index; apply does
  not split, modify, or re-read the source to rediscover work.
- Unreadable, low-confidence, duplicate, unsupported, missing, and ambiguous
  pages produce no output. The archive remains read-only and existing review
  files remain protected from overwrite.

## 0.12.0 - 2026-09-13

### Added

- Added owner-scoped RFI work audit export as a lossless JSON file and an
  Excel-safe CSV file from one previewed operation.
- Added live event-count, storage-size, and 10,000-event capacity visibility to
  the RFI work dashboard.

### Safety

- Audit files use unique names, never overwrite an existing destination, and
  remain journaled and undoable. Invalid or foreign-owner events are excluded.
- Formula-like CSV text is neutralized before Excel opens it, each output is
  bounded to 16 MiB, and export never deletes or changes work-item history.

## 0.11.0 - 2026-09-13

### Added

- Added one-click launch of the next journal-producing operation from eligible
  RFI work cards: RFI Log after operator checking, Process Scans after the
  publication email is archived, and System Sync after the INTEC scan is filed.
- Added a separate preview-first work-item update after a fully successful
  operation, linked to the exact local journal that proved the effect.
- RFI Log evidence can populate newly assigned exact RFI numbers; scan and
  Worley-sync evidence must cover every RFI already attached to the work item.

### Safety

- Partial, cancelled, failed, missing, foreign-owner, undone, mismatched, and
  unsupported journals cannot advance an RFI work stage.
- Process Scans counts only successfully filed document steps, not `_review`
  routing, and Worley synchronization requires the same RFI to be proven in
  both committed workbook-update steps.
- Outlook drafts never advance publication or archive stages. If previously
  accepted source evidence later becomes invalid, the work item moves to Needs
  Review instead of retaining an unsupported success state.

## 0.10.1 - 2026-09-13

### Fixed

- Localized the shared back-button accessible label and the home-screen
  preview-first safety badge in Turkish, English, Russian, and Azerbaijani.

## 0.10.0 - 2026-09-13

### Added

- Made every Today, Resume, and Needs Review card directly open its existing
  RFI work item in the shared schema-driven editor with the item identity,
  current stage, due date, and review state already filled.

### Safety

- Added a reusable schema-level `remember` policy and marked RFI work-item
  fields transient, so task identities and prior transition values cannot leak
  into the next new item through saved form preferences.
- The shortcut still produces the same preview-first, explicitly approved,
  no-clobber, journaled work-item event; it adds no document mutation path.

## 0.9.0 - 2026-09-13

### Added

- Added offline-first RFI work items with explicit lifecycle stages, optional
  due dates, strict canonical RFI numbers, review flags, and mandatory reasons
  for manual stage changes.
- Added compact Today, Resume, and Needs Review queues to the home screen.
- Added an authenticated, bounded work-item query and a schema-driven operation
  for creating or advancing work without an operation-specific React form.

### Safety

- Every work-item event is previewed, explicitly approved, written as a new
  no-clobber file, journaled, and undoable; official Excel files and RFI folders
  remain unchanged and authoritative.
- Event storage is append-only, owner-scoped, size/count bounded, and rejects
  symlinks, malformed records, non-ASCII RFI codes, and unknown stages.
- Concurrent transitions never use last-writer-wins; a stale branch sends the
  item to Needs Review while retaining the last proven stage.

## 0.8.0 - 2026-09-13

### Added

- Promoted the device-readiness foundation to a SemVer minor release because it
  adds a backward-compatible user capability.
- Added a tested `next fix|feature|breaking` version command so completed work
  is classified consistently instead of extending one prerelease number.

### Changed

- Established PATCH for compatible fixes, MINOR for compatible features, and
  MAJOR for incompatible contract changes as the mandatory commit workflow.

### Safety

- Version correction was committed separately without rewriting earlier Git
  history, preserving exact rollback points for the preceding implementation.

## 0.7.0-rc.11 - 2026-09-13

### Added

- Added an authenticated, read-only **Device readiness** panel to the home
  screen. It checks bundled OCR/language availability, enumerated printers, and
  interrupted journal or print-queue recovery work without scanning user files.
- Added one-click navigation from an attention result to Recovery Center and a
  manual refresh action.
- Added the accepted offline-first RFI work-item lifecycle and evidence boundary
  as ADR 0002, ready for incremental implementation.

### Changed

- Reconciled the README and roadmap with the current seven-operation Rust
  application and the 0.7 reliability/unified-work priorities.

### Safety

- A healthy device-readiness result is intentionally not treated as Excel,
  OneDrive, Office, or printer-output certification. File-specific safety checks
  remain in operation preview and platform certification remains separate.

## 0.7.0-rc.10 - 2026-09-13

### Fixed

- Corrected **Analyze RFI System & Sync Worley Signatures** against the current
  production workbook layouts:
  - discovers each RFI Log `RFI No.` and `REMARKS` column from exact headers
    instead of writing to former fixed columns 20/21, which are current
    shared-formula Folder Link columns;
  - updates only literal `REMARKS` cells and the `Signed By Worley CIV`
    `STATUS` / `Signed By Worley MEC` `Worley Status` control cells;
  - preserves existing remarks by appending the exact `Signed by Worley` line;
  - rejects missing, merged, formula-bearing, unexpected, stale, or altered
    targets before unsafe mutation;
  - applies a closed package-delta verifier so shared formulas, `calcChain.xml`,
    VBA, styles, links, and every non-target OOXML member remain unchanged;
  - revalidates both workbook fingerprints before the first mutation and again
    before each workbook step;
  - supports actual per-RFI selection during plan revision.

### Verification

- Added an operator-supplied production-template acceptance test. Copies of the
  current RFI Log and Inspection Status Follow Up passed guarded preview/output
  verification without changing their source files.

## 0.7.0-rc.9 - 2026-09-12

### Fixed

- Resolved `path grant does not allow this access` (`BackendError::path_access_denied`) in Analyze RFI System & Sync Worley Signatures (`rfi.system_sync`):
  - Fixed path grant selection in `preview_rfi_system_sync` which previously defaulted to matching the read-only `system_root` grant when RFI Log or Follow Up workbooks were stored inside the selected system folder.
  - Dynamically promoted `system_root` provisional grant to `ReadWrite` when workbooks are nested inside or equal to the system directory, avoiding conflicting capability grants and safely granting write authority.
  - Explicitly resolved grants with write intent (`g.access.allows(PathIntent::Write)`), and resolved candidate paths against canonical capabilities in `authorize_capability_for_grant`.
  - Added missing `provisional.commit()` to preserve granted path capabilities across preview -> apply boundaries.
  - Fixed `reserved plan ID requires an empty plan ID` by initializing `Plan` with an empty `plan_id` for reserved plan binding.

## 0.7.0-rc.8 - 2026-09-12

### Added

- Added **Analyze RFI System & Sync Worley Signatures** (`rfi.system_sync`) operation:
  - Scans single RFI system folder structure containing discipline directories (`CIV`, `MEC`, `ELE`, `PRC`) and canonical RFI folders (e.g. `AZE-SUN-INT-QMS-RFI-CIV-0103`).
  - Inspects scanned PDF files matching `RFI No. ...` for Worley's signature in the page 1 bottom-right signature box labeled `Others` (identifying signatures such as E.Mammadov via dark ink density, handwriting, and stamp recognition).
  - Synchronizes RFI Log discipline sheets (`CIV`, `MEC`, `ELE`, `PRC`): writes `Signed by Worley` in the `REMARKS` column (column 20 for CIV/ELE/PRC, column 21 for MEC) for signed RFIs, leaving unsigned RFIs untouched.
  - Synchronizes Follow Up Final ITP sheets (`Signed by CIV` and `Signed by MEC`):
    - Civil: maps ITP item `2.3.1` to Follow Up column `3.1`, matched by `Area` column == RFI Log `Tag No.`, writing short RFI code (e.g. `CIV-0103`).
    - Mechanical: maps ITP items `1.3.5` -> column `3.5`, `3.3.1` -> column `3.1`, and `4.3.2` -> column `3.2`, matched by `PCS number` == `tagNo`, `Section` == `area`, and `Area` == `location`, writing short RFI code (e.g. `MEC-0700`).
  - Generates comprehensive monochrome preview table with discipline filtering, status details, and individual toggle checkboxes.
  - Multi-workbook apply with automatic no-clobber backups and lossless journaled undo.
  - Complete localized UI strings in `en`, `tr`, `ru`, and `az`.

### Fixed

- Resolved `More than one safe Follow Up table was found. Keep one exact RFI NO./STATUS table.` (`error.rfi.followUp.layoutAmbiguous`) in Worley signature status synchronization when handling real-world Follow Up workbooks containing multiple discipline sheets (e.g. `Signed by CIV` and `Signed by MEC`).
- Added multi-sheet layout discovery and atomic preview/apply synchronization across all valid discipline Follow Up sheets in a single transaction.
- Added smart sheet disambiguation prioritizing recognized discipline conventions (`SIGNED BY`, `FOLLOW UP`, `CIV`, `MEC`, `ELE`, `PRC`) while preserving strict ambiguity rejection for unknown dummy sheets.

## 0.7.0-rc.6 - 2026-09-12

### Fixed

- Resolved `unknown workbook relationship class` validation error in Worley signature synchronization (`rfi.sync_worley`) when loading real-world Excel workbooks (`.xlsm` / `.xlsx`).
- Added full support for Microsoft Office relationship namespaces across versions (e.g. `2006`, `2007`, `2011`, `2014`, `2017`, `2019`, `2020`) and package-level relationship namespaces.
- Expanded allowed workbook and worksheet relationship classes to recognize standard Excel elements including `volatileDependencies`, `calcFeatures`, `features`, `vbaProjectSignature`, `customXmlMappings`, `xmlMaps`, `customData`, `customDataProperties`, `customUI`, `attachedToolbars`, `activeXControlBinary`, `usernames`, `revisionLog`, and `revisionHeaders`.

## 0.7.0-rc.5 - 2026-09-11

### Fixed

- Eliminated UI button overlap and obscured form fields by decoupling action bar from scrollable form/review content (`flow-scroll`) and pinning action buttons cleanly at the bottom.
- Resolved unclosed `@media (prefers-color-scheme: dark)` brace in `styles.css` that leaked media query scope across subsequent global styles.
- Fixed detached table header bug where regular data columns stuck to `right: 0` during horizontal scrolling when row actions were disabled; now explicitly targeted with `editor-actions-header`.
- Added clear separation border and shadow to sticky row action columns to prevent scrolled table cells from bleeding behind buttons.
- Added `white-space: nowrap` and `flex-shrink: 0` to `.button` and icon buttons to prevent label wrapping and squishing across different window sizes and localized languages.
- Added auto-close behavior and focus management to `CompactDateInput` year select to prevent overlaying rows beneath it.
- Added 1-click clear button (`X`) to path inputs when a file or folder is selected.
- Added `max-height` and scrolling to `.modal` to guarantee action buttons are always accessible on low-height viewports.

## 0.7.0-rc.4 - 2026-09-11

### Added

- High-contrast monochrome dark mode adhering to Rule 11 with obsidian backdrop (`#0a0a0a`), clean surface tiers (`#141414`, `#1e1e1e`, `#2a2a2a`), crisp white text, and distinct card borders.
- 1-click header theme switcher toggling between Light, Dark, and System with `Alt + T` operator keyboard shortcut.
- Full localization in Turkish, English, Russian, and Azerbaijani.

## 0.7.0-rc.3 - 2026-09-11

### Added

- Live preview table search box and instant filter tabs (`All`, `Changes`, `Manual Review`) with dynamic filtered row count.
- Global operator keyboard shortcuts: `Ctrl/Cmd + Enter` to trigger Preview or Apply, `Escape` to return to form or close modals, and `Space` to toggle row selection or enable/disable state.
- Localized filter labels in Turkish, English, Russian, and Azerbaijani.

## 0.7.0-rc.2 - 2026-09-11

### Added

- Automated cross-compilation pipeline for Windows x64 (`tools/build_windows_cross.sh`) packaging standalone Tesseract OCR binaries and `eng.traineddata` into both NSIS installer and zero-install portable zip.
- HTML5 drag-and-drop on all single and multi-file/folder form inputs with monochrome active drop states.
- Automatic parameter persistence and re-issue of opaque path tokens on operation load.
- 1-click diagnostic report download in Preferences -> Support.
- Robust header discovery across non-standard contractor RFI logs (detecting RFI and Worley status columns across first 10 rows and 64 columns).

### Safety

- Preserved strict non-worksheet sheet relationship checks in OOXML workbook discovery.
- All 321 Rust tests and 39 frontend/tooling tests passing with zero warnings.

## 0.7.0-rc.1 - 2026-08-29

### Added

- Exact, package-preserving RFI Follow Up preview, revision, apply, recovery,
  and targeted undo with bounded authority and no-clobber publication.
- Windows-only Outlook Reply All draft preparation that revalidates the exact
  selected message, preserves the existing quote/signature, displays only an
  unsent draft, and never synthesizes recipients or calls Send.
- A bounded, redacted, local-only diagnostics preview/copy surface with no
  telemetry or user-document scan.
- Commit-bound Windows installer, checksum, smoke, cold-start, peak-memory,
  package-audit, uninstall, and artifact-manifest evidence.

### Safety

- RFI matching is exact whole-value matching; only `Completed → Completed` and
  `Ongoing → In Progress` are automatically selectable.
- Outlook draft undo is available only for the exact unchanged durable draft;
  otherwise it fails closed.

### Changed

- The desktop application authority is fully Rust-owned; the retired application
  sidecar, interpreter runtime, child-process routing, and frozen packaging have
  been removed.
- Version, source/package audit, dependency-license, checksum, and packaged-smoke
  gates now run with Node, Rust, and platform-native tools only.

### Safety

- Final builds fail fast if retired runtime paths or shell/external-binary wiring
  reappear, and package inspection rejects interpreter libraries, bytecode,
  frozen-runtime artifacts, or an application sidecar.
- The packaged binary smoke verifies isolated bootstrap/settings plus real OCR and
  Process Scans apply/journal/undo before an artifact can be accepted.

## 0.6.0 - 2026-08-21

### Added

- Tauri 2 + React/TypeScript desktop shell while preserving the proven Python
  document engine and private JSON-lines sidecar boundary
- Safe signed-RFI first-page staging with read-only archive guarantees,
  duplicate/ambiguous match reporting, and original folder-structure retention
- Persistent, cancellable, resumable mixed-file print queues with per-file,
  group, and all-files configuration
- Compact individual, sheet, RFI-group, and all-row RFI Log editing
- Canonical `VERSION`, machine-readable `releases.json`, repository-wide version
  checks, complete historical changelog, and tag-driven release workflow

### Changed

- The active UI stack moves from Flutter/PySide rollback implementations to a
  lean Tauri webview; legacy settings, journals, trash, and Supabase rows remain
  compatible
- Windows packages are native NSIS installers with the Python sidecar and pinned
  Tesseract bundled without a companion console window

### Safety

- Webview code never receives Supabase access/refresh tokens or arbitrary
  filesystem/network authority
- Every mutation remains preview → approval → apply → journal → undo
- Signed archive PDFs remain read-only; publication never overwrites an existing
  review file and recovers safely from OneDrive/antivirus locks
- Release builds require synchronized Python, Node, Rust, Tauri, and lockfile
  versions before packaging

## 0.5.4 - 2026-08-03

### Fixed

- RFI inspection description metinlerindeki UTF-8 bullet karakterinin Windows
  code-page katmaninda `â€¢` olarak gorunmesi; sidecar JSON hatti artik Unicode'u
  ASCII-safe escape'lerle tasiyor ve istemci dogru `•` karakterini geri kuruyor
- `CIV-5.2.2` ve `CIV-5.2.3` icin description, forms, AZFEN type ve INTEC type
  degerleri uretim workbook'undaki gercek Sheet1 kayitlariyla sabitlendi

### Safety

- Workbook'tan okunan tum lookup metinleri NFC normalize edilir ve yalnizca
  guvenli oldugunda UTF-8/CP1252 mojibake otomatik onarilir
- Tum CIV, MEC, ELE ve PRC presetlerinde bos alan, uyusmayan AZFEN/INTEC item
  baglantisi veya cakisan duplicate tespit edilirse preview artik durur
- 234 uretim presetinin tamami gercek RFI Log uzerinde read-only denetlendi;
  eksik, cakisan veya supheli encoding kaydi bulunmadi

## 0.5.3 - 2026-08-01

### Added

- MEC RFI kayitlarinda `ROW No.` alani artik karma sheet editorunde gorunur ve
  duzenlenebilir; deger Excel'deki gercek H sutununa yazilir ve dogrulanir
- MEC icin Excel tabanli mail onizleme/kopyalama `ROW No.` basligini, degerini ve
  kaynak workbook bicimini sheet'e ozel 15 kolonlu secime dahil eder

### Changed

- Item No. secenekleri UI'da `CIV-`, `MEC-`, `ELE-` ve `PRC-` onekleri olmadan
  gosterilir; plan ve Excel'e yazilan asil deger tam disiplin onekini korur
- Kompakt RFI satir aksiyonlari Material'in guvenli tiklama alanini koruyacak
  genislikte tutulur; uc buton artik hucre disina tasmaz

### Safety

- Karma CIV/MEC editorunde yalniz MEC Row No. hucresinin duzenlenebilir oldugunu,
  MEC H sutununa yazmayi ve workbook-backed kopyaya alinmasini kalici testler
  dogrular

Bu proje [Semantic Versioning](https://semver.org/) kullanır. Kullanıcıya görünen
her feature veya fix, yeni Windows paketi üretilmeden önce burada kaydedilir.

## 0.5.2 - 2026-07-31

### Fixed

- Sheet bazlı Excel kopyalama butonlarının dar pencerelerde Done butonunun
  arkasına taşması; kopyalama aksiyonları artık kendi tam genişlikte, satır
  kırabilen alanında ve alt footer'ın üzerinde duruyor
- Çoklu-sheet RFI işleminde inspection saatlerinin bütün sheet'ler için tek
  ortak zaman çizelgesinden verilmesi; CIV, MEC, ELE ve PRC artık yalnızca kendi
  sheet'indeki mevcut saatleri devam ettiriyor

### Safety

- Dört kopyalama butonunun kompakt pencere genişliğinde Done alanıyla
  çakışmadığını doğrulayan widget testi
- CIV overflow, MEC devam saati ve bağımsız ELE/PRC başlangıç saatlerini aynı
  çoklu-sheet planında doğrulayan kalıcı regresyon testi

## 0.5.1 - 2026-07-30

### Fixed

- Yerel Windows paketinde PyInstaller sidecar klasörünün yanlışlıkla bir seviye
  fazla içeri kopyalanması nedeniyle arka plan servisinin başlayamaması
- Uygulamanın standart düz paket yerleşimine ek olarak iç içe PyInstaller
  yerleşimini de güvenli fallback olarak algılaması

### Safety

- Yerel runner tabanlı paketleyici sidecar klasörünün kendisini değil içeriğini
  kopyalıyor, beklenen `sidecar\docflow_sidecar.exe` yolunu zorunlu tutuyor ve
  paketi teslim etmeden önce gerçek executable ile `ping` çalıştırıyor
- Düz ve iç içe paket yerleşimleri ile geliştirme override'ı Flutter testleriyle
  kalıcı olarak doğrulanıyor

## 0.5.0 - 2026-07-30

### Added

- CIV, MEC, ELE ve PRC için aynı işlem içinde ayrı adetler seçebilen kompakt
  çoklu-sheet RFI seçim alanı
- Sheet bilgisi görünür tek düzenleme tablosunda tüm otomatik alanları değiştirme,
  Area bazlı aynı RFI satırını çoğaltma ve özel değer yazma desteği
- İşlem sonucunda yalnızca kayıt eklenen sheet'ler için ayrı Excel-native
  kopyalama butonları

### Changed

- RFI numarası, sonraki satır ve ITP/Item eşleşmeleri her sheet için bağımsız
  hesaplanırken inspection saatleri tüm sheet'lerde çakışmayı engelleyecek şekilde
  ortak planlanıyor
- Mail okuma CIV/MEC ile sınırlı değil; ELE ve PRC disiplinlerini de algılıyor
- Çoklu-sheet apply/undo işlemi tek workbook yazımı ve tek geri alma kaydıyla
  atomik olarak çalışıyor

### Safety

- CIV, MEC, ELE ve PRC formülleri, farklı sütun yerleşimleri, item/duplicate veri
  doğrulamaları ve dokunulmayan Expiry/Status/Validity alanları kalıcı testlerle
  korunuyor
- 18 binden fazla satırlı gerçek RFI Log güvenli kopyasında dört sheet birlikte
  uygulandı, sheet bazlı mail tabloları tekrar Excel'den okundu ve undo'nun dosyayı
  hash seviyesinde birebir geri getirdiği doğrulandı

## 0.4.4 - 2026-07-30

### Fixed

- Excel penceresinin filtreleme ve native clipboard kopyası tamamlanmadan
  görünmesi; kullanıcının seçim veya hücre değişikliğiyle COM otomasyonunu
  durdurabilmesi
- Excel artık hazırlık boyunca gizli, ekran güncellemesi ve kullanıcı
  etkileşimi kapalı çalışıyor; yalnız kopya hazırlandıktan sonra görünür ve
  etkileşimli hale geliyor

### Safety

- Script sırası için kalıcı test: Excel görünürlüğü ve etkileşimi yalnız
  `Range.Copy()` tamamlandıktan sonra açılabilir
- Gerçek Excel integration testi native Biff12/HTML/RTF biçimlerini ve doğru
  iki RFI satırını gizli hazırlık akışıyla doğruluyor

## 0.4.3 - 2026-07-30

### Fixed

- Birden fazla farklı RFI numarası kopyalanırken bazı Office type-library
  sürümlerinin `AutoFilter(..., 7)` COM çağrısını reddetmesi
- Çoklu seçimde aynı native Excel görünümünü problemli COM overload'u yerine
  hedef satırların görünürlüğünü doğrudan yöneterek oluşturma
- Başarısız Excel otomasyonunun boş `EXCEL.EXE /automation -Embedding`
  süreçleri bırakması; hata sinyali cleanup sonrasına taşındı ve PID tabanlı
  ikinci kapatma güvencesi eklendi

### Safety

- Gerçek Microsoft Excel integration testi iki farklı RFI için genişletildi:
  yalnız hedef satırlar, header ve Subcontractor kopyalanırken eski RFI ve
  Expiry panoya alınmıyor
- Kontrollü hata testinde yeni Excel süreci kalmadığı doğrulandı

## 0.4.2 - 2026-07-30

### Fixed

- 18 binden fazla satırlık RFI Log dosyasını kopyalama öncesinde rastgele hücre
  erişimiyle tekrar tekrar tarayan ve butonu süresiz yükleniyor durumunda
  bırakan okuma yolu tek geçişli akışa çevrildi
- Gerçek boyuttaki workbook'ta mail tablosu verisi saniyeler içinde hazırlanıp
  Excel'in native biçimli panosuna aktarılıyor

### Safety

- Büyük read-only worksheet'lerde veri satırlarına rastgele erişimi reddeden
  kalıcı regresyon testi eklendi
- Üretim boyutundaki güvenli workbook kopyasıyla Excel Biff12/Biff8, HTML ve
  RTF pano formatları uçtan uca doğrulandı

## 0.4.1 - 2026-07-30

### Fixed

- Güncel RFI Log dosyasında K:Q'dan L:R'ye kayan Sheet1 eşleme kolonlarının
  header yapısından dinamik bulunması
- Yeni CIV/MEC satırlarında VLOOKUP formüllerinin gerçek eşleme kolonlarıyla
  yazılması; calculated-column ve data-validation uyumluluğunun korunması
- Excel'deki bozuk ayraçlı ITP adlarının normal mail metniyle eşleşmesi ve
  birleşik MEC Tag değerlerinin kısaltılmadan çıkarılması
- Uygulama içindeki yaklaşık mail tablosu önizlemesi yerine gerçek workbook'un
  Excel'de filtrelenip header dâhil native Office formatlarıyla kopyalanması

### Safety

- RFI yazma testi gerçek üretim workbook kopyasında Excel COM ile açılma,
  formül tutarlılığı ve data-validation sonuçları üzerinden doğrulandı
- Kopyalama yalnız Excel'in kendi panosunu kullanır; Biff, HTML, RTF ve Office
  bağlantı formatları uygulama tarafından yeniden yazılmaz

## 0.4.0 - 2026-07-29

### Added

- Process Scans için varsayılan mevcut klasörlü davranışı koruyan, isteğe bağlı
  klasörsüz çıktı modu
- Klasörsüz modda adlandırılmış PDF'leri doğrudan seçilen hedef klasöre
  taşıma veya orijinalleri koruma seçiliyse kopyalama

### Safety

- Eski kayıtlı ayarlar ve eski istemci istekleri için klasörlü davranışın
  otomatik olarak korunması
- Process Scans klasörlü/klasörsüz hedef, kopyalama, apply ve diğer özellikler
  için genişletilmiş regresyon kontrolleri

## 0.3.0 - 2026-07-29

### Added

- Uygulama sonucunda Excel'den yeniden okunan başlık ve hücrelerle Outlook
  mail tablosu önizleme penceresi
- Excel kolon genişliklerini kullanan yatay/dikey kaydırılabilir, hücre metni
  seçilebilir tablo ve tek tıklamalı biçimli kopyalama

### Fixed

- Excel'in clipboard HTML belgesindeki `<head>` CSS stillerinin korunması
- Unicode içerikte CF_HTML byte offset'lerinin yeniden hesaplanarak Outlook'a
  eksik veya bozuk tablo yapıştırılmasının önlenmesi

## 0.2.4 - 2026-07-28

### Fixed

- Gemini REST `TextResponseFormat.MimeType` alanında metin MIME değeri yerine
  zorunlu `APPLICATION_JSON` enum değerinin gönderilmesi
- Güncel `responseFormat`, modern `responseJsonSchema` ve legacy
  `responseSchema` arasında üç kademeli structured-output uyumluluğu

## 0.2.3 - 2026-07-28

### Fixed

- Gemini Generate Content API'nin güncel `responseFormat` structured-output
  biçimine geçiş
- HTTP 400 uyumsuzluğunda güncel ve legacy istek biçimleri ile desteklenen
  modeller arasında kontrollü otomatik fallback
- `N3_PCS-02_S7/S8` gibi escaped underscore ve slash içeren MEC Tag
  numaralarının bozulmadan çıkarılması
- Kısa MEC Item numaralarının seçilen ITP eşleşmesiyle offline çözülmesi

## 0.2.2 - 2026-07-27

### Fixed

- Yeni kullanıcılara kapatılan `gemini-2.5-flash-lite` varsayılanı yerine
  ücretsiz katmanı bulunan `gemini-3.1-flash-lite` kullanılması
- Seçilen Gemini modeli 404 döndürürse güncel Flash modellerine otomatik geçiş
- Yeni Gemini modellerinde deprecated olan `temperature` parametresinin kaldırılması

## 0.2.1 - 2026-07-27

### Fixed

- Gemini structured-output şemasındaki geçersiz boş discipline enum değeri
- Markdown biçimli Azerice maillerde RFI adedi çıkarımı
- `PCU-14/15/16` gibi kompakt Tag listelerinin ayrı satırlara genişletilmesi
- `2.1` gibi kısa Item numaralarının seçilen ITP eşlemesiyle çözülmesi

## 0.2.0 - 2026-07-27

### Added

- RFI talep e-postalarından offline veya isteğe bağlı Gemini Free ile taslak çıkarma
- Excel'in yerel biçimini koruyan kesin RFI satırı kopyalama
- Uygulamanın sol altında kalıcı sürüm rozeti
- App, sidecar ve build manifesti arasında zorunlu sürüm eşleşme kontrolü
- Küçük ve hızlı Windows UI güncelleme artifact'i

### Fixed

- RFI Log data validation ve calculated-column uyumluluğu
- Process Scans OCR regresyon koruması
- Çoklu dosya yazdırmada kesin sıra ve kullanıcı müdahalesi bekleme

## 0.1.0 - 2026-07-26

- İlk doğrulanmış Windows paketi
- Process Scans, RFI Log ve Batch Print işlemleri

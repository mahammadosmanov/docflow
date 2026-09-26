# Feature catalog

## Registered operations

| Operation | Operator outcome |
|---|---|
| `rfi.process_scans` | OCR, name, and file top-level RFI PDFs or route exceptions to review |
| `rfi.group_files` | Detect exact RFIs in top-level filenames and content; copy a related set only when that RFI also has matching EML/MSG evidence |
| `rfi.log_add` | Extract/edit requests and apply guarded multi-sheet RFI Log rows |
| `rfi.follow_up_sync` | Add/update exact CIV multi-area rows and MEC rows with distinct exact route contexts, resolve `PCS-` tags and explicit PCU area suffixes, split exact MEC composite location values, quietly skip supported records absent from the current template (including unprepared FINAL rows), remove Cancelled/Rejected entries, and synchronize FINAL Worley state in Signed By sheets plus the current PCU FINAL Worley column; worksheet Remarks cells are not changed and duplicate-context or otherwise ambiguous matches remain manual review |
| `rfi.outlook_reply_draft` | Display one verified unsent Reply All draft from the selected message |
| `rfi.outlook_archive_selected` | Revalidate one selected Outlook message and create a guarded Unicode MSG staging copy |
| `files.batch_print` | Run and recover a strictly ordered mixed-file print queue; tolerate transient EPSON status/title/query behavior without duplicate submission, and optionally restore previous safe settings |
| `rfi.stage_signed_first_pages` | Identify nested single/combined Worley scans, search every RFI System subfolder with bounded multi-core discovery, and create review copies with exact page-one replacement |
| `rfi.normalize_pdf_orientation` | Inspect every PDF page below CIV/MEC/ELE/PRC in parallel and create fidelity-checked corrected review copies |
| `rfi.system_sync` | Scan exact canonical RFI PDFs and guard RFI Log plus Follow Up updates using only bottom-right OTHERS-cell Worley evidence |
| `rfi.complete_status_sync` | Recursively find exact completed RFI PDFs and preview guarded RFI Log Inspection Status updates to Completed |
| `files.replica_publish` | Publish one selected RFI System source to two targets with backup-first verified replacement, Undo, and optional standing authorization |

## Application features

- Generic schema-driven forms, remembered safe values, typed presets, compact
  multi-row editing, all/selection actions, search/filter, and keyboard shortcuts.
- A successful RFI Log Apply can continue directly into a separate, reviewable
  Outlook Reply All Plan containing the exact approved tables for every affected
  discipline. It opens an unsent draft only; it never sends or invents message text.
- Batch Print always starts without old files or silently restored settings. One
  explicit action can restore the last previewed printer/paper/layout/queue
  settings, while named presets remain available for reusable procedures.
- The isolated Excel exporter avoids pre-open calculation-mode writes that fail
  on some desktop Excel versions. It applies a bounded PDF envelope check, then
  uses Windows' native PDF decoder and page count as the print-readiness
  authority before printer submission. Pathless diagnostics retain the failing
  COM or native PDF stage.
- Settings support diagnostics includes a pathless snapshot of the latest owned
  print queue: printer inventory availability, selected printer, file formats,
  native/controlled routes, requested options, attempts, outcomes, counts, and
  bounded error details. Preview, copy, and download remain local and redacted.
- An accepted job whose final Windows spool receipt is unavailable is shown as
  sent-unconfirmed and the queue continues. It is never retried automatically;
  destructive deletion or uncertain cancellation still pauses for review.
- Preview impact counts, exact effect paths, approved workbook cell comparisons,
  and warning focus/filtering. Signed-page Preview adds an ordered live stage
  trail, sticky section navigator, approved rows first, higher-resolution
  before/after page comparison with source filenames and zoom, and a structured
  per-PDF warning/Review-copy summary.
- OCR-backed preview effects, signed-page rows, orientation decisions, and
  manual-review items display their recognition score out of 100. The score is
  review evidence, not a probability or automatic approval.
- Responsive plan panes keep long warnings, compact tables, modal content, and
  Apply/Cancel actions reachable through bounded vertical or table scrolling.
- Persistent local history, explicit recovery center, guarded undo, and
  recoverable review trash.
- Optional Supabase sign-in/approval/admin and metadata-only shared history.
- Copy-only redacted diagnostics.
- Turkish, English, Russian, and Azerbaijani UI with light/dark monochrome themes.
- Optional per-user Windows startup. Replica publication can run at a selected
  interval, down to one minute, while DocFlow is open after one explicit profile approval. Settings
  can pause or resume that stored profile, shows its live state, and reflects an
  automatic safety pause; any unsafe failure pauses it.
- Automatic replica publication waits for recently changed source files to
  settle. Per-profile maximum file effects and total MiB act as mass-change
  circuit breakers; exceeding either pauses automation before Apply.
- Bounded multi-core Rust analysis for Process Scans, combined signed-page OCR,
  RFI file-name/content grouping,
  recursive signed-input and archive discovery, PDF orientation discovery and
  page analysis and RFI System discovery/signature detection; state-changing
  effects remain ordered and journaled.
- Every registered operation reports localized preparation, discovery, analysis,
  planning, waiting, or ready stages with bounded item counters and the current
  file where available.
- Durable local incremental scanning for RFI System Sync and PDF orientation.
  Completed files survive interruption, unchanged files are reused, transient
  failures retry, scan counts are visible in Preview, and full rescan is an
  explicit task-local option.
- Replica publication also checkpoints SHA-256 work per source and target root.
  Later runs reuse only exact unchanged file fingerprints, show reused/fresh
  counts in Preview, and resume from completed hash checkpoints after interruption.
  Cached values only reduce comparison work; Apply still validates live hashes.
- Completion Status Sync uses bounded multi-core, read-only filename discovery
  at every depth. It shows the exact PDF evidence and workbook-cell changes,
  permits row/all selection, leaves conflicting status values for review, and
  applies through the same backup, OOXML verification, journal, and Undo path.
- Signed-page warnings identify the source PDF and affected page/RFI where known.
  Apply copies each affected input PDF once to `_review` with the relative input
  folder layout preserved; it never overwrites, mutates the signed input, or
  repeats a combined source PDF for each warned page.

## Intentional limits

- Process Scans discovers top-level PDFs only. It separates sequential RFI
  batches by page, keeps pages without an RFI candidate with the current RFI,
  and sends uncertain pages to `_review`. CIV/MEC/ELE/PRC outputs use matching
  discipline folders. Page orientation correction defaults on and uses the
  existing bounded detector; uncertain files go unchanged to `_review`.
  Signed-page input recursion defaults
  on but can be deliberately disabled; RFI System archive lookup is always recursive.
- Outlook drafts are never auto-sent. Selected-mail archival creates only a
  local MSG copy; it never changes, moves, deletes, or sends mailbox content.
- Replica publication never propagates target-only files back to the source and
  never deletes them automatically. It is one-way publication, not bidirectional merge.
- Native Office and printer behavior is accepted only on a real Windows host.
- PDF orientation classification requires bundled Tesseract OSD and English
  data. It uses multi-resolution OSD first, then a conservative four-direction
  OCR comparison only for unresolved pages. A PDF with any still-ambiguous or
  unreadable page is left unchanged for manual review.

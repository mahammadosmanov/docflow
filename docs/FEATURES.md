# Feature catalog

## Registered operations

| Operation | Operator outcome |
|---|---|
| `rfi.process_scans` | OCR, name, and file top-level RFI PDFs or route exceptions to review |
| `rfi.process_scans_fast_covers` | Process top-level multi-page RFI PDFs with separate template-led cover detection and create missing discipline/RFI folders; normal Process Scans remains unchanged |
| `rfi.group_files` | Detect exact RFIs in top-level filenames and content; copy a related set only when that RFI also has matching EML/MSG evidence |
| `rfi.log_add` | Extract/edit requests and apply guarded multi-sheet RFI Log rows |
| `rfi.follow_up_sync` | Add/update exact CIV multi-area rows and MEC rows with distinct exact route contexts; exclude CIV RFIs whose Tag No starts `PCS-`, including removing them from existing groups; split exact MEC composite location values; skip supported records absent from the current template; remove Cancelled/Rejected entries; synchronize eligible FINAL Worley state and uniquely mapped RFI Log statuses; never change worksheet Remarks; leave ambiguous matches in manual review |
| `files.batch_print` | Run and recover a strictly ordered mixed-file print queue; tolerate transient EPSON status/title/query behavior without duplicate submission, and optionally restore previous safe settings |
| `rfi.stage_signed_first_pages` | Identify nested single/combined Worley scans, search every RFI System subfolder with bounded multi-core discovery, and create review copies with exact page-one replacement |
| `rfi.normalize_pdf_orientation` | Inspect every PDF page below CIV/MEC/ELE/PRC in parallel and create fidelity-checked corrected review copies |
| `rfi.system_sync` | Scan exact canonical RFI PDFs and guard RFI Log plus Follow Up updates using only bottom-right OTHERS-cell Worley evidence |
| `rfi.complete_status_sync` | Recursively find exact completed RFI PDFs and preview guarded RFI Log Inspection Status updates to Completed |

## Application features

- Generic schema-driven forms, remembered safe values, typed presets, compact
  multi-row editing, all/selection actions, search/filter, and keyboard shortcuts.
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
- A failed Follow Up Apply can restore its displaced original through an
  explicit Undo action when the journal proves exact file identity, content,
  and security metadata; a collision or mismatch leaves both files untouched.
- Optional Supabase sign-in/approval/admin and metadata-only shared history.
- Copy-only redacted diagnostics.
- Turkish, English, Russian, and Azerbaijani UI with light/dark monochrome themes.
- Optional per-user Windows startup.
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
- Completion Status Sync uses bounded multi-core, read-only filename discovery
  at every depth. It shows the exact PDF evidence and workbook-cell changes,
  permits row/all selection, leaves conflicting status values for review, and
  applies through the same backup, OOXML verification, journal, and Undo path.
- Signed-page warnings identify the source PDF and affected page/RFI where known.
  Apply copies each affected input PDF once to `_review` with the relative input
  folder layout preserved; it never overwrites, mutates the signed input, or
  repeats a combined source PDF for each warned page.

## Intentional limits

- RFI System replica publication has internal implementation code but is not exposed as an active operation in the application.
- Process Scans discovers top-level PDFs only. In a multi-page batch, only a
  strict, canonical, high-confidence RFI identity starts a new group. Every
  page after a confirmed cover stays with that RFI until the next confirmed
  cover, even if continuation-page RFI-identity OCR is uncertain or unreadable.
  Pages before the first cover and duplicate RFI starts remain in `_review`.
  CIV/MEC/ELE/PRC outputs use matching discipline folders. Page orientation
  correction defaults on and uses the existing bounded detector; uncertain
  files go unchanged to `_review`.
  Fast Cover Scans is a separate opt-in operation. For multi-page PDFs it uses
  four-direction, low-resolution OCR on the upper page band and confirms only
  one strict RFI identity paired with `REQUEST FOR INSPECTION` and a known
  `SUNRISE PROJECT`, `INTEC`, or `LIGHTSOURCEBP` mark. Confirmed groups create
  missing per-RFI folders under the matching discipline. Single-page PDFs use the
  normal OCR path. An unresolved request-heading signal starts a Review group; no page
  is dropped or attached to the preceding RFI. Both Process Scans modes preserve
  safe XMP metadata, including streams with indirect `/Length` values, when
  separating pages. Page orientation correction defaults off in this operation
  to avoid a second full-page OCR pass; users can enable it when corrected output
  pages are needed. It shares destination layout, immutable Preview, journaled
  Apply, and Undo with Process Scans. Other cover templates remain unsupported
  by this fast detector; use normal Process Scans for them.
  Signed-page input recursion defaults
  on but can be deliberately disabled; RFI System archive lookup is always recursive.
- Native Office and printer behavior is accepted only on a real Windows host.
- PDF orientation classification requires bundled Tesseract OSD and English
  data. It uses multi-resolution OSD first, then a conservative four-direction
  OCR comparison only for unresolved pages. A PDF with any still-ambiguous or
  unreadable page is left unchanged for manual review.

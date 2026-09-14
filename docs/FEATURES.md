# Feature catalog

## Registered operations

| Operation | Operator outcome |
|---|---|
| `rfi.work_item_update` | Create or transition a resumable append-only RFI work item |
| `rfi.work_item_export` | Export owner-scoped work history to no-clobber JSON and Excel-safe CSV |
| `rfi.process_scans` | OCR, name, and file top-level RFI PDFs or route exceptions to review |
| `rfi.log_add` | Extract/edit requests and apply guarded multi-sheet RFI Log rows |
| `rfi.follow_up_sync` | Add/update mapped CIV/MEC Follow Up groups, remove Cancelled/Rejected entries, and synchronize FINAL Worley state |
| `rfi.outlook_reply_draft` | Display one verified unsent Reply All draft from the selected message |
| `files.batch_print` | Run and recover a strictly ordered mixed-file print queue; tolerate normal fast spool release and optionally restore previous safe settings |
| `rfi.stage_signed_first_pages` | Identify single/combined Worley pages and create review copies with exact page-one replacement |
| `rfi.normalize_pdf_orientation` | Inspect every PDF page below CIV/MEC/ELE/PRC in parallel and create fidelity-checked corrected review copies |
| `rfi.system_sync` | Scan the RFI system and guard RFI Log plus Follow Up Worley updates |

## Application features

- Today, Resume, and Needs Review work queues with exact journal evidence.
- Generic schema-driven forms, remembered safe values, typed presets, compact
  multi-row editing, all/selection actions, search/filter, and keyboard shortcuts.
- Batch Print always starts without old files or silently restored settings. One
  explicit action can restore the last previewed printer/paper/layout/queue
  settings, while named presets remain available for reusable procedures.
- Preview impact counts, exact effect paths, approved workbook cell comparisons,
  signed-page old/new thumbnails, and warning focus/filtering.
- OCR-backed preview effects, signed-page rows, orientation decisions, and
  manual-review items display their recognition score out of 100. The score is
  review evidence, not a probability or automatic approval.
- Responsive plan panes keep long warnings, compact tables, modal content, and
  Apply/Cancel actions reachable through bounded vertical or table scrolling.
- Persistent local history, explicit recovery center, guarded undo, and
  recoverable review trash.
- Optional Supabase sign-in/approval/admin and metadata-only shared history.
- Read-only device readiness and copy-only redacted diagnostics.
- Turkish, English, Russian, and Azerbaijani UI with light/dark monochrome themes.
- Bounded multi-core Rust analysis for Process Scans, combined signed-page OCR,
  and PDF orientation; state-changing effects remain ordered and journaled.
- Durable local incremental scanning for RFI System Sync and PDF orientation.
  Completed files survive interruption, unchanged files are reused, transient
  failures retry, scan counts are visible in Preview, and full rescan is an
  explicit task-local option.

## Intentional limits

- Process Scans reads top-level PDFs only; signed-page recursion is explicit.
- Outlook drafts are never auto-sent.
- Three-system replica transfer is roadmap work, not a registered operation.
- Native Office and printer behavior is accepted only on a real Windows host.
- PDF orientation classification requires bundled Tesseract OSD data. A PDF with
  any low-confidence or unreadable page is left unchanged for manual review.

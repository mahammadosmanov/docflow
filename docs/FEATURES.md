# Feature catalog

## Registered operations

| Operation | Operator outcome |
|---|---|
| `rfi.process_scans_fast_covers` | Fast RFI Scan: Multi-RFI separates confirmed cover groups; Single-RFI OCRs only page one and keeps the complete PDF. Preview supports canonical name correction and Rust-derived targets. The approved OCR alias is CLV→CIV; manual correction does not use aliases. Full-code candidate suffixes of one to four ASCII digits are padded to four. Optional page rotation is off by default and visible only in Multi-RFI. If direction-zero generic OCR words do not confirm a cover, the detector continues the remaining cardinal searches; uncertain/no-code pages use the unchanged-copy review route. That rotation path pins source SHA-256 before parallel analysis, rechecks afterward, and rejects changes before Plan creation. |
| `rfi.group_files` | Detect exact CIV/MEC/ELE/PRC RFIs in top-level filenames and content; after EML/MSG evidence eligibility, Preview and copy approved files to `<destination>/<discipline>/<full-code>/<source filename>` |
| `rfi.log_add` | Extract/edit requests and apply guarded multi-sheet RFI Log rows |
| `rfi.follow_up_sync` | Route CIV multi-tag and MEC `(Block, Section)` records to existing prepared rows, including every distinct eligible FINAL Worley destination; fan comma-separated CIV `Tag No` tags and MEC `Area` sections only to their distinct prepared rows. MEC matching ignores `Inspection Location`. No-match supported records stay outside template scope; partial contexts remain reviewable, independently confirmed rows may sync, and unmatched CIV tags are named in an unselectable diagnostic. Conflicts, ambiguous contexts, and unrelated placements cannot authorize shared-cell status changes. An already-present RFI with a cached formula status is a no-op only when the cached value matches the projected status; other unsafe targets remain manual. No rows or sections are created; remove Cancelled/Rejected entries; never change worksheet Remarks. |
| `files.batch_print` | Print ordered mixed-file jobs with recovery and existing printer settings; its optional read-only RFI finder selects exact First Page `.docx` and Sketch `.pdf` documents by canonical code, `CIV-1319`-style input, or one-to-four-digit number, showing missing types/versions and requiring explicit choice for cross-discipline bare numbers |
| `rfi.stage_signed_first_pages` | Identify nested single/combined Worley scans using high-confidence consensus-confirmed Strict or approved OcrAlias identities, search the full RFI System by exact RFI PDF basename with one-to-four-digit suffix normalization, and create no-clobber review copies with exact page-one replacement |
| `rfi.normalize_pdf_orientation` | Searchable standalone normalization uses rendered-content OSD and directional OCR, plus unique exact canonical header-ID evidence only when the same ID and direction agree at both 250/350 DPI. Source hashes bind analysis/cache and Apply; unresolved files remain unchanged for review-copy handling. |
| `rfi.system_sync` | Scan exact canonical RFI PDFs and guard RFI Log plus Follow Up updates using only bottom-right OTHERS-cell Worley evidence |
| `rfi.complete_status_sync` | Recursively find exact completed RFI PDFs and preview guarded RFI Log Inspection Status updates to Completed |
| `rfi.generate_concrete_documents` | Batch-generate CON-CIV-010/011/013/014 from RFI Log data; require Concrete Specimen Test Follow-Up only when Civil Forms includes CON-CIV-010, with configurable template values and no-overwrite safeguards |


## Application features

- Generic schema-driven forms, remembered safe values, typed presets, compact
  multi-row editing, all/selection actions, search/filter, and keyboard shortcuts.
- Fast RFI Scan is the only active scan-separation operation. Multi-RFI requires
  high-confidence consensus on a Strict or approved OCR-alias identity plus
  visible cover evidence; its independent cover rescue is limited to unresolved
  strong-vote candidates and does not override stable text-layer identities or
  conflicts. Optional rotation continues cardinal cover searches when
  direction-zero generic OCR words do not confirm a cover, and pins/rechecks
  source hashes around parallel analysis. Uncertain/no-code pages still go to
  review. Its preview exposes first-page Single-RFI mode, OCR alias evidence,
  and safe canonical code correction without allowing path or page edits.
- Batch Print always starts without old files or silently restored settings. One
  explicit action can restore the last previewed printer/paper/layout/queue
  settings, while named presets remain available for reusable procedures.
- Batch Print's RFI finder accepts canonical codes, `CIV-1319` discipline-number
  inputs, and one-to-four-digit numbers, which are padded to four digits. It
  searches the selected RFI System read-only, identifies only exact full-code
  First Page `.docx` and Sketch `.pdf` names, shows missing types and multiple
  versions, and requires an explicit choice where a bare number spans
  disciplines. Only enabled selections flow into the existing Batch Print
  preview and queue.
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
  trail, in-flow section navigator, approved rows first, higher-resolution
  before/after page comparison with source filenames and zoom, and a structured
  per-PDF warning/Review-copy summary.
- OCR-backed preview effects, signed-page rows, orientation decisions, and
  manual-review items display their recognition score out of 100. The score is
  review evidence, not a probability or automatic approval.
- Persistent navigation keeps Home, Review, History, Recovery, Preferences,
  and conditional Admin destinations available across authenticated screens.
  Fast RFI Scan is the primary home task; the remaining registered operations
  stay searchable and category-filtered. The sidebar becomes a compact icon
  rail or bottom bar at narrower widths.
- The shared responsive workspace allocates vertical space from the live header
  and keeps one stage-content scroll owner, with primary actions reachable on
  short or narrow windows. Forms and long paths wrap for reading; wide tables
  retain readable minimum widths and independent horizontal scrolling.
- Data-region heights and table-column widths can be adjusted with pointer or
  keyboard controls and restored with Fit/Reset. This is transient presentation
  state; it does not change app preferences, workflow data, or native authority.
- Persistent local history, explicit recovery center, guarded undo, and
  recoverable review trash.
- A failed Follow Up Apply can restore its displaced original through an
  explicit Undo action when the journal proves exact file identity, content,
  and security metadata; a collision or mismatch leaves both files untouched.
- Optional Supabase sign-in/approval/admin and metadata-only shared history.
- Copy-only redacted diagnostics.
- Turkish, English, Russian, and Azerbaijani UI with light/dark monochrome themes.
- Optional per-user Windows startup.
- Bounded multi-core Rust analysis for Fast RFI Scan, combined signed-page OCR,
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
- The standalone PDF orientation operation is searchable with explicit
  localized page-rotation wording. Fast RFI Scan uses the same canonical
  rotation decision; its named correction switch defaults off and appears only
  in Multi-RFI mode. Single-RFI keeps the full PDF without rotation.
- Completion Status Sync uses bounded multi-core, read-only filename discovery
  at every depth. It shows the exact PDF evidence and workbook-cell changes,
  permits row/all selection, leaves conflicting status values for review, and
  applies through the same backup, OOXML verification, journal, and Undo path.
- Signed-page warnings identify the source PDF and affected page/RFI where known.
  Apply copies each affected input PDF once to `_review` with the relative input
  folder layout preserved; it never overwrites, mutates the signed input, or
  repeats a combined source PDF for each warned page.
- Concrete document generation batches selected CON-CIV-010/011/013/014 RFIs from the Civil RFI Log and supplied templates. CON-CIV-010 maps concrete-pour fields from Concrete Specimen Test Follow-Up, calculates ambient temperature from plant temperature, and accepts optional water/cement ratios; CON-CIV-011 uses a configurable ambient-temperature range; 013/014 update only report number and location. Missing Follow-Up rows for 010 are warned by RFI and skipped without blocking available documents. Existing outputs are never overwritten.

## RFI print integration smoke

Use only the private fixture copy, never the original Ventoy volume:

```sh
cd /tmp/docflow-rfi-workflows
npm run tauri dev
```

In Batch Print, select `/tmp/docflow-ventoy-scenarios.NsjJVp/group-files/source`
as the RFI System folder, enter `CIV-1316`, select First Page and Sketch, and
search. Confirm the exact-code `.docx` and case/separator-variant Sketch `.pdf`
appear. Deselect Sketch and preview; confirm the existing Batch Print editor
contains only the `.docx`. To exercise bare-number ambiguity, add an exact-code
MEC-1316 `.docx` filename to a nested folder in the private fixture copy, search
`1316`, and confirm candidates from both disciplines start unchecked. For missing
types, request a code with only one of the selected document types. Use the
existing Apply/queue/cancel flow only with an approved test printer; otherwise
cancel at Preview. To exercise duplicate versions, copy an eligible fixture
file to another subfolder in a private fixture copy and verify both versions
appear unselected until deliberately enabled. The Windows native Office and
printer handoff requires a real Windows host with the supported Office/printer
setup; macOS discovery and UI smoke do not certify `.docx` printing.

## Intentional limits

- RFI System replica publication has internal implementation code but is not exposed as an active operation in the application.
- Fast RFI Scan inspects top-level PDFs. Multi-RFI confirms group boundaries
  only when one strict RFI identity, `REQUEST FOR INSPECTION`, and a known
  `SUNRISE PROJECT`, `INTEC`, or `LIGHTSOURCEBP` mark agree. Uncertain or
  unreadable inputs go to `_review`; files whose PDF structure cannot be safely
  separated are not split. Single-RFI reads page one only and keeps the complete
  source PDF intact, so it does not separate a combined multi-RFI scan. Only the
  exact OCR alias `CLV` becomes `CIV`; filenames, searchable PDF text, and
  manually entered identifiers do not use that alias. Preview name edits accept
  canonical CIV/MEC/ELE/PRC codes and rederive target paths from trusted settings.
  Page rotation correction defaults off to avoid another OCR pass. Other cover
  templates remain unsupported by this detector.
  Signed-page input recursion defaults on but can be disabled deliberately; RFI
  System archive lookup is always recursive.
- Native Office and printer behavior is accepted only on a real Windows host.
- PDF orientation classification requires bundled Tesseract OSD and English
  data. It uses multi-resolution OSD and conservative four-direction OCR. A
  unique exact canonical RFI ID in the top page header is an additional narrow
  fallback only when the same ID and direction appear at both 250 and 350 DPI;
  competing, noncanonical, conflicting, or still-unreadable evidence remains
  unchanged for manual review. Source hashes are pinned before analysis,
  rechecked afterward and on cache reuse, and cache profile v5 invalidates
  older orientation decisions.

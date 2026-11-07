# Changelog

All notable DocFlow changes are recorded here. The project follows Semantic
Versioning; `releases.json` is the machine-readable release catalog.

## Unreleased

## 1.46.0 - 2026-11-07

### Template documents

- New operation Create documents from a template: pick a template, the RFI Log, RFI numbers, and an output folder; values come from each RFI's row in its discipline sheet.
- Preview shows one editable row per document; the Preset column fills a stage group's fields at once, and suggestions offer each field's choices.
- Apply writes new files only, never overwriting, and Undo removes them; a template update between Preview and Apply stops the run.
- Templates can carry defaults inside the file, such as {{item|Underground LV cable}} or {{area|({area}) {tagNo}}}.

## 1.45.0 - 2026-11-07

### Template editor

- Admins and chosen template editors create templates from Word or Excel files that mark values with {{placeholders}}; Rust opens the file and uploads it.
- Each field gets a label, a starting value that can use RFI Log tokens such as {rfiNo} or ({area}) {tagNo}, suggestion choices, and a multi-line option.
- Presets fill several fields at once, for example one ITP stage group with its stage, activity, item, and drawing.
- Edits land in a draft; publishing shares the new version with every DocFlow user, and published versions never change.
- The Admin screen can mark users as template editors.

## 1.44.0 - 2026-11-07

### Added

- New **Templates** screen lists the document templates shared with every DocFlow user, with search and discipline filters (ADR 0055).
- Rust syncs published templates at startup, every 30 minutes, and on demand. Each file must match its SHA-256 and open as a valid Office file before it is cached. Templates stay usable offline, and a failed download keeps the previous version.
- New `docs/supabase_templates.sql` creates the shared store: versioned, immutable published templates, an editor role (`can_edit_templates`), row-level security, and a private Storage bucket. An admin must run it once in Supabase.

## 1.43.0 - 2026-11-06

### Fixed

- Dragging a folder or file onto an operation field now works in the desktop app. Before, the webview could send only the file name, so drops failed.
- Rust now records dropped paths from the native window event and gives them to the field the drop landed on, once and only within 10 seconds. The webview no longer sends paths (`droppedPaths` is removed from `paths/select`), so it cannot select a folder the operator did not choose (ADR 0054).
- Native Windows acceptance of drops is still pending (ACCEPTANCE row 23).

## 1.42.5 - 2026-11-06

### Changed

- Home operation cards now show at most two lines of description, and the Fast RFI Scan card shows three. The full text still appears on each operation's screen and is read in full by assistive technology. No translations changed.

## 1.42.4 - 2026-11-06

### Changed

- Split `frontend/src/styles.css` (4,399 lines) into 12 files under `frontend/src/styles/`, imported in the original cascade order. Design tokens are in `tokens.css`. The built CSS is byte-identical to before.

## 1.42.3 - 2026-11-06

### Fixed

- Updated `zip` to 9.4.1. On Windows, 9.1.2 panicked when it checked a non-ASCII member name for reserved device names; the new parser robustness test caught this.
- Updated the yanked `async-executor` 1.14.0 to 1.14.1 so `cargo deny` passes.
- Moved the cross-platform Rust CI job from the retired `macos-14` runner to `macos-15`.

## 1.42.2 - 2026-11-06

### Changed

- Add `docs/ACCEPTANCE.md`, one status row per native Windows/Office/Outlook/printer/cloud gate with version, date, and evidence, and link it from the documentation map and known issues.

## 1.42.1 - 2026-11-06

### Changed

- Move changelog entries for 0.x and 1.0–1.29 verbatim into `docs/changelog/`; `version_manager notes` still finds archived versions.

## 1.42.0 - 2026-11-06

### Added

- Home shows up to three of the signed-in operator's recently journaled operations as one-click shortcuts.
- React tests run `axe-core` accessibility checks on sign-in, Home, and an operation form.

## 1.41.6 - 2026-11-06

### Changed

- Fast RFI Scan now defaults to Keep originals (copy instead of move); operators who prefer moving sources to recoverable trash can still turn it off, and saved values are unchanged.

## 1.41.5 - 2026-11-06

### Fixed

- Add deterministic mutation tests that feed 2,000 corrupted XLSX packages, PDFs, and email texts each to the untrusted-document parsers and fail on any panic.

## 1.41.4 - 2026-11-06

### Fixed

- Record every Rust panic (location, thread, redacted message) as one activity-log line so support exports show crashes in background work.
- Replace the remaining bare `unwrap()` calls in non-test Rust with explained `expect()` invariants and deny new ones with `clippy::unwrap_used`.

## 1.41.3 - 2026-11-06

### Changed

- Run Rust clippy and tests on Windows and macOS for every pull request so platform-only code is checked before release tags.
- Cache Rust build output in CI with `Swatinem/rust-cache`.
- Add `cargo deny` (advisories, bans, sources) and `npm audit --omit=dev` gates.
- Fail a pull request that does not change `VERSION`.
- Check Rust formatting in the pre-commit hook and run Dependabot weekly.

## 1.41.2 - 2026-11-06

### Security

- Scope the updater signing secret to the signed build step and withhold it from dependency installation and tests.
- Install npm dependencies with `--ignore-scripts` in the Windows release build.
- Pin every GitHub Action to a full commit SHA and disable persisted checkout credentials in the release job.
- Always install the SHA-256-verified official Tesseract installer instead of the unverified Chocolatey package.

## 1.41.1 - 2026-11-06

### Fixed

- Format and lint the drawing-label orientation code so the Windows release gate (`cargo fmt --check`, `clippy -D warnings`) passes.

## 1.41.0 - 2026-11-06

### Added

- Follow Up sync now routes ELE clause `1.6` (ITP `ELE-0005` / item `ELE-5.1.6`) to the matching prepared ELE Block row by exact Tag No, including non-`PCS-` blocks such as `A4` and `A12/1`.

## 1.40.0 - 2026-11-04

### Changed

- Orientation detection falls back to drawing labels (such as `S9`, `A17`, `PCS-19`) when OSD and running-text OCR refuse a sparse drawing page; it applies a turn only when one turn has at least three labels and at least twice the next best.

## 1.39.0 - 2026-11-01

### Added

- Add a manual "Check for updates" row in Settings that shows the current version or an available update.

## 1.38.3 - 2026-11-01

### Fixed

- Show the update banner only when an update is available, installing, or failed to install; hide it when up to date or after a background check fails.
- Add a Dismiss control to the update banner.

## 1.38.2 - 2026-11-01

### Fixed

- Publish the signed NSIS installer itself as the Windows updater payload,
  matching Tauri's `.exe.sig` output.

## 1.38.1 - 2026-11-01

### Fixed

- Generate updater bundler configuration with the trusted public key and release
  endpoint so signed Windows packages can be built.
- Trim the repository variable's trailing newline before Rust configures update signature verification.

## 1.38.0 - 2026-11-01

### Added

- Add signed in-app update checks and installation for Windows x64, backed by immutable GitHub Release assets.


## 1.37.6 - 2026-11-01

### Fixed

- Recognize trailing whitespace before a closing clause parenthesis in Follow Up headers so ELE-6.2.5.3 entries match the prepared Earthing group.

## 1.37.5 - 2026-11-01

### Changed

- Fill supported ELE RFI/status pairs in matching prepared blank slots while keeping the approved ITP/item and row-creation limits.

## 1.37.4 - 2026-10-31

### Fixed

- Keep the encoded worksheet target test valid on Windows filesystems.

## 1.37.3 - 2026-10-31

### Fixed

- Restrict a PDF reader import to supported targets and tests to keep Windows Clippy clean.

## 1.37.2 - 2026-10-31

### Fixed

- Re-export the controlled Excel PDF artifact to Windows adapters so the Windows target compiles.

## 1.37.1 - 2026-10-31

### Fixed

- Keep the shared frontend test helper in source control so clean Windows builds can resolve test imports.

## 1.37.0 - 2026-10-31

### Changed

- Sync statuses for existing ELE Follow Up RFIs in the two approved ITP/item groups; leave `1.6`, unlisted items, and missing RFI identifiers untouched.

## 1.36.280 - 2026-10-28

### Changed

- Moved RFI print finder tests into a `native` tests `print_finder` module.

## 1.36.279 - 2026-10-28

### Changed

- Moved native settings and saved operation parameter tests into a `native` tests `settings` module.

## 1.36.278 - 2026-10-28

### Changed

- Moved Excel export supervisor test fixtures into a `fixtures` submodule.

## 1.36.277 - 2026-10-28

### Changed

- Moved zip container preflight worksheet discovery tests next to the archive preflight tests in `ooxml`.

## 1.36.276 - 2026-10-28

### Changed

- Moved interprocess lock tests into a `journal` tests submodule.

## 1.36.275 - 2026-10-28

### Changed

- Moved PDF publish retry and no-clobber tests into an `fs_ops` tests submodule.

## 1.36.274 - 2026-10-28

### Changed

- Moved native sign-out and re-sign-in tests into a `native` tests auth module.

## 1.36.273 - 2026-10-28

### Changed

- Moved batch print row field helpers into a `batch_print` fields module.

## 1.36.272 - 2026-10-28

### Changed

- Moved batch print file classification helpers into a `batch_print` files module.

## 1.36.271 - 2026-10-28

### Changed

- Moved atomic file replace and no-clobber helpers into an `fs_ops` support atomic module.

## 1.36.270 - 2026-10-28

### Changed

- Moved support bundle validation into a `diagnostics` validate module.

## 1.36.269 - 2026-10-28

### Changed

- Moved native preview step builders into a `preview_steps` module.

## 1.36.268 - 2026-10-28

### Changed

- Moved scan OCR planning and inspection helpers into a `process_scans` inspect module.

## 1.36.267 - 2026-10-28

### Changed

- Moved scan cover preview planning into a `process_scans` preview module.

## 1.36.266 - 2026-10-28

### Changed

- Moved scan group output planning into the `process_scans` output module.

## 1.36.265 - 2026-10-28

### Changed

- Moved scan output plan helpers from `process_scans` into its output module.

## 1.36.264 - 2026-10-28

### Changed

- Moved the signed status text helper to the shared follow-up codes and reused it in routing.

## 1.36.263 - 2026-10-28

### Changed

- Reused the shared signed status value set in follow-up apply validation.

## 1.36.262 - 2026-10-28

### Changed

- Reused the shared signed status value set in RFI system sync checks.

## 1.36.261 - 2026-10-28

### Changed

- Shared the signed status value set across OOXML guarded patch checks.

## 1.36.260 - 2026-10-28

### Changed

- Reused the signed status helper when refreshing signed slot proposals.

## 1.36.259 - 2026-10-28

### Changed

- Extracted applied-file verification from the S5 undo path into a helper.

## 1.36.258 - 2026-10-28

### Changed

- Extracted workbook signature parsing into a helper.

## 1.36.257 - 2026-10-28

### Changed

- Shared capability authorization and anchoring across filesystem effects through one helper.

## 1.36.256 - 2026-10-28

### Changed

- Extracted the unsafe signed slot manual-review row into a helper.

## 1.36.255 - 2026-10-28

### Changed

- Shared blocking native task dispatch in the backend through one helper.

## 1.36.254 - 2026-10-28

### Changed

- Extracted replica target directory planning into a helper.

## 1.36.253 - 2026-10-28

### Changed

- Shared PDF needs-review step construction in process scans.

## 1.36.252 - 2026-10-28

### Changed

- Shared private S5 artifact authorization in the follow-up workbook apply path.

## 1.36.251 - 2026-10-28

### Changed

- Extracted additional concrete batch row building into a helper.

## 1.36.250 - 2026-10-28

### Changed

- Shared write-path authorization in filesystem effects through one helper.

## 1.36.249 - 2026-10-28

### Changed

- Extracted the RFI file copy blocker check into a named helper.

## 1.36.248 - 2026-10-28

### Changed

- Extracted the print job failure warning into a helper.

## 1.36.247 - 2026-10-28

### Changed

- Extracted RFI system folder discovery into a helper.

## 1.36.246 - 2026-10-28

### Changed

- Extracted pending failed-apply publication validation into a helper.

## 1.36.245 - 2026-10-28

### Changed

- Extracted S5 follow-up undo path validation into a helper.

## 1.36.244 - 2026-10-28

### Changed

- Counted placement unmapped rows only after a target layout is found, removing the increment-and-undo pattern.

## 1.36.243 - 2026-10-28

### Changed

- Counted signed unmapped rows only after a target layout is found, removing the increment-and-undo pattern.

## 1.36.242 - 2026-10-28

### Changed

- Shared the job pause marking used by print queue failure arms.

## 1.36.241 - 2026-10-28

### Changed

- Shared the backend-error PDF hash helper across process scans.

## 1.36.240 - 2026-10-28

### Changed

- Extracted granted input reading from concrete document preview.

## 1.36.239 - 2026-10-28

### Changed

- Merged duplicate existing-destination checks in RFI file group preview.

## 1.36.238 - 2026-10-28

### Changed

- Extracted discipline search directory discovery from the system root scan.

## 1.36.237 - 2026-10-28

### Changed

- Shared cancelled journal state selection in print queue apply.

## 1.36.236 - 2026-10-28

### Changed

- Shared scan group candidate insertion between multi-RFI and single-RFI PDF scans.

## 1.36.235 - 2026-10-28

### Changed

- Extracted unplaced signed-record manual-review queueing into a shared helper in RFI Follow Up placement.

## 1.36.234 - 2026-10-28

### Changed

- Shared S5 canonical-file matching and committed result construction in RFI Follow Up undo.

## 1.36.233 - 2026-10-28

### Changed

- Extracted workbook backup rollback after failed replacement into a helper.

## 1.36.232 - 2026-10-28

### Changed

- Extracted backend preview operation routing into a helper.

## 1.36.231 - 2026-10-28

### Changed

- Extracted RFI file-group missing-email grouping into a helper.

## 1.36.230 - 2026-10-28

### Changed

- Extracted publish status row disposition mapping and editor assembly into helpers.

## 1.36.229 - 2026-10-28

### Changed

- Extracted replica publish source index and schedule limit validation into helpers.

## 1.36.228 - 2026-10-28

### Changed

- Extracted ambient range validation and input grant root collection from concrete document preview.

## 1.36.227 - 2026-10-28

### Changed

- Extracted print queue apply result metadata assembly into a helper.

## 1.36.226 - 2026-10-28

### Changed

- Shared the modified-time helper and built each multi-RFI group candidate once in PDF scan planning.

## 1.36.225 - 2026-10-28

### Changed

- Shared the signed status text helper across RFI Follow Up signed placement.

## 1.36.224 - 2026-10-28

### Changed

- Extracted per-candidate Worley signature analysis from system root scan into a helper.

## 1.36.223 - 2026-10-28

### Changed

- Extracted per-RFI review row mapping from system sync preview into a helper.

## 1.36.222 - 2026-10-28

### Added

- Unit tests for the Worley signed remark marker helper.

## 1.36.221 - 2026-10-28

### Added

- Unit tests for RFI discipline selection edge cases.

## 1.36.220 - 2026-10-28

### Changed

- Extracted approved-row validation from RFI file-group revision into a helper.

## 1.36.219 - 2026-10-28

### Added

- Unit tests for RFI Follow Up status signature update decisions.

## 1.36.218 - 2026-10-28

### Added

- Unit tests for RFI preview path canonicalization error context and existing-path resolution.

## 1.36.217 - 2026-10-28

### Added

- Unit tests for review root canonicalization.

## 1.36.216 - 2026-10-28

### Added

- Unit tests for RFI file-group offered step authorization.

## 1.36.215 - 2026-10-28

### Added

- Unit tests for the S5 retained-storage budget check and detected discipline count adjustment.

## 1.36.214 - 2026-10-28

### Added

- Unit tests for system-sync disposition and print queue final status selection.

## 1.36.213 - 2026-10-28

### Changed

- Moved RFI file-group offered step authorization into a helper.

## 1.36.212 - 2026-10-28

### Fixed

- Kept RFI preview error keys as literals so the ARB catalog check passes.

## 1.36.211 - 2026-10-28

### Changed

- Shared canonicalize-or-validation error mapping across RFI native preview paths.

## 1.36.210 - 2026-10-28

### Changed

- Shared review root canonicalization between review preview and recoverable delete plan.

## 1.36.209 - 2026-10-28

### Changed

- Moved detected-discipline sheet count adjustment from RFI log preview into a helper.

## 1.36.208 - 2026-10-28

### Changed

- Moved RFI file-group editor table metadata into a helper.

## 1.36.207 - 2026-10-28

### Changed

- Shared the S5 retained-storage budget check between follow-up apply and undo.

## 1.36.206 - 2026-10-28

### Changed

- Moved parallel scan OCR task planning into a helper.

## 1.36.205 - 2026-10-28

### Changed

- Moved review-page outputs from process scan preview into a helper.

## 1.36.204 - 2026-10-28

### Changed

- Moved split-source trash move steps from process scan preview into a helper.

## 1.36.203 - 2026-10-28

### Changed

- Moved unreadable-PDF note outputs from process scan preview into a helper.

## 1.36.202 - 2026-10-28

### Changed

- Moved system-sync RFI Log remark update evaluation into a pure helper.

## 1.36.201 - 2026-10-28

### Changed

- Moved system-sync Follow Up status update evaluation into a pure helper.

## 1.36.200 - 2026-10-28

### Changed

- Moved follow-up undo applied-state expectation parsing into a pure helper.

## 1.36.199 - 2026-10-28

### Changed

- Moved replica publish editor table metadata into a pure helper.

## 1.36.198 - 2026-10-28

### Changed

- Moved print queue final status selection into a pure helper.

## 1.36.197 - 2026-10-28

### Changed

- Moved completion-status disposition selection into a pure helper.

## 1.36.196 - 2026-10-28

### Changed

- Moved system-sync row disposition selection into a pure helper.

## 1.36.195 - 2026-10-28

### Changed

- Moved the parallel orientation detection pass out of the process scan preview.

## 1.36.194 - 2026-10-28

### Changed

- Moved the PDF inspection phase out of the parallel process scan preview.

## 1.36.193 - 2026-10-28

### Changed

- Reused the sync workbook reader in completion-status preview and moved its update step building into a helper.

## 1.36.192 - 2026-10-28

### Changed

- Moved follow-up undo publication outcome handling out of undo_follow_up.

## 1.36.191 - 2026-10-28

### Changed

- Moved unmatched RFI file review planning out of the file-group preview loop.

## 1.36.190 - 2026-10-28

### Changed

- Moved replica target extra-file warning reporting out of the replica publish preview.

## 1.36.189 - 2026-10-28

### Changed

- Moved per-attempt print cleanup review recording out of the print queue apply loop.

## 1.36.188 - 2026-10-28

### Changed

- Split PDF discovery and per-code output grouping out of the process scans cover-mode preview.

## 1.36.187 - 2026-10-28

### Changed

- Split system-sync workbook reads and workbook update step building out of the indexed preview function.

## 1.36.186 - 2026-10-28

### Changed

- Split concrete document template selection and output step writing out of the preview function.

## 1.36.185 - 2026-10-28

### Changed

- Moved uncached orientation PDF inspection classification out of `preview_indexed` into `operations/pdf_orientation/uncached.rs`; behavior unchanged.

## 1.36.184 - 2026-10-28

### Changed

- Moved orientation preview plan metadata construction out of `preview_indexed` into `orientation_meta` in `operations/pdf_orientation.rs`; behavior unchanged.

## 1.36.183 - 2026-10-28

### Changed

- Extracted multi-RFI page grouping into `group_pages` and the duplicated no-OCR plan into `no_ocr_plan` in `process_scans/scan_pdf.rs`; behavior unchanged.

## 1.36.182 - 2026-10-28

### Changed

- Extracted the repeated plan-bound workbook path authorization in Follow Up apply into `authorize_workbook` in `fs_ops/follow_up.rs`; behavior unchanged.

## 1.36.181 - 2026-10-28

### Changed

- Split the S5 completion transition match into apply/undo, failed-restore, recovery, and failure-preparation groups in `journal/s5_transition.rs`; behavior unchanged.

## 1.36.180 - 2026-10-28

### Changed

- Split the S5 staging transition match into artifact-parent, retained-artifact, and publication groups in `journal/s5_transition.rs`; behavior unchanged.

## 1.36.179 - 2026-10-28

### Changed

- Moved the RFI email review warning aggregation into `rfi_log/preview_steps.rs` as `email_review_warning`; behavior unchanged.

## 1.36.178 - 2026-10-28

### Changed

- Replaced the signed-page comparison preview byte and row counters with a `PreviewBudget` helper in `stage_signed_pages/revise.rs`; behavior unchanged.

## 1.36.177 - 2026-10-28

### Changed

- Split the S5 transition state machine into staging and completion transition groups in `journal/s5_transition.rs`; behavior unchanged.

## 1.36.176 - 2026-10-28

### Changed

- Moved signed-page review editor metadata construction out of the `stage_signed_pages` preview into `stage_signed_pages/support.rs`; behavior unchanged.

## 1.36.175 - 2026-10-28

### Changed

- Moved the signed-page archive ambiguity warning and review recording out of the `stage_signed_pages` preview into `stage_signed_pages/warnings.rs`; behavior unchanged.

## 1.36.174 - 2026-10-28

### Changed

- Moved the S5 transition state machine out of `journal/s5.rs` into `journal/s5_transition.rs`, with lease facts passed as an explicit struct; behavior unchanged.

## 1.36.173 - 2026-10-28

### Changed

- Moved per-sheet RFI Log add batch construction out of the preview loop into `rfi_log/preview_steps.rs`; behavior unchanged.

## 1.36.172 - 2026-10-28

### Changed

- Moved signed-candidate page inspection and fidelity/widget rejection out of the `stage_signed_pages` preview into `operations/stage_signed_pages/signed_candidate.rs`; behavior unchanged.

## 1.36.171 - 2026-10-28

### Changed

- Moved parallel signed-page inspection and page OCR prefetch out of the `stage_signed_pages` preview into `operations/stage_signed_pages/parallel.rs`; behavior unchanged.

## 1.36.170 - 2026-10-28

### Changed

- Moved OOXML sheet guard token comparison and unique direct-child lookup helpers out of `ooxml/sheet_patch.rs` into `ooxml/sheet_patch/tokens.rs` and `ooxml/sheet_patch/lookup.rs`; behavior unchanged.

## 1.36.169 - 2026-10-28

### Changed

- Moved RFI Log add preview email extraction, workbook-update step construction, and editor context out of `rfi_log.rs` into `rfi_log/preview_steps.rs`; behavior unchanged.

## 1.36.168 - 2026-10-28

### Changed

- Moved the parallel page OCR cache adapter (`CachedPageOcr`) out of `operations/stage_signed_pages.rs` into `operations/stage_signed_pages/ocr_cache.rs`; behavior unchanged.

## 1.36.167 - 2026-10-28

### Changed

- Moved journal record model types and S5 permit/lease types out of `journal.rs` into `journal/model.rs` and `journal/permits.rs`; behavior unchanged.

## 1.36.166 - 2026-10-28

### Changed

- Moved RFI Log revise and clipboard/table payload builders out of `rfi_log.rs` into `rfi_log/revise.rs` and `rfi_log/payloads.rs`; behavior unchanged.

## 1.36.165 - 2026-10-28

### Changed

- Moved the complete-status sync preview out of `rfi_system_sync/preview.rs` into `preview/complete_status.rs`; behavior unchanged.

## 1.36.164 - 2026-10-28

### Changed

- Split `ParsedPdf` object access, inspection, and graph hashing out of `pdf/parsed.rs` into `parsed/objects.rs`, `parsed/inspection.rs`, and `parsed/hashing.rs`; behavior unchanged.

## 1.36.163 - 2026-10-28

### Changed

- Split OOXML XML parsing/serialization and cell-reference helpers out of `ooxml/xml.rs` into `xml/parse.rs` and `xml/cell_reference.rs`; behavior unchanged.

## 1.36.162 - 2026-10-28

### Changed

- Split OOXML relationship validation and target path normalization out of `ooxml/relationships.rs` into `relationships/validation.rs` and `relationships/paths.rs`; behavior unchanged.

## 1.36.161 - 2026-10-28

### Changed

- Moved Follow Up editor metadata and projection-budget helpers out of `rfi_follow_up/scan.rs` into `scan/editor.rs`; behavior unchanged.

## 1.36.160 - 2026-10-28

### Changed

- Extracted the duplicate source-row merge from `scan_source_records` into `merge_source_record`; behavior unchanged.

## 1.36.159 - 2026-10-28

### Changed

- Split `mapping_details` into named helpers in `rfi_follow_up/source/mapping_details.rs`; detail text unchanged.

## 1.36.158 - 2026-10-28

### Changed

- Split orientation checkpoint finish and plan-file steps out of `pdf_orientation/support.rs` into `support/checkpoint.rs` and `support/plan_file.rs`; behavior unchanged.

## 1.36.157 - 2026-10-28

### Changed

- Moved the plan table choice-label lookup into `editorChoiceLabel` in `plan/editorRules.ts` with unit tests; behavior unchanged.

## 1.36.156 - 2026-10-28

### Changed

- Moved the Outlook reply draft unit tests out of `outlook_reply_draft.rs` into `outlook_reply_draft/tests.rs`; behavior unchanged.

## 1.36.155 - 2026-10-28

### Changed

- Split RFI code parsing and clause routing out of `rfi_follow_up/source.rs` into `source/rfi_codes.rs` and `source/clause_routing.rs`; behavior unchanged.

## 1.36.154 - 2026-10-28

### Changed

- Moved S5 journal accessors and prepare/cancel transitions out of `journal/s5.rs` into `journal/s5_accessors.rs`; behavior unchanged.

## 1.36.153 - 2026-10-28

### Changed

- Moved the per-PDF approved-plan loop out of `preview_indexed` into `plan_file` with a `PlanTotals` counter struct; behavior unchanged.

## 1.36.152 - 2026-10-28

### Changed

- Moved the per-file checkpoint finish step out of `preview_indexed` into a helper with a typed input struct; behavior unchanged.

## 1.36.151 - 2026-10-28

### Changed

- Extracted the per-PDF prepare step (fingerprint, source hash, checkpoint reuse or inspection) from `preview_indexed` in `src-tauri/src/operations/pdf_orientation.rs` into `prepare_pdf` in `operations/pdf_orientation/support.rs`. Behavior is unchanged; the source is still hashed before and after inspection.

## 1.36.150 - 2026-10-28

### Changed

- Extracted the `PdfPageRotate` step construction from `preview_indexed` in `src-tauri/src/operations/pdf_orientation.rs` into `rotation_step` in `operations/pdf_orientation/support.rs`. The step's fields and metadata are unchanged.

## 1.36.149 - 2026-10-28

### Changed

- Extracted the cached-page rebuild from `preview_indexed` in `src-tauri/src/operations/pdf_orientation.rs` into `cached_page_results` in `operations/pdf_orientation/support.rs`. Behavior is unchanged; the structural-validity checks remain in the caller.

## 1.36.148 - 2026-10-28

### Changed

- Extracted the per-file page approval loop from `preview_indexed` in `src-tauri/src/operations/pdf_orientation.rs` into the pure helper `approve_file_pages` in `operations/pdf_orientation/support.rs`. No I/O moved; behavior is unchanged. Gated by the full Rust suite.

## 1.36.147 - 2026-10-28

### Changed

- Moved the PDF orientation helpers (`empty_plan`, `canonical_directory`, `overlaps`, `normalize_rotation`, `warning`, `truncate`) from `src-tauri/src/operations/pdf_orientation.rs` into `operations/pdf_orientation/support.rs`. `pdf_orientation.rs` drops from 806 to 750 lines. Behavior is unchanged; 12 orientation tests pass.

## 1.36.146 - 2026-10-28

### Changed

- Moved `BackendRouter::select_paths` and `reveal_path` from `src-tauri/src/backend.rs` into `backend/paths.rs`. `backend.rs` drops from 560 to 462 lines. Behavior is unchanged; 16 backend tests pass.

## 1.36.145 - 2026-10-28

### Changed

- Moved `RecordIndex` and `place_record` from `src-tauri/src/rfi_follow_up/classify.rs` into `classify/placement.rs`. `classify.rs` drops from 690 to 406 lines. Behavior is unchanged; 33 `rfi_follow_up` tests pass.

## 1.36.144 - 2026-10-28

### Changed

- Extracted the `rfi.normalize_pdf_orientation` and `rfi.log_add` browser-demo preview branches from `previewOperation` into `previewPdfOrientation` and `previewLogAdd` in `frontend/src/lib/mockPlanHandlers.ts`. Browser-only demo data; no runtime behavior change; 294 frontend tests pass.

## 1.36.143 - 2026-10-28

### Changed

- Extracted four browser-demo preview branches (`rfi.complete_status_sync`, `rfi.group_files`, `files.batch_print`, `demoFastCoverOperation`) from `previewOperation` into named helpers in `frontend/src/lib/mockPlanHandlers.ts`. `previewOperation` drops from about 480 to about 180 lines. Browser-only demo data; no runtime behavior change; 294 frontend tests pass.

## 1.36.142 - 2026-10-28

### Changed

- Moved the browser-demo `reviseOperation` from `frontend/src/lib/mockPlanHandlers.ts` into `frontend/src/lib/mockReviseHandler.ts`. `mockPlanHandlers.ts` drops from 653 to about 500 lines; `mockBridge.ts` imports the revise handler from the new module. Browser-only demo data; no runtime behavior change.

## 1.36.141 - 2026-10-28

### Changed

- Moved the demo print operation definition from `frontend/src/lib/demoOperations.ts` into `frontend/src/lib/demoPrintOperation.ts`; `mockFixtures.ts` imports it from the new module. `demoOperations.ts` drops from 608 to 418 lines. Browser-only demo data; no runtime behavior change.

## 1.36.140 - 2026-10-28

### Changed

- Moved the platform unit test module from `src-tauri/src/platform/mod.rs` into `platform/tests.rs`. `mod.rs` drops from 621 to 164 lines. Test bodies and `include_str!` paths are unchanged; all 111 `platform::` tests pass on macOS.

## 1.36.139 - 2026-10-28

### Changed

- Moved the Windows print submission classification helpers (`WindowsSubmittedJobStatus` through `validate_windows_image_frame_count`) from `src-tauri/src/platform/mod.rs` into `platform/windows_submission.rs`. `mod.rs` drops from 778 to 621 lines. The helpers are cfg-gated as before (`windows` or `test`); Windows-only code was not compiled on this Mac, so Windows build is unverified here. Behavior is unchanged.

## 1.36.138 - 2026-10-28

### Changed

- Moved `apply_follow_up` from `src-tauri/src/fs_ops/apply.rs` into `fs_ops/apply/follow_up_apply.rs`. `apply.rs` drops from 559 to 416 lines. Behavior is unchanged.

## 1.36.137 - 2026-10-28

### Changed

- Moved `apply_rfi_system_sync` from `src-tauri/src/fs_ops/apply.rs` into `fs_ops/apply/rfi_system_apply.rs`. `apply.rs` drops from 805 to 559 lines. Behavior is unchanged.

## 1.36.136 - 2026-10-28

### Changed

- Extracted plan editor column-width state, clamping, and resize setter from `frontend/src/plan/EditablePlanTable.tsx` into `frontend/src/plan/useEditorColumnWidths.ts`. Behavior is unchanged.

## 1.36.135 - 2026-10-28

### Changed

- Moved OCR direction scoring (`DirectionScore` and its selection and summary helpers) from `src-tauri/src/ocr/orientation.rs` into `ocr/orientation/scoring.rs`. `orientation.rs` drops from 767 to 575 lines. Behavior is unchanged.

## 1.36.134 - 2026-10-28

### Changed

- Moved PDF object serialization from `src-tauri/src/pdf/parser.rs` into `pdf/serialize.rs`, and stream decoding (`decode_stream`, `apply_predictor`) into `pdf/codec.rs`. `parser.rs` drops from 770 to 572 lines. Behavior is unchanged.

## 1.36.133 - 2026-10-28

### Changed

- Moved the `BackendMethod` roster and its authorization table from `src-tauri/src/backend.rs` into `backend/method.rs`. `backend.rs` drops from 770 to 560 lines. Behavior is unchanged.

## 1.36.132 - 2026-10-28

### Changed

- Moved the support-bundle sanitizer (`sanitize_value` and its key, path and error-code helpers) from `src-tauri/src/diagnostics.rs` into `diagnostics/sanitize.rs`. `diagnostics.rs` drops from 795 to 660 lines. Behavior is unchanged.

## 1.36.131 - 2026-10-28

### Changed

- Moved the RFI Log column layout tables (`Layout`, `RfiStatusColumns`, `layout`, `status_columns`) from `src-tauri/src/rfi_log.rs` into `rfi_log/column_layout.rs`. `rfi_log.rs` drops from 806 to 707 lines. Behavior is unchanged.

## 1.36.130 - 2026-10-28

### Changed

- Moved the signed-by placement functions (`place_signed_record`, `refresh_signed_slots`, `signed_slot_rows`) from `src-tauri/src/rfi_follow_up/classify.rs` into `classify/signed.rs`. `classify.rs` drops from 1,052 to 690 lines. Behavior is unchanged.

## 1.36.129 - 2026-10-28

### Fixed

- `docs/ROADMAP.md` described the selected-message Unicode MSG archive as implemented. The operation has no operator route (`rfi.outlook_archive_selected` appears only in the Rust scaffolding and tests). The roadmap item now adds that route before classic-Outlook certification.

## 1.36.128 - 2026-10-28

### Fixed

- `docs/KNOWN_ISSUES.md` said the compiler reports the Outlook archive scaffolding (`prepare_archive`, `bind_archive`) as unused. Both carry `#[allow(dead_code)]`, so no warning is emitted. The entry now says that.

## 1.36.127 - 2026-10-28

### Changed

- `OperationScreen` applies preset, previous-value and field edits through one `mergeValues` helper instead of three copies of the revision bump and merge. The batch-print RFI finder parameter list is a named `RFI_FINDER_PARAMS` constant. Behavior is unchanged.

## 1.36.126 - 2026-10-28

### Changed

- Moved the `Notice`, `EmptyState` and `Centered` primitives from `frontend/src/components/ui.tsx` into `frontend/src/components/feedback.tsx`. Importing files now name the module that owns each primitive. Rendering and behavior are unchanged.

## 1.36.125 - 2026-10-28

### Changed

- Removed unused imports from `frontend/src/lib/bridge.ts`, `helpers.tsx` and two App test files, plus the unused `sharedBridge` variable in `lib/mockBridge.ts`. `tsc --noUnusedLocals` now reports nothing. No runtime behavior changed.

## 1.36.124 - 2026-10-28

### Fixed

- `docs/TRACEABILITY.md` cited `App.test.tsx` in six requirement rows (DF-012, DF-017, DF-018, DF-019, DF-021, DF-031). That file no longer exists after the App suite split, so the matrix now names `App.*.test.tsx` suites. No code or behavior changed.

## 1.36.123 - 2026-10-28

### Fixed

- The browser demo mock implements `settings/presets`, `settings/savePreset` and `settings/deletePreset`, kept per operation in memory. Batch print no longer shows `Mock method not implemented` on open. A unit test covers save, replace, list and delete. Desktop builds call the Rust backend and are unchanged.

## 1.36.122 - 2026-10-28

### Fixed

- The sign-in password visibility toggle sets an explicit `aria-label`, so screen readers announce it without relying on the `title` tooltip fallback. Visual behavior is unchanged.

## 1.36.121 - 2026-10-28

### Changed

- Remove a duplicated comment line above the paused replica-automation helper in `native.rs` and the replica plan cache helper in `native/plan.rs`. No code change.

## 1.36.120 - 2026-10-28

### Fixed

- `docs/KNOWN_ISSUES.md` no longer lists the frontend bundle size advisory as open. Review, Queue, History, Recovery and Admin already load on demand, and the build emits no advisory. The entry now records the measured chunk sizes.

## 1.36.119 - 2026-10-28

### Changed

- The saved-parameters resume card moves out of `OperationScreen.tsx` into `screens/PreviousValuesCard.tsx`, matching `PrintQueueResumeCard`. Markup and behavior are unchanged.

## 1.36.118 - 2026-10-28

### Changed

- The schema parameter visibility rule moves into `isParamVisible` in `lib/operationValues.ts`. `OperationScreen` and `SchemaField` call it instead of repeating the `visibleWhen` check. Behavior is unchanged.

### Added

- Unit tests for `isParamVisible` cover unconditioned parameters and multi-condition gating.

## 1.36.117 - 2026-10-28

### Changed

- `components/ui.tsx` moves the `ActionBar` layout component into `components/ActionBar.tsx`. `PlanView`, `ReviewScreen`, `OperationScreen` and `BusyStage` import it from the new file. Markup and behavior are unchanged.

## 1.36.116 - 2026-10-28

### Changed

- `plan/PlanSummaries.tsx` moves `SignedStageNavigator` and `SignedWarningReviewSummary` into `plan/PlanSignedSummaries.tsx`, and drops the imports only they used. `PlanView.tsx` imports them from the new file. Markup and behavior are unchanged.

## 1.36.115 - 2026-10-28

### Added

- `screens/ReviewScreen.test.tsx` covers a failed undo: the error shows and the result stays for retry. It fails without the 1.36.114 fix.

## 1.36.114 - 2026-10-28

### Fixed

- `screens/ReviewScreen.tsx` undo had no error handling. A rejected undo left an unhandled promise rejection and no message. Undo errors now show through the screen's error notice, and the screen keeps its result so the user can retry.

## 1.36.113 - 2026-10-28

### Changed

- `components/ui.tsx` moves the `BusyState` progress panel into `components/BusyState.tsx`; its test is renamed `components/BusyState.test.tsx`. `BusyStage`, `HistoryScreen`, `QueueScreen`, `RecoveryScreen` and `ReviewScreen` import it from the new file. `ui.tsx` also drops three imports that only `BusyState` used. Markup and behavior are unchanged.

## 1.36.112 - 2026-10-28

### Changed

- `screens/OperationScreen.tsx` moves the interrupted print-queue resume card into `screens/PrintQueueResumeCard.tsx` and drops four unused imports. Markup and behavior are unchanged; `tsc` is clean.

## 1.36.111 - 2026-10-28

### Changed

- `plan/PlanSummaries.tsx` moves the `PlanWarnings` list into `plan/PlanWarnings.tsx` and drops the imports only it used. `PlanView.tsx` and `App.shell-localization.test.tsx` import it from the new file. Markup and behavior are unchanged.

## 1.36.110 - 2026-10-28

### Changed

- `components/ui.tsx` moves the `Modal` dialog into `components/Modal.tsx`; its test is renamed `components/Modal.test.tsx`. `plan/SignedPageComparison.tsx` and `screens/HistoryScreen.tsx` import it from the new file. Markup and behavior are unchanged; `tsc` is clean.

## 1.36.109 - 2026-10-28

### Fixed

- `screens/ReviewScreen.tsx` looked up the per-file action by `item.path`, but actions are keyed by `selectionId`, so the "will replace an existing file" warning never showed. It now uses `selectionId`. The three batch buttons (keep, put, delete) share one `setAllActions` helper.

### Added

- `screens/ReviewScreen.test.tsx` covers the replace warning.

## 1.36.108 - 2026-10-28

### Fixed

- `screens/SchemaField.tsx` called `useState` after conditional early returns (the multi-file and path/browse branches). The multi-file field and the path/browse field move into `screens/SchemaFieldKinds.tsx` as their own components, so each hook runs unconditionally. Markup and behavior are unchanged; `tsc` is clean.

## 1.36.107 - 2026-10-28

### Changed

- `screens/SettingsScreen.tsx` moves the `SettingsGroup` and `ChoiceGrid` layout helpers into `screens/SettingsParts.tsx` and drops the imports only they used. Markup and behavior are unchanged; `tsc` is clean.

## 1.36.106 - 2026-10-28

### Changed

- `plan/PlanSummaries.tsx` moves the `WorkbookChangePreview` table into `plan/WorkbookChangePreview.tsx` and drops the imports only it used. `PlanView.tsx` imports it from the new file. Markup and behavior are unchanged; `tsc` is clean.

## 1.36.105 - 2026-10-28

### Changed

- `plan/PlanView.tsx` moves the `OutlookReplyPreview` panel into `plan/OutlookReplyPreview.tsx`. Markup and plan rendering are unchanged; `tsc` is clean and the frontend suite passes.

## 1.36.104 - 2026-10-28

### Changed

- `plan/EditablePlanTable.tsx` (562 lines) moves the per-cell editor rendering (read-only, checkbox, choice, compact date, multiline and text inputs with datalist and scan-code error) into `plan/PlanCell.tsx`. The table keeps row, column and selection state and passes each cell its value, choices, suggestions and change handler. Markup and editing behavior are unchanged; `tsc` is clean and the frontend suite passes (289 tests).

## 1.36.103 - 2026-10-28

### Changed

- `App.tsx` (486 lines) moves the `AppFrame` navigation shell and the `Splash` placeholder into `AppFrame.tsx`, and drops imports that only the moved code used. Routing, markup and behavior are unchanged; `tsc` is clean and the frontend suite passes.

## 1.36.102 - 2026-10-28

### Changed

- `screens/OperationScreen.tsx` (544 lines) moves the batch-print RFI document finder section into `screens/RfiPrintFinderPanel.tsx`, which receives the `useRfiPrintFinder` result as one `finder` prop. The reveal-destination lookup is now the named helper `destinationRevealAction` in the same file. Markup, labels and behavior are unchanged; `tsc` is clean and the frontend suite passes (289 tests).

## 1.36.101 - 2026-10-28

### Changed

- `classify_workbooks_full` in `rfi_follow_up/classify.rs` hands two pieces to named helpers: `source_issue_row` builds the manual-review row for a slot whose source record is missing or has an unknown status, and `projection_warnings` builds the unmapped, mass-deletion and no-rows warnings. Logic is unchanged; clippy is clean with `-D warnings` and the 542 lib tests pass.

## 1.36.100 - 2026-10-27

### Changed

- `operations/rfi_file_group.rs` (831 lines) keeps `preview`, `revise` and the tests and moves two groups into child files: `rfi_file_group/analysis.rs` (file discovery, PDF, package, XML and raw-text RFI code detection, and the `Detection` and `FileAnalysis` types) and `rfi_file_group/steps.rs` (destination directory, copy step and path-overlap checks). Logic is unchanged; clippy is clean with `-D warnings` and the 542 lib tests pass.

## 1.36.99 - 2026-10-27

### Changed

- `cap_fs.rs` (820 lines) keeps the capability root, authorization and missing-directory code and moves two groups into child files: `cap_fs/security.rs` (file and parent security snapshots, creation security descriptors, secured created files and the Windows creation-security helpers) and `cap_fs/retained_storage.rs` (the S5 retained-artifact storage scan and reservation types). Logic is unchanged; the source-scan test reads the new files; clippy is clean with `-D warnings` and the 542 lib tests pass.

## 1.36.98 - 2026-10-27

### Changed

- `undo.rs` (823 lines) keeps `undo_journal` and the undo-safety check and moves the rest into three files under `undo/`: `steps.rs` (per-step dispatch, restore-backup and undo planning), `quarantine.rs` (quarantined file recovery, hash verification and recovery phases), and `roots.rs` (capability root opening, root identity checks and path validation). Logic is unchanged; clippy is clean with `-D warnings` and the 542 lib tests pass.

## 1.36.97 - 2026-10-27

### Changed

- `paths.rs` (854 lines) keeps its types and moves the `PathGrantStore` methods into three files under `paths/`: `issuing.rs` (grant creation), `authorization.rs` (authorize, plan and root lookups, reopen), and `revocation.rs` (revoke and capability checks). Methods are unchanged; `revoke_grant_ids` is `pub(super)` so the sibling modules can call it. Clippy is clean with `-D warnings` and the 542 lib tests pass.

## 1.36.96 - 2026-10-27

### Changed

- The 2,845-line `ooxml/tests.rs` is split into seven concern-based files under `ooxml/tests/`: package and XML limits, sheet projection, namespace and relationship validation, archive preflight, relationships and connections, recalc and rich patches, and row insert and formulas. Shared helpers and fixtures stay in `tests.rs`. All 92 tests are kept unchanged; the lib suite still passes 542 tests and clippy is clean with `-D warnings`.

## 1.36.95 - 2026-10-27

### Changed

- The 4,180-line `platform/excel/tests.rs` export-supervisor test module is split by concern into eight files under `platform/excel/tests/export_supervisor/`: constructors and guards, export flow, deadline and worker, cleanup journal, print service, operation fence, cleanup review, and timeouts. Shared fixtures stay in `mod.rs`. All tests are kept unchanged; the lib suite still passes 542 tests and clippy is clean with `-D warnings`.

## 1.36.94 - 2026-10-27

### Changed

- The 2,683-line `frontend/src/App.test.tsx` is split into seven files by area: `App.plan`, and `App.shell-presets`, `App.shell-localization`, `App.shell-workbooks`, `App.shell-flows`, `App.shell-editing`, `App.shell-home`. Shared sign-in, bridge interception and cleanup helpers live in `frontend/src/test/appTestSupport.ts`. All 55 tests moved unchanged; the full suite passes (289 tests) and `tsc` is clean.

## 1.36.93 - 2026-10-27

### Changed

- The signed-by slot passes in `classify_workbooks_full` (`rfi_follow_up/classify.rs`) moved out: the change-marking pass into `refresh_signed_slots`, and the row-emitting pass into `signed_slot_rows`. Same loops, same order; the emitted rows are appended to `rows` exactly as before. Clippy is clean with `-D warnings` and the 33 rfi_follow_up tests pass.

## 1.36.92 - 2026-10-27

### Changed

- The routed-signed-target loop in `classify_workbooks_full` (`rfi_follow_up/classify.rs`) now lives in `place_signed_record`, next to `place_record`. Both take the shared `RecordIndex` and `&mut` state. The loop body is unchanged: body-level `continue` became `return Ok(())`, and the candidate-slot `continue`s stay in their inner loop. Clippy is clean with `-D warnings` and the 33 rfi_follow_up tests pass.

## 1.36.91 - 2026-10-27

### Changed

- The per-record placement loop in `classify_workbooks_full` (`rfi_follow_up/classify.rs`) now lives in `place_record`. It takes a `RecordIndex` of shared inputs plus `&mut` slots, manual rows and the unmapped counter. The loop body is unchanged; `continue` became `return Ok(())`. The routed-signed loop still runs inline. Clippy is clean with `-D warnings` and the 33 rfi_follow_up tests pass.

## 1.36.90 - 2026-10-27

### Changed

- The signed-unmapped manual-review row in `rfi_follow_up/classify.rs` now comes from `manual_review_row` too. Its signed status labels and `signed-unmapped` row id are set on the helper's result. Same fields and output. Clippy is clean with `-D warnings` and the 33 rfi_follow_up tests pass.

## 1.36.89 - 2026-10-27

### Changed

- The slot-pruning pass at the start of `classify_workbooks_full` (drop RFIs removed by log state or deletion evidence, then flag slots whose lifecycle status changed) now lives in `prune_slot_codes` in `rfi_follow_up/classify.rs`. Same logic and order. Clippy is clean with `-D warnings`, the 33 rfi_follow_up tests and the full 542-test lib suite pass.

## 1.36.88 - 2026-10-27

### Changed

- The second manual-review row in `rfi_follow_up/classify.rs` (target sheet found but no mapping) now calls `manual_review_row` too. Its target keeps the default route column pair, which is set after the helper returns. Same fields, same reason keys, same output. The 33 rfi_follow_up tests pass and clippy is clean with `-D warnings`.

## 1.36.87 - 2026-10-27

### Changed

- The unknown-route manual-review row in `rfi_follow_up/classify.rs` now calls the existing `manual_review_row` helper instead of building a `FollowUpRow` literal inline. Same fields, same reason keys, same output. The second inline literal uses a different target column, so it stays as is. The 33 rfi_follow_up tests pass and clippy is clean with `-D warnings`.

## 1.36.86 - 2026-10-27

### Fixed

- The multiline cell `<textarea>` in `frontend/src/plan/EditablePlanTable.tsx` now has `aria-label` set to its column label. Screen readers previously announced it without a name; the single-line inputs in the same table already had one.

## 1.36.85 - 2026-10-27

### Fixed

- The shared `Button` in `frontend/src/components/ui.tsx` now renders `type="button"` by default. Previously a `Button` inside a `<form>` defaulted to submit. The login form already sets `type="submit"` explicitly, so its behavior is unchanged; callers can still override the type.

## 1.36.84 - 2026-10-27

### Changed

- Moved the slot index construction in `classify_workbooks_full` into `index_slots` in `rfi_follow_up/classify.rs`. Named aliases (`SlotKey`, `SlotKeysBySheetCol`, `SlotKeysByCode`) replace the repeated tuple types. The record loop itself is unchanged and is still the next candidate. The 33 rfi_follow_up tests pass and clippy is clean with `-D warnings`.

## 1.36.83 - 2026-10-27

### Changed

- Split two sections out of `classify_workbooks_full` in `rfi_follow_up/classify.rs`: `build_slots` (approved-layout row scan into the `SlotMap` alias) and `deletion_evidence` (deleted-RFI count and the mass-deletion guard). The mass-deletion comment moved with its logic. The orchestrating function shrinks by about 40 lines; the file grows slightly from the helper signatures. The 33 rfi_follow_up tests pass and clippy is clean with `-D warnings`.

## 1.36.82 - 2026-10-27

### Changed

- Moved the `rfi.group_files` operation definition out of `operation_registry` in `registry.rs` into `registry/file_group.rs`. `registry.rs` is now 170 lines of assembly and shared helpers. The 8 registry tests pass and clippy is clean with `-D warnings`.

## 1.36.81 - 2026-10-27

### Changed

- Moved the `stage_signed_pages`, `normalize_pdf_orientation`, and `fast_cover_scans` operation definitions out of `operation_registry` in `registry.rs` (347 lines before) into `registry/signed_scans.rs`. Each is now a builder function. The 8 registry tests pass and clippy is clean with `-D warnings`.

## 1.36.80 - 2026-10-27

### Changed

- Moved the `rfi_follow_up`, `rfi_system_sync`, `rfi_complete_status_sync`, and `rfi_publish_status` operation definitions out of `operation_registry` in `registry.rs` into `registry/rfi_sync.rs`. Each is now a plain builder function. The 8 registry tests pass and clippy is clean with `-D warnings`.

## 1.36.79 - 2026-10-27

### Changed

- Moved the rfi-log operation definition out of `operation_registry` in `registry.rs` into `registry/rfi_log.rs`. The default-workbook parameter and provider visibility are unchanged. The 8 registry tests pass and clippy is clean with `-D warnings`.

## 1.36.78 - 2026-10-27

### Changed

- Moved the batch-print operation definition out of `operation_registry` in `registry.rs` (726 lines) into `registry/print_batch.rs`. The printer default and parameter schema are unchanged. The 8 registry tests pass and clippy is clean with `-D warnings`.

## 1.36.77 - 2026-10-27

### Changed

- Moved `ExcelExportPipeline` and its impl out of `platform/excel.rs` (821 lines) into `platform/excel/pipeline.rs`. Methods used by the sibling test module are widened to `pub(in crate::platform::excel)`. The 66 Excel tests and 6 print-queue tests pass and clippy is clean with `-D warnings`.

## 1.36.76 - 2026-10-27

### Changed

- Moved the Excel worker runtime (`ExcelWorkerRuntime`, `ExcelWorkerHandle`, `WorkerPoll`, `WorkerJoin`, and the thread-backed `ThreadWorkerRuntime` and `ThreadWorkerHandle`) out of `platform/excel.rs` (905 lines) into `platform/excel/worker.rs`. Visibility and imports only. The 66 Excel tests pass and clippy is clean with `-D warnings`.

## 1.36.75 - 2026-10-27

### Changed

- Moved the `rfi.generate_concrete_documents` operation definition out of `operation_registry` in `registry.rs` (911 lines) into `registry/concrete_documents.rs`. Same schema and parameters; the registry now builds it through a helper. The 8 registry tests pass and clippy is clean with `-D warnings`.

## 1.36.74 - 2026-10-27

### Changed

- Moved `PrintService` and its impl (queue begin, apply, verify, and cancellation wiring) out of `print_queue.rs` (883 lines) into `print_queue/service.rs`. Fields and methods that sibling print-queue modules call are widened to `pub(in crate::print_queue)`. The 6 print-queue tests pass and clippy is clean with `-D warnings`.

## 1.36.73 - 2026-10-27

### Changed

- Moved the undo path (`undo`, `recover_pending`, `undo_follow_up`, and the failed-apply restore helpers) out of `fs_ops/follow_up.rs` (966 lines) into `fs_ops/follow_up/undo.rs`. Visibility widened to `pub(in crate::fs_ops)` only where the parent calls them. The 19 fs_ops tests pass and clippy is clean with `-D warnings`.

## 1.36.72 - 2026-10-27

### Changed

- Moved the coverage helpers (`SignedDestinationCoverage`, `signed_destination_coverage`, `civ_tag_coverage`, `distinct_civ_area_rows`, `distinct_civ_zone_area_rows`, `mec_context_coverage`) out of `rfi_follow_up/routing.rs` into `rfi_follow_up/routing/coverage.rs`. Visibility and imports only; the parent re-exports them to `classify.rs`. Routing tests pass and clippy is clean with `-D warnings`.

## 1.36.71 - 2026-10-27

### Changed

- Moved the XML cell-patching helpers (`patch_civ010_numeric_cells`, `expand_civ010_batch_rows`, row shifting and cloning, `set_numeric_cell`, `set_text_cell`) out of `concrete_documents.rs` (850 lines) into `concrete_documents/cells.rs`. Visibility and imports only. The 14 concrete-document tests pass and clippy is clean with `-D warnings`.

## 1.36.70 - 2026-10-27

### Changed

- Moved the validation and journal helpers (`archive_filename`-adjacent journal checks, `validate_selected_mail_item`, `validate_undo_record`, `durable_identity_from_step`, `operation_result`, and related) out of `outlook_reply_draft.rs` (852 lines) into `outlook_reply_draft/helpers.rs`. `journal_is_safely_undoable` stays reachable from `undo.rs` through a re-export. Visibility and imports only. Outlook tests pass and clippy is clean with `-D warnings`.

## 1.36.69 - 2026-10-27

### Changed

- Moved the snapshot and hashing helpers (`snapshot_tree`, `insert_file`, `ensure_source_settled`, `enforce_automatic_change_limit`, `hash_snapshot`, `valid_sha256`, `collect_missing_directories`) out of `operations/replica_publish.rs` (893 lines) into `operations/replica_publish/snapshot.rs`. Visibility and imports only. Replica tests pass and clippy is clean with `-D warnings`.

## 1.36.68 - 2026-10-27

### Changed

- Moved the XML attribute helpers (`attribute`, the bounded attribute readers, `set_attribute`, `set_unqualified_attribute`, `remove_attribute`) out of `ooxml/xml.rs` (930 lines) into `ooxml/xml/attributes.rs`. Visibility is widened to `ooxml`-wide so existing callers resolve unchanged. Visibility and imports only. The 105 ooxml tests pass and clippy is clean with `-D warnings`.

## 1.36.67 - 2026-10-27

### Changed

- Moved the free path-guard helpers (`reject_lexical_escape`, `resolve_existing_ancestor`, `revalidate_under_root`, `inspect_ancestor`, the link and reparse checks, `expired`) out of `paths.rs` (974 lines) into `paths/filesystem.rs`. `expired` stays crate-visible through a re-export. Visibility and imports only. Path tests pass and clippy is clean with `-D warnings`.

## 1.36.66 - 2026-10-27

### Changed

- Moved `capture_unix_xattrs` and `replay_unix_xattrs` out of `cap_fs.rs` (962 lines) into `cap_fs/unix_xattrs.rs`. The module and its import are gated with `cfg(unix)`, matching the functions. Code is otherwise unchanged. cap_fs tests pass and clippy is clean with `-D warnings` on macOS. The Windows build is not verified here.

## 1.36.65 - 2026-10-27

### Changed

- Moved the row-building helpers (`plan_from_rows`, `jobs_from_rows`, `job_from_row`, `NormalizedSettings`, `rows_from_workbook`, `row_from_file`, and related) out of `operations/batch_print.rs` (961 lines) into `operations/batch_print/rows.rs`. Visibility and imports only. Batch-print and print tests pass; clippy is clean with `-D warnings`.

## 1.36.64 - 2026-10-27

### Changed

- Moved the directory-scan helpers (`record_unsupported`, `collect_discipline_pdfs`, `scan_orientation_directory`, `add_directory_steps`) out of `operations/pdf_orientation.rs` (973 lines) into `operations/pdf_orientation/directory.rs`. Visibility and imports only. The 12 orientation tests pass and clippy is clean with `-D warnings`.

## 1.36.63 - 2026-10-27

### Changed

- Moved `ScanContext`, `ScanState`, and `scan_one_pdf` out of `operations/process_scans.rs` (1,112 lines) into `operations/process_scans/scan_pdf.rs`. Visibility and imports only. Process-scans tests pass and clippy is clean with `-D warnings`.

## 1.36.62 - 2026-10-27

### Changed

- Moved `PrintJob`, its legacy-normalization type, and the custom deserializer out of `print_queue.rs` (1,115 lines) into `print_queue/job.rs`. Visibility and imports only. Print queue tests pass and clippy is clean with `-D warnings`.

## 1.36.61 - 2026-10-27

### Changed

- Moved `StagedWorkbook`, `ExportedWorksheetPdf`, and their PDF-validation helpers out of `platform/excel.rs` (1,234 lines) into `platform/excel/staged.rs`. The export pipeline stays in `excel.rs`. Code is unchanged apart from visibility (`pub(super)`) and imports. The 66 excel tests pass.

## 1.36.60 - 2026-10-27

### Changed

- Renamed `backend.rs` helper `chrono_free_timestamp` to `unix_seconds_stamp`. The old name suggested a chrono-based clock; it returns `unix:<seconds>`. Behavior unchanged; backend tests pass.

## 1.36.59 - 2026-10-27

### Changed

- Two plain `<button>` elements in `screens/HomeScreen.tsx` (operation cards) and `screens/ReviewScreen.tsx` (file rows) now set `type="button"`. They are not inside a form today, so nothing changes now; the attribute stops them from submitting a form if one wraps them later. Screen tests pass (23 files, 86 tests).

## 1.36.58 - 2026-10-27

### Changed

- On touch screens (`pointer: coarse`), the mobile navigation items (`.app-nav-item`) and the switch controls (`.switch`) get an invisible pseudo-element that enlarges their tap area to about 44 px. The visible size is unchanged, so the bottom bar keeps its layout. Desktop pointers are not affected. Checked in the browser preview at 375 px width: the nav tap area measures 127 x 58 px and the page has no horizontal scroll. Not checked on a physical touch device.

## 1.36.57 - 2026-10-27

### Fixed

- Eight streaming copy and hash buffers in the library moved from stack arrays (`[0_u8; N]`) to heap `vec!` buffers. The largest was 1 MiB (`fs_ops.rs`), which can overflow a 1 MB Windows main-thread stack. Behavior is unchanged; the reads are the same size. Clippy's `large_stack_arrays` lint now passes.

## 1.36.56 - 2026-10-27

### Changed

- Eight library call sites no longer clone a value that is not used afterward: the replica binding lookup (`fs_ops/apply.rs`), the retained-locator and recovery-locator records (`fs_ops/follow_up.rs`, `leaf_transaction.rs`), three RFI number lists in the preview (`native/preview.rs`), a grant root (`native/preview_rfi.rs`), an output folder (`operations/process_scans/output.rs`), and a JSON source value (`rfi_log/email.rs`). The compiler confirms each value is moved, not reused. No behavior change.

## 1.36.55 - 2026-10-27

### Tests

- New `plan/EditablePlanTable.test.tsx` covers the group-select button added in 1.36.54. It checks the accessible name, the visible row count, and that clicking it selects the group's rows. No app behavior change.

## 1.36.54 - 2026-10-27

### Fixed

- The group-select button in the editable plan table (`plan/EditablePlanTable.tsx`) shows only the group's row count. Its accessible name was that number, so a screen reader announced a bare digit. It now has `aria-label` "Select this group"; the visible count is unchanged. No automated test covers this button yet.

## 1.36.53 - 2026-10-27

### Fixed

- The duplicate and remove-duplicate row buttons in the RFI editor (`plan/PlanRowActions.tsx`) now carry an explicit `aria-label`, like the move buttons beside them. Their names no longer depend on the `title` fallback. The tests query them by role and name.

## 1.36.52 - 2026-10-27

### Changed

- The RFI log's static regex patterns (`rfi_log/support.rs`) now use `expect` with a message instead of a bare `unwrap`, matching the rest of the crate. A failed pattern now names the cause in the panic. No behavior change.

## 1.36.51 - 2026-10-27

### Fixed

- The Windows directory listing (`cap_fs/windows_dir.rs`) now reads `FILE_ID_BOTH_DIR_INFO` records from a `Vec<u64>` buffer instead of a `Vec<u8>`, so the buffer start is aligned for the record type. Bounds checks use the byte length. This closes the alignment item recorded in `docs/KNOWN_ISSUES.md`. The file compiles only on Windows; rustfmt checks its syntax here, but no Windows build or directory listing was run, so the Windows check is still outstanding.

## 1.36.50 - 2026-10-27

### Changed

- Every remaining unsafe block in the Windows modules now has a SAFETY note: directory handles, COM and the Excel and Outlook automation layers, the Windows PDF runtime, the startup registry entry, and the file-replace call. These files compile only on Windows, so rustfmt checks their syntax; no Windows build was run.
- One real concern is recorded in `docs/KNOWN_ISSUES.md`: the Windows directory listing (`cap_fs/windows_dir.rs`) reads records from a byte buffer and assumes an alignment the type does not guarantee. The fix and the Windows check it needs are written there. No behavior change in this release.

## 1.36.49 - 2026-10-27

### Changed

- Every unsafe call in the Windows printing module (`platform/windows/printing.rs`) now has a SAFETY note: printer enumeration and handles, device contexts, GDI+ image loading and frames, job observation, and shell and association calls. The module compiles only on Windows, so these are comments checked for syntax by rustfmt, not by a Windows build. No behavior change.

## 1.36.48 - 2026-10-27

### Changed

- Test coverage: the preset helpers (which settings a preset remembers, and the plain-language message for a failed preset write) are now exported and unit-tested. No app behavior change.

## 1.36.47 - 2026-10-27

### Changed

- Test coverage: the batch-print RFI number chooser now has unit tests for its default checked state, per-number toggling, and select-all and select-none actions. No app behavior change.

## 1.36.46 - 2026-10-27

### Changed

- Test coverage: the batch-print RFI results panel now has unit tests for its select-by-request toggle and for the ambiguous-request announcement. No app behavior change.

## 1.36.45 - 2026-10-27

### Changed

- Test coverage: the compact date picker (it clamps the day when the month changes and reports only complete dates) and the busy-stage cancel control now have unit tests. No app behavior change.

## 1.36.44 - 2026-10-27

### Changed

- The remaining unsafe calls in the clipboard, PDF OCR bridge, PDF publish, journal lock, and file-rename code now have SAFETY notes stating why each one is sound. The Windows-only files could not be compiled on this Mac; the notes are comments, and formatting checks parse them. No behavior change.

## 1.36.43 - 2026-10-27

### Changed

- Every unsafe call in the file-security layer (`cap_fs.rs`: owner and mode changes, Windows attribute calls, and extended-attribute capture and replay) now has a SAFETY note. No behavior change.

## 1.36.42 - 2026-10-27

### Changed

- Every unsafe call in the Unix directory-handle layer (`cap_fs/unix_dir.rs`) now has a SAFETY note saying why it is sound: live descriptors, NUL-terminated names, and fresh descriptors that are owned exactly once. The codebase has about 205 unsafe sites, and only about 11 had such notes before. No behavior change.

## 1.36.41 - 2026-10-27

### Fixed

- Screen-reader names for toggles: the three Settings switches (compact layout, start with Windows, unattended RFI System sync) now have explicit names. The Admin access and admin switches include the user's name, so identical toggles on different rows can be told apart. Visible text is unchanged.

## 1.36.40 - 2026-10-27

### Fixed

- Heading structure for screen readers: the home screen had an empty first heading, and most other screens had two level-1 headings (the top bar title plus the page title). The top bar title is now a paragraph by default, so every screen has exactly one level-1 heading. Settings and the workflow screen keep their top bar title as the heading, since nothing else names them. Appearance is unchanged.

## 1.36.39 - 2026-10-27

### Fixed

- Progress during a preview or apply is now readable by screen readers. The progress bar is a labelled progress bar with its current value and maximum when the total is known. The phase text (preparing, scanning, and so on) is announced as a status. Before, the bar was a plain box and phase changes were silent. Visible layout is unchanged.

## 1.36.38 - 2026-10-27

### Fixed

- Keyboard and screen-reader focus after Apply: when the result appears, focus moves to it, the same as after a preview (1.36.37). Before, focus fell back to the top of the page.

## 1.36.37 - 2026-10-27

### Fixed

- Keyboard and screen-reader focus after a preview: when a preview succeeds, focus now moves to the preview itself. Before, the Önizle button was replaced by the preview and focus fell back to the top of the page, so keyboard and screen-reader users lost their place.

## 1.36.36 - 2026-10-27

### Changed

- Turkish copy consistency: error, warning and help sentences that used the informal singular ("seç", "gir", "aç", "kontrol et") now use the formal plural ("seçin", "girin", "açın", "kontrol edin"), matching the rest of the app. Button labels keep their short form, as in "Önizle" and "Gözat". The admin hint no longer switches to informal wording. Existing tests were updated to the new strings.

## 1.36.35 - 2026-10-27

### Changed

- Review screen copy: the Turkish help sentence switches from the informal "Sen" and singular imperative to the formal wording used on the rest of the app. The batch-action bar above the rows gets its own label ("Tüm dosyalar için") instead of repeating the help sentence. The new key is translated in all four languages.

## 1.36.34 - 2026-10-27

### Changed

- When a preview fails because a required folder or value is missing, the matching field is now marked invalid (`aria-invalid` and a stronger border). Before, only the message above the form said so, with no link to the field. The message is unchanged, and the field clears as soon as it is edited.

## 1.36.33 - 2026-10-27

### Fixed

- Segmented option groups (Settings theme and language, and the action choice on each Review row) now mark the selected option as pressed for screen readers. Before, the selection was shown only by color. Each Review row's group is also named after its file, so repeated "Bırak" / "Sil" choices can be told apart.

## 1.36.32 - 2026-10-27

### Fixed

- Every folder or file Browse button now carries its field name in its accessible name (for example, "Gözat: Gelen dosyalar klasörü"). Before, screen readers announced several identical "Gözat" buttons with no way to tell them apart. Visible text is unchanged.

## 1.36.31 - 2026-10-27

### Changed

- The path-grant unit tests move out of `paths.rs` into `paths/provisional_tests.rs` and `paths/lexical_guard_tests.rs`. The security code file shrinks by about 130 lines. The tests are unchanged in content.

## 1.36.30 - 2026-10-27

### Changed

- The backend router's test suite moves from `backend.rs` into `backend/tests.rs`. The router file shrinks by about 250 lines. The tests still run as before.

## 1.36.29 - 2026-10-27

### Changed

- The follow-up classifier's entry points (`classify_workbooks`, its legacy variant) and the sheet and schema helpers move from `rfi_follow_up/classify.rs` into `rfi_follow_up/classify/dispatch.rs`. The main classifier file shrinks from 1,118 to 943 lines. Behavior is unchanged.

## 1.36.28 - 2026-10-27

### Changed

- The three PDF file-effect steps (first-page replace, page rotate, and page extract) move from `fs_ops/effects.rs` into `fs_ops/effects_pdf.rs`. The effects file shrinks from 1,173 to 584 lines. Behavior is unchanged.

## 1.36.27 - 2026-10-27

### Changed

- Print failure classification (failure kinds, retry rules, and submission outcomes) moves out of `print_queue.rs` into `print_queue/failure.rs`. The public types keep their paths. Behavior is unchanged.

## 1.36.26 - 2026-10-27

### Changed

- The path and output helpers of Process Scans (destination checks, review paths, page ranges, candidate output, PDF hashing, and plan steps) now live in their own module, `operations/process_scans/output.rs`. That cuts the main file from 1,321 to 1,110 lines. Behavior is unchanged.

## 1.36.25 - 2026-10-27

### Fixed

- The known-issues note about the Outlook selected-message archive named a preview function that 1.36.8 had already removed. The note now names the code that remains: the unused preparation and binding scaffolding in `outlook_reply_draft.rs`.

## 1.36.24 - 2026-10-27

### Changed

- Three identical copies of the file-name helper `display_name` (in batch print, Process Scans, and signed-page staging) are now one shared function in `naming.rs`. No behavior change; the code is smaller to maintain.

## 1.36.23 - 2026-10-27

### Changed

- Patch updates of Rust dependencies: the ICU text-internationalization crates (2.3 to 2.4), zerovec, yoke, tinystr, and erased-serde. Clippy with warnings denied and all Rust tests pass. Crates with unclear benefit or native build risk (cc, uuid, brotli) stay where they are.

## 1.36.22 - 2026-10-27

### Security

- The zip library moves from 8.6.0 to 9.0.0. The old version had three advisories (RUSTSEC-2026-0367, -0368, -0369) about ZIP extraction: setuid/setgid bits kept, symlinks pointing outside the destination, and permissions applied through symlinks. The app only reads and writes ZIP packages and never calls `extract()`, so the exposure was not reachable here.
- Office package code now treats an entry name that is not valid UTF-8 as an error. Before, it was read leniently. Affected: workbook and document reads, the signature check, guarded patches, and the RFI file-group scan. Packages with such names now fail with a clear message instead of being rewritten.

## 1.36.21 - 2026-10-27

### Removed

- Three stray backup copies of test files (`cap_fs/tests.rs-E`, `fs_ops/tests.rs-E`, `platform/excel/tests.rs-E`) were committed by mistake. Nothing compiled or referenced them, so no behavior changes.

## 1.36.20 - 2026-10-27

### Fixed

- The README said nine registered workflows and left out two: concrete document generation and RFI publish status. It now says eleven and lists both.

## 1.36.19 - 2026-10-27

### Changed

- The HTTP client ureq moves from 3.4.2 to 3.5.0. It includes TLS dependency security fixes and tighter timeout handling. Behavior change to note: proxy URL credentials are decoded once, so a literal percent sign must be written as %25. Users with a proxy password containing a raw % need to escape it. Clippy and all Rust tests pass; no proxy was available to test here.

## 1.36.18 - 2026-10-27

### Changed

- Patch updates of the Rust dependencies: Tauri 2.12.4 to 2.12.5 (and its build, codegen, macros, and runtime crates), Tokio 1.54.0 to 1.54.1, and a few small crates (tao, wry, rfd, rustix, smallvec, open, plist, webpki-roots, tinystr). Clippy with warnings denied and all Rust tests pass.

## 1.36.17 - 2026-10-27

### Changed

- The frontend test runner vitest (dev only) moves from 4.1.11 to 5.0.5. All 263 frontend tests and the production build pass with no deprecation warnings.

## 1.36.16 - 2026-10-27

### Changed

- The test environment jsdom (dev only) moves from 30.0.1 to 30.1.3. All 263 frontend tests and the production build pass.

## 1.36.15 - 2026-10-27

### Changed

- The Node type definitions (@types/node, dev only) move from 26.2.0 to 26.6.5. Typecheck, all 263 frontend tests, and the production build pass.

## 1.36.14 - 2026-10-27

### Changed

- The icon library lucide-react moves from 1.32.0 to 1.62.0. Typecheck, all 263 frontend tests, and the production build pass; TypeScript confirms every imported icon still exists.

## 1.36.13 - 2026-10-27

### Changed

- React and React DOM move from 19.2.8 to 19.3.1. The typecheck, all 263 frontend tests, and the production build pass with no new warnings.

## 1.36.12 - 2026-10-27

### Changed

- The Tauri CLI build tool moves from 2.12.3 to 2.12.4 (patch release). The app and its tests are unchanged.

## 1.36.11 - 2026-10-27

### Changed

- The quality workflow's source checkouts no longer keep the access token in the local git config (persist-credentials: false). None of its steps push or fetch, so the build is unchanged.

## 1.36.10 - 2026-10-27

### Changed

- Correction to the 1.35.18 note. That note called the late-restore fix a likely cause of the intermittent Ctrl+Enter shortcut test failure. A reproduction through the app shows the restore never overwrote a value picked by the user in the test scenario, so the cause of that failure is still unconfirmed. The restore fix stays as a defensive change with its unit test.

## 1.36.9 - 2026-10-27

### Changed

- The release gate drops `cargo check`: clippy with all targets already type-checks the same code, and the crate defines no Cargo features. The gate, AGENTS.md, RELEASE_SAFETY.md, and TASKS.md are updated to match.

## 1.36.8 - 2026-10-27

### Changed

- Removed three functions for retired features (replica publish preview, Outlook archive preview, automatic replica tick) that were kept behind allow(dead_code) and had no callers. No behavior change.

## 1.36.7 - 2026-10-27

### Changed

- Two workbook XML attribute readers repeated the same size-limit and decoding checks. One helper does them now. No behavior change.

## 1.36.6 - 2026-10-27

### Changed

- Three places in the RFI Follow Up classifier built the same manual-review row field by field. One helper builds it now. No behavior change.

## 1.36.5 - 2026-10-27

### Changed

- Batch print rows for files and worksheets set the same printer, paper, and copy settings in two places. One helper sets them now. No behavior change.

## 1.36.4 - 2026-10-27

### Changed

- The two Excel package writers (one for files, one for the size-guarded stream) repeated the same loop that copies or replaces each workbook entry. The loop is one shared helper. No behavior change.

## 1.36.3 - 2026-10-27

### Changed

- Sign-in, cloud history, and AI-service code each had its own copy of the HTTP error mapping. It is now one conversion shared by all three. No behavior change.

## 1.36.2 - 2026-10-27

### Changed

- The Outlook reply preview on the plan screen is one component instead of two identical copies. No behavior change.

## 1.36.1 - 2026-10-27

### Changed

- The file-selection merge used by drag-and-drop and Add Files is one shared helper instead of two copies. No behavior change.

## 1.36.0 - 2026-10-27

### Added

- Windows installer for this minor release, built by the Windows release workflow.

### Changed

- Dialogs move keyboard focus inside on open, keep Tab inside, and return focus to the opener when closed.
- Errors and warnings are announced to screen readers; filter tabs and category chips report their selected state.
- The page language follows the selected interface language.
- Startup returns an error instead of crashing if the sign-in or AI-service HTTP client cannot be created.
- A window-level error boundary shows a translated message and a Reload button instead of a blank window.

### Fixed

- About 60 backend error messages that appeared as raw English are translated in Turkish, English, Russian, and Azerbaijani (1.35.4 to 1.35.8).
- Form field borders, the table search box, and keyboard focus outlines meet WCAG contrast and focus requirements (1.35.9, 1.35.10).
- Touch targets for the search box, switches, and sliders are at least 24 px (1.35.3).
- Saved settings no longer overwrite a value picked before they finished loading (1.35.18).
- Remaining English words in Turkish, Azerbaijani, and Russian screens are replaced with local wording (1.35.19 to 1.35.23).

## 1.35.24 - 2026-10-27

### Fixed

- The English search placeholder used three periods while other text uses the single ellipsis character. It now matches.

## 1.35.23 - 2026-10-27

### Fixed

- Four Turkish Follow Up and status messages still had the English word "Exact" in capitals, and one said "tam tam-değer". They now say Tam or tam değer.

## 1.35.22 - 2026-10-27

### Fixed

- Turkish signed-scan, Outlook, and archive forms used the English words "exact", "Review", and "Warning", and wrote RFI with a dotted capital I. They now say tam, İnceleme, Uyarı, and RFI.

## 1.35.21 - 2026-10-27

### Fixed

- The Turkish PDF rotation form said "review klasörü" and "Görevden çıkınca". It now says "inceleme klasörü" and "İş tamamlanınca".

## 1.35.20 - 2026-10-27

### Fixed

- Turkish and Azerbaijani settings and automation text used the English word "sync". It now says "eşitleme" (Turkish) and "sinxronizasiya" (Azerbaijani).

## 1.35.19 - 2026-10-27

### Fixed

- Some Turkish, Azerbaijani, and Russian operation descriptions still contained English: "Fast RFI Taraması" (the card is named "Hızlı RFI Taraması"), "(Completed)" in the RFI status sync description, and "Fast RFI Scan" in Russian. They now use the local wording.

## 1.35.18 - 2026-10-27

### Fixed

- If you picked a file before the remembered settings finished loading, the restore could overwrite your choice with the saved one. The restore now skips any value you already changed. This is also a likely cause of the intermittent shortcut test failure.

## 1.35.17 - 2026-10-27

### Fixed

- The Ctrl+Enter/Escape shortcut test failed intermittently in the full quality gate (it timed out waiting for the preview), blocking releases. It now waits up to 10 seconds. No app behavior changed. The root cause of the slow preview under load is not confirmed.

## 1.35.16 - 2026-10-27

### Fixed

- Startup panicked if the HTTP client for sign-in or the AI service could not be created. It now returns a startup error instead.

## 1.35.15 - 2026-10-27

### Fixed

- An unexpected error while drawing a screen left the whole window blank with no way to recover. The app now shows a translated message and a Reload button, and logs the error to the console.

## 1.35.14 - 2026-10-27

### Fixed

- The filter tabs (All, Changes, Manual review) and the category chips on the home screen showed their selected state only by color. They now set aria-pressed, so screen readers announce which filter is active.

## 1.35.13 - 2026-10-27

### Fixed

- Dialogs did not move keyboard focus into them, so Tab could reach the page behind an open dialog, and closing one left focus at the top of the page. Focus now moves into the dialog on open, Tab and Shift+Tab stay inside it, and closing returns focus to the control that opened it.

## 1.35.12 - 2026-10-27

### Fixed

- Error, warning, and info messages (notices) had no live-region role, so screen readers did not announce them when they appeared. Errors and warnings now use role="alert"; info messages use role="status".
- The Ctrl+Enter and Escape keyboard test waited 1 second for the preview, which could fail on a slow run. It now waits up to 3 seconds.

## 1.35.11 - 2026-10-27

### Fixed

- The page language was always set to English, so screen readers read Turkish, Russian, and Azerbaijani text with English pronunciation. It now follows the selected interface language.

## 1.35.10 - 2026-10-27

### Fixed

- Text fields, selects, and the table search box used a faint border (1.4 to 1.7 to 1, below the 3 to 1 non-text contrast minimum), so their edges were hard to see. They now use the strong border color, which is 4.6 to 1 on light and 3.4 to 1 on dark.

## 1.35.9 - 2026-10-27

### Fixed

- The table search box removed the browser focus outline and showed only a border color change. Keyboard users now see the same 2px focus outline as other inputs.

## 1.35.8 - 2026-10-27

### Fixed

- Eleven more backend error messages reached the UI as raw English text: RFI file-group rows and state, source files changed or unreadable, RFI row preparation, diagnostics files that are invalid or changed, missing generated files, and print queue and print row identifiers. They now have translations in all four languages.

## 1.35.7 - 2026-10-27

### Fixed

- Twenty-five more backend error messages reached the UI as raw English text: diagnostics collection and copy, clipboard writes, created and authorized folders, RFI folder, document, PDF and file-name reads, RFI schedule time, and AI extraction responses. They now have translations in all four languages.

## 1.35.6 - 2026-10-27

### Fixed

- Fourteen more backend error messages reached the UI as raw English text: review location and folder problems, print file and queue folder problems, saved print job state, path inspection, response preparation, preset values, scan destination changes, and unavailable recoverable deletion. They now have translations in all four languages.

## 1.35.5 - 2026-10-27

### Fixed

- Thirteen more backend error messages reached the UI as raw English text: print queue save and encoding, saved print job damage, file selection and reveal failures, unsafe path opens, preview and undo failures, unreadable scanned or review files, and non-absolute paths. They now have translations in all four languages.

## 1.35.4 - 2026-10-27

### Fixed

- Eleven backend error messages (copy encoding, missing folders, RFI schedule range, RFI System entry limit, RFI file-group rows, unreadable RFI Log workbook, Follow Up workbook folder, watch check failure) were shown as raw English. They now have translations in all four languages.

## 1.35.3 - 2026-10-27

### Fixed

- On phone width, three controls were below the 24 by 24 px minimum target size: the operation search box (20 px tall), the on-off switches (23 px), and the font-size slider (16 px). They are now at least 24 px tall. Checkboxes are not changed; their label is the larger clickable target.

## 1.35.2 - 2026-10-27

### Fixed

- Light-theme muted text (category chips and counts, small grey labels) measured 4.06:1 on its chip background, below the 4.5:1 minimum for small text. The light muted colour is darker now (4.96:1 on the same background). Dark theme is unchanged and already passes.

## 1.35.1 - 2026-10-27

### Fixed

- When remembered settings failed to save, the change was dropped without a message. The operation screen now shows "Settings could not be saved." in all four languages. The message clears after the next successful save.

## 1.35.0 - 2026-10-27

### Added

- Bounded number fields show their allowed range (for example `1–99`) under the input: print copies, retries, timeout, and the concrete document ambient temperatures. The range comes from the same backend bounds the form already enforces.

### Fixed

- Windows installer build for 1.34.0 failed on a Unix-style path test; fixed in 1.34.5. This minor release is the first tag that builds the Windows installer with that fix.

## 1.34.5 - 2026-10-27

### Fixed

- The Windows installer build for 1.34.0 failed in its test step. A path-guard test used `/root/dir/file.pdf`, which is not absolute on Windows. The test now builds its absolute path from the platform temp directory. No application behavior changes.

## 1.34.4 - 2026-10-27

### Fixed

- The ambient minimum and maximum temperature fields for concrete documents now carry the backend's 0 to 100 °C bounds, so the form rejects out-of-range values before preview instead of after. The print bounds and these bounds use one helper. The backend check is unchanged.

## 1.34.3 - 2026-10-27

### Changed

- Plan table row selection moved from `EditablePlanTable` into `usePlanSelection`, with tests for select all over the filtered rows, group select, toggling, and dropping selections for removed rows. No application behavior changes.

## 1.34.2 - 2026-10-27

### Changed

- Remembered-settings restore and debounced autosave moved from `OperationScreen` into `useSavedParams`, with tests. No application behavior changes.

## 1.34.1 - 2026-10-27

### Changed

- Operation keyboard shortcuts moved from `OperationScreen` into `useOperationShortcuts`, with tests for each shortcut and its stage rules. No application behavior changes.

## 1.34.0 - 2026-10-27

### Added

- Parameter schemas can carry optional integer bounds (`min`, `max`). The print operation sets them from the backend limits: copies 1 to 99, retries 0 to 3, timeout 1 to 240 minutes. The form's number inputs enforce those bounds, so out-of-range values are rejected before preview.

## 1.33.92 - 2026-10-27

### Fixed

- Turkish labels used the English word "Sheet" for sheet selection and the follow-up sheet column while other labels used "Sayfa". They now consistently say Sayfa. Workbook column headers such as Inspection Status are unchanged because they must match the workbook text.

## 1.33.91 - 2026-10-27

### Changed

- The browser-only demo bridge and its demo data are no longer in the desktop build. The bridge selection checks `import.meta.env.DEV`, so release builds drop the mock. Main bundle: 345.6 kB to 316.0 kB (gzip 102.0 kB to 94.6 kB). No behavior change in the desktop app; `vite dev` still uses the demo.

## 1.33.90 - 2026-10-27

### Fixed

- Twenty-six user-facing backend messages were shown as raw English or unaccented Turkish. They now use translated text in all four languages: AI extraction and service errors, printer unavailable, copy-count range, empty print queue or plan, selected and dropped path errors, RFI System folder errors, clipboard table size, history limit, and RFI row rules.
- Internal consistency messages (journal and S5 recovery checks) are not translated; users see them only in rare recovery cases.

## 1.33.89 - 2026-10-27

### Fixed

- Seven more backend messages were shown raw: a missing RFI Log write permission, a workbook outside any folder, invalid sheet, multi-sheet and RFI row plans, an invalid mail table source, and a plan with no copyable table. The English and unaccented Turkish text now shows as translated messages in all four languages.

## 1.33.88 - 2026-10-27

### Fixed

- Ten more backend messages were shown as plain English: review file or folder problems, the file-name and PDF-only checks, an unwatchable operation, a failed diagnostics file, an inaccessible selected folder, an unreadable print date, and an unidentifiable print row. They now use translated text in all four languages.

## 1.33.87 - 2026-10-27

### Fixed

- Ten backend messages were shown as plain English: print file unavailable, unknown operation, invalid saved print queue, empty print plan, missing review destination, missing source or selected path, missing saved print job, missing recoverable trash item, and unresolvable path. They now use translated text in all four languages.

### Changed

- A helper test fails when a mapped backend message no longer appears in the Rust source, so the mapping cannot go stale.

## 1.33.86 - 2026-10-27

### Fixed

- Path errors from the backend (traversal, escape, denied access, expired or mismatched folder grants) were shown as raw English text. They now use translated messages in all four languages.

## 1.33.85 - 2026-10-27

### Fixed

- Modal dialogs had no accessible name, so screen readers did not announce their title. The dialog is now labelled by its title.
- The modal close button was labelled with the hardcoded English word "Close" in every language. It now uses the translated `action.close` text.

## 1.33.84 - 2026-10-27

### Fixed

- Screen readers announced the operation search box, the font-size slider, and the Settings username field without a name. Each now has an accessible label, reusing the existing translated text.

## 1.33.83 - 2026-10-27

### Changed

- Removed 79 translation keys from all four catalogs that no literal or template in the frontend or backend builds. Most are leftovers of the replica publish feature retired in 1.13.0 (`param.replica.*`, `choice.replica.*`), plus unused print, RFI, and home labels. Keys built from prefixes such as `op.`, `effect.`, `progress.phase.`, and `print.status.` were kept.

## 1.33.82 - 2026-10-27

### Fixed

- Corrects 1.33.80. The watch-folder messages were not shown as raw keys: the queue screen already translates watch codes through `error.<code>`, which had translations. The `watch.tooManyEntries` and `watch.unreadable` keys added in 1.33.80 were never looked up and are removed.

### Changed

- The backend key guard resolves `watch.*` codes through the `error.` prefix, matching the queue screen.

## 1.33.81 - 2026-10-27

### Changed

- Added a catalog test that scans the backend source for `error.*`, `note.*`, and `watch.*` keys and fails when one has no English translation. No application behavior changes.

## 1.33.80 - 2026-10-27

### Fixed

- Backend messages were shown as raw keys when they had no translation. Added translations in all four languages for operation cancelled, missing preset, invalid generated document, the create-folder and Follow Up workbook notes, and the watch-folder limit and unreadable-folder errors.

## 1.33.79 - 2026-10-27

### Fixed

- Labels that stayed in English in the Turkish, Russian, and Azerbaijani catalogs are now translated: plan and print column headers (area, description, forms, item, row, sheet, format, status, printer), the sketch and first-page document types, and the printer parameter. Workbook names, paper sizes, and codes such as Tag No. and AZFEN/INTEC are unchanged.

## 1.33.78 - 2026-10-27

### Fixed

- The plan table's row-actions column label showed the raw key `editor.rowActions` to screen readers. It now has translations in Turkish, English, Russian, and Azerbaijani.

### Changed

- Added a catalog test that fails when a literal `s.t()` key used in the frontend is missing from the English catalog.

## 1.33.77 - 2026-10-27

### Changed

- Added a unit test that the backend timestamp keeps its `unix:` prefix and whole-second format. No application behavior changes.

## 1.33.76 - 2026-10-27

### Changed

- Added unit tests for the follow-up failure helpers in `fs_ops/support.rs`: warning keys, empty subjects, and backend error messages used as keys. No application behavior changes.

## 1.33.75 - 2026-10-27

### Changed

- Added unit tests for `validate_empty_params`: empty object accepted; extra fields, null, and arrays rejected. No application behavior changes.

## 1.33.74 - 2026-10-27

### Changed

- `path_selection` now uses the shared `expired` helper from `paths` instead of its own copy.
- Added unit tests for relative-path and `..` rejection and for grant expiry at the TTL boundary. No application behavior changes.

## 1.33.73 - 2026-10-27

### Changed

- Added unit tests for `useRfiPrintFinder`: required-input guards, the search request, stored results, late-result discard after invalidation, and candidate selection. No application behavior changes.

## 1.33.72 - 2026-10-27

### Changed

- Added unit tests for `frontend/src/i18n.ts`: system locale detection, fallback to Turkish, missing-key fallback, placeholder replacement, and preferred locale selection. No application behavior changes.

## 1.33.71 - 2026-10-27

### Changed

- Added unit tests for the path selection helpers in `lib/platform.ts`: first-selection rules, empty picks, dropped paths, and reveal. No application behavior changes.

## 1.33.70 - 2026-10-27

### Changed

- Added unit tests for `summarizeWorkbookCellChanges` and `canonicalRfiInText`: cell address mapping, follow-up and status changes, invalid-row skipping, non-canonical code rejection, and the preview cap. No application behavior changes.

## 1.33.69 - 2026-10-27

### Changed

- Added unit tests for `summarizePlanImpact`, which counts outputs, updates, moves, print jobs, drafts, and cleanup steps. No application behavior changes.

## 1.33.68 - 2026-10-27

### Changed

- Removed `export` from six frontend types used only in their own file (`PlanImpact`, `SignedWarningReason`, `SignedWarningReviewItem`, `WorkbookCellChange`, `ApplyScreenState`, `PresetToolbarProps`). No application behavior changes.

## 1.33.67 - 2026-10-27

### Changed

- The Windows Release workflow runs only for minor and major tags (`v*.*.0`, such as `v1.34.0` or `v2.0.0`). Patch tags no longer start an installer build, because the build cannot publish an installer to a release that already exists. Patch releases publish release notes without an installer. No application behavior changes.

## 1.33.66 - 2026-10-27

### Changed

- The plan table's row action buttons (move up and down, duplicate, and remove) move from `frontend/src/plan/EditablePlanTable.tsx` into the `PlanRowActions` component in `frontend/src/plan/PlanRowActions.tsx`. The table keeps the enabled-feature check and passes the row index, the row and group counts, and the move, duplicate, and remove handlers. `PlanRowActions.test.tsx` covers the reorder and duplicate callbacks, the edge-row disabled states, the lone-row remove guard, and the omitted buttons. `EditablePlanTable.tsx` drops from 621 to 586 lines. Behavior is unchanged.

## 1.33.65 - 2026-10-27

### Changed

- The plan table's header row (the select-all checkbox, the resizable column headers, and the action column) moves from `frontend/src/plan/EditablePlanTable.tsx` into the `PlanTableHead` component in `frontend/src/plan/PlanTableHead.tsx`. The table computes the select-all state and passes the column-width setter as a callback. `PlanTableHead.test.tsx` covers the select-all callback, the checked state, the optional selection and action columns, and one header per visible column. `EditablePlanTable.tsx` drops from 653 to 621 lines. Behavior is unchanged.

## 1.33.64 - 2026-10-27

### Changed

- The bulk editor's field, value, and apply controls move from `frontend/src/plan/EditablePlanTable.tsx` into the `PlanBulkFields` component in `frontend/src/plan/PlanBulkFields.tsx`. The table passes the bulk state and callbacks, and apply is enabled by a `canApply` flag: a selection, a field, and a value. `PlanBulkFields.test.tsx` covers the field change, the value change, and apply being enabled only when the edit is complete. `EditablePlanTable.tsx` drops from 704 to 653 lines. Behavior is unchanged.

## 1.33.63 - 2026-10-27

### Changed

- The bulk selection controls (selected count, select all, select none, and per-group picks) move from `frontend/src/plan/EditablePlanTable.tsx` into the `PlanSelectionBar` component in `frontend/src/plan/PlanSelectionBar.tsx`. The compact-layout group labels stay as they were. `PlanSelectionBar.test.tsx` covers the count, the select-all and select-none callbacks, and a group pick. `EditablePlanTable.tsx` drops from 720 to 704 lines. Behavior is unchanged.

## 1.33.62 - 2026-10-27

### Changed

- The plan table's search box, filter tabs, and filtered-row count move from `frontend/src/plan/EditablePlanTable.tsx` into the `PlanFilterBar` component in `frontend/src/plan/PlanFilterBar.tsx`. The table passes the search and filter values with their setters and the two row counts. `PlanFilterBar.test.tsx` covers the search callback, the filter-tab callback, and the count shown only while a filter is active. `EditablePlanTable.tsx` drops from 752 to 720 lines. Behavior is unchanged.

## 1.33.61 - 2026-10-27

### Changed

- The replica automation status read and its five-second refresh move from `frontend/src/screens/SettingsScreen.tsx` into the `useReplicaStatus` hook in `frontend/src/screens/useReplicaStatus.ts`. The settings screen still calls the refresh after a preference change. `useReplicaStatus.test.ts` covers the load on mount, the five-second poll, and keeping the last good status when a refresh fails.
- The `useSupportDiagnostics` hook body, added in 1.33.60, is re-indented to match the surrounding code. No behavior changes.

## 1.33.60 - 2026-10-27

### Changed

- The support diagnostics state and actions (preview, copy, activity-log save, and download) move from `frontend/src/screens/SettingsScreen.tsx` into the `useSupportDiagnostics` hook in `frontend/src/screens/useSupportDiagnostics.ts`. The screen still owns the shared error line. `useSupportDiagnostics.test.ts` covers the preview, the failed preview, the copy gate before a preview, and the copy with its status. `SettingsScreen.tsx` drops from 453 to 389 lines. Behavior is unchanged.

## 1.33.59 - 2026-10-27

### Changed

- The browser demo's plan handlers move into `frontend/src/lib/mockPlanHandlers.ts`: `MockPlanState`, `previewOperation`, and `reviseOperation`. `frontend/src/lib/mockHandlers.ts` keeps the print finder and diagnostics handlers and drops from 756 to 108 lines. Unused imports are removed from both modules. The 177 frontend tests pass. No operator-facing behavior changes.

## 1.33.58 - 2026-10-27

### Changed

- The browser demo bridge's `engine/preview` and `engine/revise` handlers move from `MockBridge` in `frontend/src/lib/mockBridge.ts` into `frontend/src/lib/mockHandlers.ts`. The four plan fields they share (print, file-group, complete-status, and fast-scan plans) become one `MockPlanState` object that the bridge owns and passes in. Unused imports are removed, and `mockBridge.ts` drops from 1,093 to 364 lines. The 177 frontend tests pass, including the App tests that drive these handlers. No operator-facing behavior changes.

## 1.33.57 - 2026-10-27

### Changed

- The browser demo bridge's `rfi/printFind` and `support/diagnostics` handlers do not use bridge state, so they move from `MockBridge` in `frontend/src/lib/mockBridge.ts` into plain exported functions in `frontend/src/lib/mockHandlers.ts`. `MockBridge` calls them from its dispatch. `mockBridge.ts` drops from 1,093 to 993 lines. The 177 frontend tests pass, including the direct `rfi/printFind` check. No operator-facing behavior changes.

## 1.33.56 - 2026-10-27

### Changed

- The preview run moves from `frontend/src/screens/OperationScreen.tsx` into the `usePreviewRun` hook in `frontend/src/screens/usePreviewRun.ts`. It saves the parameters, updates the last-saved reference, requests the engine preview, and returns to the form with the error on failure. `usePreviewRun.test.ts` covers the save-then-preview order with the plan installed, and the failure path back to the form. Behavior is unchanged.

## 1.33.55 - 2026-10-27

### Changed

- The apply run moves from `frontend/src/screens/OperationScreen.tsx` into the `useApplyRun` hook in `frontend/src/screens/useApplyRun.ts`. The hook keeps the revise-then-apply path for an edited plan, the stop back at preview when a revision adds a warning, the hand-off of fast-scan edits to the preview re-run, and the return to preview on failure. The screen's setters and progress handlers are passed in as grouped objects. `useApplyRun.test.ts` covers each path. Behavior is unchanged.

## 1.33.54 - 2026-10-27

### Changed

- The RFI reply preview and undo actions move from `frontend/src/screens/OperationScreen.tsx` into the `useResultActions` hook in `frontend/src/screens/useResultActions.ts`. `useResultActions.test.ts` covers the reply preview by plan and journal, the no-journal no-op, undo by journal with return to the previous screen, and a failed undo that stays put. Behavior is unchanged.

## 1.33.53 - 2026-10-27

### Changed

- The table copy action and its in-flight sheet state move from `frontend/src/screens/OperationScreen.tsx` into the `useCopyTable` hook in `frontend/src/screens/useCopyTable.ts`. `useCopyTable.test.ts` covers the copy by plan and sheet with the row count reported, the no-plan no-op, and a failed copy that clears the in-flight sheet. Behavior is unchanged.

## 1.33.52 - 2026-10-27

### Changed

- The preview and apply cancel handlers and their shared `cancelling` flag move from `frontend/src/screens/OperationScreen.tsx` into the `useRunCancel` hook in `frontend/src/screens/useRunCancel.ts`. The screen passes an `onPreviewCancelled` callback that returns it to the form. `useRunCancel.test.ts` covers preview cancel by operation, the return to the form after an error, apply cancel by plan, the no-plan no-op, and a failed apply cancel. Behavior is unchanged.

## 1.33.51 - 2026-10-27

### Changed

- The fast-scan preview re-run (`updateScanPreview` and its in-flight flag) moves from `frontend/src/screens/OperationScreen.tsx` into the `useScanPreviewUpdate` hook in `frontend/src/screens/useScanPreviewUpdate.ts`. The screen still owns the "preview updated" flag and passes an `onRevised` callback that installs the revised plan. `useScanPreviewUpdate.test.ts` covers the non-scan and clean-editor no-ops, a successful revision, and a failed revision. Behavior is unchanged.

## 1.33.50 - 2026-10-27

### Changed

- The batch print queue's state, its load on mount for `files.batch_print`, and the resume and discard actions move from `frontend/src/screens/OperationScreen.tsx` into the `usePrintQueue` hook in `frontend/src/screens/usePrintQueue.ts`. The hook reports errors and stage changes through the callbacks the screen passes in. `usePrintQueue.test.ts` covers the batch-only load, clearing after discard, and the return to the form when resume fails. Behavior is unchanged.

## 1.33.49 - 2026-10-27

### Changed

- The operation run's progress state (the latest event, the phase trail, and the receive and start handlers) moves from `frontend/src/screens/OperationScreen.tsx` into the `useProgressTrail` hook in `frontend/src/screens/useProgressTrail.ts`. The trail keeps its eight-entry limit and collapses repeated phases. `useProgressTrail.test.ts` covers both behaviors. Behavior is unchanged.

## 1.33.48 - 2026-10-27

### Changed

- The browser demo bridge's `recovery/list` handler moves out of the `MockBridge.call` dispatch switch in `frontend/src/lib/mockBridge.ts` into the private `recoveryListOperation` method. No operator-facing behavior changes.

## 1.33.47 - 2026-10-27

### Changed

- The browser demo bridge's `support/diagnostics` handler moves out of the `MockBridge.call` dispatch switch in `frontend/src/lib/mockBridge.ts` into the private `diagnosticsOperation` method. No operator-facing behavior changes.

## 1.33.46 - 2026-10-27

### Changed

- The browser demo bridge's `history/list` handler moves out of the `MockBridge.call` dispatch switch in `frontend/src/lib/mockBridge.ts` into the private `historyListOperation` method. The App tests cover the History screen and pass unchanged. No operator-facing behavior changes.

## 1.33.45 - 2026-10-27

### Changed

- The browser demo bridge's `rfi/printFind` handler moves out of the `MockBridge.call` dispatch switch in `frontend/src/lib/mockBridge.ts` into the private `printFindOperation` method. `mockBridge.test.ts` adds the first direct check of this route: a full RFI number resolves to its first-page candidate with the expected code, selection, and missing-first-page flag. No operator-facing behavior changes.

## 1.33.44 - 2026-10-27

### Changed

- The browser demo bridge's `engine/revise` handler moves out of the `MockBridge.call` dispatch switch in `frontend/src/lib/mockBridge.ts` into the private `reviseOperation` method, matching the `engine/preview` split in 1.33.43. Early exits return the result the switch would have returned. The App tests drive this path and pass unchanged. No operator-facing behavior changes.

## 1.33.43 - 2026-10-27

### Changed

- The browser demo bridge's `engine/preview` handler moves out of the `MockBridge.call` dispatch switch in `frontend/src/lib/mockBridge.ts` into the private `previewOperation` method. The switch now holds a one-line call for that case. Early exits inside the handler return the same result the switch would have returned. The App tests drive this path, and they pass unchanged. No operator-facing behavior changes.

## 1.33.42 - 2026-10-27

### Changed

- The check that sends a revised plan back to review when the revision adds a warning moves from `OperationScreen.tsx` into `hasNewWarnings` in `frontend/src/plan/revisionWarnings.ts`. A warning counts as new when its key and subject pair was not in the previous plan. `revisionWarnings.test.ts` covers unchanged, added-key, added-subject, and empty plans. Behavior is unchanged.

## 1.33.41 - 2026-10-27

### Changed

- `helpers.test.ts` now covers `isDuplicatePresetError`, which recognises duplicate-preset messages in their known forms and ignores unrelated errors, and `formatDate`, which returns an unparseable value unchanged and formats a valid timestamp with the requested locale. No application code changes.

## 1.33.40 - 2026-10-27

### Changed

- `frontend/src/locales/catalogs.test.ts` guards the ARB catalogs. Each of az, ru, and tr must have exactly the English message keys, ignoring `@` metadata, and each message must use the same `{placeholder}` names as English. The check passes on the current catalogs. No application code changes.

## 1.33.39 - 2026-10-27

### Changed

- `frontend/src/plan/editorDates.test.ts` covers the plan editor's date parser and formatter. The tests check two- and four-digit years, the leap day, month-length limits, the 2000–2068 year window, and a parse-format round trip. No application code changes.

## 1.33.38 - 2026-10-27

### Changed

- `frontend/src/plan/signedWarnings.test.ts` covers `signedWarningReviewItems`, the check on backend review items. The tests cover the three-value status allow-list, dropped items with a missing file name or malformed reason list, dropped reasons without a key or detail, and page-number sanitizing (only positive safe integers survive). No application code changes.

## 1.33.37 - 2026-10-27

### Changed

- `frontend/src/helpers.test.ts` covers `parseRfiInputs`, `isPathSelection`, and `humanError`. The `humanError` tests check a bare localized key, a keyed error with detail, the auth and rate-limit mappings, and the raw fallback. No application code changes.

## 1.33.36 - 2026-10-27

### Changed

- `failureMessage` in `frontend/src/screens/QueueScreen.tsx` is exported, and `QueueScreen.test.ts` covers its three paths: a known `watch.*` code uses its own text, an unknown code falls back to the generic text naming the code, and a missing code falls back to the generic text. No behavior changes.

## 1.33.35 - 2026-10-27

### Changed

- `ResultView.test.ts` covers `outlookReplyPreview`, the check that accepts a backend reply preview only when subject, plain text, and HTML are all strings. The test confirms extra fields are dropped and malformed previews are rejected. No application code changes.

## 1.33.34 - 2026-10-27

### Changed

- The home screen's operation category lookup (`getOperationCategory` and the `OperationCategory` type) moves from `frontend/src/screens/HomeScreen.tsx` into `frontend/src/lib/operationCategory.ts`. The scan and Excel groups are sets. `operationCategory.test.ts` covers each group and the system fallback. Behavior is unchanged.

## 1.33.33 - 2026-10-27

### Changed

- The plan table's option lookup moves from a closure in `frontend/src/plan/EditablePlanTable.tsx` into `choicesForRow` in `frontend/src/plan/editorRules.ts`. A direct option list wins; otherwise the options depend on another field of the row. `editorRules.test.ts` covers both paths and a column with no configuration. Behavior is unchanged.

## 1.33.32 - 2026-10-27

### Changed

- The plan table's row filter moves from a `useMemo` in `frontend/src/plan/EditablePlanTable.tsx` into `filterRowIndices` in `frontend/src/plan/editorRules.ts`. It keeps the changes and review modes and the case-insensitive search over string and number fields. `editorRules.test.ts` covers each mode, the search rules, and their combination. Behavior is unchanged.

## 1.33.31 - 2026-10-27

### Changed

- The plan editor's row duplicate, remove, and move rules move from `frontend/src/plan/EditablePlanTable.tsx` into `frontend/src/plan/editorRules.ts` as `duplicateRowAt`, `removeRowAt`, and `moveRowAt`. Each returns a new list, or `null` when the operation is refused, so the table only calls `onChange` when it changes rows. `editorRules.test.ts` covers the copy position and new id, the rule that the last row of a group cannot be removed, and the move bounds. Behavior is unchanged.

## 1.33.30 - 2026-10-27

### Changed

- The plan editor's cell write (`applyEditorValue`) moves from `frontend/src/plan/EditablePlanTable.tsx` into `frontend/src/plan/editorRules.ts`. It writes the value, then applies the editor's auto-fill and regex-fill presets. The table keeps a one-line wrapper. `editorRules.test.ts` now covers the auto-fill match and miss, a case-insensitive regex match, and a non-matching or invalid regex being ignored. Behavior is unchanged.

## 1.33.29 - 2026-10-27

### Changed

- The browser demo bridge's fixtures move from `frontend/src/lib/mockBridge.ts` into `frontend/src/lib/mockFixtures.ts`: the signed-out and demo-admin users, default preferences, the demo operation list, the path and RFI-code normalizers, and the empty-value check. `MockBridge` imports them. Behavior is unchanged; the 95 frontend tests use the bridge.

## 1.33.28 - 2026-10-27

### Changed

- Twelve frontend helpers and components that no other file imports are no longer exported: `AppFrame` and `Splash` in `App.tsx`, `SettingsGroup` and `ChoiceGrid` in `SettingsScreen.tsx`, `presetCapableValues` and `presetCommandError` in `PresetToolbar.tsx`, `getOperationCategory` and `getOperationIcon` in `HomeScreen.tsx`, `recordValue`, `previewText`, and `excelCell` in `workbookCellChanges.ts`, and `isCanonicalRfiCode` in `editorRules.ts`. They stay as module-private functions. Behavior is unchanged.

## 1.33.27 - 2026-10-27

### Removed

- Dead frontend code with no callers: the `listOperationPresets`, `saveOperationPreset`, and `deleteOperationPreset` wrappers in `frontend/src/lib/platform.ts`, and the `demoOutlookReplyOperation` and `demoReplicaPublishOperation` demo definitions in `frontend/src/lib/demoOperations.ts`. The preset toolbar and the mock bridge use their own paths, so no operator-facing behavior changes.

## 1.33.26 - 2026-10-27

### Changed

- `frontend/src/App.tsx` no longer re-exports helpers and components (`humanError`, `formatConfidencePercent`, `isPlanEditorReady`, `PlanView`, `PlanWarnings`, `summarizePlanImpact`, `canonicalRfiInText`, `summarizeWorkbookCellChanges`, `safePngThumbnail`). `App.test.tsx` imports each from its own module. The app's runtime imports are unchanged.

## 1.33.25 - 2026-10-27

### Changed

- The signed-warning review types, `signedWarningReviewItems`, and `scrollToSignedSection` move from `frontend/src/plan/PlanSummaries.tsx` into `frontend/src/plan/signedWarnings.ts`. The components in `PlanSummaries.tsx` import them from there. Behavior is unchanged. The App tests reach the signed plan path indirectly; no test calls these helpers directly.

## 1.33.24 - 2026-10-27

### Changed

- The plan impact summary (`PlanImpact` and `summarizePlanImpact`) moves from `frontend/src/plan/PlanSummaries.tsx` into `frontend/src/plan/planImpact.ts`. `PlanSummaries.tsx` re-exports the module, so existing imports are unchanged. Behavior is unchanged; the App tests cover `summarizePlanImpact`.

## 1.33.23 - 2026-10-27

### Changed

- The workbook cell change summary moves from `frontend/src/plan/PlanSummaries.tsx` into `frontend/src/plan/workbookCellChanges.ts`: `summarizeWorkbookCellChanges`, `canonicalRfiInText`, the Excel cell address helper, and the preview cap. `PlanSummaries.tsx` re-exports the module, so existing imports are unchanged. Behavior is unchanged; the App tests cover these helpers.

## 1.33.22 - 2026-10-27

### Changed

- Three pure helpers move from `frontend/src/screens/OperationScreen.tsx` into `frontend/src/screens/operationState.ts`: the progress-trail update (`nextProgressTrail`), editor row extraction from a plan (`editorRowsOf`), and the saved-parameter filter used on hydrate (`rememberedParams`). Behavior is unchanged; `operationState.test.ts` covers the trail limit, phase replacement, row extraction, and remember opt-out.

## 1.33.21 - 2026-10-27

### Changed

- The pure editor rules move from `frontend/src/plan/EditablePlanTable.tsx` into `frontend/src/plan/editorRules.ts`: column default and minimum widths, cell display formatting, confidence percentages, RFI code checks, and plan readiness. `EditablePlanTable.tsx` re-exports the module, so existing imports are unchanged. Behavior is unchanged; the 91 frontend tests cover these helpers.

## 1.33.20 - 2026-10-26

### Changed

- The batch-print RFI finder's state, invalidation, search, and selection logic move from `OperationScreen.tsx` into the `useRfiPrintFinder` hook in `frontend/src/screens/useRfiPrintFinder.ts`. The hook keeps the stale-response guard, so an older search cannot overwrite a newer one.

## 1.33.19 - 2026-10-26

### Changed

- The batch-print RFI number checkboxes and their select-all and select-none buttons move from `OperationScreen.tsx` into `frontend/src/screens/RfiInputChooser.tsx`. The parent still owns the finder invalidation, so behavior is unchanged.

## 1.33.18 - 2026-10-26

### Changed

- The RFI finder results for batch print (per-request notes, bulk-select controls, and the file list) moved from `OperationScreen.tsx` into `frontend/src/screens/RfiPrintResults.tsx`. Behavior and labels are unchanged; the 91 frontend tests cover the batch-print flow.

## 1.33.17 - 2026-10-26

### Fixed

- Below 360px the phone bottom bar shows icons only, with 40px targets, instead of shrinking the active label's neighbours to about 26px. The named current section is kept at 360px, 375px, and 390px, each checked in the browser with the longest label, Kurtarma merkezi, fully visible and every item inside the viewport.

## 1.33.16 - 2026-10-26

### Changed

- The preview and apply progress views of the operation screen are one `BusyStage` component in `frontend/src/screens/BusyStage.tsx`. The labels, cancel handlers, and cancel-button wording are unchanged.

## 1.33.15 - 2026-10-26

### Changed

- At phone width (620px and narrower), the bottom navigation shows the label of the current section next to its icon, so operators can tell the seven sections apart. The other sections stay icon-only. The longest label, Kurtarma merkezi, fits at 320px and 375px (checked in the browser; 390px not checked). Inactive icon targets shrink to about 26px at 320px, below the usual 44px touch size; this is the trade-off for naming the current section.

## 1.33.14 - 2026-10-26

### Changed

- The values sent to `engine/preview` are built by `previewValuesFor` in `frontend/src/lib/operationValues.ts`. Batch print's finder-only inputs and file list are covered by unit tests.

## 1.33.13 - 2026-10-26

### Changed

- The rule for which operation values are remembered (skip `remember: false`, never store long text or secrets) lives in `frontend/src/lib/operationValues.ts` as `persistableValues`. `OperationScreen.tsx` uses it in its three former copies, and a unit test covers the rule.

## 1.33.12 - 2026-10-26

### Changed

- The browser mock bridge rejects a preview that omits a required parameter, with the same `error.param.required.<name>` key the engine uses. Previously the browser demo accepted empty values the real app would refuse.
- Three App tests and one direct-call fixture now supply the values the real app sends: they wait for the picked workbook path, and the fast-scan fixture sets `scan_mode`.

## 1.33.11 - 2026-10-26

### Fixed

- Required operation parameters were checked only by unit tests. `engine/preview` now rejects a request that omits a required value, using the registry schema, before any operation-specific work starts. The error key is `error.param.required.<name>`, the same key the operation form already localizes.
- Nine required parameters that had no message in any catalog (`color`, `duplex`, `on_error`, `orientation`, `paper`, `retries`, `scaling`, `scan_mode`, `timeout_minutes`) now have one in Turkish, English, Russian, and Azerbaijani.

### Notes

- The browser mock bridge still does not run this check. Its demo flows intentionally fall back to sample paths for empty values, so a mock-side check needs those tests updated first.

## 1.33.10 - 2026-10-26

### Documented

- `docs/KNOWN_ISSUES.md` records that the Outlook selected-message archive preview has no operator route or roster entry. The code stays as roadmap scaffolding.

## 1.33.9 - 2026-10-26

### Removed

- Stale `#[allow(dead_code)]` attributes on items that are now used in the non-test build; the attributes were no longer needed.
- `preview_outlook_reply_draft` and its `OutlookReplyDraftValues` value type, which the live reply path (`preview_rfi_reply`) superseded.

### Documented

- The nine remaining `dead_code` allows cover replica publication (accepted design, not active) and the Outlook archive preview (no dispatch route yet). Each carries a one-line reason above it.

## 1.33.8 - 2026-10-26

### Fixed

- The S5 journal's cross-process mutation lock no longer fails spuriously in two cases. A store that already holds the lock on a thread can take it again on that thread, so nested journal writes inside one apply do not contend with themselves. Other store instances on the same thread still contend, as before. A contended acquire also waits up to about 200 ms before reporting `operation_in_progress`, which covers a lock file descriptor a child process inherited while it was still being spawned. Real holders still fail after that window.
- Regression coverage: `journal::tests` covers same-store re-entry, exclusion from other threads, release after the last permit, and a short hold being waited out. The full Rust suite passed 150 consecutive runs, where the flake had failed about 4–6% of runs.

## 1.33.7 - 2026-10-26

### Changed

- The browser-only `MockBridge` and its helpers move from `frontend/src/lib/bridge.ts` to `frontend/src/lib/mockBridge.ts`. `bridge.ts` drops to about 115 lines and still re-exports `MockBridge`, so existing imports are unchanged.

## 1.33.6 - 2026-10-26

### Changed

- `plan/EditablePlanTable.tsx` (1104 lines) moves its date helpers to `plan/editorDates.ts` and the `CompactDateInput` component to `plan/CompactDateInput.tsx`. The table file drops to 960 lines. Exports used by other screens are unchanged.

## 1.33.5 - 2026-10-26

### Changed

- `operations/process_scans.rs` (1697 lines) splits into `process_scans/ocr.rs` (OCR read types and the `OcrAdapter` trait) and `process_scans/editor.rs` (scan-editor attachment and `revise`). Public paths such as `process_scans::OcrRead` and `process_scans::revise` are unchanged, and the parent file shrinks to 1328 lines.

## 1.33.4 - 2026-10-26

### Fixed

- The sign-in and sign-up email fields show a mail icon instead of a user icon, and the password field shows a lock icon instead of a shield. Labels and behavior are unchanged.

## 1.33.3 - 2026-10-26

### Changed

- Scan-index lookup, record, and failure methods take one `ScanScope` instead of four identity strings. System-sync preview takes a `SystemSyncScan` options struct, and signed-page archive matching takes a `SignedSource` struct. Three functions keep `clippy::too_many_arguments` allows on purpose: `registry::parameter` (about 64 call sites), `push_warning_review` (12), and `execute_retained_publication`, which is a publication safety boundary.

## 1.33.2 - 2026-10-26

### Changed

- The Review, Queue, History, Recovery, and Admin screens are loaded with `React.lazy` on first visit. The main frontend chunk drops from about 351 kB to 337 kB (minified). Login, Home, Operation, and Settings stay in the main chunk.

## 1.33.1 - 2026-10-26

### Removed

- Two never-called helpers marked `#[allow(dead_code)]` are gone: `discover_layout` in `rfi_follow_up/scan.rs` and `signed_route_key` in `rfi_follow_up/routing.rs`. Other retained `dead_code` items are deliberate scaffolding for accepted or staged features and stay.

## 1.33.0 - 2026-10-26

### Added

- `tools/dto_contract.test.mjs` compares the top-level fields of the Rust serde camelCase DTOs the webview receives with the matching TypeScript interfaces in `frontend/src/types.ts`. It runs in `npm test` and fails on any missing, extra, or renamed field.

## 1.32.3 - 2026-10-26

### Changed

- Browser-only demo operation definitions moved from `frontend/src/lib/bridge.ts` to `frontend/src/lib/demoOperations.ts`. The bridge drops from about 1900 to 1190 lines. Behavior is unchanged.

## 1.32.2 - 2026-10-26

### Fixed

- The frontend `RecoveryEntry` type lists the optional `warningKey` and `locator` fields that the Rust journal recovery DTO already sends.

## 1.32.1 - 2026-10-26

### Fixed

- The Rust test CI job no longer runs `cargo clean` and `cargo check` after `cargo test`, which had forced a full rebuild for no added coverage. The clippy job still covers all targets.

## 1.32.0 - 2026-10-26

### Added

- `npm run gate` runs every verification gate listed in `AGENTS.md` in order and stops at the first failure: version contract, docs audit, source audit, Node and React tests, build, Rust format, clippy, tests, check, and license audit.

## 1.31.1 - 2026-10-26

### Fixed

- The Quality workflow also runs on pushes to `main`, so version, source, frontend, and Rust gates check direct commits as well as pull requests.
- A tracked `.githooks/pre-commit` runs the version contract and source audit before each commit. Enable it once with `git config core.hooksPath .githooks`.

## 1.31.0 - 2026-10-26

### Added

- Follow Up sync accepts an optional backup folder; the backup copy is written there (granted read-write, recorded in the journal) instead of beside the workbook. Journal recovery accepts a backup in a recorded read-write root. `DOCFLOW_DEBUG_S5` prints publication-step failures to stderr.

## 1.30.0 - 2026-10-26

### Added

- Follow Up keeps all 21 planned Roads on the Road sheet even when no RFI names them yet, and fills each ZONE (Roads 1-5 North, 6-13 Center, 14-20 South, 21 North), including empty ZONE cells of existing rows (ADR 0050).

## Earlier releases

Older entries are archived verbatim: [`1.0`–`1.29`](docs/changelog/1.0-1.29.md) and [`0.x`](docs/changelog/0.x.md).

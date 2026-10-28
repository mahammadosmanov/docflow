# Changelog

All notable DocFlow changes are recorded here. The project follows Semantic
Versioning; `releases.json` is the machine-readable release catalog.

## Unreleased

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

## 1.29.1 - 2026-10-26

### Fixed

- Follow Up row insertion no longer writes references past Excel's last worksheet row (a whole-column conditional-format or data-validation range such as `P1:P1048576` became `P1:P1048577`), which made Excel report a sheet XML error and repair the Road sheet. References are clamped, and the pre-publish check now rejects any reference outside the grid.
- Fast RFI Scan confirms a cover whose identity line `RFI No: <code>` is the only labeled code even when other RFI codes (references, remarks) also appear on the page. Several unlabeled codes, or several labeled ones, still stay unconfirmed.

## 1.29.0 - 2026-10-26

### Fixed

- Rewritten worksheets are now checked before anything is published: parseable XML, ascending unique rows and cells whose references match their position, no empty formula, no duplicated attributes, no forbidden characters. A rewrite that fails is refused instead of producing a file Excel must repair.
- Road part rows are shown as ticked rows that are applied together with the sync, instead of unselectable rows that looked like manual review.

### Added

- The activity log's cover-page trace also records the identity-looking line the OCR read, to diagnose scans whose RFI number is not confirmed.

## 1.28.0 - 2026-10-25

### Fixed

- Follow Up remarks are decided cell by cell: only RFI cells holding several RFIs with at least one not Completed contribute, and only their RFIs are listed. A remark no longer lists Completed RFIs from unrelated cells of the row (ADR 0049).

### Added

- Follow Up maintenance: empty formula cells in a formula-driven column of CIV, PCU, or Road (for example missing `Y`/`Z` on Road rows) are filled from the column's formula as reviewable rows. Typed values and existing formulas are never changed.
- The activity log records why a scanned cover page was not confirmed (cue flags and text length only), to diagnose unreadable scans.

## 1.27.0 - 2026-10-24

### Added

- Road part rows: Follow Up inserts missing `Road NN (Part k)` rows, relabels a partless Road row to `Part 1` when the Log names parts, and places new Roads in number order. New rows are cloned from their neighbour with formulas, styles, and all references (formulas on every sheet, ranges, merges, conditional formatting, filter, print area) shifted like Excel's Insert Row. Rows are never removed (ADR 0048).
- Mass-deletion brake: if most Follow Up RFIs are missing from the selected RFI Log, removal is paused with a warning instead of proposing thousands of removals.

## 1.26.0 - 2026-10-24

### Added

- Follow Up generated row remarks on CIV, PCU, and Road: when a cell holds several RFIs and at least one is not Completed, the row's Remarks lists every RFI left to right, Completed first (bold, underlined, Accent 5) then In Progress in plain text, for example `CIV-0021,0022 Completed CIV-0043 In Progress`. A generated remark is cleared once every RFI is Completed. Operator-written remarks are never changed (ADR 0047, supersedes ADR 0028).

## 1.25.0 - 2026-10-24

### Added

- Follow Up understands Road parts: an RFI whose Log tag names a Road without a part is written to every part row of that Road (including parts added later), a tag with a part goes only to that part, and lists such as `Road 15 (Part 1), (Part 2)` go to each listed part (ADR 0046).
- An RFI that no longer exists anywhere in the RFI Log is removed from its Follow Up cell in the normal reviewed sync. Bare numbers present in the Log and placeholder numbers are kept.

## 1.24.3 - 2026-10-24

### Fixed

- Follow Up percentage indicators (for example the Road sheet's completion and overall columns) now refresh after Apply. Apply sets `fullCalcOnLoad` on the workbook's calculation settings so Excel recalculates once on open; that attribute is the only workbook-part change allowed and is verified (ADR 0045).

## 1.24.2 - 2026-10-24

### Fixed

- Follow Up Apply no longer stops in recovery-required on remote or network RFI System folders. Flushing a directory handle is unsupported there (Windows error 1 or 50) and is now tolerated; all hash, identity, backup, and journal checks still run (ADR 0044).
- Activity log details for Follow Up review rows are no longer hidden when a tag contains a slash.

## 1.24.1 - 2026-10-24

### Fixed

- When a Follow Up Apply ends in recovery-required, the activity log now records the exact publication step that failed and its OS cause (for example `sync_parent: ...`), so a failure on a remote or mapped folder can be diagnosed from one exported file. Behavior and journal formats are unchanged.

## 1.24.0 - 2026-10-24

### Added

- Local activity log: every backend call is recorded with its error key, OS cause, warnings, and Manual Review reasons (counts plus samples with RFI and ITP/item details). Settings > Advanced support > Save activity log exports one redacted file to share for support (ADR 0043).

## 1.23.4 - 2026-10-24

### Fixed

- Follow Up sync ignores RFI Log rows that contain only an RFI No. (pre-numbered placeholders). Thousands of them used to appear in Manual Review as unmapped RFIs. A row with any status, ITP, item, tag, area, location, or remark is still a record (ADR 0042).

## 1.23.3 - 2026-10-24

### Fixed

- Follow Up sync no longer stops at 5,000 reviewable rows. A Follow Up needing 8,406 changes now previews; the review-row cap is 20,000 and the existing 32 MiB preview-size guard still applies (about 15 MB for 8,406 rows, so roughly 17,000 rows fit). Every other safe-processing limit is unchanged.

## 1.23.2 - 2026-10-24

### Fixed

- Follow Up sync now says which safe-processing limit was exceeded (for example the sheet name and the row it reaches versus the allowed row) instead of the generic "exceeds the safe processing limits" message. The limits themselves are unchanged.

## 1.23.1 - 2026-10-24

### Fixed

- Follow Up sync now reports the operating-system cause (for example `os error 5` or `32`) when it cannot open a workbook for writing, and explains when the filesystem reports the same file identity for the RFI Log and Follow Up. Previously every cause appeared as a generic "close the workbook" message, which hid failures on remote or mapped folders.

## 1.23.0 - 2026-10-24

### Added

- Publish status (`rfi.publish_status`): choose the folder where a requester's PDF and DOCX attachments were saved and the RFI Log; Preview pairs files by the exact RFI code in each name and proposes `Ongoing` only for literal blank Inspection Status cells. Filled statuses, missing Log rows, incomplete pairs, ambiguous names, and formula or merged cells authorize nothing. Apply and Undo reuse the guarded workbook pipeline (ADR 0041).
- Localized publish-status text in Turkish, English, Russian, and Azerbaijani.

## 1.22.0 - 2026-10-23

### Added

- Cover-first identity reading for Stage signed first pages: a confirmed fast cover read replaces the full-page OCR cascade, with automatic fallback to the full read for any page it cannot confirm (ADR 0040).
- Durable OCR page cache for Fast RFI Scan, Group Files, and Stage signed first pages: unchanged files are not re-read by later or Action Queue Previews.

## 1.21.1 - 2026-10-23

### Fixed

- Action Queue now shows which watched operations still have no remembered folders and offers a "Choose folders" button that opens the operation, instead of silently doing nothing until folders were saved elsewhere.

## 1.21.0 - 2026-10-23

### Added

- Action Queue: an opt-in, read-only inbox watcher that checks the saved folders of Fast RFI Scan, Group Files, Stage signed first pages, and Completion Status Sync, waits for files to settle, and prepares the normal Preview in the background. Ready previews are listed in a new Action Queue screen with a navigation count; Open Preview, Dismiss, and Prepare again are provided. The watcher never applies, writes, prints, or sends, and pauses while any operation runs (ADR 0039).
- Localized Action Queue text in Turkish, English, Russian, and Azerbaijani.

## 1.20.8 - 2026-10-21

### Improved

- Load locale catalogs on demand instead of bundling all four; the main JavaScript bundle drops from 685 kB to 345 kB (187 kB to 100 kB gzip) and only the active language loads at startup.

## 1.20.7 - 2026-10-21

### Improved

- Extract the per-PDF scan step from the Process Scans preview into its own function with explicit context and state types; preview_with_cover_mode drops from 845 to 338 lines with identical behavior.

## 1.20.6 - 2026-10-21

### Improved

- Apply behavior-preserving Clippy pedantic fixes (redundant closures, let-else, map_unwrap_or, identical match arms, explicit Default calls) across the Rust engine.

## 1.20.5 - 2026-10-21

### Security

- Update Tauri API/CLI to 2.12, the Tauri Rust crates and other Cargo dependencies to current compatible releases, and Vite to 8.3.5 (clears the high-severity launch-editor advisories); npm audit now reports 0 vulnerabilities.

## 1.20.4 - 2026-10-21

### Improved

- Add an opt-in OOXML timing probe (cargo test --release --test perf_ooxml -- --ignored --nocapture). On a 20,000-row by 12-column workbook it reads 240,000 cells in about 0.27 s and patches and verifies 200 guarded cells in under 2 s, so no parser hot spot needs changing.

## 1.20.3 - 2026-10-21

### Improved

- Split oversized Rust and React modules into focused submodules with unit
  tests in sibling files; behavior is unchanged.
- Build releases with fat LTO and stripped symbols (macOS binary 25.5 MB to
  16.0 MB) and reduce debug info for dev and test builds.
- Add a 2,500-line source file size guard and document the source layout.

## 1.20.2 - 2026-10-15

### Fixed

- Use writable handles for Windows PDF fixtures that preserve modification
  timestamps while changing test content.

## 1.20.1 - 2026-10-15

### Fixed

- Replace the manual Excel date-serial bounds with an inclusive range check
  accepted by the pinned Windows Clippy toolchain.

## 1.20.0 - 2026-10-15

### Improved

- Apply a unified monochrome design system across the existing workspace,
  operation, review, settings, history, recovery, and administration screens.
- Improve translated-text wrapping, high-scale readability, responsive action
  areas, and narrow-window fit without changing operation behavior.

## 1.19.0 - 2026-10-14

### Improved

- Route each exact CIV `Tag No` token to its distinct prepared Follow Up row.
  Partial tag coverage syncs eligible rows and shows an unselectable diagnostic
  naming matched and unmatched tags; duplicate targets remain Manual Review.
  Preserve CIV `PCS-` exclusions and never create target rows.


## 1.18.0 - 2026-10-14

### Improved

- Match regular and Worley-signed MEC Follow Up records by exact Block/Section
  pairs, ignore Inspection Location, and fan each distinct Area section from
  one RFI out to its corresponding existing row. No rows or sections are
  created; ambiguous pairs remain in Manual Review.

## 1.17.0 - 2026-10-14

### Improved

- Add persistent navigation across authenticated workspace, review, history,
  recovery, preferences, and administrator screens. Feature Fast RFI Scan as
  the primary home task while keeping other operations searchable and
  category-filtered. Use an icon rail or compact bottom bar at narrower
  widths; workflow preview, Apply, and Undo authority remain unchanged.

## 1.16.8 - 2026-10-14

### Fixed

- Omit supported Follow Up records with no source context matching any prepared
  destination from Manual Review, leaving existing workbook cells unchanged.
  Keep partial matches and contexts with unsafe candidate rows reviewable using
  exact matched/unmatched context details and specific target-safety reasons;
  never create destinations or edit the source RFI Log.
- Enforce the 21 approved ITP/item scope exclusions without review rows and
  remove existing occurrences only through the approved sync.

### Added

- Batch generation of CON-CIV-010/011/013/014 from RFI Log and Follow Up
  workbooks, with configurable temperature intervals and optional water/cement
  ratios. Only RFIs whose Civil Forms field includes CON-CIV-010 require Follow
  Up data; ineligible forms and missing rows warn per RFI. Existing outputs are
  never overwritten, and applicable documents remain available.


## 1.16.7 - 2026-10-14

### Improved

- Distinguish unsupported ITP/item routes from supported routes missing a
  prepared exact-context Follow Up row. Manual-review details now show the
  source route/context and any candidate destination rows without enabling
  guessed workbook updates.
- Record confirmed ITP/item exclusions as intentional Follow Up scope, not
  mappings to add. Future route changes require an approved domain decision.

## 1.16.6 - 2026-10-13

### Fixed

- Remove the artificial 500-cell cap from guarded workbook updates so Follow Up
  Sync can apply complete legitimate batches; existing workbook-size,
  worksheet-coordinate, exact-cell, protection, and merge safeguards remain.


## 1.16.5 - 2026-10-13

### Fixed

- Route regular MEC Follow Up updates by exact tag, area, and inspection
  location, including distinct destinations that share an area. Sync only
  prepared exact contexts and surface uncovered source contexts for manual
  review; never create rows or authorize duplicate/ambiguous destinations.


## 1.16.4 - 2026-10-13

### Fixed

- Route Worley FINAL updates to each distinct prepared exact-context destination
  instead of treating an existing RFI at one destination as proof that all
  destinations are complete. Missing prepared contexts remain visible for
  manual review; no rows or sections are created.
- Keep ambiguous, conflicting, or unrelated existing placements from authorizing
  shared-cell status changes. An already-present RFI with a cached formula
  status is a no-op only when that status already matches the projected value.


## 1.16.3 - 2026-10-13

### Fixed

- Prevent generic OCR words in direction zero from ending Fast RFI Scan's
  search for a confirmed cover in the remaining cardinal directions. Eligible
  pixel-baked CIV-1158 covers at 0°, 90°, 180°, and 270° now reach the same
  RFI identity and rotation target in Fast RFI Scan and standalone orientation;
  uncertain/no-code Fast Scan pages retain the unchanged-copy review path.
- Pin source SHA-256 before parallel Multi-RFI orientation/OCR analysis and
  recheck it afterward. A source replaced during analysis is rejected before
  an actionable Plan can be created.


## 1.16.2 - 2026-10-13

### Fixed

- Align standalone PDF orientation and Fast RFI Scan on one canonical rotation
  policy: normalize existing cardinal `/Rotate` metadata before applying
  render-relative corrections; already-upright pages remain idempotent, while
  non-cardinal metadata is never corrected. Unresolved standalone PDFs receive
  a manual-review warning and no output; Fast RFI Scan routes an unchanged copy
  to `_review`. Single-RFI mode keeps the complete PDF intact without rotation.
- Bind orientation analysis and cached decisions to source content: pin the
  source hash before inspection/detection, verify it afterward and on cache
  reuse, and advance the cache profile so older decisions are not reused.
- Permit a narrow orientation fallback when one exact canonical RFI identifier
  appears in the page header at both 250 and 350 DPI with the same direction.
  Competing, noncanonical, or conflicting evidence remains uncertain for review.

### Improved

- Give schema-generated switches accessible names, including the page-rotation
  control. Make the standalone rotation operation discoverable with explicit
  localized rotation wording.


## 1.16.0 - 2026-10-12

### Improved

- Rework the shared workspace for available window width and height so workflow
  content and primary actions remain reachable across narrow and short views.
  Forms and paths wrap for reading; wide tables keep readable minimum widths and
  their own horizontal scrolling rather than compressing columns.
- Add pointer and keyboard adjustment for data-region heights and table columns,
  with Fit and Reset controls. These transient presentation adjustments do not
  change application preferences, document data, or native authority.
- Fit keeps stage summaries in normal flow rather than overlaying stage data.
- Escape closes an open adjustment dialog without discarding its pending
  presentation changes; with no dialog open, the existing Escape-to-Configure
  behavior is preserved.


## 1.15.1 - 2026-10-12

### Fixed

- Normalize one-to-four-digit numeric suffixes only within anchored supported
  RFI identifiers, padding canonical output to four digits without truncating
  longer values or rewriting unrelated stored data.
- Fast RFI Scan accepts only consensus-confirmed Strict or approved OCR-alias
  identities; its narrow independent cover rescue does not override a stable
  text-layer identity or conflicting RFI.
- Signed-page staging accepts high-confidence consensus-confirmed Strict or
  approved OcrAlias identities, and resolves archives only by the exact RFI PDF
  basename. Equivalent short/four-digit archive names remain ambiguous;
  Sketch/DOCX and other filename-only evidence do not count, and existing
  destinations are never overwritten.

## 1.15.0 - 2026-10-11

### Added

- Batch Print can find exact First Page `.docx` and Sketch `.pdf` documents
  recursively in the selected RFI System using canonical RFI codes,
  discipline-number inputs, or four-digit numbers. Missing types and multiple
  versions are shown; ambiguous bare numbers require explicit selection, and
  only enabled choices enter the existing Batch Print preview and queue.

### Fixed

- RFI System Sync now treats insufficient signature evidence as unsigned
  without promoting the status to `Signed`; safe existing control statuses may
  be set to `Not Signed`, but an existing `Signed` status is never downgraded.
- Follow Up records without a matching prepared target row remain visible for
  operator review and are never silently discarded or automatically synced.

## 1.14.0 - 2026-10-10

### Changed

- Group Files now accepts only CIV, MEC, ELE, and PRC RFIs and files approved
  copies under `<destination>/<discipline>/<full-code>/<source filename>`.
  Sources remain read-only; Preview, explicit Apply, journaled creation,
  guarded Undo, per-RFI mail eligibility, multi-RFI fan-out, `_review`, and
  no-overwrite behavior are preserved.
- Retire Process Scans from the active operation list; Fast RFI Scan now offers
  Multi-RFI separation and a Single-RFI mode that OCRs only page one while
  keeping the complete PDF intact.
- Normalize only exact OCR `CLV` reads to `CIV`, expose alias evidence, and let
  operators correct canonical CIV/MEC/ELE/PRC codes in Preview. Rust derives
  the new destination; invalid, duplicate, or existing targets cannot be
  applied.
- Localize the scan mode and correction workflow in EN/TR/RU/AZ. A changed
  scan preview must be updated and reviewed before the separate Apply action.


## 1.13.10 - 2026-10-08

### Documentation

- Clarify that RFI System replica publication is not exposed in the active application; retain its requirements and design records as non-shipped intent.


## 1.13.9 - 2026-10-08

### Documentation

- Correct the registered-operation catalog to list `rfi.follow_up_sync` once and describe its status synchronization behavior.


## 1.13.8 - 2026-10-08

### Fixed

- Allow safe page separation of Epson Scan PDFs whose XMP metadata stream uses an indirect `/Length`; metadata remains copied and validated, while encrypted, signed, embedded-file, and other unsupported PDF features remain rejected.

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

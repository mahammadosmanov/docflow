# Flutter migration handoff (2026-10-20)

Start here when continuing the migration in a new session. Read this file,
then `flutter/MIGRATION_CHECKLIST.md`. `flutter/WORKER_BRIEF.md` has the
porting rules, but its Linux paths are outdated; use the paths below.

## Setup on this Mac

- Branch: `flutter-migration`. Work only here. Never touch `main` or other
  branches.
- Checkout: `/Users/rohtolos/Documents/GitHub/docflow-flutter-migration`.
  The main checkout `/Users/rohtolos/Documents/GitHub/docflow` holds another
  session's uncommitted Flutter work. Do not use it.
- Toolchain: `export PATH=$HOME/flutter-stable/bin:$PATH`. That is Flutter
  3.47.7 with Dart 3.13.5. The Homebrew `flutter` ships Dart 3.12, which is
  too old for `pubspec.yaml`.
- App: `cd flutter/app`, then `flutter analyze` and `flutter test`.
- CodeGraph: `.codegraph/` is indexed in this checkout and excluded locally
  through `.git/info/exclude`. Use `codegraph_explore` with
  `projectPath=/Users/rohtolos/Documents/GitHub/docflow-flutter-migration`.
- Xcode is not installed, so `flutter build macos` fails. Analyze and tests
  run without it.
- `frontend/` is deleted on this branch. Read the React source with
  `git show 5c778e1:frontend/src/App.tsx | sed -n 'A,Bp'`.
- Token budget is tight. Read only the Rust ranges you port. Run at most two
  subagents at a time.

## State at handoff

- `flutter analyze`: no errors or warnings. A few info lints remain.
- Last full `flutter test` run: 692 passed, 46 skipped, 1 failure (the
  paths-test fixture, fixed right after). That run predates the plan UI,
  rfi_system_sync and home/operation tests, which pass on their own
  (7 + 7 + 6). Re-run the full suite as the first step.
- Done this session:
  - Fixed the analyzer errors in `home_screen.dart` and `operation_screen.dart`.
  - Fixed arm64 macOS FFI: plain `fstat`/`stat` when the `$INODE64` aliases
    are absent, and `VarArgs` for the `open`/`openat` mode.
  - Added a `.gitignore` negation so `flutter/app/test` is tracked.
    `Test/` matched it case-insensitively on macOS.
  - Added widget tests for the home and operation screens
    (`test/ui/screens/home_operation_test.dart`).
  - Added `lib/engine/platform/platform_services.dart`
    (`ProductionPlatformServices`): native pickers through
    AppleScript, PowerShell or zenity, plus the OS opener, clipboard,
    startup and OCR support.
  - Ported the plan UI to `lib/ui/plan/`: `PlanView`, `planPreview` and
    `AppStageViews`, with tests in `test/ui/plan/`.
  - Ported `lib/engine/rfi_system_sync.dart` with tests in
    `test/engine/e7_rfi_system_sync_test.dart`.
  - `lib/main.dart` registers every screen and runs
    `buildDocFlowEngine(platform: ProductionPlatformServices())`. It also
    handles `--package-smoke`.

## Remaining work, in order

1. **Native command router (largest item).** Create
   `lib/engine/native.dart` with `class NativeBackend implements
   NativeOperations`; the interface is in `lib/engine/backend.dart`. Port
   `src-tauri/src/native.rs`:
   - Composition: `production()` at lines 245-330.
   - Path materialization: `materialize_preview` and
     `materialize_operation_values` at 978-1053, `recovery` at 1054.
   - Previews at 1062-2333. Dispatch by opId comes from `backend.rs`
     493-530:
     - `rfi.process_scans_fast_covers` and `rfi.group_files`
     - `rfi.log_add` and `rfi.follow_up_sync`
     - `rfi.generate_concrete_documents`
     - `rfi.system_sync` and `rfi.complete_status_sync`
     - `rfi.stage_signed_first_pages` and `rfi.normalize_pdf_orientation`
     - `files.batch_print`
   - Outlook previews: `preview_outlook_reply_draft` and
     `preview_outlook_archive`.
   - `find_rfi_print_files` at 2210.
   - Replica previews: `preview_replica_publish` and `cache_replica_plan`
     at 2314-2415.
   - `revise_native_plan` at 2416, `copy_native_table` at 2495,
     `preview_rfi_reply` at 2510.
   - `apply_native_plan` at 2535, cancel at 2639-2648.
   - `review_scan` and `review_preview` at 2649-2733, `try_native_undo` at
     2734.
   - Helpers at 2832-3428.
   
   Rust tests are at 3429 and later; port them too. The Dart preview
   functions already exist, for example `previewFastCovers`,
   `previewRfiFileGroup`, `rfi_log.preview`, `previewFromOpenFiles`,
   `previewSystemSync`, `previewCompleteStatusSync`,
   `previewStageSignedPages`, `previewOrientation`,
   `batch_print.preview`, `previewReplicaPublish`, and the
   `OutlookDraftService` / `PrintService` methods. `FilesystemAuthority`
   in `fs_ops.dart` already has grants, `cachePlan`, `apply`, `undo` and
   `recoverPending`.
   
   Then pass `native:` and `history: CloudHistory(...)` in
   `buildDocFlowEngine` (`lib/engine/lib.dart`) and in `main.dart`.
2. **Excel tail:** the cancel/timeout section of `platform/excel.rs`, lines
   3181-5428, into `lib/engine/platform/excel.dart`.
3. **E2 test bodies.** These tests are skipped with empty bodies and need
   the Rust bodies ported now that the authority exists:
   - `e3_journal_s5_test.dart`: 3 tests (`_authorityBlocked`). The Rust is
     in `journal.rs` 2730-2950.
   - `e9_stage_signed_test.dart` and `e9_orientation_test.dart`:
     `_applySkip`.
   - `e10_process_scans_test.dart`: `_skipApply` and `_notPortedYet`.
   - `e8_native_part8_test.dart`: 3 tests.
4. **E11:** finish the `platform/windows/printing.dart` and
   `platform/macos/printing.dart` adapters and port
   `src-tauri/tests/batch_print.rs`.
5. **PDF page renderer.** `ocr.dart` `selectPdfPageRenderer` returns an
   `UnverifiedNativePdfPageRenderer` on macOS and Windows, so OCR and
   signature rendering do not work on real machines. Port the PDFKit and
   Windows PDF renderers, which also un-skips the 4 rfi_system_sync render
   tests.
6. **Release:**
   - Bump the version with `node tools/version_manager.mjs set <next>`.
     Keep `docflowVersion` in `lib/engine/backend.dart` equal to `VERSION`.
   - Add a docs note that the Dart engine supersedes the earlier
     "no second backend" decision (`docs/DECISIONS.md`).
   - Push, then open a PR from `flutter-migration` to `main`. Its body starts
     with the attribution block and lists parity coverage and the honest
     UNVERIFIED gaps: Windows/macOS/Office/printer.

## Known gaps (UNVERIFIED)

- Windows COM (Excel, Outlook), Win32 printing, the Windows startup registry
  and the Windows PowerShell picker have not run on a Windows host.
- macOS PDFKit rendering is not ported. The AppleScript picker has not been
  tried interactively.
- `print_queue` preview/apply signature defect: preview uses microsecond
  mtime and apply uses nanosecond mtime. See `_previewApplySignatureDefect`
  in `e11_platform_test.dart`.
- `pubspec.yaml` needs Dart ^3.13.5. `desktop_drop` 0.9 conflicts with
  `file_selector`, so no `file_selector` is used.

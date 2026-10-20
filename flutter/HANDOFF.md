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

1. **Native command router: DONE (1.20.3).** `lib/engine/native.dart`
   (`NativeBackend`) composes the production backend (`production()`), and
   `buildDocFlowEngine` wires it with `history:` from `cloud`. Ported:
   path materialization, all preview dispatch (process scans, RFI file group,
   RFI log, follow-up, system sync, complete status, stage signed pages,
   orientation, Outlook archive, replica publish, concrete documents,
   batch print, Outlook reply), revise, copyTable, rfiReplyPreview,
   reviewScan/reviewPreview, findRfiPrintFiles, apply/cancel/undo, print
   queue, recovery, cloud sync. OCR is async: `RfiOcrAdapter` and
   `ReviewOcrAdapter` return `Future`, bridged over `NativeOcr`. Windows uses
   `WindowsOutlookAdapter` with `Win32ComDispatchBackend`.
   Remaining gaps: none of the native previews has a happy-path test on real
   fixtures (only error paths and an empty process-scans run). Every
   Windows/COM path is UNVERIFIED on a Windows host.
   Also: `NativeBackend.production` takes `authConfig` explicitly.
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

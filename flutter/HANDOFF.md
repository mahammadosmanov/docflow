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
2. **Excel tail: PARTIAL (1.20.4).** The Dart pipeline in `platform/excel.dart`
   already had the cancel/timeout logic. 1.20.4 ports 4 more Rust tests into
   `e12_excel_pipeline_test.dart`: error-response and received-response
   cleanup review, cancelled failed-join loser cleanup review, and the
   acquisition deadline. Still not ported from `excel.rs` 3181-5428: the
   cancelled-waiter and acquisition-contention tests (need real concurrency),
   `timeout_late_result_*`, `timeout_live_*`, the `completed_*_loser` tests,
   `*_response_poll_*`, and the PrintService-backed cancel tests (3826, 4287).
3. **E2 test bodies: PARTIAL (1.20.10).** Filled with real flows (1.20.10 adds
   the retained apply revalidation in `e8_native_part6`, orientation apply and
   source-drift tests, stage-signed archive-drift and changed-input apply tests):
   `e8_native_part8` (follow-up apply of 501 guarded updates, retained undo,
   retained recovery; real xlsx via `e8_xlsx_writer.dart`) and
   `e3_journal_s5` (failed-apply undo, user-initiated restore recovery, no
   clobber; real files on disk). This fixed a real bug: the S5 undo lease
   (`process` + interprocess permits) was never released on exit, so the next
   undo failed with `operation_in_progress`. Still skipped, with accurate
   reasons: `e9_stage_signed` cancel-mid-apply, pending-journal recovery and
   same-bytes temp-copy cases (Dart `apply` exposes no progress/cancel hook
   yet), and `e10_process_scans` apply/undo, orientation and parallel-analysis
   tests (about 25 cases, still to port).
   The Windows-only retry tests stay UNVERIFIED.
4. **E11: PARTIAL (1.20.6).** The Windows and macOS printing adapters are
   already ported (`platform/windows/printing.dart`, `platform/macos/printing.dart`),
   both UNVERIFIED on real hosts. 1.20.6 ports 5 of the 64 tests in
   `src-tauri/tests/batch_print.rs` (validation, legacy and persisted job
   metadata, forged page range, unknown printer and unsupported options).
   Remaining: the other 59 batch-print tests (preview/revise/resume/queue).
   `WindowsControlledPdfSubmitter` still does not `implements
   ControlledPdfSubmitter`, because excel.dart uses the stub `PrintJob` type.
5. **PDF page renderer: macOS DONE, Windows UNVERIFIED (1.20.9).**
   macOS: `lib/engine/pdfkit_renderer.dart` compiles an embedded Swift PDFKit
   helper with `xcrun swiftc` into the user cache, then uses PDFKit for page
   text and PNG rendering. Tested here. Poppler (`pdf_tool_renderer.dart`) is
   the fallback when xcrun is missing.
   Windows: `lib/engine/windows_pdf_renderer.dart` drives Windows.Data.Pdf
   through PowerShell and fails closed when unavailable. UNVERIFIED: no
   Windows host here. Page text is empty (no text API in Windows.Data.Pdf).
6. **Release: PREPARED (1.20.8).** `docs/adr/0039` and its index row record that
   the Dart engine supersedes the "no second backend" decision. `flutter/PR_BODY.md`
   has the attribution, parity coverage and UNVERIFIED gaps. Nothing is pushed and
   no PR is open. Still to do before the PR: a push, a PR to `main`, and the
   Rust and Windows/macOS host checks listed in the PR body.

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

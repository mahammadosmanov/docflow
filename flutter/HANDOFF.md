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
2. **Excel tail: PARTIAL (1.20.12).** `e12_excel_pipeline_test.dart` now
   ports 41 cases of the Rust `excel.rs` 3181-5428 range that run without the
   print queue: cleanup review for error, received, cancelled and timeout
   losers, panic unwind, late results with bounded retry, live-loser breaker,
   retained workspace locator, response-poll cancel and deadline, shared
   operation fence, and cancelled waiter (real async concurrency via Completer).
   STILL OPEN (12 cases, Rust lines 3181, 3280, 3368, 3434, 3514, 3663, 3743,
   3826, 3952, 4033, 4226, 4287): all run through `PrintService` with an Excel
   pipeline. Dart blocks them on a real gap, not a test gap:
   `PrintService.apply` is synchronous and `ExcelExportPipelinePort` is
   synchronous, but `ExcelExportPipeline.exportAndSubmit` is `Future`-based.
   Production also never injects an Excel pipeline (`native.dart` constructs
   `PrintService` without `excelPipeline`), so Excel print jobs return
   `error.print.excelUnavailable` in the app. Fix: make the print apply/resume
   path async and wire a production pipeline (Windows COM exporter in
   `platform/windows/excel.dart`, UNVERIFIED on this Mac).
3. **E2 test bodies: DONE for Dart-portable cases (1.20.11).** Filled with
   real flows: `e8_native_part6`/`e8_native_part8` (follow-up apply, retained
   undo and recovery, real xlsx via `e8_xlsx_writer.dart`), `e3_journal_s5`
   (failed-apply undo, restore recovery, no clobber), `e9_orientation` (apply
   and source drift), `e9_stage_signed` (apply, cancel via `applyWithProgress`,
   pending-journal recovery, stale hardlink cleanup, unrelated-copy safety,
   archive and changed-input drift), `e10_process_scans` (combined-scan
   round trip, cancellation, never-clobber, duplicates/failures, orientation,
   parallel analysis, fast-scan revision, review decisions, native OCR with
   real Tesseract on this Mac). Real bugs fixed: S5 undo lease leak (1.20.6);
   process-scans source-hash recheck after OCR was missing (now `path_invalid`).
   Remaining skips are operator-supplied inputs, matching Rust `#[ignore]`:
   `e10` operator PDF (env `DOCFLOW_OPERATOR_PROCESS_SCAN_PDF` and
   `DOCFLOW_OPERATOR_PROCESS_SCAN_RFI`), `e9_stage_signed` and `e9_orientation`
   operator Ventoy and PDF cases (`pdf_orientation.rs` 97/122/162). The
   Windows-only retry tests stay UNVERIFIED.
4. **E11: DONE for Dart-portable cases (1.20.13).** The Windows and macOS
   printing adapters are ported (`platform/windows/printing.dart`,
   `platform/macos/printing.dart`), both UNVERIFIED on real Windows/printers.
   `test/engine/e11_batch_rust_test.dart` ports all 47 `#[test]` functions in
   `src-tauri/tests/batch_print.rs` (46 run, 1 env-gated: the Ventoy PDF,
   needs `DOCFLOW_VENTOY_PRINT_PDF`, same as the Rust `#[ignore]`). Rust
   thread gates become synchronous `FakeBatchAdapter` hooks (`onSubmit`,
   `onInventory`, `onInspect`); the reentrant resume test proves the queue
   reservation. Real bug fixed in 1.20.13: path-based print signatures used
   microsecond `File.stat` mtimes while apply-time checks used handle
   nanoseconds, so every PDF apply after preview failed with
   `error.print.failed`. Signatures now come from the capability handle.
   Open (separate): `WindowsControlledPdfSubmitter` still does not
   `implements ControlledPdfSubmitter`.

5. **PDF page renderer: macOS DONE, Windows UNVERIFIED (1.20.9).**
   macOS: `lib/engine/pdfkit_renderer.dart` compiles an embedded Swift PDFKit
   helper with `xcrun swiftc` into the user cache, then uses PDFKit for page
   text and PNG rendering. Tested here. Poppler (`pdf_tool_renderer.dart`) is
   the fallback when xcrun is missing.
   Windows: `lib/engine/windows_pdf_renderer.dart` drives Windows.Data.Pdf
   through PowerShell and fails closed when unavailable. UNVERIFIED: no
   Windows host here. Page text is empty (no text API in Windows.Data.Pdf).
6. **Release: PREPARED (1.20.14).** `docs/adr/0039` and its index row record that
   the Dart engine supersedes the "no second backend" decision. `flutter/PR_BODY.md`
   has the attribution, parity coverage, source gates re-run on this Mac, and the
   UNVERIFIED gaps. Nothing is pushed and no PR is open. Still open before the PR:
   the 12 Excel print-queue cases (see item 2) and the Windows and macOS host checks.
   Source gates re-run for 1.20.14: version, docs, package source, `npm test`
   (73 pass), `npm run build`, cargo fmt, clippy, check and test, and
   `npm run audit:licenses` (after staging the local macOS Tesseract bundle).

## Known gaps (UNVERIFIED)

- Windows COM (Excel, Outlook), Win32 printing, the Windows startup registry
  and the Windows PowerShell picker have not run on a Windows host.
- macOS PDFKit rendering is verified on this Mac only (1.20.9). The AppleScript
  picker has not been tried interactively.
- `pubspec.yaml` needs Dart ^3.13.5. `desktop_drop` 0.9 conflicts with
  `file_selector`, so no `file_selector` is used.

Attribution: drafted with Claude Sonnet 5.5 (Co-Authored-By on each commit).

## Summary

Migrates the DocFlow backend from Rust (`src-tauri/src`) to Dart under
`flutter/app/lib/engine`, on branch `flutter-migration`. Records the decision
that supersedes the earlier "no second backend" rule (`docs/adr/0039`).

## What changed (this branch, since the last handoff)

- `1.20.3` Native command router: `NativeBackend` (`lib/engine/native.dart`).
  OCR adapters are async and bridged over `NativeOcr`. Windows uses
  `WindowsOutlookAdapter` with the Win32 COM backend.
- `1.20.4` Excel export cancel and timeout supervisor tests.
- `1.20.5` Follow-up apply, retained undo, retained recovery and S5 restore
  tests over real files and real xlsx packages. Fixes the S5 undo lease,
  which was never released and blocked the next undo.
- `1.20.6` Batch print validation, persisted-metadata, forged-range and
  unknown-printer tests.
- `1.20.7` PDF page rendering through poppler (`pdftoppm`, `pdftotext`).
- `1.20.8` / `1.20.9` PDFKit page renderer (Swift helper compiled with
  `xcrun swiftc`, verified on this Mac). Windows PDF renderer through
  PowerShell, fail-closed and UNVERIFIED.
- `1.20.10` / `1.20.11` Stage-signed and orientation apply tests; process-scans
  apply, undo, orientation, parallel, revision, review and native OCR tests.
  Fixes: process scans now reject a source changed during OCR (`path_invalid`).
- `1.20.12` Excel cancel, timeout, late-result, poll, panic, fence and
  cancelled-waiter tests (41 cases in `e12_excel_pipeline_test.dart`).
- `1.20.13` All 47 `src-tauri/tests/batch_print.rs` cases
  (`e11_batch_rust_test.dart`, 46 run, 1 env-gated). Fixes a real bug: path
  print signatures used microsecond mtime while apply used nanoseconds, so
  every PDF apply after preview failed with `error.print.failed`.
- Release tooling: `tools/stage_tesseract_macos.sh` license scan now fails
  closed (it used `rg`, which made a missing scanner pass silently).

## Parity coverage

- Flutter: `flutter analyze` 0 errors (13 info-level lints).
  `flutter test`: 832 pass, 16 skipped.
- Rust reference suite (`cargo test`): all suites pass, 0 failed.
- Ported and exercised by Dart tests: contracts, path selection, filesystem
  authority and capability layer, journal and S5 undo/recovery, OOXML,
  concrete documents, RFI log, RFI follow-up, RFI system sync, RFI file
  group, process scans, stage signed pages, PDF orientation, replica publish,
  Excel export pipeline (non-print-queue cases), batch print (all cases),
  native router error paths.

## Source gates re-run on this Mac

- `node tools/version_manager.mjs check`: OK (1.20.13)
- `node tools/docs_audit.mjs`: OK
- `node tools/package_audit.mjs source`: OK
- `npm test`: 73 pass
- `npm run build`: OK (chunk-size warning only)
- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo check`, `cargo test`: OK
- `npm run audit:licenses`: OK (295 Cargo, 173 npm, 61 OCR checksums), after
  staging the local macOS Tesseract bundle with `tools/stage_tesseract_macos.sh`.

## Still open

- Excel print-queue: the 12 PrintService-backed Rust cases are ported (1.20.15).
  Production Excel pipeline is wired on Windows only; the COM path is UNVERIFIED
  on a real Windows host and must be checked there before release.
- Operator-only tests (Rust `#[ignore]`, need private files): process-scan
  operator PDF, orientation operator PDF/RFI tree, Ventoy print PDF, Ventoy
  group source, `e8_current_proof`. Each is env-gated with its reason.

## UNVERIFIED gaps (not run on a real host)

- Windows: COM Excel and Outlook, Win32 printing, startup registry, PowerShell
  picker, PowerShell PDF renderer. No Windows host was used.
- macOS: the AppleScript picker has not been tried interactively. PDFKit
  rendering is verified only on this Mac.
- Printers: no real CUPS or Windows printer submission has run.
- `WindowsControlledPdfSubmitter` does not implement `ControlledPdfSubmitter`
  yet (excel.dart uses the stub `PrintJob` type).
- Build: `flutter build macos` is not run (Xcode is not installed here).
- Package gates (host bundle build, `--package-smoke`, GUI launch) are
  release-time and not run.

## Test plan

- [ ] `cd flutter/app && flutter analyze && flutter test`
- [ ] Source gates from `docs/RELEASE_SAFETY.md` (listed above)
- [ ] Windows host run: Excel, Outlook, printing (UNVERIFIED items above)
- [ ] macOS host run with and without poppler installed

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Attribution: drafted with Claude Sonnet 5.5 (Co-Authored-By on each commit).

## Summary

Migrates the DocFlow backend from Rust (`src-tauri/src`) to Dart under
`flutter/app/lib/engine`, on branch `flutter-migration`. Records the decision
that supersedes the earlier "no second backend" rule (`docs/adr/0039`).

## What changed (this branch, since the last handoff)

- `1.20.3` Native command router: `NativeBackend` (`lib/engine/native.dart`)
  with path materialization, every preview dispatch, revise, copy table,
  reply preview, review scan and preview, RFI print file discovery, apply,
  cancel, undo, print queue, recovery and cloud sync. OCR adapters are async
  and bridged over `NativeOcr`. Windows uses `WindowsOutlookAdapter` with the
  Win32 COM backend.
- `1.20.4` Excel export cancel and timeout supervisor tests.
- `1.20.5` Follow-up apply, retained undo, retained recovery and S5 restore
  tests over real files and real xlsx packages. Fixes the S5 undo lease,
  which was never released and blocked the next undo.
- `1.20.6` Batch print validation, persisted-metadata, forged-range and
  unknown-printer tests.
- `1.20.7` PDF page rendering through poppler (`pdftoppm`, `pdftotext`) on
  macOS when installed.
- Earlier on this branch: the platform services, plan UI, `rfi_system_sync`
  port, and the screen registration in `main.dart`.

## Parity coverage

- Flutter tests: 743 pass, 44 skipped (`flutter test`). `flutter analyze`
  reports 0 errors.
- Rust suite: not re-run in this PR. The Rust crate is still the reference.
- Ported and exercised by Dart tests: contracts, path selection, filesystem
  authority and capability layer, journal and S5 undo/recovery, OOXML,
  concrete documents, RFI log, RFI follow-up, RFI system sync, RFI file
  group, process scans, stage signed pages, PDF orientation, replica publish,
  Excel export pipeline (cancel and timeout included), batch print
  validation and persistence, native router error paths.

## Still skipped or partial

- Batch print: 59 of 64 cases in `src-tauri/tests/batch_print.rs` are not
  ported (preview, revise, resume, queue, apply).
- Excel: the cancelled-waiter, contention, late-result, live-loser and
  response-poll cases in `platform/excel.rs` are not ported.
- Stage signed pages and PDF orientation apply tests (`e9_*`), and process
  scans apply, orientation and parallel-analysis tests (`e10_*`).
- Operator-only fixtures (`e7`, `e8_current_proof`, `e10_ocr`): need private
  workbooks, Ventoy documents or live Tesseract.

## UNVERIFIED gaps (not run on a real host)

- Windows: COM Excel and Outlook, Win32 printing, the Windows startup
  registry, the Windows PowerShell picker, and the Windows PDF engine
  renderer. No Windows host was used.
- macOS: PDFKit rendering is not ported. Page rendering falls back to poppler
  only when installed. The AppleScript picker has not been tried interactively.
- Printers: no real CUPS or Windows printer submission has run.
- `print_queue` preview/apply mtime defect: preview uses microsecond mtime and
  apply uses nanosecond mtime (`_previewApplySignatureDefect`).
- `WindowsControlledPdfSubmitter` does not implement `ControlledPdfSubmitter`
  yet (excel.dart uses the stub `PrintJob` type).
- Build: `flutter build macos` is not run (Xcode is not installed here).

## Test plan

- [ ] `cd flutter/app && flutter analyze && flutter test`
- [ ] Rust gates from `docs/RELEASE_SAFETY.md` on the Rust crate
- [ ] Windows host run: Excel, Outlook, printing (UNVERIFIED items above)
- [ ] macOS host run with and without poppler installed

🤖 Generated with [Claude Code](https://claude.com/claude-code)

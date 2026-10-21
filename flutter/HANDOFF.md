# Flutter migration handoff (1.21.0)

## Current state

- The Dart engine (`flutter/app/lib/engine`) is the sole application and
  mutation authority. The Tauri/Rust/React application, Node tooling, and
  Rust-era docs were removed in 1.20.16 (see the commit body for the list).
- `flutter analyze`: 0 errors. `flutter test`: 844 pass, 16 skipped
  (operator-only and env-gated cases).
- Locale sources moved to `flutter/app/tool/locales/` (ARB, four languages).
  Generated catalog: `lib/l10n/` via `dart run lib/l10n/generate_catalog.dart`.
- Frozen synthetic oracles moved to `flutter/app/test/fixtures/`.
- The 12 PrintService-backed Excel cases are ported (1.20.15).

## Gates

```bash
node tools/version_manager.mjs check
node tools/docs_audit.mjs
cd flutter/app && flutter analyze && flutter test
```

CI: `.github/workflows/quality.yml` runs the same gates (Flutter 3.47.7 on
Ubuntu). That workflow has not run on GitHub from this branch.

## Remaining work, in order

1. Windows build: run the manual "Windows build" workflow
   (`.github/workflows/windows-build.yml`), then follow `flutter/WINDOWS_TEST.md`.
2. Windows host checks: Excel COM, Office open/recalculate, Outlook reply and
   MSG archive, Microsoft Print to PDF, physical/network printers.
3. Live Supabase RLS deployment and two-user verification.
4. Signing and notarization with authorized credentials.

## UNVERIFIED (not run on a real host)

- Windows runtime, installer, and packaged startup.
- Office, Excel COM, Outlook, printers, and Print to PDF.
- Live Supabase RLS.
- `ExcelExportPipeline` production wiring on Windows (`productionExcelPipeline()`)
  is compiled but not exercised on Windows.
- Operator-only tests (need private files) remain env-gated with their reasons.

# Stack migration decision

## Why this stack

The first migration to a single PySide6 process simplified the architecture,
but it made pixel-accurate product UI slower to iterate and the result did not
meet the product's visual bar. The document engine itself remained reliable.

| Option | Decision |
|---|---|
| Flutter + Python | Rejected: duplicated Dart/Python contracts and heavier UI toolchain |
| PySide6 | Retired: robust native widgets, but slower product-UI iteration |
| Electron + React | Rejected: capable, but much larger runtime and memory floor |
| Full Rust native | Rejected: rewrites proven OCR and document behavior |
| Tauri 2 + React/TS + Python engine | Selected: lean native shell, fast UI work, preserves the mature engine |

## Resulting stack

- Tauri 2 with least-privilege shell/dialog/opener plugins
- React 19, TypeScript, Vite, and Lucide icons
- existing Python 3.12+ engine, PyMuPDF, OpenCV, and pytesseract
- standard-library HTTPS for the existing Supabase contracts
- PyInstaller one-file sidecar plus Tauri native installers

The sidecar is a deliberate domain boundary, not a local web service. It
retains the Python OCR ecosystem and keeps credentials/filesystem authority out
of the webview. The UI uses a small typed bridge and schema-generated forms.

## Functionality retained

- all DocFlow 0.5.4 operations: Process Scans, multi-sheet RFI Log Add, and
  ordered mixed-file Batch Print
- editable RFI table preview, workbook backup/restore, and Excel-native copy
- offline or optional Gemini-assisted RFI email extraction
- email or username sign-in, account creation, approval gate
- administrator access controls and username changes
- schema-driven operation forms and remembered values
- progress, warnings, exact preview, apply result, journal, undo
- review keep/put/recoverable-delete workflow
- cloud history with local fallback and both undo destinations
- Turkish, English, Russian, and Azerbaijani catalogs
- system/light/dark, font, scaling, and compact density preferences
- Windows Tesseract packaging and real OCR smoke validation

Existing settings, sessions, journals, trash, and Supabase rows are compatible.
The Flutter and Qt sources remain under ignored `legacy/` only for rollback.

# Flutter migration checklist (branch flutter-migration)

Source of truth: React UI in `frontend/src` (App.tsx, components/, lib/, types.ts, styles.css, locales/*.arb).
Engine: Rust core in `src-tauri/src` is ported to Dart (owner decision 2026-10-20: rewrite engine in Dart too). Single entry `backend::frontend_call(command, params)` -> Dart `Engine.call(command, params)`.
Flutter app lives in `flutter/app`; engine in `flutter/app/lib/engine/` mirroring Rust module names. Rust fixtures in `src-tauri/tests/fixtures/` are the parity oracles; Dart tests must replay them.

| ID | Unit | Source | Status |
|----|------|--------|--------|
| F1 | Flutter scaffold, theme (from styles.css), models (types.ts), command map (commands.ts), bridge client, l10n from 4 .arb, app shell/routing (App, AppFrame, Splash), platform.ts | main/App.tsx 216-556, types.ts, commands.ts, bridge.ts, i18n.ts, platform.ts, styles.css | todo |
| W1 | Shared widgets: Header, StepIndicator, Button, IconButton, Field, Switch, Segmented, Notice, EmptyState, BusyState, ActionBar, Modal, AdjustableDataRegion/ColumnResizeHandle | components/*.tsx | todo |
| S1 | LoginScreen, PendingScreen, AdminScreen | App.tsx 557-855, 5640-5750 | todo |
| S2 | SettingsScreen, SettingsGroup, ChoiceGrid | App.tsx 5751-6137 | todo |
| S3 | HomeScreen | App.tsx 856-1093 | todo |
| S4 | OperationScreen, PresetToolbar | App.tsx 1094-2241 | todo |
| S5 | SchemaField, CompactDateInput, date helpers | App.tsx 2242-2791 | todo |
| S6 | EditablePlanTable | App.tsx 2792-3636 | todo |
| S7 | PlanView, PlanImpactSummary, ScanIndexSummary, PlanExactEffects, StepRow | App.tsx 3637-4019, 4779-4877 | todo |
| S8 | WorkbookChangePreview, PlanWarnings, SignedStageNavigator, SignedWarningReviewSummary, SignedPageComparison | App.tsx 4020-4778 | todo |
| S9 | ResultView, ReviewScreen | App.tsx 4878-5308 | todo |
| S10 | HistoryScreen, RecoveryScreen | App.tsx 5309-5639 | todo |
| E1 | Engine port: errors, contracts, naming, rfi_code, page_range, paths, path_selection, parallel(isolates), progress | src-tauri/src | todo |
| E2 | Engine port: cap_fs, fs_ops, leaf_transaction, pdf_publish | src-tauri/src | todo |
| E3 | Engine port: journal, undo, plan_store, review, scan_index | src-tauri/src | todo |
| E4 | Engine port: registry, settings, backend (command router), diagnostics, smoke | src-tauri/src | todo |
| E5 | Engine port: auth, cloud, credentials, http, gemini (Supabase/Gemini HTTP, secure storage) | src-tauri/src | todo |
| E6 | Engine port: ooxml (docx/xlsx read-write), concrete_documents, concrete_document_sources | src-tauri/src | todo |
| E7 | Engine port: rfi_log, rfi_system_sync, outlook_reply_draft, email_extract, clipboard | src-tauri/src | todo |
| E8 | Engine port: rfi_follow_up | src-tauri/src | todo |
| E9 | Engine port: pdf (parse/orient/stage), windows_pdf, operations/pdf_orientation, operations/stage_signed_pages | src-tauri/src | todo |
| E10 | Engine port: ocr (Tesseract/Windows OCR/macOS Vision via FFI/process), operations/process_scans | src-tauri/src | todo |
| E11 | Engine port: print_queue, platform/*, controlled_submit, operations/batch_print, rfi_print_find (Win32/CUPS printing) | src-tauri/src | todo |
| E12 | Engine port: platform/excel + windows excel/outlook COM adapters, excel_windows_contract | src-tauri/src | todo |
| E13 | Engine port: operations/rfi_file_group, replica_publish, platform/startup | src-tauri/src | todo |
| T1 | Tests porting App.test.tsx / bridge.test.ts / frontend-authority.test.ts | frontend/src/**/*.test.* | todo |
| V1 | Version bump, docs/traceability, CI (flutter analyze/test), final parity audit | CLAUDE.md rules | todo |

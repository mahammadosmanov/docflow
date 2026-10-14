# Fast RFI Scan Modes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Retire standalone Process Scans and extend Fast RFI Scan with a first-page-only single-RFI mode, exact OCR `CLV` → `CIV` correction, and safe editable RFI codes in Preview.

**Architecture:** Keep one active Rust-owned scan operation and the shared splitting/orientation engine it needs. Add a backward-compatible `scan_mode` enum defaulting to `multi_rfi`; single mode OCRs page 1 only and uses the full unchanged PDF as its output. Make preview code edits server-validated and regenerate paths/effects from pinned source and destination data.

**Tech Stack:** Rust/Tauri, React/TypeScript generic schema and Plan editor, Tesseract/PDF OCR, four ARB locales, Rust and Vitest suites.

**Spec:** `docs/superpowers/specs/2026-10-08-rfi-fast-scan-workflows-design.md`

## Global Constraints

- The only persisted RFI disciplines are CIV, MEC, ELE, and PRC.
- The sole automatic OCR alias is exact OCR `CLV` → `CIV`; filenames, imported text, and manual identifiers do not receive this alias.
- Ventoy's original RFI database remains read-only; all test scenarios use isolated copies.
- Preserve existing `keep_original`, `organize_into_rfi_folders`, review, journal, Undo, and no-clobber behavior.
- Single-RFI mode OCRs only page 1 and emits the complete input PDF byte-for-byte without page extraction, rotation, or metadata rewrite.
- Rust is the authority for identifiers, plans, destination paths, and filesystem effects; React receives no filesystem authority.
- Legacy journals/history/settings remain readable; the retired operation may not create new Plans.
- All user-visible strings exist in EN, TR, RU, and AZ; no new dependency or UI framework.

## Review Focus

- Existing Fast Scan settings lacking `scan_mode` must still select `multi_rfi`, not silently switch to single mode.
- Multi-page single-RFI PDFs must cause no OCR request for pages 2…N; page-count inspection must not become page OCR.
- The one-RFI output hash must equal the entire source hash, with source copy/move behavior controlled by the existing `keep_original` setting.
- OCR `CLV` must map to CIV with evidence, while filename/text/manual `CLV`, other unsupported disciplines, and one-character near-misses remain rejected.
- Forged editor rows, source/destination path changes, invalid corrected IDs, stale sources, and occupied targets must not produce unauthorized output.

---

## File Map

- `src-tauri/src/registry.rs`, `backend.rs`, `native.rs`: active operation roster, mode schema/decoding, native preview dispatch, and Plan revision dispatch.
- `src-tauri/src/operations/process_scans.rs`: shared scanner engine, typed mode selection, first-page path, output naming, and safe Plan revision.
- `src-tauri/src/rfi_code.rs`, `ocr.rs`: strict supported disciplines and OCR-only CLV alias/evidence.
- `src-tauri/src/fs_ops.rs`: retain legacy journal authorization while refusing new Process Scans previews.
- `src-tauri/src/smoke.rs`: update package-smoke operation-count contract after retiring the extra operation.
- `frontend/src/App.tsx`, `frontend/src/lib/bridge.ts`, `frontend/src/App.test.tsx`, `frontend/src/frontend-authority.test.ts`: Fast operation presentation, legacy history labels, Plan editor behavior, and demo parity.
- `frontend/src/locales/app_{en,tr,ru,az}.arb`: mode, correction, evidence, validation, and retired-history labels.
- `docs/FEATURES.md`, `PRODUCT.md`, `PRD.md`, `RFI_DOMAIN.md`, `ARCHITECTURE.md`, `IPC_PROTOCOL.md`, `SAFETY_AND_STATE.md`, `TRACEABILITY.md`, `RELEASE_SAFETY.md`, `KNOWN_ISSUES.md`, `DECISIONS.md`: active contracts and tests; retain historical records as history.
- `VERSION`, runtime manifests, `CHANGELOG.md`, `releases.json`: breaking release version identity.

## Execution order

Run after `2026-10-08-rfi-group-files-discipline-folders.md`; this plan reuses its exact four-discipline filename parser in Fast RFI Scan.

### Task 1: Retire the standalone Process Scans entry safely

**Files:**
- Modify: `src-tauri/src/registry.rs`, `backend.rs`, `native.rs`, `operations/process_scans.rs`
- Modify: `frontend/src/App.tsx`, `frontend/src/lib/bridge.ts`, `frontend/src/locales/app_{en,tr,ru,az}.arb`
- Modify: `src-tauri/tests/fixtures/protocol1_operations.json`
- Test: `src-tauri/src/smoke.rs`, `src-tauri/src/registry.rs`, `src-tauri/src/backend.rs`, `src-tauri/tests/filesystem_authority.rs`, `src-tauri/tests/process_scans.rs`, and `frontend/src/App.test.tsx`.

**Interfaces:**
- Consumes: existing `FAST_COVER_OPERATION_ID` and shared scan backend.
- Produces: one active scan operation, `rfi.process_scans_fast_covers`; `rfi.process_scans` cannot create a new Plan; legacy history/journal recovery remains supported.

- [ ] **Step 1: Add failing registry, preview, legacy-recovery, history, and package-smoke assertions.** Add `registry_exposes_only_fast_rfi_scan`, `retired_process_scans_rejects_new_preview`, `legacy_process_scans_journal_still_supports_guarded_undo`, and a frontend test that an old history row renders a translated retired label while an unrelated unknown operation ID renders a distinct translated fallback. Update the package-smoke bootstrap test to expect nine active operations. Change retained scanner integration scenarios to exercise `preview_fast_covers`; retain only tests for behavior used by Fast RFI Scan.
- [ ] **Step 2: Run focused tests and observe failure.** Run `cargo test --manifest-path src-tauri/Cargo.toml registry_exposes_only_fast_rfi_scan`, `cargo test --manifest-path src-tauri/Cargo.toml retired_process_scans_rejects_new_preview`, `cargo test --manifest-path src-tauri/Cargo.toml --test filesystem_authority legacy_process_scans_journal_still_supports_guarded_undo`, `cargo test --manifest-path src-tauri/Cargo.toml package_smoke_bootstrap_and_settings_are_isolated`, and `npm test -- --run frontend/src/App.test.tsx -t 'retired Process Scans history'`. Expected: the standard operation is still registered/dispatchable, package smoke expects ten active operations, and history has no explicit retired label.
- [ ] **Step 3: Remove the active standard operation.** Delete its registry entry, backend dispatch branch, new-preview path, and dead `preview()` standard entry point. Update the protocol-one operation fixture for the remaining nine operations. Retain `FAST_COVER_OPERATION_ID`, confirmed-cover OCR, common page grouping/orientation/PDF output helpers, and low-level legacy authorization needed for old journal recovery. Remove the standard demo operation; history must distinguish the retired ID from unknown operation IDs. Do not add the new mode field in this task.
- [ ] **Step 4: Re-run focused tests.** Run the five commands from Step 2 plus `cargo test --manifest-path src-tauri/Cargo.toml --test process_scans`. Expected: registry contains one scan operation; package smoke recognizes the updated roster; new legacy-ID previews reject; old journal recovery remains guarded and usable; retained scanner tests run only through Fast RFI Scan.

### Task 2: Implement one-RFI first-page mode and OCR alias

**Files:**
- Modify: `src-tauri/src/operations/process_scans.rs`, `src-tauri/src/ocr.rs`, `src-tauri/src/native.rs`
- Modify: `src-tauri/src/registry.rs`, `src-tauri/src/rfi_code.rs`
- Test: `src-tauri/tests/process_scans.rs`, OCR unit tests in `src-tauri/src/ocr.rs`

**Interfaces:**
- Consumes: the active Fast RFI Scan operation from Task 1 and the four-discipline exact parser from the Group Files plan.
- Produces: a typed scan mode in `ProcessScansParams`; `multi_rfi` follows current confirmed-cover grouping; `single_rfi` inspects/OCRs page index `0` only and plans one whole-PDF output; OCR-only exact CLV evidence maps to canonical CIV.

- [ ] **Step 1: Add failing mode/alias tests.** In `src-tauri/tests/process_scans.rs`, add `single_rfi_mode_ocr_reads_only_page_zero_and_preserves_complete_pdf_bytes`: use a multi-page fixture and counting `OcrAdapter`, assert only page 0 is OCR-read, output SHA-256 equals the entire input, and source retention/movement follows `keep_original`. Test both `organize_into_rfi_folders` values. In `src-tauri/src/ocr.rs` cover-detection tests, add `fast_cover_recognizes_ocr_clv_as_civ_with_alias_evidence`; assert regular `find`/`exact_codes` and filename/content matching do not receive the alias, while other unsupported OCR disciplines and near-misses remain rejected.
- [ ] **Step 2: Run focused tests and observe failure.** Run `cargo test --manifest-path src-tauri/Cargo.toml --test process_scans single_rfi_mode_ocr_reads_only_page_zero_and_preserves_complete_pdf_bytes` and `cargo test --manifest-path src-tauri/Cargo.toml fast_cover_recognizes_ocr_clv_as_civ_with_alias_evidence`. Expected: the new mode/alias behavior is not implemented.
- [ ] **Step 3: Implement typed mode branching and its schema.** Add a defaulted `ScanMode` enum (`MultiRfi`/`SingleRfi`, snake-case wire values) to `ProcessScansParams` and native request values. Register `scan_mode` with values `multi_rfi` and `single_rfi`, default `multi_rfi`; absent old settings deserialize as `multi_rfi`. In Fast Scan, retain the current cover-based loop for `multi_rfi`; for `single_rfi`, inspect page count, perform full-page OCR on page index `0` only, and plan the complete PDF as one transfer. Reuse `process_scan_targets` so discipline placement and `organize_into_rfi_folders` remain authoritative. Do not rotate or extract pages in single mode. Preserve `keep_original` behavior and no-clobber checks.
- [ ] **Step 4: Add OCR-only alias with visible evidence.** Add a bounded parser helper for exact OCR observations that recognizes only the `-RFI-CLV-####` segment and emits the CIV canonical match plus an alias marker. Use it only in OCR result extraction/cover confidence decisions; leave `find` and `exact_codes` for filenames/imported text strict. Carry the detected CLV and canonical CIV through the Plan metadata/source evidence for the preview.
- [ ] **Step 5: Re-run scanner and OCR tests.** Run both tests from Step 2, existing Fast Cover grouping tests, `cargo test --manifest-path src-tauri/Cargo.toml --test process_scans`, and OCR unit tests. Expected: only page 0 OCR in single mode; byte-identical complete PDF; old multi-cover separation remains unchanged; alias visible and narrowly scoped.

### Task 3: Add Rust-authorized code correction in Preview

**Files:**
- Modify: `src-tauri/src/operations/process_scans.rs`, `src-tauri/src/native.rs`, `frontend/src/App.tsx`
- Test: `src-tauri/tests/process_scans.rs`, `frontend/src/App.test.tsx`

**Interfaces:**
- Consumes: scan mode and OCR alias evidence from Task 2.
- Produces: `meta.editor.rows` with immutable `_rowId`, original detected code/evidence, editable `rfiCode`, source display name, method/confidence; `revise_fast_covers(plan: &Plan, payload: &Value) -> Result<Plan, BackendError>` validates rows and rebuilds all affected directory/output steps from the cached Plan's pinned root/source/page metadata.

- [ ] **Step 1: Add failing Plan revision tests.** Add `fast_scan_revision_accepts_valid_code_and_rebuilds_target_from_rust` and `fast_scan_revision_rejects_invalid_or_forged_row_fields`. Assert a valid correction changes only the canonical code-derived discipline/folder/filename; unsupported/CLV manual codes, duplicate/missing row IDs, and client-supplied path fields are rejected. Add `fast_scan_apply_refuses_source_changed_after_preview` and an occupied-target test proving no overwrite.
- [ ] **Step 2: Run focused tests and observe failure.** Run `cargo test --manifest-path src-tauri/Cargo.toml --test process_scans fast_scan_revision_` and `cargo test --manifest-path src-tauri/Cargo.toml --test process_scans fast_scan_apply_refuses_source_changed_after_preview`. Expected: no editable scanner Plan revision exists.
- [ ] **Step 3: Add Rust-owned editor metadata and revision.** Build immutable row IDs and `offeredSteps` from the original source, destination root, page indexes/hash, and detection evidence. Implement `revise_fast_covers` with exact row-set/field allowlists; validate edited identifiers against the four canonical disciplines; reconstruct targets in Rust from the selected code and existing organization option. Preserve extraction/copy/move semantics, warnings, and no-clobber. Dispatch only Fast Scan to this revision function.
- [ ] **Step 4: Render the code field through the generic editor.** Declare the `rfiCode` column as the existing text-input kind, keep detected code/evidence/source/method/confidence read-only, show CLV → CIV evidence, and display the exact recomputed target in Plan steps. Do not add a filesystem-specific frontend form or accept client paths.
- [ ] **Step 5: Re-run backend and frontend tests.** Run the focused Rust tests from Step 1, `cargo test --manifest-path src-tauri/Cargo.toml --test process_scans`, and `npm test -- --run frontend/src/App.test.tsx`. Expected: valid code edits revise only Rust-derived targets; invalid/tampered edits are rejected; four-language UI remains understandable.

### Task 4: Synchronize docs/locales and close the breaking slice

**Files:**
- Modify: `frontend/src/locales/app_{en,tr,ru,az}.arb`, `frontend/src/lib/bridge.ts`, `src-tauri/tests/fixtures/protocol1_operations.json`
- Modify: `docs/FEATURES.md`, `PRODUCT.md`, `PRD.md`, `RFI_DOMAIN.md`, `ARCHITECTURE.md`, `IPC_PROTOCOL.md`, `SAFETY_AND_STATE.md`, `TRACEABILITY.md`, `RELEASE_SAFETY.md`, `KNOWN_ISSUES.md`, `DECISIONS.md`
- Modify: `VERSION`, runtime manifests, `CHANGELOG.md`, `releases.json`

**Interfaces:**
- Consumes: Tasks 1–3 behavior and `rfi.process_scans_fast_covers` Plan revision contract.
- Produces: current docs describe a single Fast RFI Scan operation and both modes; old records remain legible; all locale keys remain equal.

- [ ] **Step 1: Update four locale catalogs and demo bridge.** Name the active operation Fast RFI Scan, describe both modes and `CLV → CIV` evidence, add validation strings, keep a translated retired label solely for old history, and mirror `multi_rfi` as the demo default. Remove dead standard-operation demo schema/description keys that are not needed for history.
- [ ] **Step 2: Update active contracts and traceability.** Replace claims that two operations are active; document the default, OCR-only alias, first-page behavior, complete-output bytes, retained setting semantics, code-edit authority, and legacy history/journal support. Update registry/protocol fixtures to the nine-operation roster. Keep historical migration/ADR records as history rather than rewriting past events.
- [ ] **Step 3: Run source gates and version checks.** Run `node tools/version_manager.mjs next breaking`, use its result with `node tools/version_manager.mjs set <version>`, update `CHANGELOG.md` and `releases.json`, then run `node tools/version_manager.mjs check`, `node tools/docs_audit.mjs`, `node tools/package_audit.mjs source`, `npm test`, `npm run build`, `npm run audit:licenses`, `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`, `cargo test --manifest-path src-tauri/Cargo.toml`, and `cargo check --manifest-path src-tauri/Cargo.toml`. Expected: all gates pass; do not fix unrelated existing warnings by suppressing them.
- [ ] **Step 4: Exercise each scan mode against private copies.** With a copied multi-RFI Ventoy sample, Preview/Apply/Undo the `multi_rfi` path and verify per-page groups. With a copied multi-page one-RFI sample, Preview/Apply/Undo `single_rfi`; verify only page one was analyzed, the output hash equals the input hash, and the configured source-retention behavior is followed. Correct a CLV preview to CIV and a second code to a different valid discipline; verify derived target paths and collision refusal. Hash the source corpus and Ventoy data before/after.
- [ ] **Step 5: Commit the breaking slice.** Stage only Fast RFI Scan behavior, registry/legacy handling, tests, docs/locales, and version/release files. Commit as `feat(rfi)!: Consolidate scan processing in Fast RFI Scan` with a breaking-change footer.

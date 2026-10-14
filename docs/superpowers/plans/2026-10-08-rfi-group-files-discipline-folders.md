# Group Files Discipline Folders Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restrict new RFI recognition to CIV/MEC/ELE/PRC and place Group Files copies beneath discipline and full-code folders.

**Architecture:** Keep discovery, mail evidence, fanout, Plan revision, and Apply unchanged. Make the shared RFI parser accept only the four supported disciplines, then construct Group Files destinations as `<destination>/<discipline>/<full-code>/<source filename>` in Rust.

**Tech Stack:** Rust/Tauri, `cargo test`, schema-driven React UI, Markdown/ARB catalogs, Node docs/version audits.

**Spec:** `docs/superpowers/specs/2026-10-08-rfi-fast-scan-workflows-design.md`

## Global Constraints

- The only persisted RFI disciplines are CIV, MEC, ELE, and PRC.
- The only automatic OCR alias is exact OCR `CLV` → `CIV`; filenames, imported text, and manual identifiers do not receive this alias.
- Ventoy's original RFI database remains read-only; all test scenarios use isolated copies.
- Group Files sources remain read-only. Every output remains Preview → explicit Apply → journal → guarded Undo; existing targets are never overwritten.
- Rust is the authority for identifiers, plans, destination paths, and filesystem effects.
- All user-visible strings exist in EN, TR, RU, and AZ.
- Preserve top-level discovery, mail-evidence eligibility, multi-RFI fanout, `_review`, and existing collision behavior.

## Review Focus

- Unsupported exact identifiers (`ARC`, `INS`, `PIP`, `STR`, `TEL`, `CLV`) must not be accepted by canonical parsing or exact content matching.
- OCR-only `CLV` correction must not leak into filename/content matching; the dedicated Fast Scan plan owns that alias.
- Missing EML/MSG evidence must still block every copy for that RFI while allowing independently eligible RFIs.
- Multi-RFI files must fan out once per supported code into distinct discipline/code folders without changing source bytes.
- An occupied file or unsafe directory at a nested destination must warn and never overwrite.

---

## File Map

- `src-tauri/src/rfi_code.rs`: canonical and bounded content-code recognition shared by RFI workflows.
- `src-tauri/src/operations/rfi_file_group.rs`: destination path construction and revision-safe offered steps.
- `src-tauri/tests/process_scans.rs`, `src-tauri/tests/fixtures/slice5_rfi_naming.json`: shared RFI parser compatibility tests and frozen behavior oracle.
- `src-tauri/tests/rfi_file_group.rs`: isolated source, mail gate, fanout, collision, and Apply/Undo scenarios.
- `frontend/src/locales/app_{en,tr,ru,az}.arb`: clarify the destination hierarchy without removing the existing operation.
- `docs/FEATURES.md`, `docs/PRODUCT.md`, `docs/PRD.md`, `docs/RFI_DOMAIN.md`, `docs/SAFETY_AND_STATE.md`, `docs/TRACEABILITY.md`: active contracts and test traceability.
- `VERSION`, runtime manifests, `CHANGELOG.md`, `releases.json`: project version identity and release notes.

### Task 1: Enforce the four canonical disciplines

**Files:**
- Modify: `src-tauri/src/rfi_code.rs`, `src-tauri/src/operations/rfi_file_group.rs`
- Test: `src-tauri/tests/process_scans.rs`, `src-tauri/tests/fixtures/slice5_rfi_naming.json`
- Test: `src-tauri/tests/rfi_file_group.rs`

**Interfaces:**
- Consumes: existing `find(&str) -> RfiMatch` and `exact_codes(&str) -> BTreeSet<String>` APIs.
- Produces: the same APIs, but strict/wrapped matches and extracted exact codes are limited to `CIV`, `MEC`, `ELE`, and `PRC`; no discipline edit-distance repair.

- [ ] **Step 1: Add failing parser and grouping tests.** Add `canonical_parser_accepts_only_supported_disciplines_and_does_not_fuzzy_repair_discipline` to the Process Scans integration tests. Assert all four supported exact identifiers remain strict; exact unsupported disciplines and `CLV` return `Miss`; `exact_codes` returns no unsupported code. Update the existing `CV` recovery assertion and frozen compatibility fixture to expect `Miss`. Add `unsupported_exact_discipline_creates_no_grouped_output` to `rfi_file_group.rs`, proving an unsupported exact code plus mail evidence creates no RFI copy.
- [ ] **Step 2: Run the focused tests and observe failure.** Run `cargo test --manifest-path src-tauri/Cargo.toml --test process_scans canonical_parser_accepts_only_supported_disciplines_and_does_not_fuzzy_repair_discipline` and `cargo test --manifest-path src-tauri/Cargo.toml --test rfi_file_group unsupported_exact_discipline_creates_no_grouped_output`. Expected: current parser recognizes unsupported disciplines or repairs `CLV`.
- [ ] **Step 3: Enforce the four-discipline contract at every recognition path.** In `rfi_code.rs`, set the supported list to the four specified values, reject unsupported captures in strict/wrapped/positional-repair/prefix-fuzzy paths, apply the same restriction in `exact_codes`, and remove nearest-discipline recovery. In `operations/rfi_file_group.rs`, make its local code-and-number-list extraction require membership in the shared exact-code set before adding a match or related-number fanout; retain the existing fanout delimiters and boundary checks. Preserve NFC/uppercase normalization and the strict `-RFI-` anchor. Do not add the OCR alias here.
- [ ] **Step 4: Re-run focused tests.** Run both tests from Step 2, the compatibility-oracle test in `process_scans.rs`, and the existing related-number Group Files test. Expected: PASS; supported fixtures and fanout remain compatible, while unsupported disciplines and the former `CV` recovery case miss.

### Task 2: Route Group Files under discipline and code folders

**Files:**
- Modify: `src-tauri/src/operations/rfi_file_group.rs`
- Test: `src-tauri/tests/rfi_file_group.rs`, `src-tauri/src/native.rs`

**Interfaces:**
- Consumes: strict four-discipline codes from Task 1 and the existing immutable `offeredSteps`/`revise` contract.
- Produces: each eligible copy step targets `<destination>/<discipline>/<full-code>/<source filename>`; directory steps are derived from the same code and remain selectable through the existing editor.

- [ ] **Step 1: Add failing four-discipline and Apply/Undo coverage.** Add `group_files_routes_supported_disciplines_into_discipline_and_code_folders`. Create separate isolated incoming mail evidence and document files for `AZE-SUN-INT-QMS-RFI-CIV-0001`, `AZE-SUN-INT-QMS-RFI-MEC-0002`, `AZE-SUN-INT-QMS-RFI-ELE-0003`, and `AZE-SUN-INT-QMS-RFI-PRC-0004`. Assert Preview includes exact nested mkdir/copy targets, leaves the source directory unchanged, and preserves existing per-row enable/disable revision behavior. Add an occupied nested-target case that expects a warning and no overwrite. Update `native::tests::rfi_file_group_apply_copies_without_touching_source_and_undoes_created_output` to expect the nested destination and exercise Apply/Undo against it.
- [ ] **Step 2: Run focused tests and observe failure.** Run `cargo test --manifest-path src-tauri/Cargo.toml --test rfi_file_group group_files_routes_supported_disciplines_into_discipline_and_code_folders` and `cargo test --manifest-path src-tauri/Cargo.toml rfi_file_group_apply_copies_without_touching_source_and_undoes_created_output`. Expected: the integration targets and native Apply/Undo path still use the flat layout.
- [ ] **Step 3: Update Rust destination construction.** In `operations/rfi_file_group.rs`, derive the discipline from the already validated full code and use `destination.join(discipline).join(code)` for normal folders and copy targets. Keep `_review` at the destination root. Preserve collision warnings, source hashes, row IDs, offered-step metadata, and the revise allowlist; never accept a client-authored path.
- [ ] **Step 4: Verify Group Files safety scenarios.** Run `cargo test --manifest-path src-tauri/Cargo.toml --test rfi_file_group` and the focused native test from Step 2. Expected: PASS for all four routes, mail evidence, fanout, collision, source read-only, revision, Apply, and Undo.

### Task 3: Update operator contracts and close the feature slice

**Files:**
- Modify: `frontend/src/locales/app_en.arb`, `app_tr.arb`, `app_ru.arb`, `app_az.arb`
- Modify: `docs/FEATURES.md`, `docs/PRODUCT.md`, `docs/PRD.md`, `docs/RFI_DOMAIN.md`, `docs/SAFETY_AND_STATE.md`, `docs/TRACEABILITY.md`
- Modify: `VERSION`, version manifests, `CHANGELOG.md`, `releases.json`

**Interfaces:**
- Consumes: the tested folder contract from Task 2.
- Produces: localized operator descriptions and active documentation that show the discipline/full-code folder hierarchy and its no-overwrite/source-read-only rules.

- [ ] **Step 1: Update localized Group Files guidance.** In all four catalogs, explain that approved copies are filed under discipline then full RFI code; keep existing keys and translate each value in its target language.
- [ ] **Step 2: Update active product and safety documentation.** Replace the old Group Files path statement with the nested layout in `FEATURES.md`, `PRODUCT.md`, `PRD.md`, `RFI_DOMAIN.md`, `SAFETY_AND_STATE.md`, and `TRACEABILITY.md`; preserve mail evidence, fanout, and no-clobber guarantees. Add/adjust traceability for the four-discipline routing and regression scenario. Leave historical ADRs intact unless they assert a current path contract.
- [ ] **Step 3: Run documentation, version, and focused Rust gates.** Run `node tools/docs_audit.mjs`, `node tools/version_manager.mjs next feature`, then use the reported version with `node tools/version_manager.mjs set <version>`; update the current changelog section and release catalog. Run `node tools/version_manager.mjs check`, `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`, and `cargo test --manifest-path src-tauri/Cargo.toml --test rfi_file_group` plus the parser test from Task 1. Expected: all commands pass and the release catalog has one current entry.
- [ ] **Step 4: Exercise the user-visible workflow on an isolated copy.** Prepare a private temporary folder containing representative mail evidence and one file for each discipline. Preview Group Files, inspect all four nested targets, Apply only the approved Plan, verify each destination and unchanged source hashes, then Undo and verify only created outputs/directories are removed. Never use the Ventoy originals as mutation targets.
- [ ] **Step 5: Commit this slice.** Stage only the Group Files, parser, focused tests, docs/locales, and version/release files. Commit as `feat(rfi): Route grouped files by discipline`.

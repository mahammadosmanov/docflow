# Printing Order v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe named print-setting presets and exact per-file PDF page ranges to the existing `files.batch_print` workflow.

**Architecture:** Rust remains authoritative for preset filtering/storage and page-range validation. A generic operation capability drives a generic frontend preset toolbar; page ranges flow through preview rows, immutable plans, persistent queues, and platform adapters with fail-closed format rules.

**Tech Stack:** Rust 2024, Tauri 2, serde/serde_json, `unicode-casefold` 0.2, React 19, TypeScript 7, Vitest/Testing Library, Windows WinRT/GDI printing, macOS CUPS.

**Spec:** `docs/superpowers/specs/2026-08-26-printing-order-v1-design.md`

## Global Constraints

- Work only in `/Users/rohtolos/.hermes/worktrees/docflow/completion` on `feat/docflow-completion`.
- Preserve strict preview-before-apply, filesystem-grant, immutable-plan, queue-resume, journal, cancellation, and file-signature behavior.
- Do not store files, paths, order, page ranges, text, secrets, arrays, or objects in presets.
- Page range is per file: PDF exact; images `all`/`1`; native Office handlers `all` only.
- Missing legacy `pageRange` deserializes as `all`.
- Frontend remains capability/schema driven; never branch on `files.batch_print` to create a custom form.
- Add no PDF library and no Office COM code in this plan.
- Automated tests must not submit a physical print job.
- Add every user-visible key in Turkish, English, Russian, and Azerbaijani.
- Follow strict RED → GREEN → REFACTOR for each behavior; preserve exact RED and GREEN evidence.
- GitHub issue: `#24`.

---

## Task 1: Generic preset storage and typed command boundary

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/src/contracts.rs`
- Modify: `src-tauri/src/registry.rs`
- Modify: `src-tauri/src/settings.rs`
- Modify: `src-tauri/src/backend.rs`
- Modify: `src-tauri/src/native.rs`
- Modify: `src-tauri/tests/fixtures/protocol1_operations.json`
- Modify: `frontend/src/types.ts`
- Modify: `frontend/src/lib/bridge.ts`
- Modify: `frontend/src/lib/commands.ts`
- Modify: `frontend/src/lib/platform.ts`

**Interfaces:**
- Produces: `OperationDef.supports_presets: bool`
- Produces: `OperationPreset { id: String, name: String, values: JsonObject }`
- Produces: `SettingsStore::{presets_for, save_preset, delete_preset}`
- Produces commands: `settings/presets`, `settings/savePreset`, `settings/deletePreset`
- Consumes: existing `operation_registry`, `ParamKind`, `SettingsStore` atomic-write pattern, and router authorization.

- [ ] **Step 1: Write failing contract and storage tests**

In `contracts.rs`, add a serialization snapshot assertion that `supportsPresets` is `true` for an enabled fixture and defaults to `false` when omitted during deserialization.

In `settings.rs`, add focused tests that exercise this wished-for API:

```rust
let saved = store
    .save_preset(
        "files.batch_print",
        "A4 duplex",
        serde_json::from_value(json!({
            "printer": "Office Printer",
            "paper": "A4",
            "duplex": "long",
            "copies": 2
        }))
        .unwrap(),
        false,
    )
    .unwrap();
assert_eq!(saved.name, "A4 duplex");
assert_eq!(store.presets_for("files.batch_print"), vec![saved]);
```

Add separate tests for:

- case-insensitive duplicate rejection;
- explicit replacement preserving the ID;
- deterministic case-insensitive ordering;
- 20-preset capacity;
- malformed empty/65-character names;
- deleting by exact ID;
- unknown top-level settings surviving every write;
- failed atomic write leaving in-memory and on-disk data unchanged.

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml settings::tests::operation_presets -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml contracts::tests::operation_supports_presets -- --nocapture
```

Expected RED: missing `OperationPreset`, `supports_presets`, and preset store methods—not syntax or fixture failures.

- [ ] **Step 3: Implement the minimal typed store**

Add this protocol type to `contracts.rs`:

```rust
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationPreset {
    pub id: String,
    pub name: String,
    pub values: JsonObject,
}
```

Add to `OperationDef`:

```rust
#[serde(default)]
pub supports_presets: bool,
```

Keep the generic `operation(...)` helper defaulting to `false`; set the field to `true` only on the Batch Print definition after construction or through an explicit helper argument whose call sites are all updated.

In `settings.rs`, add constants:

```rust
const MAX_OPERATION_PRESETS: usize = 20;
const MAX_PRESET_NAME_CHARS: usize = 64;
```

Store presets at:

```json
{"operationPresets":{"files.batch_print":[{"id":"...","name":"...","values":{}}]}}
```

Implement clone → validate → atomic write → memory swap. Do not reuse `save_section` in a way that loses concurrent sections. Decode malformed stored entries by dropping only malformed preset entries, not the full settings file.

- [ ] **Step 4: Write failing native command tests**

In `native.rs`, test the desired command boundary through `NativeBackend::dispatch`:

```rust
let saved = backend.dispatch(
    BackendMethod::SettingsSavePreset,
    json!({
        "opId": "files.batch_print",
        "name": "Office",
        "values": {
            "printer": "Office Printer",
            "copies": 2,
            "files": [{"selectionId": "forged"}],
            "email_text": "must not persist",
            "nested": {"bad": true}
        }
    }),
)?;
assert_eq!(saved["values"]["copies"], 2);
assert!(saved["values"].get("files").is_none());
assert!(saved["values"].get("email_text").is_none());
assert!(saved["values"].get("nested").is_none());
```

Also test:

- an operation without `supportsPresets` is rejected;
- invalid choice values are rejected rather than saved;
- `replace: false` rejects a duplicate;
- list and delete round-trip;
- every new command remains protected by `BackendMethod::authorize`.

- [ ] **Step 5: Run native/backend tests and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml native::tests::settings_presets -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml backend::tests::rust_and_typescript_frontend_method_rosters_match_exactly -- --nocapture
```

Expected RED: missing backend methods and TypeScript roster entries.

- [ ] **Step 6: Implement command routing and authoritative filtering**

Add the three `BackendMethod` variants, update `ALL` length and entries, `as_str`, and dispatch arms.

Add request DTOs with `deny_unknown_fields`:

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SavePresetParams {
    op_id: String,
    name: String,
    values: JsonObject,
    #[serde(default)]
    replace: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeletePresetParams {
    op_id: String,
    preset_id: String,
}
```

Implement a separate `filter_preset_values` that derives the allowlist from registry parameters whose kinds are exactly `Bool | Choice | Int`. Require JSON scalar type parity and reject an unknown static choice. Preserve only the dynamic `files.batch_print.printer` stored choice when its name is non-empty, control-free, and at most 512 Unicode scalar values; normal Preview/apply inventory validation still rejects it while unavailable. Numeric min/max behavior remains enforced by normal preview validation; the command rejects non-integral protocol numbers.

Update `frontend/src/types.ts` and `frontend/src/lib/commands.ts` with exact parameter/result types and roster entries. Update the demo operation registry in `frontend/src/lib/bridge.ts`, add typed preset wrappers to `frontend/src/lib/platform.ts`, and refresh `src-tauri/tests/fixtures/protocol1_operations.json` so the frozen registry protocol contains `supportsPresets` for every operation.

- [ ] **Step 7: Verify GREEN and affected regression tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml settings::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml native::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml backend::tests -- --nocapture
npm run build:frontend
```

Expected: all selected suites and TypeScript compilation pass.

- [ ] **Step 8: Commit the verified backend slice**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/contracts.rs src-tauri/src/registry.rs src-tauri/src/settings.rs src-tauri/src/backend.rs src-tauri/src/native.rs src-tauri/tests/fixtures/protocol1_operations.json frontend/src/types.ts frontend/src/lib/bridge.ts frontend/src/lib/commands.ts frontend/src/lib/platform.ts
git commit -m "feat: add typed operation presets"
```

---

## Task 2: Generic preset UI with file-selection preservation

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/App.test.tsx`
- Modify: `frontend/src/styles.css`
- Modify: `frontend/src/locales/app_tr.arb`
- Modify: `frontend/src/locales/app_en.arb`
- Modify: `frontend/src/locales/app_ru.arb`
- Modify: `frontend/src/locales/app_az.arb`

**Interfaces:**
- Consumes: `operation.supportsPresets`, `OperationPreset`, and Task 1 commands.
- Produces: generic `PresetToolbar` rendered in the operation form stage.
- Preserves: every current value whose schema kind is not `bool`, `choice`, or `int`, especially `files` selections.

- [ ] **Step 1: Write the failing frontend behavior tests**

Extend the bootstrap fixture's Batch Print operation with `supportsPresets: true`. Add tests that use the mock bridge to prove:

1. `settings/presets` loads when the capability is true and does not load when false.
2. Applying a preset updates printer/paper/copies but preserves the existing `files` array by exact deep equality.
3. Saving sends only preset-capable current values.
4. A duplicate response triggers an explicit confirm-and-`replace: true` retry; no silent replacement occurs.
5. Deletion requires confirmation and removes the selected item only after command success.
6. Command failure is visible through the existing error surface.

Use accessible names from localization keys rather than CSS selectors.

- [ ] **Step 2: Run the focused UI test and verify RED**

Run:

```bash
npm test -- --run frontend/src/App.test.tsx -t "operation presets"
```

Expected RED: preset controls are absent and `settings/presets` is never called.

- [ ] **Step 3: Implement the generic preset toolbar**

Add `PresetToolbar` as a generic component in `App.tsx` near the existing schema-driven field components. Its props are:

```ts
type PresetToolbarProps = {
  bridge: Bridge;
  operation: OperationDef;
  values: Record<string, unknown>;
  onApply: (preset: OperationPreset) => void;
  onError: (message: string) => void;
};
```

In `OperationScreen`, merge applied values like this:

```ts
setValues((current) => ({ ...current, ...preset.values }));
```

The backend-filtered preset contains no path-bearing values; additionally intersect keys with local schema kinds `bool`, `choice`, and `int` before merging as defense in depth.

Use `window.confirm` only for duplicate replacement and deletion. Use a normal text input for a new name. Disable buttons during each in-flight mutation. Re-fetch the authoritative list after save/delete.

Add compact responsive styles consistent with existing cards; do not add page chrome or redesign the operation screen.

- [ ] **Step 4: Add all locale keys**

Add equivalent keys in all four `.arb` files for:

- preset section/title;
- select placeholder;
- name label;
- apply/save/replace/delete;
- duplicate replacement confirmation;
- deletion confirmation;
- empty state;
- saved/deleted success;
- invalid name, duplicate, limit, stale preset, and settings-write errors.

Keep technical tokens such as printer names and page syntax unchanged.

- [ ] **Step 5: Verify GREEN and frontend regression**

Run:

```bash
npm test -- --run frontend/src/App.test.tsx -t "operation presets"
npm test
npm run build:frontend
```

Expected: focused tests, all tool/frontend tests, TypeScript, and Vite production build pass.

- [ ] **Step 6: Commit the verified UI slice**

```bash
git add frontend/src/App.tsx frontend/src/App.test.tsx frontend/src/styles.css frontend/src/locales/app_tr.arb frontend/src/locales/app_en.arb frontend/src/locales/app_ru.arb frontend/src/locales/app_az.arb
git commit -m "feat: add generic print setting presets"
```

---

## Task 3: Per-file page-range domain through preview, plan, and resume

**Files:**
- Create: `src-tauri/src/page_range.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/print_queue.rs`
- Modify: `src-tauri/src/operations/batch_print.rs`
- Modify: `src-tauri/tests/batch_print.rs`
- Modify: `frontend/src/locales/app_tr.arb`
- Modify: `frontend/src/locales/app_en.arb`
- Modify: `frontend/src/locales/app_ru.arb`
- Modify: `frontend/src/locales/app_az.arb`

**Interfaces:**
- Produces: `PageRange::parse`, `PageRange::canonical`, `PageRange::resolve_zero_based`.
- Produces: `PageRangeCapability::{Pdf { page_count: Option<u32> }, SinglePage, AllOnly}`.
- Extends: `PrintJobSupport`, `PrintSettings`, and `PrintJob.page_range`.
- Consumes: existing record-table editor, `engine/revise`, serde queue persistence, and fake adapter integration seam.

- [ ] **Step 1: Write failing parser tests first**

Create `page_range.rs` with tests before production implementation. Freeze these expectations:

```rust
assert_eq!(PageRange::parse("all").unwrap().canonical(), "all");
assert_eq!(PageRange::parse(" 3, 1-2,2,7 ").unwrap().canonical(), "1-3,7");
assert_eq!(
    PageRange::parse("1-3,5").unwrap().resolve_zero_based(5).unwrap(),
    vec![0, 1, 2, 4]
);
```

Separate tests reject `""`, `"0"`, `"-1"`, `"1,,2"`, `"1-2-3"`, `"5-3"`, `"all,2"`, non-numeric input, overflow, and a selected page above the supplied count.

- [ ] **Step 2: Run parser test and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml page_range::tests -- --nocapture
```

Expected RED: module/type/functions are missing.

- [ ] **Step 3: Implement the minimal parser and capability rules**

Use checked `u32` parsing, `BTreeSet<u32>` deduplication, and one coalescing serializer. Do not use regex or shell parsing.

Expose format validation through an explicit enum:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PageRangeCapability {
    Pdf { page_count: Option<u32> },
    SinglePage,
    AllOnly,
}
```

Provide a function that returns the canonical string or a `BackendError`:

```rust
pub fn canonical_for(
    raw: &str,
    capability: &PageRangeCapability,
) -> Result<String, BackendError>;
```

`SinglePage` accepts `all` and numeric selection resolving exactly to page `1`, then returns `all`. `AllOnly` accepts only `all`.

- [ ] **Step 4: Write failing batch-print integration tests**

Extend `FakeAdapter::inspect` to return explicit capabilities by extension and a configurable PDF page count. Add one behavior test at a time for:

1. preview rows and jobs default to `pageRange: "all"`;
2. revise canonicalizes two different per-file ranges and preserves row order;
3. an out-of-bounds PDF row rejects the entire revision and leaves the prior plan usable;
4. image `1` canonicalizes to `all`, while image `2` fails;
5. native `.docx` numeric range fails before apply;
6. `Step.meta`, physical jobs, collated copies, persisted queue JSON, and resume rows retain the canonical range;
7. a handcrafted legacy queue missing `pageRange` resumes as `all`;
8. a forged apply plan with an invalid range fails `validate_job` without submission.

- [ ] **Step 5: Run one focused integration test at a time and verify RED**

Example command:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test batch_print revise_canonicalizes_per_file_page_ranges -- --exact --nocapture
```

Expected RED: `pageRange` is missing or invalid values are currently accepted.

- [ ] **Step 6: Thread the field through the existing model**

Add a serde default helper:

```rust
fn default_page_range() -> String {
    "all".to_string()
}
```

Then add:

```rust
#[serde(default = "default_page_range")]
pub page_range: String,
```

to `PrintJob`, and `page_range: String` to `PrintSettings`.

Extend `PrintJobSupport` with `page_range: PageRangeCapability` and constructors:

```rust
controlled_pdf(page_count: Option<u32>)
single_page()
native_application()
```

Thread `pageRange` through:

- `PrintJob::default` and `settings()`;
- legacy normalization;
- `resume_plan` copy/default;
- `validate_job`;
- `plan_from_rows` step metadata;
- `job_from_row`;
- `row_from_file` default;
- editor columns as a generic `text` column;
- revised rows and physical copy/collation jobs.

Do not add a form-level page-range parameter and do not add it to presets.

- [ ] **Step 7: Add page-range locale keys**

Add four-locale labels/help/errors for the preview column and format/bounds failures.

- [ ] **Step 8: Verify GREEN and batch-print regression**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml page_range::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test batch_print -- --nocapture
```

Expected: all parser and Batch Print integration tests pass without real printer submission.

- [ ] **Step 9: Commit the verified domain/queue slice**

```bash
git add src-tauri/src/page_range.rs src-tauri/src/lib.rs src-tauri/src/print_queue.rs src-tauri/src/operations/batch_print.rs src-tauri/tests/batch_print.rs frontend/src/locales/app_tr.arb frontend/src/locales/app_en.arb frontend/src/locales/app_ru.arb frontend/src/locales/app_az.arb
git commit -m "feat: add exact per-file page ranges"
```

---

## Task 4: Platform submission honors selected pages

**Files:**
- Modify: `src-tauri/src/platform/windows/printing.rs`
- Modify: `src-tauri/src/platform/macos/printing.rs`
- Modify: `src-tauri/platform-checks/windows-printing/src/lib.rs`

**Interfaces:**
- Consumes: `PageRangeCapability`, `PageRange::resolve_zero_based`, and `PrintSettings.page_range`.
- Produces: Windows PDF inspection with known page count and selected-page rendering.
- Produces: macOS CUPS `page-ranges=<canonical>` option for numeric PDF ranges.

- [ ] **Step 1: Write failing macOS option tests**

In the existing macOS printing tests, add:

```rust
let mut settings = default_settings();
settings.page_range = "1-3,5".to_string();
assert!(cups_options(&settings).contains(&"page-ranges=1-3,5".to_string()));
```

Also prove `all` emits no `page-ranges` option and the path remains a separate argument after `--`.

- [ ] **Step 2: Run focused CUPS tests and verify RED**

Run on macOS:

```bash
cargo test --manifest-path src-tauri/Cargo.toml platform::macos::printing::tests -- --nocapture
```

Expected RED: numeric range is not present in CUPS options.

- [ ] **Step 3: Implement CUPS range forwarding and inspection capabilities**

In macOS `inspect`, return:

- `controlled_pdf(None)` for `.pdf`;
- `single_page()` for approved static image formats;
- existing unsupported behavior for other formats.

In `cups_options`, append only the already-canonical string:

```rust
if settings.page_range != "all" {
    options.push(format!("page-ranges={}", settings.page_range));
}
```

The parser guarantees only digits, commas, and hyphens; arguments still travel as separate `OsString` values.

- [ ] **Step 4: Add Windows selected-index tests at a pure seam**

Extract or reuse a pure helper callable in target-independent unit tests:

```rust
assert_eq!(selected_pdf_pages("2-3", 4).unwrap(), vec![1, 2]);
```

Then update the Windows platform-check crate so its compile-time API fixture constructs `PrintSettings { page_range: "all", ... }` and exercises the changed support constructors.

- [ ] **Step 5: Run the pure helper test and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml selected_pdf_pages -- --nocapture
```

Expected RED: helper or selected-page behavior is missing.

- [ ] **Step 6: Implement Windows exact-page rendering**

In Windows `inspect`:

- call `windows_pdf::page_count(path)` for PDF after printer/devmode validation;
- return `controlled_pdf(Some(count))`;
- return `single_page()` for approved images;
- keep native application handlers as `AllOnly`.

In `render_pdf_pages`, replace `for page in 0..pages` with iteration over `resolve_zero_based(pages)`. Preserve cancellation and timeout checks before every selected page, temporary image cleanup, `StartPage`/`EndPage`, and `AbortDoc` error handling.

No selected page may be rendered twice after canonical deduplication.

- [ ] **Step 7: Verify platform GREEN without physical submission**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml platform::macos::printing::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml selected_pdf_pages -- --nocapture
cargo check --manifest-path src-tauri/platform-checks/windows-printing/Cargo.toml --target x86_64-pc-windows-msvc
cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
```

If the Windows target or SDK is unavailable, install no unapproved toolchain silently; record that exact external/local-toolchain gate as pending and continue with all available source tests.

- [ ] **Step 8: Commit the verified platform slice**

```bash
git add src-tauri/src/platform/windows/printing.rs src-tauri/src/platform/macos/printing.rs src-tauri/platform-checks/windows-printing/src/lib.rs
git commit -m "feat: honor selected pages in print adapters"
```

---

## Task 5: Full verification, independent review, and completion evidence

**Files:**
- Modify only if a gate exposes a real regression in the files already in scope.
- Update: `docs/TASKS.md` with source/local completion and explicit external gates.
- Update: `/Users/rohtolos/.hermes/worktrees/docflow/completion-status.md` outside the repository.

**Interfaces:**
- Consumes: all prior tasks.
- Produces: exact local verification evidence and a reviewable branch for installer/release preparation.

- [ ] **Step 1: Run formatting and diff hygiene**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
git diff --check
git status --short
```

Fix only in-scope formatting or whitespace defects.

- [ ] **Step 2: Run the repository's literal local gates individually**

```bash
node tools/version_manager.mjs check
node tools/package_audit.mjs source
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
npm run audit:licenses
```

Do not combine commands with `||` or a pipe that masks exit status. Record each exit code and the real test totals shown by the tool.

- [ ] **Step 3: Run Windows compile gates**

```bash
cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
cargo check --manifest-path src-tauri/platform-checks/windows-printing/Cargo.toml --target x86_64-pc-windows-msvc
cargo check --manifest-path src-tauri/platform-checks/windows-pdf-staging/Cargo.toml --target x86_64-pc-windows-msvc
```

Classify unavailable toolchain/SDK separately from code failure.

- [ ] **Step 4: Audit the final diff**

Verify:

- no credential, token, email body, workbook content, or file path fixture was added;
- presets cannot carry non-scalar/path values;
- every range failure is fail-closed;
- old queues default to `all`;
- no physical submission test was introduced;
- all four locales contain all new keys;
- the original checkout `/Users/rohtolos/Documents/GitHub/docflow` remains clean and unchanged.

- [ ] **Step 5: Request independent code review**

Provide the independent reviewer only:

- the accepted spec path;
- Git diff from base `a439d95...`;
- static scan results;
- exact test/gate results.

Any confirmed security concern or logic error blocks completion. Apply at most two focused fix/review cycles, rerunning every affected gate after the last edit.

- [ ] **Step 6: Update task/ledger evidence**

In `docs/TASKS.md`, mark only source/local behavior that was actually verified. Keep these external gates pending:

- Windows installed runtime;
- physical/virtual printer page output and order;
- Office/Excel worksheet automation;
- signing/publication.

Check ledger slices only after parent verification, not from worker claims.

- [ ] **Step 7: Commit final verified evidence**

```bash
git add docs/TASKS.md
git commit -m "docs: record Printing Order v1 verification"
```

- [ ] **Step 8: Record final candidate identity**

```bash
git status --short
git rev-parse HEAD
git log -1 --oneline
git diff a439d95b468e354c5b99dd929c61beb4f9a4064e..HEAD --stat
```

Expected: repository clean, commits limited to the approved Printing Order v1 scope, and all local gates green. This completes source/local readiness only; installer work proceeds in its own plan after later feature slices are integrated.

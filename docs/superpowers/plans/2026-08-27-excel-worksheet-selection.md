# Excel Worksheet Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Discover Excel worksheets in exact workbook order, let users select/reorder explicit worksheet rows, and print each approved worksheet through a safe Excel-to-PDF-to-controlled-printer pipeline without modifying the source workbook.

**Architecture:** Cross-platform OOXML code discovers immutable ordered worksheet descriptors and expands Batch Print preview rows. Windows-only COM code opens a private staged workbook copy with automation locked down, exports exactly one approved worksheet to a private PDF, and submits that PDF through the existing controlled renderer. Source/local verification and physical Windows/Excel/printer certification remain separate gates.

**Tech Stack:** Rust 2024, Tauri 2, `quick-xml`, ZIP/OOXML, existing filesystem capabilities, Windows `windows` crate COM/OLE APIs, React schema-driven preview, Rust integration/platform-check tests.

**Spec:** `docs/superpowers/specs/2026-08-27-excel-worksheet-selection-design.md`

## Global Constraints

- `.xlsx` and `.xlsm` only; legacy `.xls` remains unsupported.
- Discovery never starts Excel and never reads cell values.
- Source workbook bytes never change.
- Worksheet identity is `(workbookIndex, sheetId, relationshipId, partPath, name, visibility)` and is revalidated before export.
- Visible sheets default enabled; hidden/veryHidden default disabled and require explicit opt-in.
- Worksheet rows accept `pageRange = all` only in this delivery.
- Excel COM never prints directly; it exports one worksheet to one private PDF.
- Macro execution, events, alerts, link updates, recent-file registration, and external refresh are disabled before workbook open.
- Excel 4/XLM macro sheets, external links, connections, query tables, and Internet/Restricted Zone MOTW fail closed in v1.
- No physical print submission in automated tests.
- Windows/Excel/printer runtime smoke remains external until executed on the exact installer.

---

## File Map

- `src-tauri/src/ooxml.rs`: ordered, bounded, traversal-safe worksheet discovery.
- `src-tauri/src/print_queue.rs`: serde-compatible immutable optional worksheet job descriptor and queue persistence.
- `src-tauri/src/cap_fs.rs`: capability-owned staged workbook/PDF handles; no raw user-path reopen.
- `src-tauri/src/operations/batch_print.rs`: preview expansion, revise identity checks, all-only page-range rule.
- `src-tauri/src/platform/excel.rs`: target-independent export supervisor, fake seams,
  generation fence, cleanup ownership, and error classification.
- `src-tauri/src/platform/mod.rs`: exports the target-independent Excel supervisor.
- `src-tauri/src/platform/windows/excel.rs`: production late-bound Excel COM adapter.
- `src-tauri/src/platform/windows/mod.rs`: exports the Windows COM adapter.
- `src-tauri/src/platform/windows/printing.rs`: worksheet export → controlled PDF submit integration.
- `src-tauri/tests/batch_print.rs`: cross-platform expansion/revise/order/resume behavior.
- `src-tauri/platform-checks/windows-printing/`: Windows compile-contract checks without Office/printer submission.
- `frontend/src/types.ts`: additive reorderable editor metadata.
- `frontend/src/App.tsx`: accessible Move Up/Move Down row controls.
- `frontend/src/App.test.tsx`: exact revised worksheet-order behavior.
- `frontend/src/styles.css`: existing-style compact reorder controls.
- `frontend/src/locales/app_{tr,en,ru,az}.arb`: worksheet visibility/export diagnostics.

---

## Task 1: Ordered, identity-bearing OOXML worksheet discovery

**Files:**
- Modify: `src-tauri/src/ooxml.rs`
- Test: unit tests in `src-tauri/src/ooxml.rs`

**Interfaces:**
- Produces `WorksheetVisibility::{Visible, Hidden, VeryHidden}`.
- Produces `WorksheetDescriptor { workbook_index, sheet_id, relationship_id, part_path, name, visibility }`.
- Produces `WorkbookSafetyFacts { has_vba_project, has_xlm_macro_sheet, external_link_count, connection_count, query_table_count }`.
- Produces `OrderedWorkbookDiscovery { worksheets, safety }` from `ordered_workbook_discovery<R: Read + Seek>(source: &mut R)`; discovery never reopens a path.
- Preserves the existing `sheet_infos_from_path` behavior by projecting ordered descriptors into its current name-keyed map only after duplicate checks.

- [ ] **Step 1: Write the ordered-discovery RED tests**

Build synthetic OOXML ZIP fixtures in tests with sheet order `MEC`, `CIV`, `Hidden`, `VeryHidden`, relationship targets, and unrelated workbook members. Assert exact vector order and every identity/state field.

```rust
let mut source = File::open(&path).unwrap();
let discovery = ordered_workbook_discovery(&mut source).unwrap();
let sheets = discovery.worksheets;
assert_eq!(
    sheets.iter().map(|sheet| sheet.name.as_str()).collect::<Vec<_>>(),
    vec!["MEC", "CIV", "Hidden", "VeryHidden"],
);
assert_eq!(sheets[2].visibility, WorksheetVisibility::Hidden);
assert_eq!(sheets[3].visibility, WorksheetVisibility::VeryHidden);
assert_eq!(sheets[0].part_path, "xl/worksheets/sheet7.xml");
```

Add one test per rejection: duplicate name; duplicate `sheetId`; duplicate `r:id`; duplicate resolved part; external relationship target; `../` traversal; non-worksheet relationship; missing target; unknown state; zero worksheets; 251 worksheets. Add bounded safety-fact fixtures for VBA, XLM macro-sheet relationships, external workbook links, connections, query tables, unknown relationship classes, and malformed counts. The parser may report VBA but must flag every other active-data class before any row is authorized.

- [ ] **Step 2: Run RED**

Run:
```bash
cargo test --manifest-path src-tauri/Cargo.toml ooxml::tests::ordered_worksheet -- --nocapture
```
Expected: missing types/function.

- [ ] **Step 3: Implement minimal ordered discovery**

Parse `xl/workbook.xml` sheet elements in document order, resolve IDs through `xl/_rels/workbook.xml.rels`, normalize targets under `xl/`, and reject every ambiguous/unsafe condition before returning any descriptor. Never route through `BTreeMap` before preserving order. Bound all relevant strings and sheet count.

- [ ] **Step 4: Run GREEN and existing OOXML fidelity tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml ooxml::tests::ordered_worksheet -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test ooxml_fidelity -- --nocapture
```

- [ ] **Step 5: Commit verified discovery slice**

```bash
git add src-tauri/src/ooxml.rs
git commit -m "[verified] feat: discover ordered Excel worksheets"
```

---

## Task 2: Expand Batch Print preview into immutable worksheet rows

**Files:**
- Modify: `src-tauri/src/print_queue.rs`
- Modify: `src-tauri/src/operations/batch_print.rs`
- Modify: `src-tauri/tests/batch_print.rs`
- Modify: `frontend/src/types.ts`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/App.test.tsx`
- Modify: `frontend/src/styles.css`
- Modify: four ARB locale files

**Interfaces:**
- Adds serde-defaulted `PrintJob.worksheet: Option<WorksheetDescriptor>`.
- Replaces the ad-hoc signature map with `FileSignature { size, mtime_ns, sha256: Option<String> }` while preserving legacy `size`/`mtimeNs` JSON; worksheet jobs require SHA-256.
- Adds immutable `PrintJob.workbook_safety: Option<WorkbookSafetyFacts>` for worksheet jobs.
- Adds worksheet identity/state columns to preview rows.
- Adds `editor.reorderable = true`; the generic editor renders localized accessible Move Up/Move Down controls.
- Keeps old queues file-level when `worksheet` is absent.
- Produces enabled jobs in exact revised row order.

```rust
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSignature {
    pub size: u64,
    pub mtime_ns: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}
```

`file_signature_from_open_file(file, include_sha256)` seeks/rewinds the already-open
authorized handle, checked-converts nanoseconds to `u64`, and includes SHA-256 only when
requested. Deserializing legacy signatures without `sha256` stays valid; constructing a
worksheet job without a valid SHA-256 is rejected.

- [ ] **Step 1: Write preview expansion RED**

Create one selection containing `first.pdf`, `book.xlsx`, and `last.png`; fake discovery returns three workbook sheets plus safe facts. Assert rows are `first.pdf`, all workbook rows in workbook order, then `last.png`. Assert visible sheets enabled, hidden/veryHidden disabled, each workbook row shares exact `size`/`mtimeNs`/lowercase SHA-256 and safety facts, and no cell values exist in plan/meta JSON. Unsafe XLM/link/connection/query-table facts produce no authorized workbook rows and one localized warning.

- [ ] **Step 2: Observe RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test batch_print preview_expands_excel_worksheets -- --nocapture
```
Expected: one file-level Excel row rather than per-sheet rows.

- [ ] **Step 3: Implement minimal expansion**

Detect `.xlsx`/`.xlsm` case-insensitively after capability authorization, hash the already-open file handle, call ordered OOXML discovery, and derive opaque row IDs from immutable file/worksheet identity. Reject total expanded rows above 250 and every unsafe workbook fact except `has_vba_project`. Keep non-Excel behavior and legacy signatures unchanged.

- [ ] **Step 4: Write and implement revise/revalidation RED→GREEN**

Tests must reject inserted/removed/duplicate rows, forged worksheet fields, changed worksheet state/name/relation/part, source signature drift, enabled zero rows, and any non-`all` worksheet page range. A positive test reorders sheets and enables one hidden sheet; approved queue order must match exactly.

- [ ] **Step 5: Write and implement serde compatibility RED→GREEN**

Deserialize a frozen old queue/job JSON without `worksheet`; assert `None` and unchanged behavior. Persist/reload worksheet jobs; assert the exact descriptor survives retry/resume.

- [ ] **Step 6: Add localized visibility/export diagnostics**

Add exact keys in Turkish, English, Russian, Azerbaijani for visible/hidden/veryHidden, Move Up/Move Down, workbook changed, sheet changed/missing, unsafe XLM/link/connection/query-table/MOTW, all-only page range, Excel unavailable, export timeout, and export failure. Keep sheet names unchanged.

- [ ] **Step 7: Write frontend row-reorder RED**

Render an Excel worksheet preview with `editor.reorderable = true`; click the second row's localized Move Up button and apply. Assert the `engine/revise` payload contains every original row exactly once in the displayed order, including disabled/hidden rows, and immutable descriptor fields are unchanged.

```bash
npx vitest run --config frontend/vite.config.ts frontend/src/App.test.tsx -t "reorders worksheet rows before revise"
```

Expected RED: no accessible Move Up control.

- [ ] **Step 8: Implement accessible generic row ordering and observe GREEN**

Add optional `reorderable?: boolean` editor metadata. Render bounded Up/Down buttons using existing table styles, disable them at edges, and reorder the complete local rows array without regenerating `_rowId`. Rerun the exact RED command and the full `App.test.tsx` file.

- [ ] **Step 9: Verify affected and full suites**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test batch_print -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
npm test
npm run build
git diff --check
```

- [ ] **Step 10: Commit verified queue/UI slice**

```bash
git add src-tauri/src/print_queue.rs src-tauri/src/operations/batch_print.rs src-tauri/tests/batch_print.rs frontend/src/types.ts frontend/src/App.tsx frontend/src/App.test.tsx frontend/src/styles.css frontend/src/locales/app_tr.arb frontend/src/locales/app_en.arb frontend/src/locales/app_ru.arb frontend/src/locales/app_az.arb
git commit -m "[verified] feat: select Excel worksheet print rows"
```

---

## Task 3: Windows Excel COM exporter with a fakeable boundary

**Files:**
- Create: `src-tauri/src/platform/excel.rs`
- Modify: `src-tauri/src/platform/mod.rs`
- Create: `src-tauri/src/platform/windows/excel.rs`
- Modify: `src-tauri/src/platform/windows/mod.rs`
- Modify: `src-tauri/src/platform/windows/printing.rs`
- Modify: `src-tauri/src/print_queue.rs`
- Modify: `src-tauri/src/cap_fs.rs`
- Modify: `src-tauri/tests/batch_print.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Test: unit tests beside exporter + platform-check crate

**Interfaces:**

```rust
pub struct ExcelExportRequest {
    pub request_id: Uuid,
    pub generation: u64,
    pub staged_workbook: StagedWorkbook,
    pub approved_signature: FileSignature,
    pub safety: WorkbookSafetyFacts,
    pub worksheet: WorksheetDescriptor,
    pub deadline: Instant,
    pub cancellation: Arc<AtomicBool>,
}

pub struct StagedWorkbook {
    workspace: TempDir,
    root: CapabilityRoot,
    workbook: AnchoredPath,
    workbook_path: PathBuf,
}

pub struct ExportedWorksheetPdf {
    workspace: StagedWorkbook,
    pdf: AnchoredPath,
    pdf_path: PathBuf,
}

pub trait WorksheetPdfExporter: Send + Sync {
    fn export_pdf(&self, request: ExcelExportRequest)
        -> Result<ExportedWorksheetPdf, PrintFailure>;
}

pub trait WorkbookSourcePolicy: Send + Sync {
    fn validate_source(
        &self,
        source: &AnchoredPath,
        safety: &WorkbookSafetyFacts,
    ) -> Result<(), PrintFailure>;
}

pub trait ControlledPdfSubmitter: Send + Sync {
    fn submit_pdf(
        &self,
        job: &PrintJob,
        pdf: &ExportedWorksheetPdf,
        cancellation: Arc<AtomicBool>,
    ) -> Result<(), PrintFailure>;
}
```

`StagedWorkbook` and `ExportedWorksheetPdf` own private directory capabilities and anchored handles. Neither exposes a user path or borrowed cancellation closure. The COM worker never receives `ControlledPdfSubmitter`. `PrintQueueService` receives an optional pipeline containing the source policy, exporter, and submitter; absence on non-Windows is `Unsupported`.

- [ ] **Step 1: Write fake-exporter orchestration RED**

With fake source-policy/exporter/submitter, assert one owned request per enabled worksheet in exact queue order, no export for disabled rows, source capability/signature/safety passed exactly, policy runs before the existing staging copy, page range forced `all`, generation mismatch/timeout/cancel never reaches submitter, and private cleanup occurs after terminal submission/failure.

- [ ] **Step 2: Observe RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml platform::excel::tests::export_supervisor -- --nocapture
```

Expected RED: missing `ExcelExportRequest`, owned artifact types, exporter/submitter traits, and supervisor.

- [ ] **Step 3: Implement target-independent traits and fake supervisor only**

Add the owned request/artifact types and orchestration state machine in target-independent `platform/excel.rs`. Add an optional Excel pipeline to `PrintQueueService`; for worksheet jobs, its existing `submit_approved_job` source anchor is first checked by a fake `WorkbookSourcePolicy`, then the existing temp directory/root/staged anchor are moved into `StagedWorkbook` instead of being stack-owned. Retain the original anchor for post-export signature verification. Inject fake policy/exporter/submitter and make only the Step 1 supervisor test GREEN. Do not add `platform/windows/excel.rs`, COM dispatch, production MOTW parsing, or production PDF-submit adaptation in this step.

- [ ] **Step 4: Write pure COM configuration/order tests first**

Extract a deterministic operation plan whose exact order can be tested without Office:

```text
CoInitializeEx(APARTMENTTHREADED)
create Excel.Application
AutomationSecurity = ForceDisable
Calculation = Manual
CalculateBeforeSave = false
DisplayAlerts = false
EnableEvents = false
AskToUpdateLinks = false
open private staged copy with UpdateLinks=0, ReadOnly=true, AddToMru=false, IgnoreReadOnlyRecommended=true
resolve Worksheets.Item(workbook_index)
verify Name and visibility
ExportAsFixedFormat PDF, standard quality, IgnorePrintAreas=false, OpenAfterPublish=false
Close SaveChanges=false
Quit
release reverse order
CoUninitialize
```

Tests reject any missing/reordered safety setter before `Open`, any calculate/refresh/update-link call, and any request whose safety facts or source `Zone.Identifier` require Protected View. Missing zone stream is allowed; malformed/Internet/Restricted zone or any other read error fails closed.

- [ ] **Step 5: Implement production late-bound COM**

Only after Step 4 is observed RED, add `platform/windows/excel.rs`, implement the production `WorkbookSourcePolicy` MOTW parser and late-bound COM adapter, and adapt the private Windows PDF renderer through `ControlledPdfSubmitter`. Use the existing direct `windows` crate with only required COM/OLE features. Never spawn PowerShell/VBScript or interpolate commands. Bound BSTR/string lengths and HRESULT mapping. Move the owned `ExcelExportRequest` onto one STA worker. The worker can only return an artifact/result over a channel. The supervisor compares request ID + generation + deadline + owned atomic cancellation before calling the submitter. Late results are dropped. If bounded teardown fails, atomically trip a process-lifetime one-way circuit breaker before returning; later exports fail until restart. Never terminate, attach to, or reuse an Excel process outside the request.

- [ ] **Step 6: Implement immutable private staging**

Before copy, inspect the authorized source's Windows `Zone.Identifier` stream and safety facts. Copy from the already-open capability, never a raw user path, into a private no-clobber temp; verify SHA-256 before open. Never save. After export, queue orchestration reopens the original anchored source and compares the typed signature. Validate PDF nonempty/parseable and return owned cleanup capability. A stuck worker retains/quarantines its own temp lifetime; never log workbook values.

- [ ] **Step 7: Hidden/veryHidden fail-closed behavior**

Call `ExportAsFixedFormat` directly without changing visibility. If Excel refuses, return the explicit export error. Never change worksheet visibility, workbook structure, protection, or save the staged copy.

- [ ] **Step 8: Windows platform-check compile gate**

Update the existing Windows printing platform-check crate's mirrored contracts and dependency features for `FileSignature`, worksheet/safety descriptors, owned request/artifacts, exporter/submitter traits, Windows COM adapter, and `Zone.Identifier` reader. Run:

```bash
cargo check --manifest-path src-tauri/platform-checks/windows-printing/Cargo.toml --target x86_64-pc-windows-msvc
```

If the macOS host lacks Windows SDK headers/link libraries, record that as environment-blocked and run this gate on the Windows certification host; do not call it a source pass locally.

- [ ] **Step 9: Run local/fake/full gates**

Run exporter fake tests, batch-print tests, Rust suite, fmt, clippy, `npm test`, `npm run build`, source/license audits, Windows platform-check, and diff-check without launching Excel or a printer.

- [ ] **Step 10: Commit verified source slice**

```bash
git add src-tauri/src/platform/excel.rs src-tauri/src/platform/mod.rs src-tauri/src/platform/windows/excel.rs src-tauri/src/platform/windows/mod.rs src-tauri/src/platform/windows/printing.rs src-tauri/src/print_queue.rs src-tauri/src/cap_fs.rs src-tauri/tests/batch_print.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/platform-checks/windows-printing
git commit -m "[verified] feat: export selected Excel worksheets to PDF"
```

---

## Task 4: Exact controlled-PDF submission and recovery semantics

**Files:**
- Modify: `src-tauri/src/platform/windows/printing.rs`
- Modify: `src-tauri/src/platform/excel.rs`
- Modify: `src-tauri/src/print_queue.rs`
- Modify: `src-tauri/src/cap_fs.rs`
- Test: Windows printing unit/platform tests + `src-tauri/tests/batch_print.rs`

- [ ] **Step 1: Write submission RED tests**

Fake exporter returns a capability-owned multi-page PDF. Assert `ControlledPdfSubmitter`—not native Excel printing—receives it only for the current generation with approved printer/settings, `pageRange=all`, owned cancellation token, and existing printer uncertainty classes unchanged. Mutate the original source signature after fake export and assert the submitter call count remains zero.

- [ ] **Step 2: Implement export→render→cleanup**

Immediately before staging, reauthorize the source grant, anchor/open it, revalidate typed signature + descriptor + safety facts, and inspect MOTW. Copy through capability APIs into owned staging, export, reopen the retained original anchor and require the approved signature again, then validate the PDF and submit through `ControlledPdfSubmitter`. Source drift after export fails with zero printer submission. Cleanup only after the renderer no longer needs the file. Never retry a post-`StartDocW` uncertain result automatically.

- [ ] **Step 3: Failure/cancellation matrix**

Tests cover cancellation before export, cancellation during export, COM timeout before submission, invalid/empty PDF, renderer failure before `StartDocW`, uncertain failure after `StartDocW`, Excel teardown failure, cleanup failure, and source drift. Only post-submission paths use printer uncertainty; pre-submission export failures are failed/cancelled with zero printer effect.

- [ ] **Step 4: Retry/resume**

A retry revalidates and re-exports from the authorized source; it never trusts/reuses an old temp PDF. Restart recovery keeps existing active/uncertain queue semantics.

- [ ] **Step 5: Verify local source gates**

Run focused fake/platform tests, full Rust suite, source/license audits, fmt, clippy, `npm test`, `npm run build`, Windows printing platform-check, and diff-check.

- [ ] **Step 6: Commit verified integration**

```bash
git add src-tauri/src/platform/excel.rs src-tauri/src/platform/windows/printing.rs src-tauri/src/print_queue.rs src-tauri/src/cap_fs.rs src-tauri/tests/batch_print.rs
git commit -m "[verified] feat: print selected Excel worksheets safely"
```

---

## Task 5: Windows/Excel/printer runtime certification

**Files:**
- Modify only release evidence/task docs after successful smoke.
- Never modify production workbook originals.

- [ ] **Step 1: Build exact Windows installer**

On a clean Windows host with pinned Rust/Node/MSVC/Windows SDK and Excel installed, build the exact committed candidate using the canonical script. Record commit, installer path, size, SHA-256, toolchain versions, and package audit.

- [ ] **Step 2: Run installer smoke**

Install/launch/uninstall with `tools/smoke_windows_installer.ps1`. Verify no Python/PyInstaller/sidecar artifacts, native OCR resources present, and app launches.

- [ ] **Step 3: Create sanitized workbook fixtures**

Use synthetic or approved sanitized `.xlsx` and `.xlsm` fixtures with visible/hidden/veryHidden sheets, print areas, multiple pages, formulas, chart/object noise, external links, macros, protected workbook, and password prompt negative case. Record source hashes; do not open/modify QC originals.

- [ ] **Step 4: Excel runtime matrix**

Verify discovery order/state, visible default enablement, explicit hidden opt-in, exact one-sheet output, print area respected, source hash/mtime unchanged, no macro/event/link execution, no save prompt, timeout/cancel cleanup, missing Excel, protected/blocked workbook fail-closed, and no orphan Excel process.

- [ ] **Step 5: Physical printer matrix**

Only with explicit printer approval: print one bounded synthetic worksheet job. Record exact page/order/copy result and queue identity. Test cancellation/uncertainty without automatic retry. Never use production documents.

- [ ] **Step 6: Close evidence honestly**

Mark source/local verified separately from Windows/Excel runtime and physical printer certification. Do not claim full release readiness if any exact-host gate is missing.

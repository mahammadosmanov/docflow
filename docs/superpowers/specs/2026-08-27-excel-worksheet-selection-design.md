# Excel Worksheet Selection Design

**Date:** 2026-08-27
**Status:** Pre-approved under unattended full-authority instruction; implementation starts only after Printing Order v1 is source/local verified
**Issue:** #25
**Operation:** `files.batch_print`

## 1. Goal

When a selected print input is `.xlsx` or `.xlsm`, discover its worksheets without starting Excel, expose one preview row per worksheet in workbook order, let the user enable and reorder rows, and print each approved worksheet through DocFlow's controlled PDF pipeline.

The source workbook must remain immutable. Worksheet selection is per order and never enters a print preset.

## 2. Chosen architecture

Use this pipeline:

```text
OOXML discovery
  → immutable per-sheet preview rows
  → revise/revalidate against source signature and worksheet identity
  → private staged workbook copy
  → Excel COM ExportAsFixedFormat for exactly one worksheet
  → private PDF
  → existing controlled PDF renderer and printer queue observation
  → private artifact cleanup
```

Do not call `Worksheet.PrintOut`. Direct Excel submission cannot provide DocFlow's reliable job identity, cancellation, uncertainty classification, and exact per-page rendering semantics.

## 3. Non-goals

This slice does not:

- edit or save source workbook content;
- store selected worksheets in presets;
- automate Excel on macOS;
- claim Windows/Excel runtime certification from cross-compilation;
- use PowerShell, VBScript, shell interpolation, or an arbitrary child process;
- update links, run VBA/XLM macros, refresh data connections, or recalculate external data;
- print chart sheets, dialog sheets, macro sheets, or unsupported relationship targets;
- add page-range selection within an Excel worksheet in the first delivery; worksheet rows accept `all` only;
- submit a physical print job in automated tests.

## 4. OOXML discovery

### 4.1 Ordered worksheet descriptor

Add a read-only ordered API alongside the existing name-keyed `sheet_infos_from_path` update API:

```rust
pub enum WorksheetVisibility {
    Visible,
    Hidden,
    VeryHidden,
}

pub struct WorksheetDescriptor {
    pub workbook_index: u32,
    pub sheet_id: u32,
    pub relationship_id: String,
    pub relationship_type: String,
    pub part_path: String,
    pub name: String,
    pub visibility: WorksheetVisibility,
}

pub struct OrderedWorkbookDiscovery {
    pub worksheets: Vec<WorksheetDescriptor>,
    pub safety: WorkbookSafetyFacts,
}

pub fn ordered_workbook_discovery<R: Read + Seek>(
    source: &mut R,
) -> Result<OrderedWorkbookDiscovery, OoxmlError>;
```

Rules:

- Preserve `<sheets><sheet ...>` document order; do not route through `BTreeMap`.
- `workbook_index` is 1-based and immutable within the approved plan.
- Parse absent `state` as `visible`; accept only `visible`, `hidden`, and `veryHidden`.
- Preserve exact sheet name, `sheetId`, `r:id`, normalized worksheet part path, and relationship type.
- Accept only the exact Transitional `http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet` or Strict `http://purl.oclc.org/ooxml/officeDocument/relationships/worksheet` relationship URI, and require its target to resolve under `xl/` without traversal.
- Reject duplicate names, duplicate sheet IDs, duplicate relationship IDs, duplicate part paths, missing targets, external targets, non-worksheet relationships, and zero worksheets.
- Bound worksheet count to 250 per workbook and total expanded preview rows to the existing `MAX_FILES` limit of 250.
- Inspect workbook protection, external links/connections, and macro-sheet relationships as safety facts for the Windows exporter. Ordinary formulas and print areas are allowed.

The discovery API consumes an already-authorized `Read + Seek` handle and never reopens a
user path. The existing `sheet_infos_from_path` remains for targeted OOXML updates: it opens
once under its existing caller contract, delegates to discovery, then projects the ordered
descriptors into its current map.

### 4.2 Preview behavior

For each Excel input:

- expand to one row per worksheet in workbook order;
- visible worksheets start enabled;
- hidden and veryHidden worksheets start disabled but remain visible in the table with localized state labels;
- non-Excel inputs remain one file-level row;
- preserve the selected file order, with a workbook's sheet rows contiguous at that position;
- one source workbook signature is copied into every derived row;
- no Excel process starts during preview.

Discovery also returns bounded `WorkbookSafetyFacts` without reading cells:

```rust
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookSafetyFacts {
    pub has_vba_project: bool,
    pub has_xlm_macro_sheet: bool,
    pub has_workbook_protection: bool,
    pub external_link_count: u32,
    pub connection_count: u32,
    pub query_table_count: u32,
}
```

`.xlsm` may contain VBA, but it is opened only after `AutomationSecurity =
ForceDisable`. Excel 4/XLM macro sheets, external workbook links, data connections,
and query tables are rejected in v1 because the exporter cannot prove that opening
them is side-effect-free. Workbook protection is carried as an immutable fact and the
Task 3 source policy rejects it before COM `Open`; v1 does not infer sheet protection,
password state, or decryptability. Unknown or malformed relationship classes fail closed.

If OOXML discovery fails, no rows from that workbook are authorized; add one localized warning and continue other files using existing preview semantics.

## 5. Immutable row and job contract

Each worksheet row carries immutable metadata:

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

The JSON field is exactly `mtimeNs`, matching the existing queue contract. Legacy
file-level jobs may omit `sha256`; every worksheet job requires a lowercase 64-hex
SHA-256. Preview computes the typed signature from the already-authorized open file,
and revise/apply compare the same type rather than constructing ad-hoc maps.

```json
{
  "_rowId": "opaque UUID",
  "path": "source workbook display path",
  "format": "xlsx",
  "worksheet": {
    "workbookIndex": 2,
    "sheetId": 17,
    "relationshipId": "rId4",
    "relationshipType": "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet",
    "partPath": "xl/worksheets/sheet3.xml",
    "name": "MEC",
    "visibility": "hidden"
  },
  "signature": { "size": 123, "mtimeNs": 456, "sha256": "..." },
  "workbookSafety": {
    "hasVbaProject": false,
    "hasXlmMacroSheet": false,
    "hasWorkbookProtection": false,
    "externalLinkCount": 0,
    "connectionCount": 0,
    "queryTableCount": 0
  },
  "enabled": false,
  "pageRange": "all"
}
```

`PrintJob` gains a serde-defaulted optional `worksheet` descriptor. Old queues remain file-level jobs when the field is absent.

Queue JSON remains field-compatible, including legacy file-level jobs and missing
page-range defaults. A queue created before persisted source/root filesystem identities
cannot be resumed safely from stored path strings alone: Resume fails closed with
`error.print.queueSourceReselectionRequired`, and the existing resume card keeps its
Discard action so the user can discard the stale queue and explicitly reselect the files.
No automatic migration converts an unverified visible path into a capability.

The worksheet descriptor, signature, and safety facts are not editable. The user may
edit only existing print settings, row enablement, and row order. The generic editor
gets an explicit `reorderable` flag and accessible Move Up/Move Down controls; disabled
and hidden rows remain movable, and `engine/revise` receives the exact displayed order.

## 6. Revise and apply revalidation

`engine/revise`:

1. requires every original row ID exactly once;
2. rejects inserted, removed, duplicate, or forged rows;
3. preserves submitted row order for enabled rows;
4. compares every immutable worksheet field to the original row;
5. revalidates the source workbook signature;
6. reruns OOXML discovery;
7. matches the exact tuple `(workbook_index, sheet_id, relationship_id, relationship_type, part_path, name, visibility)`;
8. rejects any drift before replacing the cached plan;
9. requires `pageRange == "all"` for Excel worksheet rows in this slice.

Apply performs the same signature and descriptor revalidation immediately before staging/export. A forged plan cannot create a worksheet job outside approved metadata.

## 7. Windows Excel exporter

### 7.1 Platform boundary

Add a target-independent export supervisor behind traits that can be faked in
cross-platform tests. The queue service creates one capability-backed private staged
workbook from the already-authorized open source file; the exporter never reopens the
user path. The request, deadline/cancellation state machine, private artifact ownership,
cleanup, and late-completion fence live in `platform/excel.rs`; only the production COM
dispatch implementation lives in `platform/windows/excel.rs`:

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

`platform/mod.rs` exports the target-independent supervisor. `platform/windows/mod.rs`
exports the production COM adapter only on Windows. `StagedWorkbook` owns its private
directory capability and anchored workbook handle; `ExportedWorksheetPdf` owns the PDF
capability and cleanup lifetime. The Windows print adapter exposes the existing controlled
PDF path through `ControlledPdfSubmitter`; tests inject fake exporter and submitter
implementations without compiling or starting Office.

`PrintQueueService` receives an optional Excel pipeline containing
`WorkbookSourcePolicy`, `WorksheetPdfExporter`, and `ControlledPdfSubmitter`. For a
worksheet job, `submit_approved_job` keeps the original `AnchoredPath`, validates MOTW and
safety facts through the policy, moves its existing temp directory/root/staged anchor into
`StagedWorkbook`, and retains the original anchor for the post-export signature check.
Non-worksheet staging and every non-Windows constructor remain unchanged; a missing Excel
pipeline is `Unsupported`, never a native-print fallback.

### 7.2 COM safety sequence

On a dedicated single-threaded apartment worker:

1. before staging, read the authorized Windows `Zone.Identifier` stream; reject Internet/
   Restricted Zone, malformed content, or any error other than stream-not-found;
2. require approved `WorkbookSafetyFacts` and reject XLM, external links, connections,
   and query tables;
3. copy the already-open authorized source into a capability-owned private directory;
4. hash the staged copy and require exact equality with the approved source hash;
5. initialize COM STA and create a fresh `Excel.Application` late-bound;
6. set `AutomationSecurity = msoAutomationSecurityForceDisable`, `Calculation =
   xlCalculationManual`, `CalculateBeforeSave = false`, `EnableEvents = false`,
   `DisplayAlerts = false`, `AskToUpdateLinks = false`, and `Visible = false` before open;
7. open the staged copy with `UpdateLinks = 0`, `ReadOnly = true`, empty passwords,
   `IgnoreReadOnlyRecommended = true`, `Notify = false`, `AddToMru = false`, and normal
   load mode; never call calculate, refresh, update-link, repair, or Protected View APIs;
8. resolve `Worksheets.Item(workbook_index)` and verify exact name and visibility;
9. call `Worksheet.ExportAsFixedFormat` to a new private PDF with standard quality,
   `IgnorePrintAreas = false`, and `OpenAfterPublish = false`;
10. close workbook with `SaveChanges = false`, quit Excel, and release in reverse order;
11. return the capability-owned exported PDF; queue orchestration rechecks the original
    anchored source signature after export and owns all cleanup.

Any workbook password prompt, Protected View, repair prompt, macro/XLM execution route, external-link update requirement, missing Excel, descriptor mismatch, timeout, cancellation, COM exception, or empty/malformed PDF fails closed.

Hidden/veryHidden rows are explicit opt-in. The exporter first attempts direct `ExportAsFixedFormat` without changing visibility. If Excel refuses, fail closed; v1 never changes visibility, workbook structure, or protection, even on the staged copy.

### 7.3 Timeout and uncertainty

Before PDF submission, a COM timeout/cancellation is a failed or cancelled no-print result.
`ExcelExportRequest` owns its cancellation token and all staged state, so a detached STA
worker never borrows caller memory. The worker can only return `ExportedWorksheetPdf`; it
has no reference to `ControlledPdfSubmitter`. The supervisor accepts a result only when
request ID and generation still match and cancellation/deadline remain clear. A late
result is destroyed without submission. If bounded cooperative teardown fails, atomically
trip a process-lifetime one-way circuit breaker before returning, reject every later Excel
request until restart, and leave the worker owning its quarantined temp lifetime. Never
kill, attach to, or reuse an Excel process not created by that request.

After the PDF renderer calls `StartDocW`, retain the existing Windows print uncertainty rules unchanged.

## 8. Controlled printing and cleanup

A worksheet job's platform submit path:

1. revalidates source and descriptor;
2. exports one worksheet to one PDF;
3. reopens the original anchored source and requires the approved signature again;
4. validates PDF page count and structure;
5. constructs an internal controlled PDF job with the original settings and `pageRange = all`;
6. invokes the existing PDF renderer only after the post-export source recheck passes;
7. preserves existing printer job observation, retry, cancellation, and uncertainty semantics;
8. removes the private PDF and staged workbook after terminal success/failure/cancellation when safe;
9. leaves an audit-safe cleanup locator, not workbook data, if cleanup cannot be proven.

Queue order is the exact enabled revised row order. Copies/collation expand worksheet jobs using existing logic. Retry re-exports from the still signature-bound source; it never reuses an untrusted prior PDF.

## 9. Security and privacy

- The source workbook is read-only and never saved.
- No workbook cell values enter logs, cloud metadata, queue summaries, or preset storage.
- Queue/job metadata contains only source capability identity, file signature, and worksheet identity/state.
- Macro execution is disabled before open; events, alerts, link updates, and add-to-recent are disabled.
- Paths never enter a command line or shell.
- Temporary directories are private, randomly named, and no-clobber.
- COM automation is available only on Windows and only for an approved immutable plan owned by the current caller/session.
- Hidden/veryHidden metadata is user-visible; no hidden worksheet is silently enabled.

## 10. Error/localization keys

Add Turkish, English, Russian, and Azerbaijani keys for:

- workbook discovery failure;
- workbook has too many worksheets / expanded order is too large;
- worksheet visible/hidden/veryHidden labels;
- worksheet identity changed;
- workbook changed after preview;
- only `all` page range is supported for Excel worksheet jobs;
- Excel is unavailable;
- workbook needs password, repair, Protected View, or unsafe link/macro handling;
- worksheet PDF export failed/timed out/cancelled;
- private export cleanup requires review.

## 11. TDD slices

1. Ordered OOXML descriptor parser: order, state, exact identity, relationship type/target, duplicate and bound failures.
2. Batch preview expansion: mixed file order, visible defaults, hidden labels, maximum rows, no Excel start.
3. Revise: exact row identity/order, descriptor immutability, source drift, page-range all-only.
4. Queue compatibility: optional worksheet field, copies/collation/order, persistence, resume, legacy jobs.
5. Export lifecycle seam: exact staged hash, request arguments, one worksheet/one PDF, cancellation/timeout, cleanup, source unchanged.
6. Windows COM compile fixture: typed automation calls and release order, with no Excel process in automated tests.
7. End-to-end fake exporter: revised worksheet rows produce controlled PDF submissions in exact order.
8. Full regression, formatting/clippy, frontend build/tests, Windows cross-check, independent review.

Every production behavior follows an observed focused RED.

## 12. Acceptance criteria

- `.xlsx` and `.xlsm` inputs show worksheets in workbook order with visible/hidden/veryHidden state.
- Visible sheets are enabled by default; hidden states require explicit selection.
- The user can disable and reorder worksheet rows; revised enabled order is exact.
- Worksheet identity and workbook signature are revalidated at revise and immediately before export.
- A selected worksheet becomes exactly one private PDF and one controlled print job per physical copy.
- Presets never contain file or worksheet selection.
- The source workbook hash and metadata remain unchanged.
- Existing non-Excel print behavior and legacy queue fields remain compatible; queues
  predating persisted filesystem identities use the explicit discard-and-reselect
  migration instead of automatic resume.
- Local/cross-platform automated tests never start Excel or print physically.
- All supported locale keys exist.

## 13. External certification

Source/local completion remains distinct from:

- exact Windows installer candidate installation;
- supported Microsoft Excel version matrix;
- real `.xlsx` and macro-disabled `.xlsm` export;
- visible/hidden/veryHidden behavior;
- print area, scaling, orientation, formulas, charts/images, and page-layout fidelity;
- protected/password/repair/Protected View failure behavior;
- COM timeout and orphan `EXCEL.EXE` cleanup;
- exact worksheet order on a controlled virtual or physical printer.

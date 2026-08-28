# RFI Follow Up Sync Design

**Date:** 2026-08-27
**Status:** Proposed; awaiting coordinator review and a docs-only commit before implementation
**Issue:** #26
**Operation:** `rfi.follow_up_sync`
**Depends on:** accepted S4 at `7c33e4beaa7af1ba595ef52eed5e46bac8fd2d14`

## 1. Goal

Add a separate, explicit RFI Follow Up synchronization operation. It reads statuses from an authorized RFI Log workbook, discovers one unambiguous `RFI No.`/`Status` table in an authorized Follow Up workbook, previews every populated Follow Up row, and updates only explicitly enabled status cells.

The only automatic mappings are:

| Exact source status | Exact Follow Up value |
|---|---|
| `Completed` | `Completed` |
| `Ongoing` | `In Progress` |

Everything else is non-actionable manual review. Preview and revise are inert. Apply revalidates both workbook identities, bytes, metadata, layout, source evidence, and selected target cells immediately before any backup or canonical-leaf effect. A verified no-clobber backup and a durable write-ahead retained-publication transaction preserve every competing byte sequence: the current canonical name is displaced destination-no-clobber to a unique retained leaf and verified, then the empty canonical name is created with create-new/O_EXCL and populated through that already-open destination handle from an already-open verified private output handle. Apply, recovery, and undo never overwrite or automatically delete a canonical, retained, backup, recovery, or mismatched leaf. Successful apply deliberately keeps the displaced original as non-authoritative recovery evidence.

## 2. Chosen architecture

Use the existing Rust authority, generic operation schema, plan store, filesystem grants, OOXML package patcher, journal, recovery list, cloud-metadata projection, and undo path:

```text
opaque RFI Log + Follow Up file selections
  → operation-scoped read / read-write directory capabilities
  → anchored handles (never a raw-path reopen for workbook reads)
  → namespace-strict bounded OOXML discovery and exact status evidence
  → immutable review rows; only safe `enabled` values are editable
  → `engine/revise` exact-row and current-source revalidation
  → atomic in-place `PlanStore` replacement under the old digest
  → `engine/apply` operation-specific workbook executor
  → create and verify a persistent private package-preserving exact-cell output
  → immediate source/target fingerprint + selected-cell revalidation
  → verified same-parent no-clobber backup + journaled private-artifact directory
  → persist original/backup/private-output hashes, identities, retained locator, and phase
  → destination-no-clobber displace the canonical name; verify the retained bytes/identity
  → atomically secure-create canonical O_EXCL; journal owned-handle identity before writing
  → stream and verify through that destination handle; canonical is visibly partial while writing
  → hash/identity crash reconciliation that never overwrites, restores, or deletes an unknown leaf
  → the same retained-evidence transaction for exact `engine/undo`
```

`rfi.follow_up_sync` is not a mode of `rfi.log_add`. The operation has its own registry entry, preview/revise functions, plan rows, warning keys, and apply dispatch. `rfi.log_add` keeps its existing plan and update behavior.

No new React-reachable backend method is added. The operation uses the existing exact methods:

- `paths/select`
- `engine/preview`
- `engine/revise`
- `engine/apply`
- `engine/cancel`
- `engine/undo`
- `recovery/list`

## 3. Non-goals

This slice does not:

- add or edit RFI Log rows;
- combine Follow Up synchronization with `rfi.log_add`;
- implement issue #26 reply-content or Outlook `ReplyAll` draft work; both are outside this S5 workbook design and never part of this workbook transaction;
- accept `.xls`, CSV, Google Sheets, or an arbitrary database;
- search by filename, substring, OCR repair, Levenshtein distance, punctuation repair, or nearby-cell inference;
- infer a table from formatting, merged headings, a sheet name, or relative placement;
- map `Cancelled`, `Rejected`, blank, differently cased, or unknown source statuses;
- let the user edit an RFI number, source status, target coordinate, proposed status, disposition, or reason;
- enable a manual-review or already-synchronized row;
- overwrite an unknown nonblank Follow Up status;
- recalculate formulas, update links, run macros, or start Office during automated preview/apply tests;
- claim Microsoft Excel compatibility from synthetic ZIP tests;
- add a second backend/runtime, shell command, PowerShell/VBScript path, or new dependency.

## 4. Registry and wire contract

### 4.1 Operation definition

Insert this fifth operation in `operation_registry`, after `rfi.log_add` and before `files.batch_print`, preserving the exact existing relative order of all four operations. The complete order is `rfi.process_scans`, `rfi.log_add`, `rfi.follow_up_sync`, `files.batch_print`, `rfi.stage_signed_first_pages`. Use the existing `operation` and `parameter` constructors:

```rust
let rfi_follow_up = operation(
    "rfi.follow_up_sync",
    vec![
        parameter(
            "rfi_log_workbook",
            ParamKind::File,
            "param.rfi.followUp.rfiLogWorkbook",
            Value::Null,
            true,
            Vec::new(),
            BTreeMap::new(),
            Map::new(),
            Some("param.rfi.followUp.rfiLogWorkbook.help"),
        ),
        parameter(
            "follow_up_workbook",
            ParamKind::File,
            "param.rfi.followUp.followUpWorkbook",
            Value::Null,
            true,
            Vec::new(),
            BTreeMap::new(),
            Map::new(),
            Some("param.rfi.followUp.followUpWorkbook.help"),
        ),
    ],
);
```

Both pickers use the existing `Excel (.xlsm, .xlsx)` filter. File selections remain opaque `PathSelection` DTOs in the webview. `filter_saved_operation_params` already excludes file parameters, so neither visible path nor selection ID is saved as an operation parameter or preset.

The frozen `protocol1_operations.json` snapshot becomes an additive five-operation snapshot. Protocol version remains `1`; existing method names and the first four operation definitions do not change. `smoke.rs` must expect five operations rather than four.

### 4.2 Preview values

The webview sends opaque `PathSelection` values. `NativeBackend::materialize_preview` first resolves each one against the current caller, exact operation ID, exact parameter name, and `SelectionKind::File`, then the operation decodes the trusted internal envelope:

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RfiFollowUpPreviewValues {
    #[serde(rename = "rfi_log_workbook")]
    rfi_log_workbook: PathBuf,
    #[serde(rename = "follow_up_workbook")]
    follow_up_workbook: PathBuf,
}
```

The explicit Serde names are the registry parameter names. `NativeBackend::materialize_operation_values` reinserts each materialized path under the unchanged snake_case `name`, so no camelCase transform is permitted at this internal decode boundary. Both registry defaults are `null`, so the user must explicitly select each workbook and no persisted visible path is trusted. `NativeBackend::preview_rfi_follow_up` rejects selecting the same filesystem object for both roles. It issues:

- a `GrantAccess::Read` grant rooted at the RFI Log parent;
- a `GrantAccess::ReadWrite` grant rooted at the Follow Up parent.

The plan binding contains both grant IDs. The RFI Log grant can never authorize a write. The Follow Up grant authorizes the selected target and its `.docflow-backups` child, but the immutable plan authorizes only the exact selected workbook and exact cells.

Add these crate-internal authority helpers so preview reads use the same capability boundary as apply:

```rust
// paths.rs
pub(crate) fn issue_provisional<'a>(
    &'a self,
    scope: &GrantScope,
    requests: &[(PathBuf, GrantAccess)],
) -> Result<ProvisionalPathGrants<'a>, BackendError>;

pub(crate) fn authorize_capability_for_grant(
    &self,
    grant: &PathGrant,
    candidate: &Path,
    intent: PathIntent,
) -> Result<AuthorizedPath, BackendError>;

// fs_ops.rs
pub(crate) fn issue_provisional_grants<'a>(
    &'a self,
    scope: &GrantScope,
    requests: &[(PathBuf, GrantAccess)],
) -> Result<ProvisionalPathGrants<'a>, BackendError>;

pub(crate) fn anchor_granted_path(
    &self,
    grant: &PathGrant,
    path: &Path,
    intent: PathIntent,
) -> Result<AnchoredPath, BackendError>;
```

`issue_provisional` accepts 1–8 requests, issues them in order, revokes already-issued IDs on any issue failure, and returns the existing drop-revoked `ProvisionalPathGrants`; the authority method only delegates. Follow Up passes exactly `[rfi-parent/Read, follow-up-parent/ReadWrite]`, caches a plan with cloned grant IDs, and calls `commit()` only after cache success. `authorize_capability_for_grant` looks up `grant.grant_id`, removes/rejects expiry, requires the complete stored `PathGrant` to equal the supplied issuance result, validates access and lexical/capability containment, and returns the stored-root `AuthorizedPath`. `anchor_granted_path` delegates to it and calls `anchor()`. Neither accepts a caller-supplied scope. Domain code receives `&mut File` from `AnchoredPath::open_read`, not a user pathname.

## 5. Exact limits and normalization

The new module `src-tauri/src/rfi_follow_up.rs` owns these constants:

```rust
pub(crate) const OPERATION_ID: &str = "rfi.follow_up_sync";
const SOURCE_FIRST_DATA_ROW: u32 = 10;
const MAX_SOURCE_DATA_ROWS_PER_SHEET: u32 = 10_000;
const MAX_HEADER_ROWS: u32 = 32;
const MAX_HEADER_COLUMNS: u32 = 64;
const MAX_FOLLOW_UP_DATA_ROWS: u32 = 10_000;
const MAX_PREVIEW_ROWS: usize = 500;
const MAX_SOURCE_EVIDENCE_PER_RFI: usize = 500;
const MAX_PREVIEW_AUTHORITY_BYTES: u64 = 8 * 1024 * 1024;
// ooxml.rs: crate-visible parser/apply contract
pub(crate) const MAX_MERGE_VALIDATION_COMPARISONS_PER_WORKBOOK: u64 = 1_000_000;
pub(crate) const MAX_APPLY_MERGE_COMPARISONS: u64 = 2_000_000;

// rfi_follow_up.rs: preview/revise source+target account
const MAX_FOLLOW_UP_MERGE_COMPARISONS: u64 = 2_000_000;
const MAX_CELL_TEXT_CHARS: usize = 20_000;
```

The existing OOXML package limits remain authoritative: 256 MiB package, 512 MiB total uncompressed bytes, 64 MiB per entry, 4,096 entries, 250 worksheets, 256 XML depth, and 1,000,000 XML nodes.

Normalization is deliberately narrow:

- **RFI value:** Unicode NFC, trim leading/trailing Unicode whitespace, uppercase, then require the entire value to match `AZE-SUN-INT-QMS-RFI-[A-Z]{3}-[0-9]{4}`. Equality is equality of those entire canonical strings.
- **Status value:** Unicode NFC plus leading/trailing Unicode-whitespace trim only. Comparison remains case-sensitive.
- **Header value:** Unicode NFC, leading/trailing Unicode-whitespace trim, then ASCII uppercase. The only accepted RFI header token is `RFI NO.` and the only accepted status header token is `STATUS`.

There is no internal-whitespace collapse, punctuation deletion, substring extraction, wrapped-number recovery, OCR substitution, discipline repair, or fuzzy fallback. The existing permissive `rfi_code::find`/`canonical_and_find` functions are not used by this operation.

Any source or Follow Up cell text longer than 20,000 Unicode scalar values fails preview. If a source sheet extends beyond row `10_009`, or a chosen Follow Up table extends more than 10,000 rows below its header, preview fails rather than silently truncating. More than 500 populated Follow Up review rows also fails rather than returning a partial plan. A canonical RFI may contribute at most 500 source evidence rows across the four projected sheets. The 501st evidence row for the same canonical RFI fails the whole preview with `error.rfi.followUp.limitExceeded` before any per-target clone, row DTO, editor JSON, or step authority is built; it is never truncated or summarized. The count limit is not the memory bound: the complete serialized preview/editor plus immutable `FollowUpStepMeta` authority projection must also fit `MAX_PREVIEW_AUTHORITY_BYTES` (8 MiB). Exceeding either independent limit fails with `error.rfi.followUp.limitExceeded`; being exactly at a count boundary does not override the byte budget.

## 6. OOXML read and mutation APIs

### 6.1 Anchored, namespace-strict read API

Refactor the existing path reader around this handle API while keeping `read_workbook(&Path)` behavior compatible:

```rust
pub fn read_workbook_from_reader<R: Read + Seek>(
    source: &mut R,
) -> Result<WorkbookData, OoxmlError>;

pub fn package_has_signature<R: Read + Seek>(
    source: &mut R,
) -> Result<bool, OoxmlError>;
```

`read_workbook(&Path)` remains an adapter that opens once and delegates. Preview/revise/apply for Follow Up call `ordered_workbook_discovery`, `read_workbook_from_reader`, and `package_has_signature` on anchored handles, rewinding between calls.

Extend worksheet read facts without changing existing cell values:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CellRange {
    pub first_row: u32,
    pub first_column: u32,
    pub last_row: u32,
    pub last_column: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorksheetData {
    pub max_row: u32,
    pub max_column: u32,
    pub cells: BTreeMap<(u32, u32), CellData>,
    pub column_widths: BTreeMap<u32, f64>,
    pub has_sheet_protection: bool,
    pub merged_ranges: Vec<CellRange>,
}

impl WorksheetData {
    pub fn merged_range_containing(&self, row: u32, column: u32) -> Option<&CellRange> {
        self.merged_ranges.iter().find(|range| {
            range.first_row <= row
                && row <= range.last_row
                && range.first_column <= column
                && column <= range.last_column
        })
    }
}
```

The existing XML model already stores expanded element/attribute namespaces. Worksheet parsing now requires one root whose expanded name is `{http://schemas.openxmlformats.org/spreadsheetml/2006/main}worksheet` or `{http://purl.oclc.org/ooxml/spreadsheetml/main}worksheet`, exactly one direct canonical `sheetData` child, canonical direct `row` children, and canonical direct `c` children. Every physical canonical row must have a valid in-range `row@r`, and that row identity must be unique across `sheetData`, including empty rows. The parser updates physical `max_row` from each validated row element before inspecting cells, so `<row r="10010"/>` enforces the row limit. Cells must agree with their containing row identity. Cell value/formula/inline-string children must use the same accepted namespace. A local-name match is never sufficient. A canonical `<c>` outside the hierarchy, duplicate physical row identity, row/cell disagreement, duplicate cell coordinate, or foreign shadow is malformed before map insertion. Thus disjoint cells from two `<row r="1">` elements can never synthesize one trusted logical row.

`OoxmlError` becomes a structured error with crate-visible `OoxmlErrorKind::{Invalid, LimitExceeded}` and `pub(crate) fn kind(&self) -> OoxmlErrorKind`; constructors and the message remain private to `ooxml.rs`. `read_workbook_from_reader` keeps its existing signature and enforces the crate-visible parser constant internally. `rfi_follow_up::read_workbook_for_follow_up<R: Read + Seek>` is the only S5 reader adapter: it delegates to `read_workbook_from_reader` and maps only `LimitExceeded` to `error.rfi.followUp.limitExceeded`; malformed OOXML remains `error.rfi.followUp.workbookInvalid`. Focused tests exercise both branches through this adapter, so callers never parse error text.

`sheetProtection` and `mergeCells/mergeCell@ref` are accepted only as canonical direct worksheet children under the same namespace. Merge ranges are validated for syntax, ordering, bounds, overlap, and the 10,000-range limit. Existing handle-based `ordered_workbook_discovery` supplies ordered `WorksheetDescriptor` identities/visibility and `WorkbookSafetyFacts`, including workbook protection. `package_has_signature` returns true for an OOXML signature-origin relationship or any member below `_xmlsignatures/` (case-insensitive package-path comparison); conflicting evidence is invalid, not safe. Strict and Transitional packages remain supported, but namespace shadowing does not.

### 6.2 Guarded exact-cell patch and closed delta verifier

Keep `patch_workbook_sheets` unchanged for `rfi.log_add`. Add a guarded adapter for Follow Up:

```rust
pub(crate) const MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_GUARDED_CELL_UPDATES: usize = 500;

#[derive(Clone, Debug, PartialEq)]
pub struct GuardedCellUpdate {
    pub expected: CellData,
    pub replacement: CellUpdate,
}

pub fn patch_workbook_sheets_guarded<R, W>(
    source: &mut R,
    destination: &mut W,
    updates_by_sheet: &BTreeMap<
        String,
        BTreeMap<(u32, u32), GuardedCellUpdate>,
    >,
) -> Result<(), OoxmlError>
where
    R: Read + Seek,
    W: Read + Write + Seek;

pub fn verify_guarded_workbook_delta<R1, R2>(
    source: &mut R1,
    destination: &mut R2,
    updates_by_sheet: &BTreeMap<
        String,
        BTreeMap<(u32, u32), GuardedCellUpdate>,
    >,
) -> Result<(), OoxmlError>
where
    R1: Read + Seek,
    R2: Read + Seek;
```

Guarded apply is a new validation pass and has a separate, explicit target-only work account rather than reusing stale preview counters. While parsing the fresh source handle, guarded patching charges every merge-overlap comparison to the same parser-produced workbook count. It rejects a count above `MAX_APPLY_MERGE_COMPARISONS`, initializes an internal `MergeComparisonBudget` with `MAX_APPLY_MERGE_COMPARISONS - parsed_count`, and charges every selected-cell merge-membership comparison before comparing a range. Thus fresh target overlap validation plus selected-cell membership is bounded by exactly 2,000,000 comparisons per guarded apply attempt. The budget is internal to `patch_workbook_sheets_guarded`; its unchanged public signature cannot receive a caller-forged/reset counter. A limit failure occurs before the first destination workbook byte; the caller-owned, already-journaled owner-only empty private-output handle may remain retained, but backup/displacement/canonical effect has not begun. The typed failure maps through the Task 5 apply boundary to `error.rfi.followUp.limitExceeded` without changing the cached plan's classification-time account.

Rules:

- source is the already-open identity-validated Follow Up `File`; destination is the non-clone owner-only `SecuredCreatedFile` returned by `create_new_secured` through the pinned writable parent;
- the writer wraps destination in a counting boundary and fails before byte `MAX_PACKAGE_BYTES + 1`; any partial private output remains journaled, and quota reserves the full constant rather than guessing compressed growth;
- neither API accepts, opens, reopens, creates, removes, or renames a pathname; callers seek both handles explicitly and the same destination handle is used for write, delta verification, hashing, and later publication;
- the update set is nonempty and contains at most 500 coordinates;
- worksheet names and map coordinates are exact; duplicate approved coordinates are rejected while converting the representable ordered `FollowUpApprovedUpdate` vector into this map, before this API is called;
- every selected target cell physically exists and its current `CellData { value, formula, style }` equals `expected` before any output is written;
- each replacement is `CellUpdate::Text("Completed")` or `CellUpdate::Text("In Progress")`; every other update kind/value is rejected;
- duplicate-validation rewriting is disabled (`None`), unlike the RFI Log append path;
- an absent, formula-bearing, or merged target status cell is never authorized, so this operation never creates a cell, removes a formula, changes `calcChain.xml`, or changes a row/table placement;
- the guarded writer preserves every target-cell attribute except the text-representation `t` attribute and preserves every non-value child subtree; it replaces only the canonical value representation with the exact inline-string representation required for the approved text;
- all non-target ZIP members, including the target worksheet `.rels`, workbook relationships, VBA project, custom XML, drawings, charts, styles, shared strings, content types, external links, unknown extensions, and `calcChain.xml`, remain byte-identical.

`verify_guarded_workbook_delta` is a runtime gate, not only a fixture assertion. It reads raw ZIP members from the already-open anchored Follow Up source handle and the same already-open private-output destination handle, requires the same member roster, and byte-compares every member except an approved target worksheet part. For each modified worksheet it emits a namespace-expanded canonical event stream containing event order/kind, expanded element names, ordered expanded attributes and decoded values, and text/CDATA/comment/PI/declaration payloads. It locates target cells only through the canonical `worksheet/sheetData/row/c` hierarchy and then:

1. requires every non-target event token to match exactly;
2. requires target `<c>` coordinates, style and every attribute other than `t` to match;
3. requires every target child subtree other than the one canonical `v`/`is` value representation to match exactly and requires no `f` child;
4. requires output `t="inlineStr"` plus exactly one canonical `<is><t>{approved replacement}</t></is>` representation whose decoded text equals that coordinate's exact `Completed` or `In Progress` update, allowing `xml:space="preserve"` only when that approved text requires it;
5. replaces only that approved representation with a coordinate/value marker in each stream and requires the resulting complete streams to be equal.

Any changed row attribute, ordering token, validation, table part, hyperlink, drawing/comment relationship, extension subtree, unknown event, or non-target cell fails `error.rfi.followUp.patchVerification` before the canonical workbook name is touched. Semantic workbook reread then independently requires every selected value/style and all parsed non-target cells to match. ZIP order/compression and lexical XML quoting may change, so package byte equality is not claimed; exact undo instead restores the byte-identical backup and original SHA-256.

## 7. Source RFI Log evidence

Expose, but do not duplicate, the existing RFI Log sheet/status layout:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RfiStatusColumns {
    pub sheet: &'static str,
    pub rfi_column: u32,
    pub status_column: u32,
}

pub(crate) fn status_columns() -> [RfiStatusColumns; 4];
```

It returns, in this exact order:

```text
CIV: RFI 1, Status 16
MEC: RFI 1, Status 17
ELE: RFI 1, Status 16
PRC: RFI 1, Status 16
```

The values are projections of the current `rfi_log::layout` constants. In literal terms the actual projections are `CIV: 1/16`, `MEC: 1/17`, `ELE: 1/16`, `PRC: 1/16` (RFI/status), in that order. Existing `rfi.log_add` behavior stays unchanged; no generic 3/4/8/9 constants are added.

The Follow Up scanner requires all four exact source sheets. Either workbook-level protection or protection on any of those four source sheets fails preview; protected input is not treated as trustworthy synchronization evidence. For rows 10 through the physical maximum in each sheet, it records only a full exact canonical RFI. Formula-bearing RFI or status cells do not authorize a mapping. Evidence is represented by:

```rust
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceStatusEvidence {
    sheet: String,
    row: u32,
    rfi_column: u32,
    status_column: u32,
    rfi_cell: CellSnapshot,
    status_cell: Option<CellSnapshot>,
}
```

`status_cell=None` means the projected source status coordinate has no physical `<c>` element; its normalized status value is `""`. The projected `status_column` remains explicit so missing-cell evidence cannot be mistaken for a different column.

A canonical RFI group produces an automatic proposed status only when every evidence row has a recognized source status and every recognized row maps to the same target value. Repeated identical `Completed` rows or repeated identical `Ongoing` rows are safe up to the explicit 500-evidence-per-RFI bound. Formula evidence is `sourceFormula`; an RFI/status evidence cell participating in a merged range is `sourceMerged`; an all-blank group is `sourceStatusBlank`; an all-identical nonblank unknown group is `sourceStatusUnknown`; and every mixed recognized/unknown/blank group, `Completed`/`Ongoing` mix, or multiple-unknown group is `sourceConflict`. None is selectable. Source merge/formula/status analysis is computed once per bounded canonical group and reused by target rows; classification must not repeat a linear merge-range scan for every duplicate target row.

Merge overlap validation and later membership lookup use one auditable end-to-end comparison account even though parsing precedes classification. Each `WorkbookData` carries private parser-produced `merge_validation_comparisons: u64` and `merge_validation_complete: bool` facts exposed only by crate-visible read-only accessors; no setter/deserializer exists. `Default` is deliberately non-authoritative (`0/false`). The parser sets `complete=true` only after every worksheet's overlap validation succeeds. Production S5 classification rejects `false` as workbook-invalid before using the count; its tests obtain authoritative data through `read_workbook_for_follow_up`, not a forged default struct. During parse, every range-vs-range comparison used to reject overlapping canonical merge ranges consumes one checked unit before the comparison; a workbook fails with the typed worksheet-limit error before comparison `MAX_MERGE_VALIDATION_COMPARISONS_PER_WORKBOOK + 1`, and exactly 1,000,000 validation comparisons are allowed across all of that workbook's worksheets. The facts are not accepted from JSON or a caller.

S5 classification checked-adds the source and target parser counts, rejects a sum above `MAX_FOLLOW_UP_MERGE_COMPARISONS`, and initializes one mutable `MergeQueryBudget` with `MAX_FOLLOW_UP_MERGE_COMPARISONS - parsed_sum`. All S5 header, source-evidence, target-RFI, and target-status merge-membership calls consume that same remainder before each `CellRange` comparison. Therefore overlap validation plus membership across both workbooks cannot exceed exactly 2,000,000 range comparisons. Every S5 call site propagates the typed limit as `error.rfi.followUp.limitExceeded`, and no unbudgeted overlap or `merged_range_containing` path remains in Follow Up discovery/classification. This bounds the existing linear representations without expanding wide ranges into cells; maximum-count overlap and many-distinct-RFI fixtures fail predictably before CPU amplification, independent of the later serialization budget.

## 8. Bounded Follow Up layout discovery

Inspect worksheets in workbook order. A header candidate is one row within rows 1–32 and columns 1–64 containing exactly one canonical `RFI NO.` cell and exactly one canonical `STATUS` cell. Formula-bearing or merged header cells are invalid.

Exactly one header candidate must exist across the entire workbook. Zero candidates, multiple candidate rows, duplicate RFI/status headers on a candidate row, hidden/veryHidden candidate worksheets, workbook protection, target-sheet protection, or a signed package fails the whole preview. The operation does not choose the first match, prefer a sheet named “Follow Up,” or infer adjacent columns.

```rust
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpLayout {
    worksheet: WorksheetDescriptor,
    header_row: u32,
    rfi_column: u32,
    status_column: u32,
}
```

Data rows are `header_row + 1 ..= WorksheetData.max_row` for the one selected worksheet. A row is included in review when either its RFI or status cell is nonblank after status normalization. A fully blank row is ignored. Every included row preserves exact physical row and target-cell snapshots; there is no “next available row” or formatting-based placement.

## 9. Review-row DTO and classification

### 9.1 Exact DTOs

```rust
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkbookFingerprint {
    identity: FilesystemObjectIdentity,
    size: u64,
    mtime_ns: u64,
    sha256: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CellSnapshot {
    value: String,
    formula: Option<String>,
    style: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum FollowUpDisposition {
    #[serde(rename = "rfi.followUp.disposition.ready")]
    Ready,
    #[serde(rename = "rfi.followUp.disposition.alreadySynced")]
    AlreadySynced,
    #[serde(rename = "rfi.followUp.disposition.manualReview")]
    ManualReview,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpRow {
    #[serde(rename = "_rowId")]
    row_id: String,
    #[serde(rename = "_sheet")]
    sheet: String,
    #[serde(rename = "_row")]
    row: u32,
    rfi_no: String,
    source_statuses: Vec<String>,
    current_status: String,
    proposed_status: Option<String>,
    disposition: FollowUpDisposition,
    reason_key: String,
    selectable: bool,
    enabled: bool,
    target: FollowUpReviewTarget,
    source_evidence: Vec<SourceStatusEvidence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpReviewTarget {
    worksheet: WorksheetDescriptor,
    row: u32,
    rfi_column: u32,
    status_column: u32,
    rfi_cell: Option<CellSnapshot>,
    status_cell: Option<CellSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpCellTarget {
    worksheet: WorksheetDescriptor,
    row: u32,
    rfi_column: u32,
    status_column: u32,
    rfi_cell: CellSnapshot,
    status_cell: CellSnapshot,
}
```

Before any `FollowUpRow.source_evidence` vector, editor `Value`, approved-update vector, or owned step meta is cloned/materialized, classification holds borrowed references to the bounded source groups and lightweight target facts. A private borrowed serialization projection mirrors every field that will be emitted into the complete preview/editor and immutable `FollowUpStepMeta`. It is written with `serde_json::to_writer` into a checked `CountingLimitWriter` that allocates no output bytes, uses checked `u64` addition, and returns `error.rfi.followUp.limitExceeded` before accepting byte `MAX_PREVIEW_AUTHORITY_BYTES + 1`. Only after this exact borrowed projection fits may the owned row/editor/step authority be built. Approximate `size_of`, record-count multiplication, timeout/OOM tests, and serialize-then-measure are forbidden substitutes. The final owned cache-candidate structure is serialized once more through the same bound after its final opaque plan ID is reserved and before plan caching so later DTO changes cannot make the preflight stale.

S5 preview does not rely on the existing `PlanStore::insert_inner` assigning an ID after validation. `PlanStore::reserve_plan_id` returns an opaque `ReservedPlanId` with private ID storage; the type is crate-visible but is neither `Clone` nor serializable and cannot be constructed outside `plan_store.rs`. `ReservedPlanId::assign_to_empty(&self, &mut Plan)` rejects a nonempty `plan.plan_id`, then assigns its canonical lowercase hyphenated UUID without consuming the token. `insert_bound_reserved` consumes that exact token, acquires the existing plans mutex, requires `plan.plan_id` to equal the token's private ID, requires the key to be vacant, computes the digest over that unchanged plan, and inserts without rewriting any serialized field. A caller can budget-check the assigned plan but cannot substitute another canonical UUID and still pass insertion. Collision/mismatch/failure leaves the store unchanged; callers may retry the whole inert preview with a new token but never silently substitute an ID after budget validation. Existing `insert_bound`/`cache_plan` behavior remains unchanged for every other operation. `FilesystemAuthority::cache_reserved_plan` validates grants/binding and moves the token into the reserved API. Thus `validate_owned_plan_budget` sees the exact final cached bytes, and no post-cache validation or rollback is needed.

`analyze_source_groups` builds one `SourceAnalysis` per bounded canonical RFI and classification looks it up for every target row. It does not call `analyze_source` or rescan merge ranges per target. A `#[cfg(test)]` analysis-call counter is allowed only inside this module to prove that 500 duplicate target rows referencing one source group invoke source analysis exactly once; it is not a production API or serialized field. A module-unit materialization counter likewise proves an over-budget borrowed projection returns before the first owned row/evidence clone. Both counters are absent from non-test builds.

`FollowUpReviewTarget` represents every physical review row, including status-only rows with no RFI `<c>` and valid-RFI rows with no status `<c>`. `FollowUpCellTarget` is constructed only for `ready` rows after both cells are proven physical and safe; it is the exact preauthorized target carried into `FollowUpApprovedUpdate`.

`rfiNo` is the full canonical RFI for a valid match, NFC+trim visible target text for an invalid nonblank RFI, and `""` for blank/missing RFI. `sourceStatuses` is the sorted/deduplicated vector of normalized evidence values (including `""` when present), `currentStatus` is the normalized target value, and `reasonKey` always carries the full `warn.rfi.followUp.reason.*` key. `_sheet` is the exact worksheet name and `_row` is its physical row.

`WorkbookFingerprint` is computed from the same already-open anchored file: the existing serialized `journal::FilesystemObjectIdentity`, metadata size, checked nanosecond modification time since Unix epoch, and lowercase 64-hex SHA-256; the handle is rewound after hashing. Expose the existing private `cap_fs::file_identity(&File)` as `pub(crate)` rather than adding a second platform identity implementation. Preview fingerprints each handle before discovery/read and again afterward, requiring exact equality so a scan never approves a moving snapshot. A missing/unrepresentable identity or timestamp fails closed.

The exact `_rowId` format is:

```text
rfi-follow-up:{sheetId}:{row}:{statusColumn}
```

All three numeric components are canonical base-10 with no padding. Since `sheetId` is unique in an accepted workbook and the one layout has one status column, IDs are unique and deterministic for the approved workbook state.

### 9.2 Classification matrix

Rows are classified in this order:

1. target RFI cell contains a formula → `manual_review`, `warn.rfi.followUp.reason.targetRfiFormula`;
2. target RFI coordinate belongs to a merged range → manual, `targetRfiMerged`;
3. blank target RFI → manual, `blankRfi`;
4. target RFI not an exact full canonical RFI → manual, `invalidRfi`;
5. canonical RFI appears in more than one Follow Up row → every row is manual; rows without an earlier RFI formula/merge reason use `targetDuplicate`;
6. no source evidence → manual, `sourceMissing`;
7. source formula evidence → manual, `sourceFormula`;
8. source RFI/status evidence belongs to a merged range → manual, `sourceMerged`;
9. every source status is blank → manual, `sourceStatusBlank`;
10. every source status is the same nonblank value other than exact `Completed`/`Ongoing`, including exact `Cancelled` and `Rejected` → manual, `sourceStatusUnknown`;
11. source evidence mixes recognized/unknown/blank values, maps inconsistently, or contains multiple different unknown values → manual, `sourceConflict`;
12. target status cell has no physical `<c>` element → manual, `targetCellMissing`;
13. target status cell contains a formula → manual, `targetFormula`;
14. target status coordinate belongs to a merged range → manual, `targetMerged`;
15. normalized target status is nonblank and not exact `Completed`/`In Progress` → manual, `targetStatusUnknown`;
16. target status already equals the proposed status → `already_synced`, `alreadySynced`;
17. otherwise → `ready`, `readyCompleted` or `readyOngoing`.

Only `ready` rows have `selectable=true` and default `enabled=true`. `already_synced` and every manual row have `selectable=false`, `enabled=false`. A physically present blank target status cell is safe for an exact recognized source. An absent logical status coordinate is manual-only; this operation never creates its `<c>` element, a worksheet row, RFI value, table, header, or inferred placement. A controlled `Completed` or `In Progress` target may be changed only to the exact mapped value. An unknown nonblank target value is never clobbered.

### 9.3 Editor metadata

The plan uses the existing generic `recordTable` editor with this exact metadata shape (`rows` is the complete ordered `Vec<FollowUpRow>` serialization):

```rust
json!({
    "kind": "recordTable",
    "layout": "compact",
    "titleKey": "rfi.followUp.editor.title",
    "rowCountKey": "rfi.followUp.editor.rowCount",
    "requiredFields": ["enabled"],
    "groupField": "rfiNo",
    "selectionGroupFields": [],
    "columns": columns,
    "rows": rows,
    "duplicateEnabled": false,
    "selectionEnabled": false,
    "bulkEnabled": false,
    "reorderable": false,
    "detailsEnabled": true,
    "detailsDefaultVisible": false,
    "copyEnabled": false,
})
```

It has no bulk selection, duplication, row deletion, copy-table action, or reordering. `columns` is this exact vector, in order:

```rust
vec![
    json!({
        "key": "enabled",
        "labelKey": "rfi.followUp.column.enabled",
        "kind": "bool",
        "bulk": false,
        "disabledWhen": {"field": "selectable", "equals": false},
    }),
    json!({"key": "rfiNo", "labelKey": "rfi.followUp.column.rfiNo", "kind": "readonly"}),
    json!({"key": "sourceStatuses", "labelKey": "rfi.followUp.column.sourceStatuses", "kind": "readonly"}),
    json!({"key": "currentStatus", "labelKey": "rfi.followUp.column.currentStatus", "kind": "readonly"}),
    json!({"key": "proposedStatus", "labelKey": "rfi.followUp.column.proposedStatus", "kind": "readonly"}),
    json!({
        "key": "disposition",
        "labelKey": "rfi.followUp.column.disposition",
        "kind": "readonly",
        "display": "localizedKey",
    }),
    json!({
        "key": "reasonKey",
        "labelKey": "rfi.followUp.column.reason",
        "kind": "readonly",
        "display": "localizedKey",
        "detail": true,
    }),
    json!({"key": "_sheet", "labelKey": "rfi.followUp.column.sheet", "kind": "readonly", "detail": true}),
    json!({"key": "_row", "labelKey": "rfi.followUp.column.row", "kind": "readonly", "detail": true}),
]
```

The enabled column's `disabledWhen` rule disables every row where `selectable == false`. Keep the existing broad `EditorColumn.display?: string` declaration and add only the missing optional predicate:

```ts
disabledWhen?: { field: string; equals: string | number | boolean };
```

This slice applies `disabledWhen` only to the existing `kind: "bool"` checkbox branch because that is the only behavior required by Follow Up. It does not broaden select, text, multiline, compact-date, row-selection, or bulk-edit behavior. `display: "localizedKey"` is a new handled literal in the existing string-typed field and applies only to read-only cells, rendering `s.t(value)` while retaining the raw key in `title`. The backend treats all presentation metadata as untrusted and validates row authority independently.

When at least one row has `manual_review`, preview/revise emits exactly one `warn.rfi.followUp.manualRows` warning whose subject is the canonical base-10 manual-row count. It emits no per-row warning. `warn.rfi.followUp.noRowsSelected` is emitted exactly when the installed plan has zero enabled ready rows.

## 10. Plan, revise, and apply contracts

### 10.1 Step metadata

Every plan, including a zero-step plan, stores this strict operation authority block at `plan.meta["rfiFollowUp"]` so a safe row can be re-enabled through `engine/revise` without regenerating paths or a backup name. A zero-step plan is revise-only: backend `engine/apply` rejects it with `error.rfi.followUp.noRowsSelected` before process/OS permit acquisition, claim, cancellation registration, journal/quota/filesystem access, or plan-state mutation. `S5AuthorityRecord::initial_step` is therefore constructed only after a nonempty exact one-step claim.

```rust
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpPlanMeta {
    rfi_log_workbook: String,
    follow_up_workbook: String,
    backup_workbook: String,
    rfi_log_fingerprint: WorkbookFingerprint,
    follow_up_fingerprint: WorkbookFingerprint,
    layout: FollowUpLayout,
}
```

The block is immutable during revise. It duplicates no executable authority: `PlanStore`/`PlanBinding` still owns caller/session/operation/grants, and apply executes only emitted steps.

A plan with at least one enabled ready row has exactly one `StepKind::WorkbookUpdate`:

- `src`: exact Follow Up workbook display path;
- `dst`: exact no-clobber S5 backup path in the Follow Up canonical parent;
- `noteKey`: `note.rfi.followUp.workbookUpdate`.

Follow Up uses an S5-specific basename so bounded retained-storage scans never infer ownership from generic user files. `rfi.log_add` keeps its existing convention unchanged:

```text
.docflow-s5-backup-<preview-uuid-simple>.<original-extension>
```

The step meta is exactly:

```rust
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FollowUpStepMeta {
    rfi_log_workbook: String,
    rfi_log_fingerprint: WorkbookFingerprint,
    follow_up_fingerprint: WorkbookFingerprint,
    layout: FollowUpLayout,
    updates: Vec<FollowUpApprovedUpdate>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpApprovedUpdate {
    row_id: String,
    canonical_rfi: String,
    proposed_status: String,
    target: FollowUpCellTarget,
    source_evidence: Vec<SourceStatusEvidence>,
}
```

`FollowUpStepMeta` is crate-visible because `fs_ops.rs` carries the decoded value between crate-visible `rfi_follow_up` functions. Its fields and nested DTOs remain private; `fs_ops.rs` does not inspect them directly, so no private field type crosses the module API.

Only enabled ready rows enter `updates`. If there are no enabled ready rows, the plan has zero steps and includes `warn.rfi.followUp.noRowsSelected`. The exact editor `requiredFields: ["enabled"]` keeps Apply disabled while every row is false but lets a user re-enable a safe row and submit `engine/revise`; the immutable `rfiFollowUp` block lets Rust rebuild the exact original-authority step.

### 10.2 Revise and atomic authority replacement

`rfi_follow_up::revise` receives the original plan, submitted rows, and freshly opened anchored RFI Log/Follow Up handles. It:

1. requires every original `_rowId` exactly once, in original order;
2. rejects inserted, removed, duplicated, reordered, or unknown rows before any keyed map is built;
3. deserializes each already-representable `serde_json::Value` row with `deny_unknown_fields` (duplicate JSON object member names are outside this boundary because `Value::Object` is already a unique-key map);
4. requires every field except `enabled` to equal the original row exactly;
5. requires `enabled=false` when `selectable=false`;
6. allows `enabled` to change only on a `ready` row;
7. recomputes both fingerprints, source evidence, unique layout, target snapshots, classifications, and row IDs;
8. requires the recomputed immutable rows to equal the original immutable rows;
9. rebuilds the one exact workbook step from enabled rows while preserving the original authorized `src`, no-clobber backup `dst`, and non-editor metadata, or produces a zero-step warning plan that still preserves the original editor/workbook evidence.

Successful native revise does not insert a second plan. Add this `PlanStore` API:

```rust
pub fn replace_bound_available(
    &self,
    plan_id: &str,
    caller: &CallerIdentity,
    expected_digest: &str,
    replacement: Plan,
) -> Result<Plan, BackendError>;
```

Under one `plans` mutex acquisition it finds the exact `plan_id` without an eager remove/prune, validates the stored `PlanBinding` owner/session against `caller`, requires the actual `PlanState::Available`, requires the original TTL still valid, requires both stored `StoredPlan.digest` and a fresh `plan_digest(&stored.plan)` to equal `expected_digest`, and requires the replacement operation to equal both the old plan operation and `binding.operation_id`. Before mutation it assigns the preserved opaque `plan_id` to the replacement and computes its digest. Only after every check succeeds does it atomically replace exactly `StoredPlan.plan` and `StoredPlan.digest`. The current `PlanBinding` has only `owner_id`, `session_id`, `operation_id`, and `grant_ids`; it is preserved unchanged. `backend_plan_id`, `origin`, `created_at`, `PlanState`, and therefore the original TTL deadline also remain unchanged. The only current states are `Available` and `Claimed`; no Applying or terminal state is introduced. Any expiry, claimed/apply race, concurrent revise, digest mismatch, caller/session mismatch, or operation mismatch leaves every actual stored field unchanged; normal resolve/cleanup may prune it later under the store's existing policy. After success the same opaque ID resolves only to the replacement; the superseded, more-permissive content and old digest have no claim/apply authority.

`FilesystemAuthority::replace_cached_plan` validates the preserved grants and delegates to this API. `NativeBackend::revise_native_plan` resolves the old record, performs workbook/row validation, then calls only `replace_cached_plan`; it never calls `cache_plan`/`insert_bound` on successful Follow Up revise. A revise-time workbook/layout/cell change fails with a localized validation key and leaves the original available and unchanged; it never silently refreshes the proposal. The user must preview again.

### 10.3 Apply, retained evidence, and immediate revalidation

`FilesystemAuthority::execute_step` dispatches `StepKind::WorkbookUpdate` by the bound operation ID:

- `rfi.log_add` → existing `execute_workbook_update` unchanged;
- `rfi.follow_up_sync` → new `execute_follow_up_workbook_update`;
- every other operation → no-effect rejection.

The existing `GlobalMutationLock` is process-local and only the first tier. Nonzero S5 apply inspects without mutation, acquires process then OS permit, binds both into nonforgeable `S5PermitBundle`, obtains a validated root roster, and only then consumes both through `claim_s5_for_apply_bound_under_permit`, which repeats caller/state/TTL/digest/operation and exact grant-roster checks. Contention or unsafe lock/root state leaves the plan `Available`; no cancellation, journal, quota, or workbook access occurs. Undo and recovery use the same permit bundle; all S5 authority objects own it until terminal drop. The permanent lock file is create-new-or-open-existing, regular/non-reparse identity-checked, synced when new, and never deleted. Unix holds `flock(LOCK_EX|LOCK_NB)` on `O_NOFOLLOW|O_CLOEXEC`; Windows holds `OPEN_EXISTING|FILE_FLAG_OPEN_REPARSE_POINT` with share zero. The open handle lives inside the bundle and process death releases it.

Every S5 journal transition additionally uses revision CAS. Old JSON defaults `revision` to zero. While both permits are held, `write_s5_cas` requires `expected_revision == token.revision()`, rereads and validates disk, requires journal ID/revision/provenance match, serializes the allowed candidate at checked `expected+1`, and durably creates/replaces. Only complete durable success updates the mutable token revision; any error leaves the in-memory token at the prior revision and forbids retry. A failure after possible namespace replacement is outcome-unknown and restart recovery rereads disk. Absent expected-zero uses O_EXCL+parent sync and never replaces; existing records use CAS replacement. Legacy write uses the same OS lock and rejects candidate or existing-target S5.

Canonical replacement preserves security metadata and named streams as well as unnamed workbook bytes. Recreation requires exactly one hard link. Reparse, EFS/compressed/sparse/offline/recall/object-ID/transactional and other unmodeled property classes fail pre-effect; Unix likewise rejects unmodeled immutable/append-only/platform flags.

Windows never hardens a shareable file after creation. A descriptor preflight computes exact token needs: SACL/CAP access requires assigned `SeSecurityPrivilege`; owner/group outside the caller's normally assignable identities additionally requires `SeRestorePrivilege`; a mandatory label the caller cannot assign normally additionally requires `SeRelabelPrivilege`. Missing required privilege is a clean pre-effect blocker, while a current-owner/assignable-label descriptor does not invent an unnecessary privilege dependency. A dedicated joined worker thread duplicates the caller token, enables only the computed set, restores the exact prior thread token and every privilege state on success/error/panic, closes duplicates, never modifies the process token, and carries no impersonation across async work. `open_security_metadata` is pinned-parent/root-relative, reparse-safe, requests `READ_CONTROL|ACCESS_SYSTEM_SECURITY`, and identity-matches the open data handle.

`CreationSecurityDescriptor` is derived from the captured source and same-parent security snapshot: owner/group and explicit ACEs are supplied and auto-inheritance/protection semantics are reconstructed. `create_new_secured` is S5-only; legacy `create_new` is unchanged. Windows uses parent-relative `NtCreateFile(NtDisposition::Create)` with non-null `OBJECT_ATTRIBUTES.SecurityDescriptor`, modeled initial attributes, desired data/attribute/synchronize plus verification-only `READ_CONTROL|ACCESS_SYSTEM_SECURITY`, and unnamed-stream `ShareAccess=0`. It deliberately does not request `WRITE_DAC`/`WRITE_OWNER`: no post-create descriptor mutation exists. Private output uses a protected owner-only descriptor. Returned `SecuredCreatedFile` exposes no raw-handle/clone escape and keeps the unnamed stream/delete-sensitive opens fenced through final verification.

Windows share modes are per-stream and do not fence attribute/EA-only access. The confidentiality guarantee therefore covers principals excluded by the atomically installed DACL; it does not claim adversarial isolation from another process already authorized by that final DACL. DocFlow processes remain serialized by the OS-wide permit. The unnamed-stream byte/name/identity verification defines publication linearization before terminal CAS. Access/EA/ADS/attribute/link evidence is a bounded handle-based sequential observation, not an atomic filesystem-wide metadata snapshot; persistent drift present when its component is recaptured becomes recovery-required, but transient/ABA mutation by an authorized external process is outside the threat model. Later undo/recovery revalidates current state. Tests prove denied-principal failure, unnamed-stream/delete-sensitive sharing, and deterministic stable authorized metadata-drift detection without demanding impossible ADS/EA exclusivity or atomic multi-class metadata observation.

`FileSecuritySnapshot::capture` is bounded to 128 xattrs/alternate streams and 1 MiB auxiliary data. Windows inventories named streams with `FileStreamInformation`, then parses `BackupRead(FALSE)`. It requires exactly one unnamed `BACKUP_DATA` header whose 64-bit size equals open-file logical size, calls `BackupSeek` for exactly that payload, verifies reconstructed skipped count, and excludes default bytes from the auxiliary budget. It separately accepts bounded `BACKUP_EA_DATA` and named `BACKUP_ALTERNATE_DATA`; only named ADS are normalized against the named `FileStreamInformation` census. Every other class is rejected and context is aborted on every exit. `BackupWrite(FALSE)` replays only EA/ADS through the owned secured handle and always aborts. A workbook larger than 1 MiB with bounded EA/ADS succeeds without reading or replaying default bytes.

Canonical serialization is versioned/length-prefixed and covers complete owner/group/DACL/SACL/CAP/mandatory label, modeled flags, EAs and ADS while excluding timestamps/pointers/padding. Secure creation is verified immediately. Auxiliary streams are applied/verified while the owned wrapper holds unnamed-stream `ShareAccess=0`; that mode is not treated as an ADS/EA lock. Only after `canonical_auxiliary_ready` may workbook bytes be written. Backup/apply/undo use the original snapshot. Private output remains owner-only. No operation-owned workbook byte is ever present under inherited broader access or a capturable pre-hardening handle.

The 8-GiB S5 quota is an exact **logical retained payload** quota, not filesystem allocation accounting. Per artifact, `logical_charge_bytes = unnamed_stream_logical_length + auxiliary_charge_bytes`; auxiliary charge is the checked byte length of canonical length-prefixed xattr/EA/named-ADS records including names, type tags, lengths, and values, excluding security descriptors and the skipped unnamed `BACKUP_DATA`. Every matching S5 artifact scan opens the object no-follow, measures both components, and rejects inaccessible/unbounded/overflowing metadata. Apply reserves three files and checked `2 * original_default_length + MAX_PACKAGE_BYTES + 3 * original_auxiliary_charge`; undo reserves one file and checked `current_applied_default_length + current_applied_auxiliary_charge`. Canonical replacement is not retained and is not charged. Exactly-at-limit is allowed; any greater projection fails before the next artifact effect.

Security is late-bound. Initial file/parent captures are provisional and recaptured before backup/displacement and after displacement. Parent or metadata drift fails before recreation; create-time inheritance is verified before bytes, and authorized auxiliary drift is checked again at the final linearization recapture. Journals retain only SHA-256 predicates, never descriptors or stream contents. Recovery exact-applied requires bytes, identity, name binding, and security digest; it never repairs metadata.

The Follow Up executor performs this sequence:

1. decode crate-visible `FollowUpStepMeta` with `deny_unknown_fields`; require 1–500 updates, reject duplicate sheet/coordinates while scanning the ordered `Vec<FollowUpApprovedUpdate>` before building a `BTreeMap`, and allow only the two replacement values;
2. authorize/anchor RFI Log read-only and Follow Up read-write; authorize same-parent backup/retained leaves through the pinned canonical parent, and authorize only private output below the possibly absent `.docflow-backups` parent; require distinct workbook identities;
3. open both anchored workbooks and require preview fingerprints, exact source evidence, exact target worksheet/layout, and selected target cell snapshots;
4. require Follow Up mutable-open success; otherwise return `error.rfi.followUp.workbookLocked` with no artifact effect;
5. with both mutation permits held, revalidate both workbooks and capture bounded target plus parent security. Every transition uses revision CAS. Bounded no-follow enumeration aggregates `.docflow-backups` and canonical parent, accepts only four exact S5 prefixes, and measures each regular artifact's checked logical charge including auxiliary records. Reject over 64 candidates, non-regular/inaccessible/unbounded entries, duplicate directory identity, or overflow. Apply uses the exact three-file/default-plus-three-auxiliary reservation above and fails before effect if projected count exceeds 64 or logical charge exceeds 8 GiB;
6. before the final missing shared `.docflow-backups` `Mkdir`, durably persist identity-free `pending_artifact_parent { path, locator }`. The no-clobber directory primitive returns typed `Created { anchored identity }`, `AlreadyExists`, or `CreatedIdentityUnknown`; it never infers ownership from a later pathname observation. Live `Created` followed by sync failure is known partial. `AlreadyExists` clears the pending intent and proceeds without an ownership claim. `CreatedIdentityUnknown`, or restart with intent plus a present directory, is `WarningOutcomeUnknown`: preserve the directory, keep the safe intent locator, and never record it as operation-created. Restart absence is exact no-effect. A successful completion CAS clears pending and sets `ArtifactParentReady`; it persists `created_artifact_parent` only for the live typed `Created(identity)` branch and leaves it absent for `AlreadyExists`. Then allocate the unique private-output leaf and durably persist `pending_retained_artifact { role, path, locator, creationState: intent }` before create;
7. create the owner-only private output through `create_new_secured_identified`. `Created { file, identity }` changes pending state to `created(identity)` before bytes; `CreatedIdentityUnknown { file }` changes it to `created_identity_unknown` and routes to `WarningOutcomeUnknown`, never no-effect. If that terminal CAS fails, the earlier durable intent remains for bounded restart reconciliation; a present leaf is preserved as unknown rather than claimed. The identified path verifies access, replays/verifies EA/ADS, writes/verifies OOXML through the same owned wrapper, and finalizes evidence plus `PrivateOutputReady` in one CAS;
8. cancellation checkpoint; cancellation here leaves both selected workbooks unchanged. Any operation-owned directory/private-output artifact already created is retained and journaled rather than unlinked by name;
9. immediately before backup, reopen both original anchors, require complete workbook predicates, recapture security/streams from the identity-matched Follow Up handles, and require equality to the provisional snapshot;
10. allocate backup in canonical parent and CAS-write `pending_retained_artifact { role: Backup, path, locator, creationState: intent }`; create it through `create_new_secured_identified`. Persist `created(identity)` before descriptor/auxiliary/default-byte work; `CreatedIdentityUnknown` is outcome-unknown. Verify descriptor, replay/verify EA/ADS, copy/sync unnamed bytes, then one CAS replaces pending with complete backup evidence and `BackupReady`;
11. cancellation checkpoint; cancellation now leaves canonical Follow Up unchanged and retains the exact backup/private output with journal evidence;
12. reopen both originals once more, require complete workbook predicates and mutable-open, recapture the complete Follow Up security/stream snapshot, and require equality. Any content or metadata drift is retained-artifact-only failure with canonical unchanged;
13. allocate retained-original in the canonical parent. Before canonical effect, one CAS writes pending `RestoreBackup` and `pending_publication` with all locators/identities/byte hashes, complete security hashes, separate destination access/auxiliary hashes, empty created/retained identities, and phase `prepared`;
14. take the final clean cancellation checkpoint. The shared helper receives `&CancellationToken` and checks it exactly here, before displacement. Once displacement begins, cancellation is not treated as a clean cancellation and no later checkpoint interrupts classification;
15. durably set phase `displacing`; move whatever currently occupies the canonical name destination-no-clobber to the empty unique retained leaf with the pinned parent. This name-relative source operation can move racer bytes, but it cannot overwrite or delete them. Any rename or parent-sync uncertainty is inspected, never blindly retried;
16. open the retained leaf, record what was observed, and require its identity, byte hash, and freshly recaptured complete security/stream digest to equal the durably expected original. On mismatch, never move it back, publish from it, overwrite it, or delete it; persist recovery-required. On exact match, CAS-record retained predicates and phase `retained`;
17. CAS-set `creating_canonical`, then call `create_new_secured`/O_EXCL with the exact original creation descriptor and `ShareAccess=0`. A racer wins untouched. The empty file is already restricted at creation;
18. immediately verify creation security through `SecuredCreatedFile`, obtain identity, CAS-write `canonical_secured` plus identity, and flush the parent. A crash before the CAS leaves an unknown but restricted empty canonical that recovery preserves;
19. replay/verify EA/ADS through the owned wrapper, CAS-set `canonical_auxiliary_ready`, then stream unnamed bytes; final byte/name/identity verification defines publication linearization, while bounded component recapture rejects stable authorized drift. Sync, hash, recapture predicates, and verify package. Empty/partial default-stream bytes remain inaccessible to principals excluded by the creation-time DACL;
20. require the destination-handle identity to remain `createdCanonicalIdentity`, require destination byte hash and security hash to equal the durably recorded applied predicates, require the backup and retained original bytes/security to remain exact, and finally require the canonical name to resolve no-follow to `createdCanonicalIdentity`. If a racer renames/replaces canonical after create, never remove or overwrite the foreign canonical; persist recovery-required. The created exact identity remains classifiable in the journal even when its name no longer resolves there;
21. after all checks pass, durably set publication phase `applied`, append completed `RestoreBackup` carrying original/applied identities plus byte/security hashes and backup identity, append/finalize de-duplicated exact backup and retained-original evidence while preserving the existing private-output evidence in `retained_evidence`, clear `pending_publication`, `pending_retained_artifact`, and `follow_up_phase`, and return the changed-cell count. No retained leaf or backup is unlinked.

The canonical name is temporarily absent after displacement and visibly empty/partial after create-new. Those states are intentional but not atomic: the exact original remains at its retained locator, and the exact backup/private output remain at their recorded locators. S5 never calls `AnchoredPath::replace_with`, `atomic_replace_capability_with_retry`, `rollback_from_backup_capability`, source-name publish rename, or name-based evidence cleanup. Legacy helpers remain untouched solely for existing operations.

A drift before step 10 is no canonical effect. A drift after backup/private-output creation but before displacement leaves only retained artifacts. From step 15 onward every non-success is outcome-unknown/recovery-required unless the exact applied state is durably classified. No mismatched retained identity is restored, published, or deleted, and no retry can convert a later edit into an overwrite.

### 10.4 Exact new symbol inventory

In addition to the DTOs and OOXML/authority symbols already declared above, implementation adds these exact symbols. They are new; no existing repository API is assumed under these names.

```rust
// plan_store.rs
pub(crate) struct ClaimedS5Apply { /* private ApplyClaim + S5PermitBundle + ValidatedGrantRootRoster */ }

impl ClaimedS5Apply {
    pub(crate) fn plan(&self) -> &Plan;
    pub(crate) fn binding(&self) -> &PlanBinding;
    pub(crate) fn digest(&self) -> &str;
    pub(crate) fn created_at(&self) -> SystemTime;
    pub(crate) fn caller(&self) -> &CallerIdentity;
    pub(crate) fn roots(&self) -> &ValidatedGrantRootRoster;
}

pub(crate) fn claim_s5_for_apply_bound_under_permit(
    &self,
    plan_id: &str,
    caller: &CallerIdentity,
    permits: S5PermitBundle,
    roots: ValidatedGrantRootRoster,
) -> Result<ClaimedS5Apply, BackendError>;

pub fn inspect_available_bound_operation(
    &self,
    plan_id: &str,
    caller: &CallerIdentity,
) -> Result<String, BackendError>;

pub fn claim_for_apply_bound_under_permit(
    &self,
    plan_id: &str,
    caller: &CallerIdentity,
    mutation: GlobalMutationPermit,
) -> Result<ApplyClaim, BackendError>;

pub fn replace_bound_available(
    &self,
    plan_id: &str,
    caller: &CallerIdentity,
    expected_digest: &str,
    replacement: Plan,
) -> Result<Plan, BackendError>;

// cap_fs.rs
pub(crate) fn file_identity(file: &File) -> io::Result<FilesystemObjectIdentity>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectoryAccess {
    ReadOnly,
    ReadWrite,
}

impl CapabilityRoot {
    // Compatibility-sensitive internal adapter: remains ReadWrite for legacy
    // mutation/recovery callers. Grant/replay paths use open_with_access.
    pub(crate) fn open(canonical: &Path) -> io::Result<Self>;
    pub(crate) fn open_with_access(
        canonical: &Path,
        access: DirectoryAccess,
    ) -> io::Result<Self>;
}

impl AuthorizedPath {
    pub(crate) fn create_artifact_parent_no_clobber(
        &self,
    ) -> Result<ArtifactParentCreateOutcome, CapabilityEffectError>;
    pub(crate) fn s5_retained_storage_usage(
        &self,
        max_entries: usize,
    ) -> io::Result<RetainedStorageUsage>;
}

impl AnchoredPath {
    pub(crate) fn create_new(&self) -> io::Result<File>;
    pub(crate) fn create_new_secured(
        &self,
        security: &CreationSecurityDescriptor,
    ) -> io::Result<SecuredCreatedFile>;
    pub(crate) fn create_new_secured_identified(
        &self,
        security: &CreationSecurityDescriptor,
    ) -> Result<SecuredCreateOutcome, CapabilityEffectError>;
    #[cfg(windows)]
    pub(crate) fn open_security_metadata(
        &self,
        expected: &FilesystemObjectIdentity,
    ) -> io::Result<File>;
    pub(crate) fn sync_parent(&self) -> io::Result<()>;
    pub(crate) fn retain_current_name_no_clobber(
        &self,
        retained: &AnchoredPath,
    ) -> Result<(), CapabilityEffectError>;
    pub(crate) fn capture_parent_security(
        &self,
    ) -> io::Result<ParentSecuritySnapshot>;
    pub(crate) fn verify_parent_security(
        &self,
        expected: &ParentSecuritySnapshot,
    ) -> io::Result<()>;
}

pub(crate) enum ArtifactParentCreateOutcome {
    Created { identity: FilesystemObjectIdentity },
    AlreadyExists,
    CreatedIdentityUnknown,
}

pub(crate) enum SecuredCreateOutcome {
    Created {
        file: SecuredCreatedFile,
        identity: FilesystemObjectIdentity,
    },
    CreatedIdentityUnknown {
        file: SecuredCreatedFile,
    },
}

pub(crate) struct RetainedStorageUsage {
    pub(crate) file_count: usize,
    pub(crate) logical_charge_bytes: u64,
}

pub(crate) struct RetainedStorageReservation {
    pub(crate) file_count: usize,
    pub(crate) logical_charge_bytes: u64,
}

impl RetainedStorageReservation {
    pub(crate) fn for_apply(
        original_default_stream_bytes: u64,
        original_auxiliary_charge_bytes: u64,
    ) -> io::Result<Self>;
    pub(crate) fn for_undo(
        current_applied_default_stream_bytes: u64,
        current_applied_auxiliary_charge_bytes: u64,
    ) -> io::Result<Self>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileSecuritySnapshot { /* bounded platform-private fields */ }

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParentSecuritySnapshot { /* parent identity + inheritance inputs */ }

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CreationSecurityDescriptor { /* validated create-time descriptor */ }

pub(crate) struct SecuredCreatedFile { /* non-clone owned secured handle */ }

impl FileSecuritySnapshot {
    pub(crate) fn capture(data: &File, security: Option<&File>) -> io::Result<Self>;
    pub(crate) fn creation_descriptor(
        &self,
        parent: &ParentSecuritySnapshot,
    ) -> io::Result<CreationSecurityDescriptor>;
    pub(crate) fn verify_created_access(&self, file: &SecuredCreatedFile) -> io::Result<()>;
    pub(crate) fn replay_and_verify_auxiliary(
        &self,
        file: &mut SecuredCreatedFile,
    ) -> io::Result<()>;
    pub(crate) fn verify_complete(&self, file: &SecuredCreatedFile) -> io::Result<()>;
    pub(crate) fn access_sha256(&self) -> String;
    pub(crate) fn auxiliary_sha256(&self) -> String;
    pub(crate) fn auxiliary_charge_bytes(&self) -> u64;
    pub(crate) fn sha256(&self) -> String;
}

impl CreationSecurityDescriptor {
    pub(crate) fn current_owner_only() -> io::Result<Self>;
}

impl SecuredCreatedFile {
    pub(crate) fn sync_all(&self) -> io::Result<()>;
    pub(crate) fn identity(&self) -> io::Result<FilesystemObjectIdentity>;
}

// Implements Read + Write + Seek; exposes no File/raw-handle/clone escape.

// journal.rs
pub(crate) const MAX_RETAINED_ARTIFACTS_PER_JOURNAL: usize = 8;
pub(crate) const MAX_RECOVERY_LOCATOR_SCALARS: usize = 512;
pub(crate) const MAX_S5_RETAINED_FILES: usize = 64;
pub(crate) const MAX_S5_RETAINED_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedPublicationPurpose {
    Apply,
    Undo,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedPublicationPhase {
    Prepared,
    Displacing,
    Retained,
    CreatingCanonical,
    CanonicalSecured,
    CanonicalAuxiliaryReady,
    WritingCanonical,
    VerifyingCanonical,
    Applied,
    RecoveryRequired,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedArtifactRole {
    Backup,
    PrivateOutput,
    DisplacedOriginal,
    DisplacedApplied,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FollowUpJournalPhase {
    ArtifactParentReady,
    PrivateOutputReady,
    BackupReady,
    PublicationActive,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistedRootAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantRootAccessRecord {
    pub(crate) root: String,
    pub(crate) identity: FilesystemObjectIdentity,
    pub(crate) access: PersistedRootAccess,
}

pub(crate) struct InterprocessMutationPermit { /* held OS lock-file handle */ }
pub(crate) struct S5PermitBundle { /* private GlobalMutationPermit + InterprocessMutationPermit */ }
pub(crate) struct PendingS5Undo { /* private validated core + permit bundle + recorded roots */ }
pub(crate) struct S5RecoverySession { /* private permit bundle + bounded entries + active validated roster */ }

impl PendingS5Undo {
    pub(crate) fn recorded_root_accesses(&self) -> &[GrantRootAccessRecord];
}

impl JournalStore {
    pub fn read(&self, journal_id: &str) -> Result<JournalRecord, BackendError>; // legacy only; rejects an on-disk S5 record
    pub fn write(&self, record: &JournalRecord) -> Result<(), BackendError>; // internally holds S5 OS lock; rejects candidate OR existing target S5
    pub(crate) fn inspect_operation(&self, journal_id: &str) -> Result<String, BackendError>; // narrow op-id only
    pub(crate) fn try_acquire_mutation_lock(
        &self,
    ) -> Result<InterprocessMutationPermit, BackendError>;
    pub(crate) fn bind_s5_permits(
        &self,
        process: GlobalMutationPermit,
        interprocess: InterprocessMutationPermit,
    ) -> S5PermitBundle;
    pub(crate) fn write_s5_cas(
        &self,
        record: &mut ValidatedS5Record, // token owns the permit bundle through its admission lease
        expected_revision: u64,
    ) -> Result<(), BackendError>;
    pub(crate) fn begin_s5(
        &self,
        claim: ClaimedS5Apply,
        caller: &CallerIdentity,
    ) -> Result<ValidatedS5Record, BackendError>;
    fn validate_s5_record(&self, record: JournalRecord) -> Result<ValidatedS5RecordCore, BackendError>;
    pub(crate) fn inspect_s5_for_undo(
        &self,
        journal_id: &str,
        caller: &CallerIdentity,
        permits: S5PermitBundle,
    ) -> Result<PendingS5Undo, BackendError>;
    pub(crate) fn admit_s5_undo(
        &self,
        pending: PendingS5Undo,
        roots: ValidatedGrantRootRoster,
    ) -> Result<ValidatedS5Record, BackendError>;
    pub(crate) fn begin_s5_recovery(
        &self,
        permits: S5PermitBundle,
    ) -> Result<S5RecoverySession, BackendError>;
}

impl JournalRecord {
    pub(crate) fn validate_for_history_projection(&self) -> Result<(), BackendError>;
}

pub(crate) const MAX_JOURNAL_ROOT_ENTRIES_SCANNED: usize = 4096;
pub(crate) const MAX_PENDING_S5_RECORDS: usize = 256;
pub(crate) const MAX_INVALID_S5_NOTICES: usize = 64;
pub(crate) const MAX_INVALID_S5_NOTICE_BYTES: u64 = 4096;
pub(crate) const MAX_RECOVERY_ENTRIES: usize = 256;
struct ValidatedS5RecordCore {
    record: JournalRecord,
    immutable_provenance_sha256: [u8; 32],
}
pub(crate) struct ValidatedS5Record { core: ValidatedS5RecordCore, lease: S5MutationLease }
enum S5MutationLease {
    Apply(ClaimedS5Apply),
    Undo { permits: S5PermitBundle, roots: ValidatedGrantRootRoster },
}
enum PendingS5Entry {
    Valid(ValidatedS5RecordCore),
    Invalid(InvalidS5JournalEnvelope),
    Legacy(JournalRecord), // only when every S5 discriminator field is absent
}
struct PendingS5Scan { entries: Vec<PendingS5Entry> }
struct InvalidS5JournalEnvelope { /* private, non-Clone token */ }
pub(crate) struct RetainedLocators(Vec<String>); // private validated storage

impl S5RecoverySession {
    fn scan_pending_s5(&mut self) -> Result<PendingS5Scan, BackendError>;
    pub(crate) fn next_root_accesses(&self) -> Option<&[GrantRootAccessRecord]>;
    pub(crate) fn recover_next(
        &mut self,
        roots: Option<ValidatedGrantRootRoster>,
        caller: &CallerIdentity,
    ) -> Result<Option<RecoveryEntry>, BackendError>; // classification, quarantine, transition and CAS stay internal
}

impl RetainedLocators {
    pub(crate) fn first(&self) -> &str;
    pub(crate) fn as_slice(&self) -> &[String];
}

impl ValidatedS5Record {
    pub(crate) fn apply_transition(&mut self, transition: S5Transition) -> Result<(), BackendError>;
    pub(crate) fn journal_id(&self) -> &str;
    pub(crate) fn approved_step(&self) -> &Step;
    pub(crate) fn completed_steps(&self) -> &[Step];
    pub(crate) fn plan(&self) -> Option<&Plan>; // apply lease only
    pub(crate) fn binding(&self) -> Option<&PlanBinding>; // apply lease only
    pub(crate) fn roots(&self) -> &ValidatedGrantRootRoster;
    pub(crate) fn retained_locators(&self) -> Result<RetainedLocators, BackendError>;
    pub(crate) fn revision(&self) -> u64;
    pub(crate) fn phase(&self) -> Option<FollowUpJournalPhase>;
    pub(crate) fn pending_publication(&self) -> Option<&RetainedPublication>;
    pub(crate) fn pending_artifact_parent(&self) -> Option<&PendingArtifactParent>;
    pub(crate) fn pending_retained_artifact(&self) -> Option<&PendingRetainedArtifact>;
    pub(crate) fn retained_evidence(&self) -> &[RetainedArtifactEvidence];
    pub(crate) fn prepare_failed_partial(&mut self, notice: RecoveryNotice) -> Result<(), BackendError>;
    pub(crate) fn prepare_outcome_unknown(&mut self, notice: RecoveryNotice) -> Result<(), BackendError>;
}

pub(crate) enum S5Transition {
    SetPendingArtifactParent(PendingArtifactParent),
    ResolveArtifactParent(ArtifactParentCreateOutcome),
    SetPendingRetainedArtifact(PendingRetainedArtifact),
    MarkPendingRetainedCreated(FilesystemObjectIdentity),
    MarkPendingRetainedIdentityUnknown,
    FinalizeRetainedArtifact { evidence: RetainedArtifactEvidence, phase: FollowUpJournalPhase },
    InitializePublication(RetainedPublication), // prepared-only; exactly once
    RecordRetainedPublicationEvidence(RetainedArtifactEvidence),
    RecordCanonicalSecured { identity: FilesystemObjectIdentity, descriptor_sha256: String },
    RecordCanonicalAuxiliaryReady { auxiliary_sha256: String, auxiliary_charge_bytes: u64 },
    MarkCanonicalBytesStarted,
    MarkCanonicalVerifying,
    FinalizeApplySuccess { completed_step: Step, evidence: Vec<RetainedArtifactEvidence> },
    FinalizeUndoSuccess { completed_step: Step, evidence: Vec<RetainedArtifactEvidence> },
    FinalizeRecovery { terminal: S5RecoveryTerminal, notice: Option<RecoveryNotice>, evidence: Vec<RetainedArtifactEvidence> },
    PrepareFailedPartial(RecoveryNotice),
    PrepareOutcomeUnknown(RecoveryNotice),
}

pub(crate) enum S5RecoveryTerminal { CompletedApply, CompletedUndo, FailedPartial, OutcomeUnknown }

impl PendingS5Scan {
    fn len(&self) -> usize;
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvalidS5JournalNotice {
    pub(crate) journal_id: String,
    pub(crate) source_identity: FilesystemObjectIdentity,
    pub(crate) source_sha256: String,
    pub(crate) warning_key: String,
    pub(crate) locator: String, // exact journal:<validated-id>;record
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct S5AuthorityRecord {
    initial_step: Step,
    source_path: String,
    source_identity: FilesystemObjectIdentity,
    target_path: String,
    target_identity: FilesystemObjectIdentity,
    approved_updates_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingArtifactParent {
    pub(crate) path: String,
    pub(crate) locator: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatedArtifactParent {
    pub(crate) path: String,
    pub(crate) locator: String,
    pub(crate) identity: FilesystemObjectIdentity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingArtifactCreationState {
    Intent,
    Created(FilesystemObjectIdentity),
    CreatedIdentityUnknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingRetainedArtifact {
    pub(crate) role: RetainedArtifactRole,
    pub(crate) path: String,
    pub(crate) locator: String,
    pub(crate) creation_state: PendingArtifactCreationState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetainedArtifactEvidence {
    pub(crate) role: RetainedArtifactRole,
    pub(crate) path: String,
    pub(crate) identity: FilesystemObjectIdentity,
    pub(crate) sha256: String,
    pub(crate) security_sha256: String,
    pub(crate) default_stream_bytes: u64,
    pub(crate) auxiliary_charge_bytes: u64,
    pub(crate) logical_charge_bytes: u64,
    pub(crate) locator: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetainedPublication {
    pub(crate) purpose: RetainedPublicationPurpose,
    pub(crate) canonical: String,
    pub(crate) retained: String,
    pub(crate) backup: String,
    pub(crate) publication_source: String,
    pub(crate) expected_canonical_identity: FilesystemObjectIdentity,
    pub(crate) expected_canonical_sha256: String,
    pub(crate) expected_backup_identity: FilesystemObjectIdentity,
    pub(crate) expected_backup_sha256: String,
    pub(crate) publication_source_identity: FilesystemObjectIdentity,
    pub(crate) publication_sha256: String,
    pub(crate) expected_canonical_security_sha256: String,
    pub(crate) expected_backup_security_sha256: String,
    pub(crate) publication_security_sha256: String,
    pub(crate) expected_canonical_default_stream_bytes: u64,
    pub(crate) expected_canonical_auxiliary_charge_bytes: u64,
    pub(crate) expected_canonical_logical_charge_bytes: u64,
    pub(crate) destination_access_sha256: String,
    pub(crate) destination_auxiliary_sha256: String,
    pub(crate) destination_auxiliary_charge_bytes: u64,
    pub(crate) retained_identity: Option<FilesystemObjectIdentity>,
    pub(crate) retained_sha256: Option<String>,
    pub(crate) retained_security_sha256: Option<String>,
    pub(crate) created_canonical_identity: Option<FilesystemObjectIdentity>,
    pub(crate) phase: RetainedPublicationPhase,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryNotice {
    pub warning_key: String,
    pub locator: String,
}

impl RecoveryNotice {
    pub(crate) fn from_warning(warning: &Warning) -> Self;
}

// New additive JournalRecord fields:
// pending_publication: Option<RetainedPublication>
// s5_authority: Option<S5AuthorityRecord> (immutable after initial create)
// pending_artifact_parent: Option<PendingArtifactParent>
// created_artifact_parent: Option<CreatedArtifactParent> (live typed Created only)
// pending_retained_artifact: Option<PendingRetainedArtifact>
// retained_evidence: Vec<RetainedArtifactEvidence>
// recovery_notice: Option<RecoveryNotice>
// follow_up_phase: Option<FollowUpJournalPhase>
// grant_root_accesses: Vec<GrantRootAccessRecord>
// revision: u64 (default 0; successful S5 CAS increments it)

// leaf_transaction.rs
pub(crate) enum RetainedPublicationOutcome {
    Published { created_identity: FilesystemObjectIdentity },
    CancelledBeforeDisplacement,
    RecoveryRequired(RecoveryNotice),
}

pub(crate) enum PublicationCancellation<'a> {
    Apply(&'a CancellationToken),
    NotCancellable,
}

pub(crate) fn execute_retained_publication<R, V>(
    journals: &JournalStore,
    record: &mut ValidatedS5Record,
    canonical: &AnchoredPath,
    retained: &AnchoredPath,
    publication_source: &mut R,
    destination_creation: &CreationSecurityDescriptor,
    destination_snapshot: &FileSecuritySnapshot,
    cancellation: PublicationCancellation<'_>,
    verify_destination: V,
) -> Result<RetainedPublicationOutcome, BackendError>
where
    R: Read + Seek,
    V: FnOnce(&mut SecuredCreatedFile) -> Result<(), BackendError>;

pub(crate) fn reconcile_retained_publication(
    journals: &JournalStore,
    record: &mut ValidatedS5Record,
) -> Result<Option<RecoveryEntry>, BackendError>;

// rfi_follow_up.rs
fn fingerprint_open_file(
    source: &mut File,
    error_key: &'static str,
) -> Result<WorkbookFingerprint, BackendError>;

pub fn preview_from_open_files<F, C>(
    rfi_log: &mut File,
    follow_up: &mut File,
    rfi_log_path: &Path,
    follow_up_path: &Path,
    now: DateTime<Local>,
    progress: F,
    cancelled: C,
) -> Result<Plan, BackendError>
where
    F: FnMut(ProgressEvent),
    C: Fn() -> bool;

pub fn revise_from_open_files(
    plan: &Plan,
    rows: &[Value],
    rfi_log: &mut File,
    follow_up: &mut File,
) -> Result<Plan, BackendError>;

pub(crate) fn decode_step_meta(
    step: &Step,
) -> Result<FollowUpStepMeta, BackendError>;

pub(crate) fn revalidate_open_workbooks(
    meta: &FollowUpStepMeta,
    rfi_log: &mut File,
    follow_up: &mut File,
) -> Result<(), BackendError>;

pub(crate) fn guarded_updates(
    step: &Step,
) -> Result<
    BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
    BackendError,
>;

pub(crate) fn verify_patched_output<R1, R2>(
    step: &Step,
    source: &mut R1,
    private_output: &mut R2,
) -> Result<(), BackendError>
where
    R1: Read + Seek,
    R2: Read + Seek;

pub(crate) fn changed_cell_count(
    step: &Step,
) -> Result<u64, BackendError>;

// native.rs
pub(crate) fn preview_rfi_follow_up<F>(
    &self,
    params: Value,
    progress: F,
) -> Result<Value, BackendError>
where
    F: FnMut(ProgressEvent);

// fs_ops.rs
pub fn replace_cached_plan(
    &self,
    old: &ClaimedPlan,
    caller: &CallerIdentity,
    replacement: Plan,
) -> Result<Plan, BackendError>;

pub fn recover_pending(
    &self,
    caller: &CallerIdentity,
) -> Result<Vec<RecoveryEntry>, BackendError>;

// paths.rs
pub(crate) struct ValidatedGrantRootRoster { /* private records + reopened CapabilityRoot handles */ }

impl ValidatedGrantRootRoster {
    pub(crate) fn records(&self) -> &[GrantRootAccessRecord];
    pub(crate) fn roots(&self) -> &[CapabilityRoot];
}

impl PathGrantStore {
    pub(crate) fn root_accesses_for_plan(
        &self,
        grant_ids: &[String],
        caller: &CallerIdentity,
        operation_id: &str,
    ) -> Result<ValidatedGrantRootRoster, BackendError>;

    pub(crate) fn reopen_recorded_roots(
        &self,
        records: &[GrantRootAccessRecord],
        caller: &CallerIdentity,
        operation_id: &str,
    ) -> Result<ValidatedGrantRootRoster, BackendError>;
}

fn execute_follow_up_workbook_update(
    &self,
    step: &Step,
    binding: &PlanBinding,
    caller: &CallerIdentity,
    token: &CancellationToken,
    record: &mut ValidatedS5Record,
) -> Result<u64, StepFailure>;

enum StepFailure {
    NoEffect(String),
    WarningNoEffect(Warning),
    FailedPartial {
        warning: Warning,
        retained: RetainedLocators,
    },
    OutcomeUnknown(String),
    WarningOutcomeUnknown(Warning),
    Cancelled,
}

// FilesystemAuthority::apply step-result match
Err(StepFailure::WarningOutcomeUnknown(warning)) => {
    let notice = RecoveryNotice::from_warning(&warning);
    let expected_revision = record.revision();
    record.prepare_outcome_unknown(notice)?;
    result.failed += 1;
    result.errors.push(warning); // preserve key + locator subject verbatim
    insert_effect_state(&mut result, "outcome_unknown");
    self.journals.write_s5_cas(&mut record, expected_revision)?;
    return Ok(result);
}

Err(StepFailure::FailedPartial { warning, retained }) => {
    // canonical outcome is known unchanged; operation-owned artifacts exist
    let notice = RecoveryNotice {
        warning_key: warning.key.clone(),
        locator: retained.first().to_owned(),
    };
    result.failed += 1;
    result.errors.push(warning);
    insert_effect_state(&mut result, "partial");
    insert_retained_locators(&mut result, retained.as_slice());
    let expected_revision = record.revision();
    record.prepare_failed_partial(notice)?;
    self.journals.write_s5_cas(&mut record, expected_revision)?;
    return Ok(result);
}
```

`NativeBackend::revise_native_plan` and `recovery/list` remain existing entry points. `retain_current_name_no_clobber` is same-parent and no-clobber. Legacy `create_new` remains only for old callers; S5 exclusively uses additive parent-relative `create_new_secured`. Existing replace/rollback helpers are forbidden on Follow Up branches.

Directory capability access is exhaustive. Compatibility-sensitive `CapabilityRoot::open` remains an explicit `ReadWrite` adapter for existing internal mutation/legacy undo callers; it is never used to issue a user read grant. `PathGrantStore::issue` maps `GrantAccess::Read → ReadOnly` and both `GrantAccess::Write` and `GrantAccess::ReadWrite → ReadWrite`; `issue_persisted_read_capabilities` explicitly opens `ReadOnly`; descendants preserve the root mode. Apply persists one `grant_root_accesses` record per deduplicated legacy root+identity, with access equal to the strongest bound grant for that root. A distinct RFI-Log-only root is `ReadOnly`; the Follow Up root is `ReadWrite`; when both selected files share one parent, the single root is necessarily `ReadWrite` because it contains the authorized target, while the original RFI Log grant itself remains `Read` and operation code never authorizes its path for write. S5 restart recovery/undo rejects absent/duplicate/conflicting access records and reopens each root with its recorded mode; legacy non-S5 journals with the new field absent keep the old `CapabilityRoot::open` path unchanged. On Windows, writable roots and descendants request `GENERIC_WRITE` in addition to the current read/attribute/synchronize rights, while the RFI Log read grant never receives write access. `PlatformDirectory::sync_all` rejects a read-only directory capability; for a writable capability it first runs the existing test injector and then calls real `FlushFileBuffers(self.0.as_raw_handle() as HANDLE)`, returning `last_os_error()` on failure. Source-boundary tests assert the access mapping and call order, but the Windows target and real runtime certification must prove the actual write-grant directory handle opens and flushes successfully and the read grant cannot. Until that certification passes, S5 makes no Windows power-loss durability claim.

All crash/race hook points are private `#[cfg(test)]` implementation details inside `leaf_transaction.rs` crate unit tests. If a callback helper is needed, it is compiled only under `cfg(test)` and is never exported to an integration test. `src-tauri/tests/*.rs` exercises only public/native effects and restart behavior. `rfi.log_add` stays on its existing executor and compatibility path.

## 11. Backup, journal, recovery, cancellation, and exact undo

The operation keeps the local journal as sole executable history and adds only backward-compatible optional transaction/recovery/evidence fields. It introduces no `StepKind`, backend method, cloud authority, blind retry, automatic restoration, or cleanup phase.

### 11.1 Durable transaction record, retained evidence, and result mapping

Private-output and backup creation share the exact write-ahead `PendingRetainedArtifact` lifecycle: `intent` before secure-create, `created(identity)` before any descriptor/auxiliary/default-byte work, or `created_identity_unknown` after create with failed identity acquisition. A crash preserves any present unknown leaf. Final evidence and pending-clear share one CAS. Before displacement, pending publication durably binds original/backup/source identities and byte/full-security hashes, separate destination access/auxiliary hashes, auxiliary charge, and canonical/retained/source locators. The private-output handle stays open from creation and final verification through apply publication. Undo opens the recorded backup once, validates identity plus byte/security hashes through that handle, and keeps it open through publication.

`JournalRecord::validate_record` requires bounded paths under persisted roots; backup, retained and canonical share the pinned canonical parent while private output uses its private parent. All hashes are lowercase 64-hex. Every retained evidence row requires checked `default_stream_bytes`, `auxiliary_charge_bytes`, and their exact `logical_charge_bytes` sum. `CanonicalSecured` requires created identity plus descriptor predicate; `CanonicalAuxiliaryReady` additionally requires auxiliary digest and charge and still precedes unnamed bytes. Active publication corresponds to exactly one pending Follow Up apply or undo `RestoreBackup`. S5 records require a complete one-to-one `grant_root_accesses` roster matching every deduplicated legacy root+identity, the strongest original bound access for each root, and a writable root containing canonical/backup/evidence. A distinct source-only root must be read-only; a shared source/target parent is writable only because the target grant requires it. `retained_evidence` validates every role/path/identity/hash/locator but is never executable authority. Old non-S5 records have the additive fields absent/empty and deserialize unchanged. Cloud history continues to project only existing metadata. Local history first performs a mutation-free lexical scan capped by `MAX_JOURNAL_ROOT_ENTRIES_SCANNED`; entry 4097 returns an empty fail-closed projection. It excludes exact notice names before parsing, removes the arbitrary-JSON fallback, requires successful `JournalRecord` deserialization and `validate_for_history_projection` (the same structural validator, without minting mutation authority), then projects only existing HistoryRow fields, and applies its requested row limit only after a valid row is produced; sidecars or malformed JSON cannot consume the limit or become rows. It never receives S5 authority/pending/evidence/publication/path/hash/identity/status/recovery fields.

Successful apply appends evidence for backup, private output, and displaced original; undo appends displaced applied evidence. UI exposes only ordered role/locator projections. S5 never auto-unlinks evidence. The two-directory 64-file/8-GiB logical-payload preflight uses the same default-plus-auxiliary charge for existing usage and reservations: apply charges two original default streams, bounded private default output, and three auxiliary copies; undo charges current applied default plus auxiliary. Arithmetic is checked and exact-at-limit is allowed. Authorized external edits after an artifact's verified linearization point are ordinary user filesystem mutations outside S5 quota authority and are rejected as drift by any later S5 operation; S5 itself never writes unreserved auxiliary payload.

`S5PermitBundle` is the only holder of the process-local and OS-wide permits. `ValidatedGrantRootRoster` is the only holder of the verified records plus reopened capability roots. `ClaimedS5Apply` consumes both opaque values only after exact one-to-one roster/claimed-plan grant identity+access validation and exposes the exact immutable plan/binding/digest/caller/root projections needed by `begin_s5`; it is non-Clone/non-Serialize and cannot outlive either permit. `begin_s5` consumes the claim into `ValidatedS5Record`, whose private lease keeps the claim, permits, and roots alive through executor/publication/CAS. Undo uses the targeted owner-checked `PendingS5Undo` path and consumes a replay-validated roster into the same token. Recovery instead owns a `S5RecoverySession`; for a valid core, `next_root_accesses` exposes only its immutable recorded roster, `FilesystemAuthority` asks `PathGrantStore::reopen_recorded_roots` for an opaque roster, and `recover_next` consumes/revalidates that roster before effect and retains it through transition/CAS. `None` is accepted only for invalid/legacy entries that perform no workbook-root mutation; cores/envelopes never escape. No caller-built raw record, grant vector, generic claim, or independently held token can enter S5 mutation.

Each transition is lease-gated: apply tokens can use only apply/common variants, undo tokens only undo/common variants, and `FinalizeRecovery` is callable only inside `S5RecoverySession`; mismatched variants fail before clone mutation. No whole-object publication rewrite exists after `InitializePublication`. Evidence variants fill exactly one previously absent identity/hash/security predicate for the exact expected phase. `FinalizeApplySuccess`, `FinalizeUndoSuccess`, and `FinalizeRecovery` each validate their exact terminal predecessor, completed Step, evidence roles and immutable authority, then atomically append/deduplicate evidence, append the completed step where applicable, set terminal journal state/result, and clear pending publication/artifact/retained state plus `followUpPhase` in one transition and one CAS. Legacy `read` refuses to return on-disk S5 and narrow `inspect_operation` supplies routing only. Legacy `write` internally holds the same OS mutation lock from existing-target inspection through durable replacement, validates both candidate and existing target, and refuses when either is S5, so clearing candidate discriminators cannot downgrade an existing journal. Only token/session CAS may persist S5. Tests enumerate every candidate discriminator and the exact read-S5/clear-all/write-existing downgrade.

Pre-displacement failures evaluate typed creation outcomes plus validated journal evidence on every error path. A live `Created` directory is immediately copied into in-memory `created_artifact_parent`; the next completion or terminal CAS durably records that exact identity. Only the live typed create branch may set this field—recovery and pathname observation may not. That field, an identified private leaf, completed step, retained evidence, or effectful phase is known effect; bare intent is not. `CreatedIdentityUnknown`, or restart intent whose leaf/name is present without durable identity, is outcome-unknown rather than no-effect or known partial. When canonical is proven unchanged and operation ownership is proven, the executor returns `FailedPartial`; the apply handler alone prepares the terminal transition and performs one CAS. Empty/partial identified private outputs, size-limit failures after bytes, pre-displacement drift, backup failure, and parent-sync after typed `Created` use this branch. CAS failure claims no terminal result and leaves earlier durable evidence for recovery.

Post-displacement unresolved paths return `StepFailure::WarningOutcomeUnknown` with a validated safe locator. Its apply-loop handler is the sole terminal CAS owner: `prepare_outcome_unknown` atomically validates the in-memory `RecoveryRequired` state/phase/notice, result projection sets `effectState=outcome_unknown`, and `write_s5_cas` writes exactly once. CAS failure leaves the last durable active-publication record for recovery and claims no terminal result. This branch remains distinct from known partial and no-effect.

The locator is non-secret and bounded: `journal:<journal-id>;root:<grant-root-index>;leaf:<operation-owned-leaf-name>`. Absolute local paths remain only in the validated journal. `RecoveryEntry` adds optional `warningKey`/`locator`; the recovery UI localizes the key and displays the locator.

Invalid raw S5 journal bytes are never rewritten through CAS. `begin_s5_recovery` consumes a permit bundle; its private scan performs one mutation-free journal-root iteration and stops fail-closed at `MAX_JOURNAL_ROOT_ENTRIES_SCANNED + 1`. It separately caps valid/invalid cores, notices, and results; preflights the complete existing-notice/invalid-ID union before mutation; excludes notice names from journal candidates; and enforces journal bytes, duplicate-free exact wire keys, and filename-ID parity.

The exact notice filename is `.invalid-s5-<source_sha256>.json`, where the suffix is the envelope’s lowercase 64-hex source hash; no raw journal ID enters a filename. This lexical class is excluded from journal candidates and `cloud.rs` local history before any requested limit. `S5RecoverySession::recover_next` alone can consume its internal invalid envelope or validated core. Quarantine reopens the exact root-relative source and requires identity/hash parity before create-new owner-only notice+fsync. Existing notices are no-follow regular/non-reparse owner-only, at most 4096 bytes, and duplicate-free parsed. Originals remain untouched. The session retains both permits through every notice/CAS and emits at most 256 results; no iterator exports a core or envelope.

### 11.2 Exact hash-and-identity crash classification

Recovery first validates journal owner, root identities, path containment, no-reparse anchors, exact backup identity/hash, exact publication-source identity/hash, and every available canonical/retained identity/hash. It opens a recorded source by name only once, validates the durably recorded identity and hash through that `File`, and never publishes from a merely matching path or hash. “Absent” means a no-follow anchored `NotFound`; access/metadata errors are unknown, never absence.

For apply, `O/Iₒ` is the expected original hash/identity, `A/Ic` is the publication hash plus durably recorded created-canonical identity, `P/Ip` is the private-output hash/identity, and `R/Ir` is the exact retained original recorded after displacement. Undo uses the same table with expected canonical `A/Ia`, publication source `O/Ib` from the exact backup handle, and retained applied `A/Ir`.

| Canonical name | Retained leaf | Durable created identity | Exact classification/action |
|---|---|---|---|
| `O/Iₒ` | absent | absent | no canonical effect before displacement; retain backup/private output, durably clear pending publication/artifact plus `follow_up_phase`, transition to exact no-canonical-effect classification, never delete an artifact |
| absent | `R/Ir == O/Iₒ` | absent | exact displacement recognized, but canonical is absent; preserve all evidence and require recovery, with no automatic restoration or publication |
| any foreign identity/hash | `R/Ir == O/Iₒ` | absent or present | racer/foreign canonical wins untouched; preserve retained original/source/backup and require recovery |
| `A/Ic` | `R/Ir == O/Iₒ` | `Ic` | exact applied state only when canonical, retained, source, and backup byte/security predicates match; durably complete journal classification, keep every retained artifact |
| identity `Ic` with empty/partial/wrong hash | exact or absent | `Ic` | interrupted open-handle write or later same-identity edit cannot be distinguished safely; preserve and require recovery, never truncate/rewrite automatically |
| any canonical after phase `creating_canonical` | any | not durably recorded | crash after create but before identity journal; canonical ownership is unknown, so preserve it and require recovery |
| name does not resolve to `Ic` after `Ic` was recorded | any | `Ic` | created file was renamed/replaced or a racer occupies canonical; never remove/overwrite the current name, preserve recorded evidence, require recovery |
| any | wrong/unrecorded retained identity/hash | any | never restore, publish, overwrite, or delete the retained leaf; require recovery even when canonical is absent |
| any | any | any, with missing/wrong backup or publication source | no automatic classification as success and no publication/cleanup; require recovery |

A post-displacement directory flush error and every create/write/file-sync/name-resolution uncertainty are classified from these exact states; the effect is never blindly retried. Recovery may durably complete a no-effect or exact-applied journal classification, but it does not delete evidence or move a retained object back to canonical. Any unrecorded identity remains untouched and `RecoveryRequired`.

`FilesystemAuthority::recover_pending(caller)` acquires process-local then journal-root permits, calls the declared bounded `scan_pending_s5`, and consumes each opaque entry while holding both permits. Pending parent intent plus absence is no effect; presence is preserved as outcome-unknown and never creates provenance. A pre-existing durable `created_artifact_parent` is accepted only when current identity matches and is then known partial; mismatch/access ambiguity is outcome-unknown. Pending private/backup intent plus absence is no effect; presence without durable created identity is preserved outcome-unknown. `created(identity)` plus exact identity is known partial. `created_identity_unknown` is outcome-unknown. Closed phases use the publication matrix. Invalid tokens use the notice path. Only non-S5 records with every S5 field absent use legacy logic. Native recovery calls this route, never pathname-only `JournalStore::recover_pending`; repeated calls remain bounded and idempotent.

### 11.3 Cancellation

Apply cancellation before any proven artifact effect is `CancelledClean`; after a typed `Created`, identified retained artifact, backup, or effectful phase it is `CancelledPartial`. A `CreatedIdentityUnknown` or restart-present bare intent is outcome-unknown, not clean or known partial. Cancellation and non-cancellation share this exact typed classifier. Apply passes `PublicationCancellation::Apply(token)`; the final clean checkpoint remains immediately before displacement.

There is no cancellation checkpoint or clean-cancellation semantics after displacement begins. A late cancellation is deferred while the helper reaches either exact applied classification or durable recovery-required classification. If implementation ever observes cancellation between retained-leaf validation and create-new canonical, it must return localized `WarningOutcomeUnknown`/recovery-required because canonical has already been displaced; it may not report clean cancellation or restore the retained leaf.

### 11.4 Exact undo uses an open backup handle and retains all evidence

`engine/undo` may perform one provisional validated read only to discriminate the immutable operation ID. On S5 it fails closed on native `object.contains_key("destination")` (including null) or direct `destination.is_some()`, before selection resolution, pending undo, cloud mutation, or filesystem effect. S5 undo is intentionally non-cancellable after admission. It acquires both permits, rereads the exact journal, and uses only that locked record. Before any artifact it validates current canonical and backup identities plus byte/security/stream digests, including a fresh complete snapshot from the backup that equals the journaled original predicate. It reopens the persisted roots, reserves quota, and keeps the exact backup handle/snapshot open through publication. Pending undo records the complete expected predicates.

Undo has no cancellation checkpoint. It retains/verifies applied canonical, derives the descriptor from exact same-parent backup/parent snapshots, secure-creates O_EXCL, records `canonical_secured`, replays/verifies EA/ADS, records `canonical_auxiliary_ready`, writes unnamed bytes, and performs the same final complete linearization recapture. A racer or mismatch is preserved and recovery-required.

Successful undo restores the byte-identical pre-apply Follow Up workbook, including macros and unrelated package members, while retaining the backup, displaced applied leaf, displaced original, and any persistent private output. S5 undo clears only pending undo/publication state after durable exact classification. It does not run reverse-`Copy` cleanup, remove operation-created artifact directories, or use displaced original as restore authority. No S5 apply, recovery, or undo path uses source-name publication rename, overwrite rename, check-then-replace, replacement retry, automatic restoration of a retained leaf, or name-based deletion.

## 12. Unsafe and protected workbook policy

Preview fails closed for:

- same source and target filesystem identity;
- non-`.xlsx`/`.xlsm`, malformed, encrypted, oversized, traversal-bearing, duplicate-member, or unsupported OOXML packages;
- signed Follow Up packages;
- RFI Log or Follow Up workbook protection;
- protection on any scanned `CIV`/`MEC`/`ELE`/`PRC` source sheet;
- protected or hidden/veryHidden discovered Follow Up worksheet;
- ambiguous/missing headers or over-limit tables;
- unavailable metadata/hash/identity.

A formula-bearing or merged selected status cell is a row-level manual review because other rows in the same otherwise-safe sheet may remain actionable. Unknown nonblank target statuses are also row-level manual review. Formula source evidence never authorizes a mapping.

`.xlsm` is supported because no Office process starts and the package writer preserves `vbaProject.bin`; macros are never executed. External links, connections, drawings, charts, custom XML, and unknown inert members are preserved, not refreshed. A real Office-created fixture gate remains mandatory before release claims.

## 13. Localization keys

Every new user-visible key must exist with independent text in English, Azerbaijani, Russian, and Turkish:

```text
op.rfi.follow_up_sync.title
op.rfi.follow_up_sync.desc
param.rfi.followUp.rfiLogWorkbook
param.rfi.followUp.rfiLogWorkbook.help
param.rfi.followUp.followUpWorkbook
param.rfi.followUp.followUpWorkbook.help
error.param.required.rfi_log_workbook
error.param.required.follow_up_workbook
rfi.followUp.editor.title
rfi.followUp.editor.rowCount
rfi.followUp.column.enabled
rfi.followUp.column.rfiNo
rfi.followUp.column.sourceStatuses
rfi.followUp.column.currentStatus
rfi.followUp.column.proposedStatus
rfi.followUp.column.disposition
rfi.followUp.column.reason
rfi.followUp.column.sheet
rfi.followUp.column.row
rfi.followUp.disposition.ready
rfi.followUp.disposition.alreadySynced
rfi.followUp.disposition.manualReview
warn.rfi.followUp.reason.blankRfi
warn.rfi.followUp.reason.invalidRfi
warn.rfi.followUp.reason.targetRfiFormula
warn.rfi.followUp.reason.targetRfiMerged
warn.rfi.followUp.reason.targetDuplicate
warn.rfi.followUp.reason.sourceMissing
warn.rfi.followUp.reason.sourceFormula
warn.rfi.followUp.reason.sourceMerged
warn.rfi.followUp.reason.sourceStatusBlank
warn.rfi.followUp.reason.sourceStatusUnknown
warn.rfi.followUp.reason.sourceConflict
warn.rfi.followUp.reason.targetCellMissing
warn.rfi.followUp.reason.targetFormula
warn.rfi.followUp.reason.targetMerged
warn.rfi.followUp.reason.targetStatusUnknown
warn.rfi.followUp.reason.alreadySynced
warn.rfi.followUp.reason.readyCompleted
warn.rfi.followUp.reason.readyOngoing
warn.rfi.followUp.noRowsSelected
warn.rfi.followUp.manualRows
error.rfi.followUp.workbookInvalid
error.rfi.followUp.sameWorkbook
error.rfi.followUp.layoutMissing
error.rfi.followUp.layoutAmbiguous
error.rfi.followUp.protected
error.rfi.followUp.signedPackage
error.rfi.followUp.limitExceeded
error.rfi.followUp.sourceChanged
error.rfi.followUp.targetChanged
error.rfi.followUp.workbookLocked
error.rfi.followUp.patchVerification
error.rfi.followUp.recoveryRequired
error.rfi.followUp.retainedStorageFull
rfi.followUp.retainedRole.backup
rfi.followUp.retainedRole.privateOutput
rfi.followUp.retainedRole.displacedOriginal
rfi.followUp.retainedRole.displacedApplied
note.rfi.followUp.workbookUpdate
note.rfi.followUp.restoreBackup
```

Subjects contain only bounded workbook display names, sheet/row coordinates, RFI numbers, or counts. They never include an entire workbook cell dump or filesystem secret.

## 14. Compatibility and regression boundary

- `rfi.log_add` keeps its exact operation ID, parameters, preview rows, compatibility oracle, formula generation, consecutive-duplicate validation behavior, apply path, executor internals, and copy-table path; S5 never routes through it.
- Existing `WorkbookSafetyFacts`, worksheet-print descriptors, print queues, settings, and protocol-1 methods remain wire-compatible.
- The Rust registry snapshot and `frontend/src/lib/bridge.ts` demo bootstrap both expose five operations in the same canonical order; method rosters remain exactly 31 because no method is added.
- `docs/RELEASE_SAFETY.md` says “exact five-operation registry” and adds the RFI Follow Up regression boundary in the same implementation change.
- Existing journals deserialize unchanged because no new `StepKind` is introduced and new journal/RecoveryEntry fields are additive optional fields. Legacy pending records retain their existing classification path.
- Existing generic undo remains authoritative for non-`RestoreBackup` steps; Follow Up `RestoreBackup` uses the shared retained-evidence/open-handle transaction and bypasses destructive S5 backup/directory cleanup.
- Existing frontend editors are unchanged when `disabledWhen` and `display: "localizedKey"` are absent. `disabledWhen` is implemented only for bool checkboxes in this slice.
- EN/AZ/RU/TR catalog key equality remains exact, including both generated required-parameter keys.
- No existing compatibility fixture is rewritten to hide a regression. The new frozen fixture is `src-tauri/tests/fixtures/rfi_follow_up_compatibility_oracle.json`.

The new compatibility oracle covers, at minimum: exact `Completed`; exact `Ongoing`; already synchronized; blank/unknown source status; source formula/merge; same-status source duplicate; conflicting source duplicate; missing source; invalid/formula/merged target RFI; duplicate target RFI; physically present blank target status; unknown nonblank target status; absent target status cell; target status formula/merge; deterministic row IDs; exact row order; default `enabled`/`selectable`; and the zero-step no-selection warning.

## 15. Test strategy

Every production behavior follows an observed focused RED → minimal GREEN → related regression cycle.

1. **Namespace-strict OOXML foundation:** canonical Strict/Transitional worksheet hierarchy, duplicate-coordinate rejection, foreign-namespace shadow rejection, merge/protection/signature facts, and handle reads go GREEN before classification is tested.
2. **Pure matching/classification and bounded layout:** exact whole-RFI canonicalization, status map, duplicate matrix, no fuzzy fallback, unique header pair, limits, and formula/merge cases over the now-complete facts.
3. **Anchored preview and registry:** two opaque snake_case selections, exact grants, same-file rejection, source/target bytes unchanged, additive five-operation Rust/demo snapshots, picker filters, locale keys, and release canon.
4. **Revise/store/editor:** exact row vector/order/immutability, atomic same-ID successful replacement with original TTL/binding/backend provenance, failure leaves old plan unchanged, bool-only disabled checkbox, details-open localized reason, and no-step warning.
5. **Guarded patch fidelity:** selected value representation is the only allowed canonical event-token delta; row metadata, validations, table/hyperlink/drawing/comment relationships, extension subtrees, styles, and every unrelated member survive; runtime verification rejects injected non-cell changes.
6. **Retain/create-new apply transaction:** RED crash/race matrix precedes production helper work and covers dual-workbook drift, lexical-before-directory backup authorization, pre-effect hashes/identities, wrong retained identity, create collision, crash before created-identity journal, visible partial write, post-create replacement, Windows flush uncertainty, localized outcome-unknown mapping, and exact applied classification.
7. **Journal/recovery/undo:** actual `recovery/list` wiring, every canonical/retained/created/source/backup identity+hash state, permanent evidence, same open-handle create-new undo lifecycle, later-edit preservation without restoration, no S5 cleanup, and old-journal compatibility.
8. **Compatibility:** frozen new oracle plus unchanged `slice6_rfi_compatibility_oracle.json`, OOXML fidelity, filesystem authority, cloud metadata, frontend, build, and source/package audits.
9. **Independent exact-candidate review:** immutable staged candidate fingerprint, all literal gates bound to it, read-only reviewer verdict, and complete evidence invalidation after any edit.

Automated tests synthesize inert OOXML packages and never start Office. They assert source RFI Log hash/mtime and Follow Up hash remain unchanged through preview/revise and through every pre-effect failure.

## 16. Acceptance criteria

- Bootstrap and the demo bridge expose a separate fifth `rfi.follow_up_sync` operation with exactly two required Excel file selections; protocol method count remains 31.
- Preview reads both workbooks through caller/session/operation-bound anchored capabilities and mutates nothing; its internal DTO decodes exact snake_case registry keys.
- The only automatic source mappings are exact `Completed → Completed` and `Ongoing → In Progress`.
- RFI equality uses the exact full canonical string; no fuzzy, substring, or placement inference exists.
- Worksheet evidence comes only from the canonical Strict/Transitional hierarchy; duplicate physical row IDs, duplicate coordinates, empty high-row extent, and foreign shadows fail closed.
- Header discovery is unique and bounded to 32 rows × 64 columns; data and preview limits fail closed.
- Safe rows alone default enabled. Blank/unknown/missing-cell/formula/merged/unmatched/duplicate/conflicting/unknown-target rows are visible, manual-only, and disabled. Already-synchronized rows are visible and disabled.
- The user can toggle only safe `ready` bool checkboxes; revise rejects all representable row/metadata forgeries and source drift.
- Successful revise atomically preserves the same plan ID, binding/grants, backend origin/id, `created_at`, and TTL while removing old plan contents/authority; failure leaves the old record unchanged.
- Apply changes only exact approved Follow Up status coordinates and never changes RFI Log bytes.
- Both workbook predicates and the target's complete security/stream snapshot are revalidated immediately before backup and displacement; retained original metadata is revalidated after displacement before recreation.
- Unknown nonblank Follow Up statuses are not clobbered.
- Runtime canonical event-token verification proves the modified worksheet delta is restricted to approved cell value representations; all other worksheet events/attributes/subtrees/relationships and unrelated package members are preserved.
- Backup authorization is lexical before effect. Backup is atomically descriptor-created no-clobber in canonical parent; EA/ADS and unnamed bytes are verified, with authorized metadata drift handled by final recapture rather than a false global share lock.
- Original/applied byte/security hashes, expected original and backup identities/byte/security hashes, exact private-output identity/hash, and unique retained locator are revision-CAS durable before canonical displacement.
- Apply and undo retain destination-no-clobber, create canonical O_EXCL with non-null original descriptor and `ShareAccess=0`, verify/CAS `canonical_secured`, replay/verify auxiliary streams, CAS `canonical_auxiliary_ready`, then stream unnamed bytes. No pre-hardening handle can be captured; publication is not atomic.
- Successful apply permanently retains the exact displaced original in addition to the verified backup/private output. Successful undo permanently retains the backup and displaced applied leaf. S5 performs no automatic retained-leaf, backup, or operation-created-directory cleanup.
- Zero-step apply is backend-rejected before mutation and remains revise-only. Nonzero apply, targeted undo, and recovery use opaque sessions that own both permits and validated roots through every operation/CAS; raw legacy read/write cannot service or downgrade S5.
- Invalid S5 notice sidecars are excluded before local-history limiting and never become history rows.
- Journal recovery is wired through `FilesystemAuthority` and recognizes only exact durably recorded canonical/retained/created/source/backup identities plus byte/security hashes. Wrong or unrecorded identities remain untouched and recovery-required; a mismatched retained leaf is never restored.
- Undo rejects destination overrides, is non-cancellable after admission, verifies complete applied/backup byte/security/stream predicates, secures the empty replacement before bytes, preserves racers, and restores exact original bytes/metadata/streams.
- `StepFailure::FailedPartial` preserves the original localized failure key, validated locators, and `effectState=partial` when canonical is unchanged and operation ownership is proven. `StepFailure::WarningOutcomeUnknown(Warning)` covers both pre-displacement create/provenance ambiguity (`CreatedIdentityUnknown` or restart-present bare intent) and every post-displacement unresolved outcome; it preserves the recovery-required key, bounded locator, and `effectState=outcome_unknown`.
- Apply cancellation is explicit: the final clean checkpoint is before displacement; undo is non-cancellable and there is no clean-cancellation semantics after the first canonical effect.
- Existing `rfi.log_add`, print, generic protocol, legacy journal, and frontend compatibility suites remain green.
- `frontend/src/lib/bridge.ts` and `docs/RELEASE_SAFETY.md` agree with the canonical five-operation roster.
- Windows requires exact-candidate compile plus release-account proof of computed privilege handling, atomic descriptor-at-create, adversarial retained-handle denial, complete security/EA/ADS round-trip, >1-MiB default-stream `BackupSeek`, owner-only private output, and real `FlushFileBuffers`/crash tests. Missing required privilege blocks S5 rather than downgrading fidelity.
- Every new key, including `error.param.required.rfi_log_workbook` and `error.param.required.follow_up_workbook`, exists in EN/AZ/RU/TR.
- External real-Office fixture certification remains a separate external gate; local/synthetic tests never claim it.

## 17. External Office fixture certification

Source/local completion remains distinct from Microsoft Office certification. On a clean supported Windows host with the exact installed candidate and Microsoft Excel:

1. Create approved sanitized Office-authored pairs with the package matrix plus narrow protected DACL, explicit audit SACL, mandatory label/CAP where supported, EA, DOS attribute, `Zone.Identifier`, another benign ADS, and a broader-access parent. Add protected/signed/formula/merge negatives.
2. Record candidate commit/installer SHA-256, Windows/Excel versions, fixture source SHA-256/size/mtime, worksheet/cell matrix, and VBA project hash. Never use production QC workbooks or confidential cell content.
3. Run preview and revise; verify both fixture files remain byte- and metadata-identical and manual rows cannot be enabled.
4. Apply a bounded set; verify backup/canonical complete byte/security/stream predicates, retained original, package fidelity, and Excel behavior. Prove a broader-parent principal cannot read private output, backup, or canonical; an authorized observer may see empty/partial canonical only after original access controls are effective.
5. Prove destination-bearing undo fails, then ordinary undo restores exact SHA-256 plus complete redacted security/stream digest; retain all evidence.
6. Verify missing privilege, unreadable/over-bound metadata, and content/ACL/SACL/xattr/EA/ADS drift fail before recreation. Verify cross-process contention preserves the same available plan, stale warning CAS fails, and cancellation/recovery/racer/flush matrices never overwrite, restore, or clean up automatically.
7. Save sanitized evidence under the repository's release-evidence process only after the exact candidate passes. Record failures honestly; Office certification is not inferred from macOS, cross-compilation, or synthetic OOXML tests.

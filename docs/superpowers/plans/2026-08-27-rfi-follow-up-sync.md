# RFI Follow Up Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a separate, fail-closed RFI Follow Up operation that previews exact RFI/status matches and changes only enabled, preauthorized Follow Up status cells with verified backup, open-handle create-new publication, permanent retained evidence, recovery, and exact undo.

**Architecture:** Rust opens both workbooks through anchored capabilities and performs bounded OOXML discovery. Apply uses atomically supplied security descriptors, unnamed-stream zero sharing, durable identity/hash/charge evidence, destination-no-clobber displacement, and a non-clone secured replacement handle. Windows ADS/EA share limitations and the authorized-principal threat-model boundary are explicit. Publication remains visibly non-atomic but no broader pre-hardening handle is capturable. Undo uses the same lifecycle from an identity-validated same-parent backup.

**Tech Stack:** Rust 2024, Tauri 2, `quick-xml`, `zip`, SHA-256, existing capability filesystem and journal/undo services, React 19, TypeScript 7, Vitest, ARB localization.

**Spec:** `docs/superpowers/specs/2026-08-27-rfi-follow-up-sync-design.md`

**GitHub issue:** `#26`

**Accepted base:** `7c33e4beaa7af1ba595ef52eed5e46bac8fd2d14`

**Execution precondition:** The coordinator has reviewed and committed this design and plan in a docs-only commit after the accepted base. The implementation executor starts from that clean commit, records it in the evidence ledger, and never absorbs these documents into a production/test commit. The final candidate comparison still uses the accepted S4 base so the complete S5 design, plan, implementation, and tests are reviewed together.

## Global Constraints

- Create the separate operation ID `rfi.follow_up_sync`; do not add a mode to `rfi.log_add`.
- Keep protocol version `1` and use only existing generic backend methods.
- Use exactly two required file parameters: `rfi_log_workbook` and `follow_up_workbook`.
- Accept `.xlsx` and `.xlsm` only.
- Read selected workbooks from anchored handles; domain preview/revise must not reopen a user path.
- Source RFI matching is whole-value NFC + trim + uppercase against `AZE-SUN-INT-QMS-RFI-[A-Z]{3}-[0-9]{4}`; Follow Up displays and stores only exact short `(CIV|MEC|ELE|PRC)-[0-9]{4}` values, with the fixed prefix added internally only for source lookup.
- Derive `Signed` only from a case-sensitive trimmed REMARKS line exactly equal to `Signed by Worley`; otherwise derive `Not Signed`.
- Keep blank/invalid/unmatched/missing-cell/formula/merged/duplicate-target/conflicting-source and unknown target-status rows manual-only with `enabled=false` and `selectable=false`. Only blank, `Signed`, and `Not Signed` target statuses are recognized.
- Discover exactly one `RFI NO.`/`STATUS` header pair within 32 rows × 64 columns; never infer a sheet or neighboring column.
- Enforce 20,000 source data rows per sheet, 10,000 Follow Up data rows, 500 source evidence rows per canonical RFI across all projected sheets, 500 preview rows/updates, 20,000 Unicode scalar values per cell, at most 1,000,000 merge-overlap validation comparisons per workbook and 2,000,000 total overlap-plus-membership comparisons across the source/target preview/revise pair, and an exact 8-MiB borrowed-serialization budget over the complete preview/editor plus immutable step-authority projection. Guarded apply is a fresh target-only validation pass with its own exact 2,000,000 overlap-plus-membership comparison account.
- Parse worksheet evidence only from accepted Strict/Transitional SpreadsheetML `worksheet/sheetData/row/c` hierarchy; reject duplicate coordinates and foreign-namespace shadows before classification.
- Preview and revise are inert.
- A successful revise replaces the same cached plan atomically, preserves plan/binding/backend provenance and original TTL, and leaves no superseded apply authority.
- Apply never recalculates the approved proposal; it only revalidates approved fingerprints, layout, evidence, and cells.
- Runtime verification permits only the approved target-cell value-representation XML delta and proves every other worksheet event/attribute/subtree/relationship unchanged.
- Unknown nonblank Follow Up statuses are never clobbered.
- Workbook protection on either input, source-sheet/target-sheet protection, and signed, hidden/veryHidden, ambiguous, malformed, encrypted, or over-limit targets fail closed.
- The RFI Log grant is read-only. Only the selected Follow Up workbook, its verified backup/private output, and operation-owned retained evidence leaves are mutable.
- Use existing `Copy` and `RestoreBackup` step kinds; additive journal transaction/recovery fields must preserve old-journal deserialization.
- Apply and undo use a destination-no-clobber rename only to retain the current canonical name. Publication never renames a source leaf: canonical is create-new/O_EXCL and is populated through its already-open `File`. Never call overwrite replacement/retry/rollback helpers for S5, never restore a mismatched retained leaf, and never automatically delete retained or backup evidence.
- Private-output parent authorization is lexical before journaled creation; backup is separately authorized and created in the already pinned canonical parent so inheritance semantics are reproducible.
- `disabledWhen` is implemented only for the bool checkbox branch required here; do not broaden unused editor controls or bulk behavior.
- The Rust registry, demo bridge, and release-safety canon all say five operations; backend/frontend method rosters stay exactly 31.
- Every user-visible key exists independently in English, Azerbaijani, Russian, and Turkish.
- Every production behavior follows an observed focused RED before its minimal GREEN.
- Preserve all existing RFI Log, print, settings, journal, queue, protocol, and frontend behavior.
- Local/synthetic tests never start Office and cannot certify Microsoft Excel runtime behavior.
- Implement access-aware directory capabilities: read grants remain read-only; only write grants request Windows `GENERIC_WRITE`.
- S5 apply/undo/recovery hold process-local then journal-root OS-wide mutation permits; every S5 journal transition is revision-CAS. S5 undo rejects destination overrides and is explicitly non-cancellable after admission.
- Recreated canonical and same-parent backup files preserve complete bounded Unix security/xattrs or Windows owner/group/DACL/SACL/CAP/label/EAs/attributes/ADS. Windows supplies security atomically in `NtCreateFile`, uses `ShareAccess=0`, verifies it before auxiliary/default bytes, and exact-skips the default `BACKUP_DATA` stream. Required privilege absence is pre-effect failure. Private output is atomically owner-only. Recovery includes complete digest; real Windows privilege, retained-handle, ADS, flush and crash certification is mandatory.

---

## File Map

- Create `src-tauri/src/rfi_follow_up.rs`: exact normalization, bounded source/layout scan, row classification, plan/revise DTOs, selected update generation, and output verification.
- Create `src-tauri/src/leaf_transaction.rs`: shared journaled retain/create-new/open-handle-stream/classify lifecycle used by Follow Up apply, startup recovery, and undo.
- Modify `src-tauri/src/lib.rs`: export `rfi_follow_up` and crate-private `leaf_transaction`.
- Modify `src-tauri/Cargo.toml`: add `Win32_Security_Authorization` to existing `windows-sys` features for token privilege and complete descriptor handling; existing WDK/FileSystem features supply handle stream enumeration. No new crate.
- Modify `src-tauri/src/cap_fs.rs`: expose identity/access-aware roots, bounded security/parent snapshots, selective BackupRead/Seek/Write auxiliary handling, atomic `create_new_secured`, no-follow quota and parent sync, same-parent no-clobber retain, and real Windows `FlushFileBuffers`; preserve legacy `create_new`/`replace_with`.
- Modify `src-tauri/src/rfi_log.rs`: expose the existing four exact RFI/status column projections without changing append behavior.
- Modify `src-tauri/src/ooxml.rs`: anchored reader adapter, canonical namespace/hierarchy parsing, duplicate/shadow rejection, protection/merge/signature facts, guarded exact-cell patch, and runtime canonical-event delta verifier.
- Modify `src-tauri/src/paths.rs`: exact-grant capability authorization used before a plan binding exists.
- Modify `src-tauri/src/plan_store.rs`: atomic same-ID `replace_bound_available` with original provenance/TTL preservation and focused race/failure tests.
- Modify `src-tauri/src/journal.rs`: additive transaction/evidence/recovery DTOs, default-zero revision and S5 CAS, permanent cross-process mutation permit, strict validation, validated pending-record iteration, compatibility tests, quota constants, and recovery/result DTO projection.
- Modify `src-tauri/src/undo.rs`: route Follow Up `RestoreBackup` through the shared retained-evidence transaction, reconcile pending undo states, and skip S5 backup/directory cleanup.
- Modify `src-tauri/src/fs_ops.rs`: anchoring wrappers, atomic plan replacement wrapper, operation-specific Follow Up apply, explicit localized outcome-unknown mapping, shared recovery route, and crate-unit-only transaction injection points while preserving the RFI Log executor.
- Modify `src-tauri/src/registry.rs`: insert the fifth operation in canonical order and update registry validation.
- Modify `src-tauri/src/native.rs`: exact snake_case preview decode, dual grants, anchored preview, revise branch, in-place plan replacement, `recovery/list` wiring through `FilesystemAuthority`, and pre-resolution S5 undo destination rejection.
- Modify `src-tauri/src/backend.rs`: preview dispatch and two Excel picker filters; no new method.
- Modify `src-tauri/src/smoke.rs`: require five operations.
- Modify `src-tauri/tests/fixtures/protocol1_operations.json`: additive protocol-1 operation snapshot.
- Create `src-tauri/tests/fixtures/rfi_follow_up_compatibility_oracle.json`: frozen exact row/classification cases.
- Create `src-tauri/tests/rfi_follow_up_native.rs`: synthetic OOXML preview/revise/apply/undo/compatibility tests.
- Modify `src-tauri/tests/ooxml_fidelity.rs`: namespace-shadow, duplicate-coordinate, exact event-delta, guarded status-cell, and package-preservation coverage.
- Modify `src-tauri/tests/filesystem_authority.rs`: persisted transaction/restart recovery and legacy-journal compatibility coverage.
- Modify `src-tauri/tests/rfi_log_native.rs`: regression assertion that existing Slice 6 oracle/apply behavior remains unchanged.
- Modify `frontend/src/App.tsx`: bool-only `disabledWhen`, read-only `localizedKey`, and localized recovery locator display.
- Modify `frontend/src/App.test.tsx`: operation render, details-open reason, disabled manual rows, exact revise payload, and recovery locator behavior.
- Modify `frontend/src/types.ts`: additive optional `warningKey`/`locator` on `RecoveryEntry`.
- Modify `frontend/src/lib/bridge.ts`: fifth demo operation definition and bootstrap roster.
- Modify `frontend/src/lib/bridge.test.ts`: exact five-operation demo roster test.
- Modify `frontend/src/lib/frontend-authority.test.ts`: four-catalog key equality, exact required Follow Up keys, and five-operation release-canon assertion.
- Modify `frontend/src/locales/app_{en,az,ru,tr}.arb`: complete Follow Up operation/editor/reason/error/note/required-parameter/recovery text.
- Modify `docs/RELEASE_SAFETY.md`: replace four-operation canon with five and add the RFI Follow Up regression boundary.

---

## Task 1: Build namespace-strict OOXML facts before classification

**Exact symbol ledger:** Task 1 RED `follow_up_authority_` defines and rejects malformed/duplicate `FollowUpApprovedUpdate`; Step 7 implements it in `rfi_follow_up.rs`; the same filter and full native target run GREEN before commit.

**Files:**
- Create: `src-tauri/src/rfi_follow_up.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/ooxml.rs`
- Modify: `src-tauri/src/rfi_log.rs`
- Create: `src-tauri/tests/fixtures/rfi_follow_up_compatibility_oracle.json`
- Test: `src-tauri/tests/rfi_follow_up_native.rs`
- Test: `src-tauri/tests/ooxml_fidelity.rs`

- [ ] **Step 1: Write only the namespace/hierarchy/safety RED tests**

Add these six focused tests before any classification test:

- `worksheet_facts_accept_only_canonical_strict_or_transitional_hierarchy`
- `worksheet_facts_reject_duplicate_coordinates_before_indexing`
- `worksheet_facts_reject_foreign_namespace_cell_shadowing`
- `worksheet_facts_capture_formula_merge_protection_signature_and_visibility`
- `worksheet_facts_merge_overlap_validation_budget_is_bounded`
- `worksheet_facts_reject_duplicate_physical_rows_and_count_empty_row_extent`

Fixtures must independently prove:

- direct canonical `worksheet/sheetData/row/c` cells work under exactly the Transitional or Strict SpreadsheetML URI;
- zero/multiple canonical `sheetData` nodes, canonical `c` outside direct `row`, row/c coordinate mismatch, or a repeated canonical coordinate fail closed;
- every physical canonical row has a unique valid `row@r`; two `<row r="1">` elements with disjoint `A1`/`B1` cells are rejected before authority synthesis, and an empty `<row r="10010"/>` trips the physical row limit;
- an element whose local name is `c` and whose `r` parses as a coordinate but whose namespace is foreign is rejected rather than ignored as a shadow;
- merge ranges, formulas, workbook/sheet protection, signed-package relationships, and hidden/veryHidden state are all surfaced before a caller can classify rows;
- duplicate coordinates are represented as ordered XML input and rejected before any `BTreeMap` insertion. Do not write an impossible duplicate-`BTreeMap`-key or duplicate-JSON-object-field test.

- [ ] **Step 2: Run the foundation tests to verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test ooxml_fidelity worksheet_facts_ -- --nocapture
```

Expected: FAIL because the namespace-strict fact APIs/validation and typed workbook-wide overlap budget do not exist.

- [ ] **Step 3: Implement the minimal namespace-strict facts and handle reader**

In `ooxml.rs`:

```rust
pub fn read_workbook_from_reader<R: Read + Seek>(
    source: &mut R,
) -> Result<WorkbookData, OoxmlError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OoxmlErrorKind {
    Invalid,
    LimitExceeded,
}

impl OoxmlError {
    pub(crate) fn kind(&self) -> OoxmlErrorKind;
}

pub(crate) const MAX_MERGE_VALIDATION_COMPARISONS_PER_WORKBOOK: u64 = 1_000_000;

impl WorkbookData {
    pub(crate) fn merge_validation_comparisons(&self) -> u64;
    pub(crate) fn merge_validation_complete(&self) -> bool;
}

pub(crate) struct MergeComparisonBudget { /* private remaining count */ }

// rfi_follow_up.rs
pub(crate) fn read_workbook_for_follow_up<R: Read + Seek>(
    source: &mut R,
) -> Result<WorkbookData, BackendError>;
```

Keep `read_workbook(&Path)` as a thin `File::open` adapter. Resolve namespaces from XML declarations/prefixes and accept only:

```rust
const TRANSITIONAL_SPREADSHEETML_NS: &str =
    "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const STRICT_SPREADSHEETML_NS: &str =
    "http://purl.oclc.org/ooxml/spreadsheetml/main";
```

Build worksheet facts only from one canonical `worksheet > sheetData > row > c` chain. Maintain a `seen_row_refs` set before cell parsing; reject missing/invalid/out-of-range or duplicate physical `row@r`, update `max_row` from every row including empty rows, require each cell row to equal its container, then reject duplicate cell coordinates and foreign shadows before map insertion. Preserve existing bounded ZIP/member/XML protections and add reusable facts for formula, merge, protection, signature, and visibility checks. Expose the actual RFI Log projection without duplicating or changing `layout`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RfiStatusColumns {
    pub sheet: &'static str,
    pub rfi_column: u32,
    pub status_column: u32,
}

pub(crate) fn status_columns() -> [RfiStatusColumns; 4];
```

`status_columns()` returns exactly `CIV: 1/16`, `MEC: 1/17`, `ELE: 1/16`, `PRC: 1/16`, in that order. These values are projections of the current private `layout`; do not add unrelated 3/4/8/9 constants.

Export `rfi_follow_up` from `lib.rs`, but do not implement row classification yet.

- [ ] **Step 4: Run the foundation tests to verify GREEN**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test ooxml_fidelity worksheet_facts_ -- --nocapture
```

Expected: PASS. Do not proceed while namespace/hierarchy/merge/formula/protection/signature facts are inferred or fixture-only.

- [ ] **Step 5: Write the independent matching/layout/classification RED tests and frozen oracle**

Now add independent tests named:

- `canonical_rfi_uses_the_entire_nfc_trimmed_uppercased_value_without_internal_repair`
- `source_matrix_allows_only_exact_completed_and_ongoing`
- `source_duplicates_require_same_safe_status`
- `follow_up_layout_requires_one_exact_header_pair_within_bounds`
- `classification_keeps_unsafe_rows_visible_but_unselectable`
- `formula_and_merge_facts_drive_manual_review`
- `source_evidence_per_rfi_limit_fails_before_projection_amplification`
- module-unit `projection_budget_exact_boundary_and_one_byte_over`
- module-unit `projection_budget_rejects_500_by_500_before_owned_materialization`
- module-unit `source_group_analysis_runs_once_for_500_duplicate_targets`
- module-unit `merge_query_budget_exact_boundary_and_one_over`
- `follow_up_reader_maps_parser_limit_to_exact_key`
- `merge_query_budget_bounds_distinct_rfi_range_amplification`
- `compatibility_oracle_matches_exact_rows`
- `follow_up_authority_rejects_unknown_fields_and_duplicate_updates`

Freeze an oracle that includes exact `Completed`, exact `Ongoing`, already synchronized, blank/unknown source status, source formula/merge, same-status duplicate, conflicting duplicate, missing source, invalid/formula/merged/duplicate target RFI, physically present blank status, absent status cell, formula/merged target status, unknown nonblank target status, exact order, deterministic row IDs, enabled/selectable defaults, and no-selection behavior. Preserve real `CIV`/`MEC`/`ELE`/`PRC` worksheet provenance when deduplicating source evidence. No fuzzy/substring/placement test may pass.

- [ ] **Step 6: Run the classification tests to verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native follow_up_authority_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib rfi_follow_up::tests::projection_budget_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib rfi_follow_up::tests::source_group_analysis_runs_once_for_500_duplicate_targets -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib rfi_follow_up::tests::merge_query_budget_ -- --nocapture
```

Expected: FAIL because matching/layout/classification APIs, the borrowed budget writer, and once-per-group analysis do not exist. Formula/merge tests are now reachable because Step 4 made their facts real. The private module-unit commands are mandatory RED evidence; the integration target cannot compile or observe their `#[cfg(test)]` counters.

- [ ] **Step 7: Implement the minimal pure matcher and bounded scanner**

Implement `rfi_follow_up.rs` DTOs and pure helpers exactly as the design specifies:

- `canonical_rfi` and exact status mapping;
- source evidence aggregation with provenance and formula/merge flags, accepting exactly 500 short evidence rows for one RFI with one short target only when the independent byte budget also fits, and rejecting the 501st row split across at least two projected sheets with `error.rfi.followUp.limitExceeded` before row/editor/step projection;
- a private borrowed preview/editor/step projection serialized into a checked non-allocating `CountingLimitWriter`; exact 8-MiB and one-byte-over module-unit tests must prove the boundary without timeout/OOM assertions;
- a 500-evidence × 500-target permitted-count fixture with long-but-individually-valid values must fail the borrowed byte preflight with `limitExceeded`, while a `#[cfg(test)]` materialization counter remains zero;
- once-per-canonical-group source analysis reused by every target row so duplicate targets do not repeat merge/status scans; a module-unit counter proves one source analysis call for 500 duplicate targets;
- private parser-produced `WorkbookData.merge_validation_comparisons` plus `merge_validation_complete`, with crate-visible read-only accessors and no setter/deserializer; `Default` is `0/false`, production S5 rejects incomplete facts, and only successful all-sheet parse sets true. Comparisons are charged before every range-vs-range overlap check and capped at `MAX_MERGE_VALIDATION_COMPARISONS_PER_WORKBOOK = 1_000_000` across each workbook; focused OOXML tests prove exact boundary, one-over typed failure, overlap rejection, no per-sheet reset, incomplete-default rejection, and no uncharged validation path;
- structured `OoxmlErrorKind::{Invalid, LimitExceeded}` plus `read_workbook_for_follow_up`, with focused S5 tests proving only the typed parser limit maps to `error.rfi.followUp.limitExceeded` and malformed input remains `error.rfi.followUp.workbookInvalid`; no message parsing or duplicated limit constant;
- one mutable checked `MergeQueryBudget` initialized to `MAX_FOLLOW_UP_MERGE_COMPARISONS = 2_000_000` minus the checked sum of both trusted parser counts and shared by every S5 header/source/target membership lookup; exact total boundary/one-over tests plus adversarial maximum-overlap and many-distinct-RFI/many-range fixtures prove deterministic `limitExceeded` before unbounded CPU work;
- unique bounded Follow Up layout discovery and exact row classification;
- deterministic row IDs and plan/editor metadata;
- crate-visible `FollowUpStepMeta` with private fields/nested DTOs, consumed only through crate-visible module functions;
- `Vec`-first duplicate update/row validation before keyed map construction.

Do not read raw paths here. Do not create a backup or output. Do not implement apply.

- [ ] **Step 8: Run the complete Task 1 slice GREEN**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test ooxml_fidelity worksheet_facts_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native follow_up_authority_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib rfi_follow_up::tests::projection_budget_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib rfi_follow_up::tests::source_group_analysis_runs_once_for_500_duplicate_targets -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib rfi_follow_up::tests::merge_query_budget_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_log_native -- --nocapture
```

Expected: PASS. Confirm the existing RFI Log compatibility oracle is unchanged.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/ooxml.rs src-tauri/src/rfi_log.rs src-tauri/src/rfi_follow_up.rs src-tauri/tests/ooxml_fidelity.rs src-tauri/tests/rfi_follow_up_native.rs src-tauri/tests/fixtures/rfi_follow_up_compatibility_oracle.json
git commit -m "feat(rfi): add namespace-safe follow-up classification"
```

---


## Task 2: Add the fifth registry/demo operation and anchored inert preview

**Exact symbol ledger:** Task 2 preview RED names `FollowUpPlanMeta`, `fingerprint_open_file`, and `preview_rfi_follow_up`; implementation is in `rfi_follow_up.rs`/`native.rs`; exact test `follow_up_preview_uses_two_exact_grants_and_is_inert` under filter `native::tests::follow_up_preview_` runs RED and GREEN before commit.

**Files:**
- Modify: `src-tauri/src/rfi_follow_up.rs`
- Modify: `src-tauri/src/plan_store.rs`
- Modify: `src-tauri/src/paths.rs`
- Modify: `src-tauri/src/fs_ops.rs`
- Modify: `src-tauri/src/registry.rs`
- Modify: `src-tauri/src/native.rs`
- Modify: `src-tauri/src/backend.rs`
- Modify: `src-tauri/src/smoke.rs`
- Modify: `src-tauri/tests/fixtures/protocol1_operations.json`
- Modify: `src-tauri/tests/rfi_follow_up_native.rs`
- Modify: `frontend/src/lib/bridge.ts`
- Modify: `frontend/src/lib/bridge.test.ts`
- Modify: `frontend/src/lib/frontend-authority.test.ts`
- Modify: `frontend/src/locales/app_{en,az,ru,tr}.arb`
- Modify: `docs/RELEASE_SAFETY.md`

**Interfaces:**

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

// rfi_follow_up.rs
pub(crate) fn validate_owned_plan_budget(plan: &Plan) -> Result<(), BackendError>;

// plan_store.rs
pub(crate) struct ReservedPlanId { /* private, non-Clone, non-Serialize */ }

impl ReservedPlanId {
    pub(crate) fn assign_to_empty(&self, plan: &mut Plan) -> Result<(), BackendError>;
}

pub(crate) fn reserve_plan_id() -> ReservedPlanId;

pub(crate) fn insert_bound_reserved(
    &self,
    reservation: ReservedPlanId,
    origin: BackendOrigin,
    binding: PlanBinding,
    plan: Plan,
) -> Result<Plan, BackendError>;

// fs_ops.rs
pub(crate) fn cache_reserved_plan(
    &self,
    reservation: ReservedPlanId,
    origin: BackendOrigin,
    binding: PlanBinding,
    plan: Plan,
) -> Result<Plan, BackendError>;
```

No `BackendMethod` variant or frontend method-roster entry is added.

- [ ] **Step 1: Write registry/demo/release/locale RED tests**

Select registry operations by ID, then assert `rfi.follow_up_sync` has exactly the two required `File` parameters, both defaults are `null`, and `supportsPresets=false`. Add:

```rust
assert_eq!(operation_registry(RegistryFacts::default()).len(), 5);
assert_eq!(BackendMethod::ALL.len(), 31);
```

Update backend picker expectations so both new parameter names and existing `rfi.log_add/workbook` return `Excel (.xlsm, .xlsx)`.

In `bridge.test.ts`, call the demo `MockBridge` bootstrap and assert the exact operation IDs/order are:

```ts
[
  "rfi.process_scans",
  "rfi.log_add",
  "rfi.follow_up_sync",
  "files.batch_print",
  "rfi.stage_signed_first_pages",
]
```

The literal full order is `rfi.process_scans`, `rfi.log_add`, `rfi.follow_up_sync`, `files.batch_print`, `rfi.stage_signed_first_pages`; existing relative order is unchanged.

In `frontend-authority.test.ts`, assert all four ARB catalogs have identical key sets; assert both `error.param.required.rfi_log_workbook` and `error.param.required.follow_up_workbook` exist in every locale; assert `docs/RELEASE_SAFETY.md` contains the exact phrase `exact five-operation registry` and an RFI Follow Up regression row. This is the RED before editing the canon.

- [ ] **Step 2: Observe the REDs with real test entry points**

```bash
cargo test --manifest-path src-tauri/Cargo.toml registry::tests::canonical_registry_matches_protocol_one_snapshot -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml backend::tests::rfi_log_workbook_picker_offers_macro_enabled_and_standard_excel_files -- --exact --nocapture
npx vitest run --config frontend/vite.config.ts frontend/src/lib/bridge.test.ts frontend/src/lib/frontend-authority.test.ts
```

Expected: RED for missing fifth Rust/demo operation, snapshot/filter, required locale keys, and release canon. Use only the existing Vitest entry points shown above.

- [ ] **Step 3: Implement the exact operation, demo roster, locales, and release canon**

Insert the exact operation block from the design in canonical order. Add its serialized object to `protocol1_operations.json` with existing camelCase contract fields, exact snake_case parameter `name`s, `null` defaults, empty choices/labels/visibility, and `supportsPresets: false`. Change smoke expectations from four to five operations but keep 31 methods.

In `frontend/src/lib/bridge.ts`, define the matching demo operation and insert it in the exact five-operation bootstrap list. Update every ARB with independent text for the full design key list, including:

```text
error.param.required.rfi_log_workbook
error.param.required.follow_up_workbook
error.rfi.followUp.recoveryRequired
```

Change `docs/RELEASE_SAFETY.md` to `exact five-operation registry` and add the Follow Up row/invariants without weakening existing four operation rows.

- [ ] **Step 4: Run registry/demo/release/locale GREEN**

```bash
cargo test --manifest-path src-tauri/Cargo.toml registry::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml backend::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml smoke::tests -- --nocapture
npx vitest run --config frontend/vite.config.ts frontend/src/lib/bridge.test.ts frontend/src/lib/frontend-authority.test.ts
```

Expected: PASS; five operations in both Rust and demo, 31 methods, exact locale parity, updated release canon.

- [ ] **Step 5: Write anchored-preview and exact-Serde RED tests**

Add `follow_up_preview_uses_two_exact_grants_and_is_inert` plus a decode test. Assert:

- exact snake_case `rfi_log_workbook`/`follow_up_workbook` values succeed while camelCase aliases/unknown keys fail;
- forged display paths, wrong caller/session/operation/parameter selections, and same-file identities fail;
- RFI Log grant is read-only, Follow Up grant read-write, and the binding contains exactly those grants;
- `opId`, one workbook step, no-clobber backup path, editor rows, fingerprints, and updates match the design;
- both workbook hashes/sizes/mtimes and parent directory contents are unchanged;
- no `.docflow-backups` directory exists after preview;
- cancellation remains inert.

Add `follow_up_preview_rejects_owned_plan_over_budget_before_cache`. Construct a final owned `Plan` whose exact reserved `planId` and all metadata make it one byte over. Assert `validate_owned_plan_budget` returns `error.rfi.followUp.limitExceeded`, `cache_reserved_plan` is never called, provisional grants are revoked, and no plan ID or filesystem effect is published. Add an exact-at-limit module-unit writer case so the limit is inclusive.

Add focused `plan_store::tests::insert_bound_reserved_` RED tests proving reservation is store-inert; `ReservedPlanId` is non-Clone/non-Serialize with private storage; `assign_to_empty` rejects a nonempty plan ID and assigns a canonical lowercase hyphenated UUID; reserved insertion consumes the token, preserves the exact serialized `plan_id` and every other `Plan` field, and digests that exact plan; a plan ID mutated after assignment, collision, binding failure, and occupied ID leave the store unchanged; and legacy `insert_bound` remains unchanged. Add `follow_up_preview_reserved_plan_budget_matches_cached_bytes`: use the real S5 cache path, resolve the stored returned `Plan`, require byte-for-byte equality with the budget-validated plan, and serialize it through the same bound.

- [ ] **Step 6: Observe anchored-preview RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml native::tests::follow_up_preview_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml plan_store::tests::insert_bound_reserved_ -- --nocapture
```

Expected: FAIL because preview dispatch, exact decode, and dual-grant anchoring do not exist.

- [ ] **Step 7: Implement the minimal native preview route**

Decode only exact registry parameter keys:

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

Call `materialize_preview(params)` first, then issue exactly `[rfi-parent/Read, follow-up-parent/ReadWrite]` under one scope. Anchor only the two existing workbook leaves, compare identities, and call `preview_from_open_files` in the blocking task. Compute the future backup path lexically; do not create its directory or attempt to anchor that absent leaf during preview. Reserve the inert opaque `ReservedPlanId`, call `assign_to_empty` on the complete owned `Plan`, and then call `rfi_follow_up::validate_owned_plan_budget(&plan)`. That helper serializes the exact final plan through the checked non-allocating 8-MiB `CountingLimitWriter`; it does not clone the plan or allocate a second JSON buffer. Move the same reservation token into `cache_reserved_plan` with `BackendOrigin::Rust` and cloned grant IDs; insertion compares its private ID to the already-budgeted `plan.plan_id` and must not rewrite any serialized field. Consume provisional-grant `commit()` only after insertion succeeds. Any assignment, budget, mismatch, collision, or insertion failure revokes provisional grants and leaves no cached plan or filesystem effect.

Implement `PathGrantStore::issue_provisional` with a 1–8 bound, ordered issues, explicit prior-ID revocation on error, and existing drop guard. `authorize_capability_for_grant` looks up exact grant ID, rejects expiry, requires complete grant equality/access, and returns capability authorization; `anchor_granted_path` calls `AuthorizedPath::anchor` and never reopens a caller raw path. Add the existing backend preview match branch only; no command/special form/direct React filesystem access.

- [ ] **Step 8: Run preview and route/security GREEN**

```bash
cargo test --manifest-path src-tauri/Cargo.toml native::tests::follow_up_preview_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml plan_store::tests::insert_bound_reserved_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml native::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml backend::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml registry::tests -- --nocapture
npx vitest run --config frontend/vite.config.ts frontend/src/lib/bridge.test.ts frontend/src/lib/frontend-authority.test.ts
```

- [ ] **Step 9: Review and commit**

```bash
git diff --check
git add src-tauri/src/rfi_follow_up.rs src-tauri/src/plan_store.rs src-tauri/src/paths.rs src-tauri/src/fs_ops.rs src-tauri/src/registry.rs src-tauri/src/native.rs src-tauri/src/backend.rs src-tauri/src/smoke.rs src-tauri/tests/fixtures/protocol1_operations.json src-tauri/tests/rfi_follow_up_native.rs frontend/src/lib/bridge.ts frontend/src/lib/bridge.test.ts frontend/src/lib/frontend-authority.test.ts frontend/src/locales/app_en.arb frontend/src/locales/app_az.arb frontend/src/locales/app_ru.arb frontend/src/locales/app_tr.arb docs/RELEASE_SAFETY.md
git commit -m "[verified] feat: preview RFI Follow Up synchronization"
```

---

## Task 3: Atomic same-ID revise authority and bool-only safe editor

**Files:**
- Modify: `src-tauri/src/plan_store.rs`
- Modify: `src-tauri/src/fs_ops.rs`
- Modify: `src-tauri/src/rfi_follow_up.rs`
- Modify: `src-tauri/src/native.rs`
- Modify: `src-tauri/tests/rfi_follow_up_native.rs`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/App.test.tsx`

**Interfaces:**

```rust
// plan_store.rs
pub fn replace_bound_available(
    &self,
    plan_id: &str,
    caller: &CallerIdentity,
    expected_digest: &str,
    replacement: Plan,
) -> Result<Plan, BackendError>;

// rfi_follow_up.rs
pub fn revise_from_open_files(
    plan: &Plan,
    rows: &[Value],
    rfi_log: &mut File,
    follow_up: &mut File,
) -> Result<Plan, BackendError>;
```

```diff
   display?: string;
+  disabledWhen?: { field: string; equals: string | number | boolean };
```

The existing string-typed `display` field accepts the new handled literal `"localizedKey"`; do not narrow it.

- [ ] **Step 1: Write `PlanStore` atomic replacement RED tests**

In `plan_store.rs`, add focused unit tests that insert one real bound `PlanState::Available` plan, then prove a successful replacement using only the fields and states that exist in the repository:

- keeps the identical opaque `plan_id`;
- preserves the complete `PlanBinding { owner_id, session_id, operation_id, grant_ids }` unchanged and preserves stored `backend_plan_id`/`origin`;
- preserves original `created_at` and expiration deadline rather than refreshing TTL;
- changes plan contents/digest exactly once;
- makes the old digest/content unavailable to resolve/claim/apply.

Add independent failures for wrong caller, wrong session, operation mismatch, expected-digest mismatch, expired entry, existing `PlanState::Claimed`, concurrent revise/revise, and revise/apply claim. `PlanState` remains exactly `Available | Claimed`; do not invent Applying or terminal variants. Every failure must compare the actual snapshot fields before/after and find them unchanged; in each race exactly one lock-protected authority transition wins.

- [ ] **Step 2: Observe `PlanStore` RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml plan_store::tests::replace_bound_available_ -- --nocapture
```

Expected: FAIL because the atomic API does not exist.

- [ ] **Step 3: Implement only the atomic store primitive under one lock**

`replace_bound_available` acquires the `plans` mutex once, looks up the target without eager removal/pruning, validates caller/session, `PlanState::Available`, original TTL, `StoredPlan.digest`, a fresh `plan_digest(&stored.plan)`, and replacement operation. It assigns the old ID to the candidate, computes the candidate digest, and only then atomically replaces exactly `StoredPlan.plan` and `StoredPlan.digest`. Preserve `binding` byte-for-byte unchanged together with `backend_plan_id`, `origin`, `created_at`, state, grants, and the original TTL deadline. An expired target is rejected unchanged for later normal cleanup. No remove-then-insert gap and no new binding field/state are allowed. Do not wire native revise yet.

- [ ] **Step 4: Run the `PlanStore` GREEN**

```bash
cargo test --manifest-path src-tauri/Cargo.toml plan_store::tests::replace_bound_available_ -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Write revise authority RED tests**

Add `follow_up_revise_accepts_only_enabled_on_ready_exact_rows`. Start from a real preview and independently submit removed, inserted, duplicated, reordered, or forged rows; immutable-field/unknown-field changes; `enabled=true` on nonselectable rows; and over-limit rows. These are representable `Vec<Value>` cases. Do not test duplicate JSON object fields after `serde_json::Value::Object` has already collapsed to a unique-key map.

Positive cases disable/re-enable ready rows, preserve exact order and immutable fields, and build only selected updates. Zero selection yields zero steps plus `warn.rfi.followUp.noRowsSelected` while retaining immutable plan authority. Workbook/evidence/layout/cell drift fails without replacing the cached plan or writing either workbook. Successful revise must retain the same plan ID/provenance/TTL and make the old digest/content unavailable.

- [ ] **Step 6: Observe revise RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native follow_up_revise_ -- --nocapture
```

Expected: FAIL because native revise still has no Follow Up branch/in-place replacement.

- [ ] **Step 7: Implement native anchored revise and atomic install**

Add `FilesystemAuthority::replace_cached_plan` as a validating wrapper. In the Follow Up branch of `NativeBackend::revise_native_plan`, resolve the old plan/binding, re-anchor both exact workbooks through preserved grants, call `revise_from_open_files`, and then call only `replace_cached_plan`; successful native revise must not call `cache_plan`/`insert_bound`. Revalidate exact row order, every immutable field, both fingerprints/evidence/layout/cells, and preserve original `src`/backup `dst`/metadata.

- [ ] **Step 8: Run revise GREEN and operation regressions**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native follow_up_revise_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml plan_store::tests::replace_bound_available_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_log_native -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test signed_pages -- --nocapture
```

Expected: PASS while existing operation branches remain behavior-compatible.

- [ ] **Step 9: Write the frontend details/checkbox RED test**

Add `renders RFI Follow Up reasons and revises only safe enablement`. Render ready/manual/already-synced rows. Assert the ready bool checkbox is enabled and the other two disabled. Because existing details are collapsed by default, first click the localized show-details control, then assert the localized reason is visible:

```ts
const strings = makeStrings("en");
fireEvent.click(
  screen.getByRole("button", { name: strings.t("editor.action.showDetails") }),
);
expect(
  screen.getByText(strings.t("warn.rfi.followUp.reason.sourceConflict")),
).toBeVisible();
```

Toggle the ready row and Apply. Assert `engine/revise` receives every row once, original order, with only that row's `enabled` changed. Exercise zero-step then safe re-enable without a new preview.

- [ ] **Step 10: Observe frontend RED**

```bash
npx vitest run --config frontend/vite.config.ts frontend/src/App.test.tsx -t "renders RFI Follow Up reasons and revises only safe enablement"
```

Expected: RED because the manual bool checkbox is enabled and localized-key display is absent.

- [ ] **Step 11: Implement only the required generic behavior**

Add `editorCellDisabled` and apply it only to the `kind === "bool"` checkbox branch. Do not change select/input/textarea/date controls or bulk-edit paths in this slice; the Follow Up editor already disables bulk/selection/duplication/reordering in metadata. For read-only columns with `display === "localizedKey"`, render `s.t(text)` and keep raw key in `title`. Existing editor behavior remains unchanged when these optional fields are absent.

- [ ] **Step 12: Run frontend and revise GREEN**

```bash
npx vitest run --config frontend/vite.config.ts frontend/src/App.test.tsx -t "renders RFI Follow Up reasons and revises only safe enablement"
npx vitest run --config frontend/vite.config.ts frontend/src/App.test.tsx
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml plan_store::tests::replace_bound_available_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native follow_up_revise_ -- --nocapture
```

- [ ] **Step 13: Review and commit**

```bash
git diff --check
git add src-tauri/src/plan_store.rs src-tauri/src/fs_ops.rs src-tauri/src/rfi_follow_up.rs src-tauri/src/native.rs src-tauri/tests/rfi_follow_up_native.rs frontend/src/App.tsx frontend/src/App.test.tsx
git commit -m "[verified] feat: atomically revise Follow Up plans"
```

---


## Task 4: Guarded exact-value patch with runtime event-delta proof

**Exact symbol ledger:** Task 4 RED names `decode_step_meta`, `guarded_updates`, `verify_patched_output`, and `changed_cell_count`. `rfi_follow_up.rs` implements them only after RED. Filters `duplicate_approved_coordinates`, `guarded_follow_up_`, and `guarded_follow_up_apply_merge_budget_` execute every seam in GREEN.

**Files:**
- Modify: `src-tauri/src/cap_fs.rs`
- Modify: `src-tauri/src/ooxml.rs`
- Modify: `src-tauri/src/rfi_follow_up.rs`
- Modify: `src-tauri/tests/ooxml_fidelity.rs`
- Modify: `src-tauri/tests/rfi_follow_up_native.rs`

**Interfaces:**

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

- [ ] **Step 1: Write exact event-delta RED tests**

Add `guarded_follow_up_patch_changes_only_authorized_value_events` for both Transitional and Strict worksheets. Build an `.xlsm` package whose target sheet has selected styled status cells plus nonselected text/number/bool/formula cells, row attributes/spans, data validation, hyperlink, table/drawing/comment relationships, extension subtrees, unknown namespaced content, whitespace, and custom attributes. Include byte sentinels for every unrelated ZIP member, relationship part, styles/shared strings/content types/custom XML/external links/VBA/calc chain.

After two guarded updates, assert:

- selected cells retain `r`, style, and every non-value attribute/subtree;
- each output uses exact `t="inlineStr"><is><t>{approved replacement}</t></is>` representation whose decoded text is that coordinate's approved `Completed` or `In Progress` value;
- all normalized worksheet event tokens outside those exact value representations are equal;
- target worksheet `.rels` and every non-target member are byte-identical.

Then tamper separate outputs by changing a row attribute, non-target cell, target non-value child, validation, relationship, extension subtree, and unrelated ZIP member. Every tampered output must be rejected by the production `verify_guarded_workbook_delta`, proving the runtime gate rather than fixture-only fidelity.

Add `guarded_writer_stays_on_pinned_handles_after_visible_parent_swap`: anchor source and writable output parent, replace the visible `.docflow-backups` path with a symlink/reparse target, then pass the already-open source and create-new destination handles into the writer/verifier. Assert no file appears in the replacement path and every byte read/written/verified remains bound to the pinned handles.

Add exact integration test `duplicate_approved_coordinates_rejected_before_guarded_map`: deserialize an ordered `Vec<FollowUpApprovedUpdate>` with two entries for the same sheet/coordinate, call the production map builder, assert exact invalid-plan error, zero destination bytes, and no call to guarded patching. The existing `duplicate_approved_coordinates` command filter must execute this named test in RED and GREEN.

Add separate integration rejects for empty/more-than-500 maps, absent/wrong/formula/merged selected cells, wrong expected style/value, nonallowed replacement values/update kinds, signed/protected target, foreign namespace shadow, destination collision, and output that would write byte `MAX_PACKAGE_BYTES + 1`. In private `ooxml.rs` module tests add `guarded_follow_up_apply_merge_budget_exact_boundary_and_one_over`: the fresh target parse's trusted overlap count plus every selected-cell membership comparison shares `MAX_APPLY_MERGE_COMPARISONS = 2_000_000`; exact limit is accepted, one-over asserts crate-visible `OoxmlErrorKind::LimitExceeded` before the first destination workbook byte, and no worksheet or selected coordinate can reset the account. The integration crate verifies the public no-write effect but does not access the crate-private discriminator. The caller's already-journaled empty private-output handle may remain retained. The size failure leaves the partial destination handle/file for journaled retention and never opens or unlinks by path. In `rfi_follow_up_native.rs`, feed two approved update DTOs for the same sheet/coordinate in the ordered `Vec` and require rejection before map construction; do not test duplicate keys in a `BTreeMap`.

- [ ] **Step 2: Observe RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test ooxml_fidelity guarded_follow_up_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib ooxml::tests::guarded_follow_up_apply_merge_budget_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native duplicate_approved_coordinates -- --nocapture
```

Expected: FAIL because guarded patch/delta APIs and Vec-first duplicate validation are missing.

- [ ] **Step 3: Implement the surgical guarded writer and canonical event verifier**

Validate the entire update map before creating the no-clobber destination. Parse the fresh target with the same overlap-accounting parser, reject a parser count above `MAX_APPLY_MERGE_COMPARISONS`, create one internal `MergeComparisonBudget` from the checked remainder, and charge every selected-cell merge-range comparison before comparison; the guarded API accepts no caller budget and cannot be reset per sheet/cell. Require each selected physical cell and complete expected `CellData`, no formula/merge, and exact approved replacement string. In the target cell preserve every attribute except its text-representation `t` attribute and every non-value child event/subtree. Replace only canonical `<v>` or `<is>` representation with one namespace-correct inline string; insert before canonical `extLst` when no representation exists. Never clear the whole `children` vector, create `<c>`/row, rewrite validation, or touch RFI/header/table placement.

Implement a namespace-resolved canonical event stream. For every selected coordinate the verifier:

1. proves source and output each have exactly one direct canonical cell at the journaled hierarchy;
2. requires source complete attributes/children to equal the approved expected snapshot;
3. requires all target-cell attributes other than `t`, and all non-value child subtrees/events, exactly equal;
4. requires output `t="inlineStr"` plus exactly one canonical inline-string value whose decoded text equals that coordinate's approved `Completed` or `In Progress` replacement, with `xml:space="preserve"` only when needed;
5. substitutes a coordinate/value marker only for the approved value representation and requires the complete token streams equal.

Expose `MAX_PACKAGE_BYTES` and implement guarded patching over generic seekable source/destination boundaries. Tests use test-owned seekable files only; this slice does not create or wire persistent private output through legacy `create_new`. Compare every non-target ZIP member byte-for-byte and verify canonical event deltas. Persistent output wiring is deferred until `create_new_secured` is GREEN, then uses the same wrapper through patch, sync, hash, and verification.

- [ ] **Step 4: Run guarded GREEN and regressions**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test ooxml_fidelity guarded_follow_up_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib ooxml::tests::guarded_follow_up_apply_merge_budget_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test ooxml_fidelity -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml ooxml::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native duplicate_approved_coordinates -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_log_native -- --nocapture
```

- [ ] **Step 5: Review and commit**

```bash
git diff --check
git add src-tauri/src/cap_fs.rs src-tauri/src/ooxml.rs src-tauri/src/rfi_follow_up.rs src-tauri/tests/ooxml_fidelity.rs src-tauri/tests/rfi_follow_up_native.rs
git commit -m "[verified] feat: verify exact Follow Up XML deltas"
```

---


## Task 5: Retained-evidence create-new apply, crash recovery, and undo

**Exact symbol ledger:** Task 5 test `follow_up_retained_apply_revalidates_open_workbooks_at_every_boundary` names `revalidate_open_workbooks`; Step 11 implements it in `rfi_follow_up.rs`. Filter `fs_ops::tests::follow_up_retained_apply_` runs RED/GREEN and the full native/filesystem suites rerun before commit.

**Files:**
- Create: `src-tauri/src/leaf_transaction.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/cap_fs.rs`
- Modify: `src-tauri/src/plan_store.rs`
- Modify: `src-tauri/src/paths.rs`
- Modify: `src-tauri/src/journal.rs`
- Modify: `src-tauri/src/cloud.rs`
- Modify: `src-tauri/src/undo.rs`
- Modify: `src-tauri/src/fs_ops.rs`
- Modify: `src-tauri/src/native.rs`
- Modify: `src-tauri/src/rfi_follow_up.rs`
- Modify: `src-tauri/tests/rfi_follow_up_native.rs`
- Modify: `src-tauri/tests/filesystem_authority.rs`
- Modify: `frontend/src/types.ts`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/App.test.tsx`

**Interfaces:**

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

// cap_fs.rs
pub(crate) fn file_identity(file: &File) -> io::Result<FilesystemObjectIdentity>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectoryAccess { ReadOnly, ReadWrite }

impl CapabilityRoot {
    // Legacy internal adapter remains ReadWrite; grant/replay paths are explicit.
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
pub enum RetainedPublicationPurpose { Apply, Undo }

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedPublicationPhase {
    Prepared, Displacing, Retained, CreatingCanonical,
    CanonicalSecured, CanonicalAuxiliaryReady, WritingCanonical, VerifyingCanonical,
    Applied, RecoveryRequired,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedArtifactRole {
    Backup, PrivateOutput, DisplacedOriginal, DisplacedApplied,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FollowUpJournalPhase {
    ArtifactParentReady, PrivateOutputReady, BackupReady, PublicationActive,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistedRootAccess { ReadOnly, ReadWrite }

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
    pub fn read(&self, journal_id: &str) -> Result<JournalRecord, BackendError>; // legacy only; rejects on-disk S5
    pub fn write(&self, record: &JournalRecord) -> Result<(), BackendError>; // holds S5 OS lock; rejects candidate or existing-target S5
    pub(crate) fn inspect_operation(&self, journal_id: &str) -> Result<String, BackendError>;
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
        record: &mut ValidatedS5Record, // owns permit bundle through admission lease
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
pub(crate) struct RetainedLocators(Vec<String>);

impl S5RecoverySession {
    fn scan_pending_s5(&mut self) -> Result<PendingS5Scan, BackendError>;
    pub(crate) fn next_root_accesses(&self) -> Option<&[GrantRootAccessRecord]>;
    pub(crate) fn recover_next(
        &mut self,
        roots: Option<ValidatedGrantRootRoster>,
        caller: &CallerIdentity,
    ) -> Result<Option<RecoveryEntry>, BackendError>; // all mutation stays internal
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
    pub(crate) fn plan(&self) -> Option<&Plan>;
    pub(crate) fn binding(&self) -> Option<&PlanBinding>;
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
    InitializePublication(RetainedPublication),
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

// journal.rs
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvalidS5JournalNotice {
    pub(crate) journal_id: String,
    pub(crate) source_identity: FilesystemObjectIdentity,
    pub(crate) source_sha256: String,
    pub(crate) warning_key: String,
    pub(crate) locator: String,
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

pub(crate) enum RetainedPublicationOutcome {
    Published { created_identity: FilesystemObjectIdentity },
    CancelledBeforeDisplacement,
    RecoveryRequired(RecoveryNotice),
}

pub(crate) fn reconcile_retained_publication(
    journals: &JournalStore,
    record: &mut ValidatedS5Record,
) -> Result<Option<RecoveryEntry>, BackendError>;

// fs_ops.rs
fn execute_follow_up_workbook_update(
    &self,
    step: &Step,
    binding: &PlanBinding,
    caller: &CallerIdentity,
    token: &CancellationToken,
    record: &mut ValidatedS5Record,
) -> Result<u64, StepFailure>;

pub fn recover_pending(
    &self,
    caller: &CallerIdentity,
) -> Result<Vec<RecoveryEntry>, BackendError>;



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
    result.errors.push(warning); // preserve localized key + locator subject
    insert_effect_state(&mut result, "outcome_unknown");
    self.journals.write_s5_cas(&mut record, expected_revision)?;
    return Ok(result);
}

Err(StepFailure::FailedPartial { warning, retained }) => {
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

`JournalRecord` gains optional `pending_publication`, optional `pending_artifact_parent`, optional `pending_retained_artifact` with typed creation state, default-empty `retained_evidence`, optional `recovery_notice`, optional `follow_up_phase`, default-empty `grant_root_accesses`, and default-zero `revision`; `RecoveryEntry` gains optional `warningKey`/`locator`. No step kind or backend method is added.

- [ ] **Step 1: Write capability, security-metadata, preclaim, interprocess-lock, and Windows flush RED tests**

In `cap_fs.rs` and `plan_store.rs` unit tests, plus the named cross-process lock tests in `journal.rs`, prove:

- `secured_create_anchored_create_new_no_clobber` is the new Task 5 RED: absent anchored name returns the live handle, existing destination yields `AlreadyExists` untouched, and no overwrite/truncate path exists;
- `retain_current_name_no_clobber` moves the current canonical name only to an absent same-parent retained leaf; an existing retained destination wins and both leaves remain untouched;
- a racer replacing canonical before displacement may be moved, but its exact bytes remain at the retained leaf and nothing is overwritten or deleted;
- post-rename directory-sync failure is outcome-unknown and the rename is never retried;
- cross-parent/cross-device retention is rejected with no copy fallback;
- compatibility `CapabilityRoot::open` remains `ReadWrite` for legacy internal mutation; ordinary issuance maps `Read → ReadOnly` and both `Write`/`ReadWrite → ReadWrite`; persisted-read issuance explicitly uses `ReadOnly`; descendants preserve access and no RFI Log grant is upgraded;
- bounded no-follow retained-storage enumeration aggregates both artifact parents, accepts only four exact S5 prefixes, and computes checked logical charge as default length plus canonical auxiliary-record bytes;
- `FileSecuritySnapshot` requires one hard link and round-trips modeled Unix metadata plus Windows owner/group/complete DACL+SACL/CAP/label/EAs/attributes/ADS. Windows parses one unnamed `BACKUP_DATA` header and exact-skips its 64-bit payload with `BackupSeek`; default bytes never enter the 1-MiB auxiliary budget, EA is validated separately, and only named ADS are compared to `FileStreamInformation`;
- S5-only `create_new_secured` atomically supplies the final descriptor. DACL-excluded principals never receive inherited access. Stable injected metadata drift makes the primitive verifier return mismatch/error; lifecycle `RecoveryRequired` persistence is deliberately deferred to Step 9 RED and Step 11 implementation;
- `create_new_secured_identified` has executable branches for identified create and injected post-create identity failure; the latter returns `CreatedIdentityUnknown` while retaining the live secured handle. It can never return an ordinary no-effect I/O error after successful O_EXCL create;
- `create_artifact_parent_no_clobber` returns exact `Created(identity)`, `AlreadyExists`, or injected `CreatedIdentityUnknown`; no later name lookup upgrades `AlreadyExists`/unknown to creator ownership;
- `s5_preclaim_returns_nonforgeable_exact_authority`: claim requires and consumes `S5PermitBundle` plus `ValidatedGrantRootRoster`; omitting either does not compile at the API seam. Narrow plan/binding/digest/caller/root projections drive `begin_s5`; token lifetime tests prove permits/roots remain owned until terminal drop. Contention leaves plan `Available`;
- Windows scoped privilege tests prove descriptor preflight requests `SeSecurityPrivilege` plus only conditionally needed `SeRestorePrivilege`/`SeRelabelPrivilege`; only the worker thread is impersonated, all prior states restore on success/error/panic, and missing privilege/wrong identity fails closed;
- the Windows production `PlatformDirectory::sync_all` source contains the existing injector followed by `FlushFileBuffers` on a writable opened directory handle and propagates `last_os_error()`; a read-only handle is rejected before the call.

Do not add a general source-name publication rename or an identity-by-name delete helper.

- [ ] **Step 2: Observe capability/Windows RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::retained_publication_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::directory_access_and_retained_quota_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::file_security_snapshot_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::secured_create_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::backup_stream_skip_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml plan_store::tests::s5_preclaim_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml journal::tests::interprocess_mutation_lock_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::windows_directory_sync_uses_flush_file_buffers -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-run --target x86_64-pc-windows-msvc
```

Expected: FAIL because retain/sync/access/quota/security/secured-create/S5-claim/OS-lock APIs, Task 5 `AnchoredPath::create_new`, Windows authorization feature, and real Windows flush are absent.

- [ ] **Step 3: Implement only capability/security primitives, preclaim APIs, OS lock, manifest feature, and real Windows flush**

Expose existing `file_identity`. Keep compatibility opening. Implement the Step 1 RED-covered `AnchoredPath::create_new` now, plus `create_new_secured_identified`, `create_artifact_parent_no_clobber`, `sync_parent`, no-clobber retention, and parent-security methods. Both create helpers linearize no-clobber creation before identity acquisition and return typed unknown-effect outcomes if identity capture fails after creation; no plain `io::Error` erases that effect. Private parent handles remain hidden. Implement bounded no-follow usage enumeration.

Implement the security snapshots and non-clone wrapper. Atomic descriptor tests use a restricted-token process excluded by the DACL. Same-owner primitive tests inject stable ADS, EA, attribute, and hard-link drift and assert verifier mismatch only. They do not assert journal state; Step 9 introduces the lifecycle RED that requires persisted `RecoveryRequired`. Document the transient/ABA threat-model boundary and test default-stream/delete-sensitive sharing separately.

For auxiliary capture, inventory named streams, exact-skip one unnamed `BACKUP_DATA`, validate EA independently, and compare only named ADS. Define auxiliary logical charge as canonical record bytes including names/tags/lengths/values. Retained scans measure default plus auxiliary charge and reject overflow/inaccessibility. Test >1-MiB default data with small EA/ADS and exact quota boundaries. Backup lives in canonical parent; private output uses protected owner-only creation.

Quota RED cases all match the Step 2/4 Cargo filter: `directory_access_and_retained_quota_scan_counts_auxiliary_records`, `directory_access_and_retained_quota_apply_reservation_charges_three_auxiliary_copies`, `directory_access_and_retained_quota_undo_reservation_charges_retained_applied_auxiliary`, `directory_access_and_retained_quota_exact_limit_allowed_and_one_byte_over_rejected`, and `directory_access_and_retained_quota_auxiliary_or_arithmetic_overflow_fails_closed`. Step 3 implements only pure checked reservation arithmetic plus census; Step 11/15 consume it. Fixtures vary auxiliary charge at constant ordinary length.

In `plan_store.rs`, S5 claim consumes an already-bound `S5PermitBundle` and opaque `ValidatedGrantRootRoster`, revalidates caller/op/binding/digest/TTL and requires the roster records/identities/accesses to equal the claimed plan’s complete deduplicated grant set before storing both in non-Clone/non-Serialize `ClaimedS5Apply`. Implement only narrow immutable projections required by `journal.rs`/executor. Generic claim remains legacy-only and cannot mint S5 authority.

In `src-tauri/Cargo.toml`, add `Win32_Security_Authorization` to the existing `windows-sys` feature list required by descriptor/token privilege code. Do not add a crate or change package-manager artifacts.

On Windows, open writable ambient and descendant directory handles with `GENERIC_WRITE` in addition to the current rights; read-only grants retain the current read-only rights. After rejecting `ReadOnly`, keep `injected_directory_sync_failure()?` and then call:

```rust
if unsafe { FlushFileBuffers(self.0.as_raw_handle() as HANDLE) } == 0 {
    Err(io::Error::last_os_error())
} else {
    Ok(())
}
```

Use the already-open writable directory handle (`FILE_FLAG_BACKUP_SEMANTICS`). Add source/behavior tests for access mapping and require Task 8 to prove on real Windows that the write-grant handle opens and flushes while the read grant cannot. Do not claim runtime/power-loss durability from source inspection or cross-compilation.

- [ ] **Step 4: Run capability/Windows source-boundary GREEN**

```bash
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::retained_publication_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::directory_access_and_retained_quota_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::file_security_snapshot_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::secured_create_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::backup_stream_skip_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml plan_store::tests::s5_preclaim_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml journal::tests::interprocess_mutation_lock_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::windows_directory_sync_uses_flush_file_buffers -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-run --target x86_64-pc-windows-msvc
cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
```

Expected: unit tests PASS. Record an unavailable Windows target/SDK exactly rather than calling it a pass; the literal compile gate remains required on the certification host.

- [ ] **Step 5: Write journal-model, warning-mapping, and compatibility RED tests**

In `journal.rs` and `fs_ops.rs` crate unit tests, prove:

- pre-S5 JSON deserializes/reserializes with `s5Authority`/`pendingPublication`/`pendingArtifactParent`/`createdArtifactParent`/`pendingRetainedArtifact`/`recoveryNotice`/`followUpPhase` absent and `retainedEvidence`/`grantRootAccesses` empty;
- S5 validation requires one access record per deduplicated legacy root identity, equals the strongest bound grant, maps a distinct source-only root read-only and target/shared parent read-write, and rejects absent/duplicate/conflicting/extra records; non-S5 old journals retain legacy compatibility;
- old JSON defaults `revision=0`; initial validated token plus expected zero uses durable O_EXCL+parent sync. Existing CAS requires expected==token==disk revision and immutable provenance, checked-increments disk and token exactly once only on durable success; stale/overflow/provenance mismatch writes nothing, while possible post-replacement failure is outcome-unknown, leaves token prior, and is never retried. Legacy write/recovery cannot rewrite S5;
- `ArtifactParentReady`, `PrivateOutputReady`, `BackupReady`, and `PublicationActive` allow only their exact pending/completed evidence combinations; every `Applying` S5 record has one representable phase;
- private-output pending state is exactly `intent`, `created(identity)`, or `created_identity_unknown`. Typed create-success with identity is known partial before bytes; typed identity failure and restart-present bare intent are outcome-unknown, never no-effect. Restart absence is no-effect;
- backup uses the same pending state machine: intent is durable before O_EXCL, identified create is CAS-recorded before descriptor/auxiliary/copy/sync work, identity failure is outcome-unknown, and final evidence+pending clear is one CAS. Failures at every boundary preserve exact classification across restart;
- shared artifact-parent creation returns typed `Created`, `AlreadyExists`, or `CreatedIdentityUnknown`. Live `Created(identity)` alone may set `createdArtifactParent`, which the next completion/terminal CAS persists; sync failure is known partial. `AlreadyExists` carries no ownership. Identity-unknown or restart-present intent is outcome-unknown, and recovery/pathname observation is forbidden from setting creator evidence;
- a valid pending publication contains exact paths/identities/hashes, checked expected-canonical default+auxiliary=logical charge, destination auxiliary charge, pre-effect predicates, optional identities, and one representable phase;
- invalid hashes, duplicate/conflicting paths, retained outside the canonical parent, paths outside persisted roots, overlong locators, impossible phase/identity combinations, or unmatched apply/undo `RestoreBackup` are rejected;
- successful evidence records backup/private-output/displaced-original/displaced-applied roles and stays non-executable; result projection contains only ordered role/localized bounded locator pairs;
- the typed creation/evidence classifier governs every S5 failure and cancellation: proven operation-owned effects become `FailedPartial`/`CancelledPartial`; bare absent intent is no-effect; created-identity-unknown, restart-present intent, or canonical uncertainty becomes `WarningOutcomeUnknown`. Every non-clean result preserves bounded journal-owned locators and terminal handlers CAS exactly once;
- `failed_partial_preserves_localized_error_and_retained_locators`: requires a nonempty bounded locator vector, persists `RecoveryNotice` using the original localized failure key and first locator, projects all locators, increments revision exactly once, rejects stale CAS, and cannot report `outcome_unknown` or no-effect;
- `validated_s5_record_is_owned_opaque_and_required_by_executor_and_cas`: apply token consumes claim/permits/roster; targeted owner-checked undo consumes `PendingS5Undo` plus replay roster; recovery cores never leave `S5RecoverySession`; `next_root_accesses` exposes only immutable records, `FilesystemAuthority` mints the opaque replay roster, and `recover_next` consumes it before a valid-core effect and holds it through CAS. `None` is accepted only for non-mutating invalid/legacy entries. Narrow projections cover journal ID, approved/completed steps, plan/binding and roots. Raw read rejects S5; legacy write rejects candidate or existing S5, including read/clear-all/write downgrade. Every publication evidence and aggregate terminal variant has exact predecessor/delta and lease-kind tests; apply cannot finalize undo/recovery, undo cannot finalize apply/recovery, and recovery variants never escape the session. Terminal variants clear all pending/phase fields and append step/evidence atomically in one CAS;
- `validated_s5_record_legacy_read_clear_all_write_is_rejected`: legacy read returns an S5-routing error, and a discriminator-cleared candidate with the same ID cannot replace the existing S5 target because write holds the OS lock and inspects the target;
- `pending_s5_scan_is_bounded_and_wire_exact`: exact duplicate-free wire `opId`/`journalId`, filename parity, journal/notice byte bounds, 4096 root entries, 256 pending entries, 64 notices, and 256 responses are exact-at-limit/one-over tested. Scan preflights the complete union before exposing any result; 63+2 fails with no mutation. Existing notices must be no-follow regular/non-reparse owner-only, ≤4096 bytes, and duplicate-free;
- `invalid_s5_record_notice_is_create_new_idempotent_and_non_forgeable`: recovery session internally owns the envelope; `recover_next` reopens exact source and verifies token identity/hash before notice derivation. No envelope/iterator escapes; collision, swap, 65th notice, malformed/oversized sidecar fail closed;
- cloud metadata excludes every transaction/evidence/recovery/path/hash field;
- `local_history_excludes_invalid_s5_sidecars_before_limit`: exact `.invalid-s5-<64-lowercase-hex>.json` names are excluded by `cloud.rs` before requested limiting; near-miss names are invalid journal-root entries rather than history rows. Mixed fixtures put sidecars, malformed objects, and valid journals ahead lexically; only validated journals consume the limit and become rows. Exactly 4096 root entries remain deterministic; entry 4097 returns empty with no partial rows;
- `zero_step_follow_up_apply_is_revise_only`: direct native apply returns `error.rfi.followUp.noRowsSelected` before permits/claim/journal/quota/filesystem access and leaves the plan byte-identical `Available`;
- `warning_outcome_unknown_preserves_localized_recovery_warning`: the branch preserves warning/effect state, calls S5 CAS with current revision, increments exactly once, and rejects stale state. Store-wide tests prove legacy `write` rejects every raw S5 discriminator, not only this branch;

- [ ] **Step 6: Observe journal/result RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml journal::tests::retained_publication_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml journal::tests::validated_s5_record_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml journal::tests::pending_s5_scan_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml journal::tests::invalid_s5_record_notice_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml fs_ops::tests::warning_outcome_unknown_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml fs_ops::tests::failed_partial_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cloud::tests::local_history_excludes_invalid_s5_sidecars_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native zero_step_follow_up_apply_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test cloud_metadata -- --nocapture
```

Expected: FAIL because the additive fields, validation, evidence projection, exact StepFailure variant, and mapping do not exist.

- [ ] **Step 7: Implement only the additive journal and result contracts**

In `journal.rs`, change the test module gate from `#[cfg(all(test, windows))]` to `#[cfg(test)]`; gate only existing Windows-specific test functions/helpers with `#[cfg(windows)]`, so every cross-platform journal filter executes at least one test on macOS/Linux. Implement permit-owning tokens/sessions, targeted undo admission, closed evidence/terminal transitions, and bounded recovery. Legacy `read` rejects S5; legacy `write` acquires the same OS lock and checks candidate plus existing target under that lock through replacement, including clear-discriminator downgrade. In `cloud.rs`, cap the mutation-free entry scan at `MAX_JOURNAL_ROOT_ENTRIES_SCANNED` (4097 returns empty), sort deterministically, exclude the exact sidecar lexical class before parsing, delete the arbitrary-object fallback, deserialize `JournalRecord`, require `validate_for_history_projection`, project only existing `HistoryRow` fields, and count the requested limit only after a valid row. In `fs_ops.rs`, reject zero-step apply before admission. Terminal handlers alone perform one CAS.

- [ ] **Step 8: Run journal/result/cloud GREEN**

```bash
cargo test --manifest-path src-tauri/Cargo.toml journal::tests::retained_publication_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml journal::tests::validated_s5_record_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml journal::tests::pending_s5_scan_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml journal::tests::invalid_s5_record_notice_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml fs_ops::tests::warning_outcome_unknown_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml fs_ops::tests::failed_partial_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cloud::tests::local_history_excludes_invalid_s5_sidecars_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native zero_step_follow_up_apply_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test cloud_metadata -- --nocapture
```

Expected: PASS.

- [ ] **Step 9: Write the complete apply/crash/race/recovery RED matrix before any production helper**

Create crate unit tests in `leaf_transaction.rs`, `fs_ops.rs`, and `journal.rs` **before** implementing `execute_retained_publication` or `reconcile_retained_publication`. Private hook-dependent journal inspection lives only in those crate unit tests; integration tests never name a private `cfg(test)` symbol. If needed, use a private `#[cfg(test)] execute_retained_publication_with_hook` callback accepted only by unit tests.

The RED matrix must programmatically cover:

- exact happy apply: one selected cell changes, RFI Log remains byte/metadata exact, backup equals original, package verifier runs, changed count is exact, and journal order is optional `Mkdir`, `Copy`, `RestoreBackup`;
- guarded writer `OoxmlErrorKind::LimitExceeded` maps exactly to `error.rfi.followUp.limitExceeded` before private-output bytes, backup, displacement, or canonical effect; `Invalid` remains workbook-invalid and no branch parses error text;
- merge-limit after an identified empty private output, output-size failure after identified partial bytes, pre-displacement drift, backup failure, and parent-sync after typed identified `Created` all use `FailedPartial`: canonical remains original, original localized key and retained locators are preserved, and terminal CAS writes `effectState=partial`. The same failures before effect are no-effect; any identity/provenance ambiguity is explicitly `WarningOutcomeUnknown`;
- `follow_up_retained_apply_revalidates_open_workbooks_at_every_boundary`: calls `revalidate_open_workbooks` before private output, backup, displacement, canonical create, and terminal classification; injected drift at each seam prevents the next effect;
- lexical backup/private-output authorization precedes journaled directory creation; anchoring waits for parent existence and pre-existing directories carry no ownership claim;
- artifact-parent typed matrix: `Created(identity)` plus sync failure is known partial; `AlreadyExists` is no ownership effect; `CreatedIdentityUnknown` and restart-present intent are outcome-unknown; restart absence is no-effect. No pathname-only recovery observation becomes creator provenance, and terminal CAS failure claims no terminal result;
- same-size bytes, mtime, identity, workbook predicate, and complete security/stream drift before backup, before displacement, and after displacement on the retained original; any drift prevents canonical recreation;
- `prepared` evidence durably contains original/applied hashes, expected original and backup identities/hashes, exact private-output identity/hash, and unique retained locator while canonical is untouched;
- final cancellation before displacement: identified operation-owned directory/private output/backup yields `CancelledPartial`; no-effect intent/absence yields clean cancellation; created-identity-unknown or restart-present intent yields outcome-unknown. No clean cancellation checkpoint exists after displacement;
- destination-no-clobber displacement moves racer bytes without overwrite, then wrong retained identity/hash persists localized recovery-required and is never restored, published, or deleted even when canonical is absent;
- exact retained original identity/hash is journaled before create-new;
- retained-storage preflight measures each artifact's default stream plus canonical auxiliary-record charge. Apply reserves three files and checked `2 * original_default + MAX_PACKAGE_BYTES + 3 * original_auxiliary_charge`; undo reserves one file and `current_applied_default + current_applied_auxiliary_charge`. Reject overflow or projections above 64/8 GiB; exactly-at-limit remains allowed;
- a second OS process cannot pass admission/quota or write/recover while the first holds the permit; rejected contention leaves the same plan `Available` with unchanged digest/TTL, and retry after release claims it once. Stale CAS cannot replace newer evidence;
- complete original/parent security is late-revalidated; descriptors are atomic. Lifecycle hooks inject stable auxiliary drift and assert persisted `RecoveryRequired`; sequential metadata observation is never called atomic;
- private output is born owner-only with unnamed-stream `ShareAccess=0`; no broader inherited principal gains access before bytes, while same-owner metadata access follows the stated threat model;
- private-output and backup path/locator/`intent` are journaled before create; `create_new_secured_identified` returns identified or created-identity-unknown. Injected identity failure after either O_EXCL is outcome-unknown, never no-effect. Identified state is CAS-recorded before subsequent work; final evidence+pending clear is atomic; crashes preserve without cleanup;
- a racer canonical created before create-new wins untouched; original/backup/private output remain retained;
- crash after atomic secure-create but before identity CAS leaves an unknown restricted canonical that recovery preserves;
- descriptor verification plus identity produce `canonical_secured`; EA/ADS verification produces `canonical_auxiliary_ready`; only then may unnamed bytes begin;
- a DACL-excluded restricted-token process obtains no access; stable same-owner ADS/EA/attribute/hard-link mutation is detected, while transient/ABA authorized mutation is explicitly outside the adversarial guarantee;
- racer rename/replacement after create leaves the foreign canonical untouched and fails final name-to-created-identity validation;
- exact applied canonical identity/hash plus exact retained original/source/backup completes journal classification without deleting any artifact;
- same hash/wrong identity for canonical, retained, backup, or private output; missing/wrong source or backup; access errors misreported as absence; parent-flush uncertainty; and repeated recovery calls all preserve bytes and remain idempotent;
- `WarningOutcomeUnknown` carries the exact warning/effect state and its terminal transition uses revision CAS; stale handler state cannot replace a newer record;
- recovery binds both permits into `S5RecoverySession`; private scan and `recover_next` retain them through every CAS/notice. Exact-bound/one-over tests prove root, core/envelope, notice and response caps; no core/envelope/iterator escapes and contention touches nothing;
- crashes after completed `Mkdir`, finalized private output, and completed backup but before the next pending record are classified from `followUpPhase`, preserve artifacts, and cannot fall through the legacy scanner;
- actual `recovery/list` calls serialized filesystem identity/hash reconciliation instead of pathname-only `pending_disposition`.

Add public/native integration tests only for observable happy apply, create collision, retained artifact presence, result warning/locator, restart/recovery-list behavior, and RFI Log regression. They must not pause at or inspect private hook points.

- [ ] **Step 10: Observe the complete safety RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml leaf_transaction::tests::retained_publication_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml fs_ops::tests::follow_up_retained_apply_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml fs_ops::tests::rfi_follow_up_recovery_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml native::tests::recovery_list_reconciles_retained_follow_up_publications -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native retained_publication_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test filesystem_authority follow_up_retained_publication_ -- --nocapture
```

Expected: FAIL because the operation executor, complete retained-publication helper, exact crash classification, permanent evidence, and native recovery wiring do not exist. Do not write production happy-path or recovery branches before observing these failures.

- [ ] **Step 11: Implement the minimal complete retain/create-new/open-handle lifecycle**

Route only `rfi.follow_up_sync` to `execute_follow_up_workbook_update`; leave `rfi.log_add` behavior-compatible. Implement the RED-covered path in this exact order:

1. reject zero-step before mutation; inspect, acquire process permit then OS permit, bind them into `S5PermitBundle`, mint `ValidatedGrantRootRoster`, and consume both through `claim_s5_for_apply_bound_under_permit`. `begin_s5` consumes the claim into a permit-owning `ValidatedS5Record`; initial CAS creates revision 1. No raw record/vector/generic claim can enter executor;
2. decode/anchor/revalidate. Call `canonical.capture_parent_security()` and later `canonical.verify_parent_security(&snapshot)` through the pinned private parent handle; never expose `PlatformDirectory`. Capture original security through its identity-matched data/security handles. Aggregate census with `RetainedStorageReservation::for_apply`, reject quota before artifact effect, then journal/create only the private-output parent under CAS;
3. create the shared parent with `create_artifact_parent_no_clobber` and classify its typed outcome exactly. Create private output with `create_new_secured_identified`; persist `created(identity)` before bytes or route `CreatedIdentityUnknown` to outcome-unknown. Then replay/verify EA/ADS and write/verify OOXML. Map guarded limit exactly without message parsing; identified empty/partial output remains known-partial evidence;
4. recapture source, write backup pending intent, create through `create_new_secured_identified`, CAS-record identified state before verification/copy, and route identity unknown exactly. Verify creation security, replay/verify EA/ADS, copy/sync bytes, then finalize evidence+pending clear in one CAS. Revalidate before displacement;
5. call `execute_retained_publication` with private output, `PublicationCancellation::Apply(token)`, original creation descriptor, and complete snapshot. The helper's sole checkpoint precedes displacement;
6. retain whatever occupies canonical destination-no-clobber, sync the pinned parent, open the retained leaf, and require the exact expected identity/hash. Wrong/unrecorded retained state becomes `RecoveryRequired`; never restore or delete it;
7. `RecordRetainedPublicationEvidence` fills exact retained identity/hashes/security, then call canonical `create_new_secured` with `ShareAccess=0`; `AlreadyExists` preserves racer;
8. verify descriptor, apply `RecordCanonicalSecured`, and flush parent. Replay/verify EA/ADS then apply `RecordCanonicalAuxiliaryReady`;
9. apply `MarkCanonicalBytesStarted` before the first unnamed byte, stream/sync, and verify through `SecuredCreatedFile`;
10. apply `MarkCanonicalVerifying`, then require destination-handle identity/byte/security hash, exact backup/source/retained evidence, and canonical no-follow name resolution to the recorded created identity; a foreign canonical remains untouched;
11. call one `FinalizeApplySuccess` transition carrying exact completed `RestoreBackup` and final evidence. It atomically validates/deduplicates roles, appends step/evidence, sets terminal state/result, clears every pending field and `followUpPhase`, then one CAS returns changed count. Do not unlink evidence;
12. implement `reconcile_retained_publication` to recognize only exact durable no-effect/applied states from design §11, preserve unknown/unrecorded identities, never restore retained to canonical, never publish during recovery, never delete evidence, and remain idempotent;
13. implement bounded recovery only through permit-owning `S5RecoverySession::recover_next`; cores/envelopes never escape. Intent absence is no-effect; restart presence without identity is outcome-unknown; identified exact pending artifacts are known partial. Invalid envelopes use capped notice creation and terminal recovery uses aggregate `FinalizeRecovery`;
14. use one typed classifier for cancellation and every failure: known operation ownership → partial, bare absent intent → no-effect, identity/provenance ambiguity or post-displacement uncertainty → outcome-unknown. Both non-clean paths project only journal-owned validated locators.

Export `leaf_transaction` crate-privately. Production code contains no hook branch. No S5 code may call a general source-name rename, `replace_with`, `move_no_clobber_to` for publication, overwrite/retry/rollback helpers, `remove_file[_durable]` for transaction artifacts, or any source-name publication path.

- [ ] **Step 12: Run the complete apply/recovery GREEN**

```bash
cargo test --manifest-path src-tauri/Cargo.toml leaf_transaction::tests::retained_publication_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml fs_ops::tests::follow_up_retained_apply_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml fs_ops::tests::rfi_follow_up_recovery_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml native::tests::recovery_list_reconciles_retained_follow_up_publications -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native retained_publication_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test filesystem_authority follow_up_retained_publication_ -- --nocapture
```

Expected: PASS, including permanent evidence, repeated recovery, old-record compatibility, and exact localized outcome-unknown mapping.

- [ ] **Step 13: Write undo open-backup-handle and permanent-evidence RED tests**

After successful apply, prove undo:

- `follow_up_retained_restore_backup_uses_targeted_validated_admission`: raw `read` rejects S5; owner-checked `inspect_s5_for_undo` holds the permit bundle, recorded-root replay yields an opaque roster, and `admit_s5_undo` is the only token minting path; wrong owner/ID/roster is pre-effect;

- native S5 undo rejects `object.contains_key("destination")` (including null) and direct S5 undo rejects `destination.is_some()` before selection resolution, pending undo, cloud mutation, or filesystem effect; the journal remains committed and canonical unchanged;
- requires current canonical applied byte/security hash and identity plus exact backup byte/security hash and identity;
- while holding both permits, rescans both artifact parents and reserves one retained-applied file with current default-stream plus auxiliary logical charge; quota failure is pre-effect and exactly-at-limit remains allowed;
- reopens each deduplicated root with recorded strongest access: distinct source-only root read-only, target/shared parent read-write; the original source grant is never changed or used for mutation;
- opens the backup once, validates identity/byte/complete security+stream digest, and keeps the handle plus recaptured snapshot through publication;
- persists pending undo `RetainedPublication` with applied expected identity/byte/security hash, exact backup source identity/original byte/security hash, fresh `.docflow-s5-retained-applied-<journal-id>-<nonce>.<ext>` locator, and no invented staged leaf;
- is explicitly non-cancellable after admission, registers no cancellation request, passes `PublicationCancellation::NotCancellable`, and is unaffected by plan/apply `engine/cancel` calls;
- destination-no-clobber retains current canonical, then requires the retained leaf to be the exact applied identity/byte/security hash;
- treats a displaced later Excel edit/racer as recovery-required, leaves canonical possibly absent, and never restores or deletes that retained edit;
- creates canonical atomically secured with the backup-derived creation descriptor and `ShareAccess=0`, CAS-records `canonical_secured`, replays/verifies auxiliary streams, CAS-records `canonical_auxiliary_ready`, then writes unnamed bytes;
- preserves a racer canonical before create and a racer replacement after create without overwrite/removal;
- restores exact original SHA-256 on success while permanently keeping backup, retained original/private output, and retained applied leaf;
- skips reverse-`Copy` backup removal and operation-created-directory cleanup for S5 only;
- reconciles every undo phase idempotently without cleanup or retained-leaf restoration.

Keep private phase hooks in `undo.rs`/`leaf_transaction.rs` crate unit tests. Integration tests assert only observable undo, retained evidence, racer preservation, and restart behavior.

- [ ] **Step 14: Observe undo RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml undo::tests::follow_up_retained_restore_backup_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml leaf_transaction::tests::retained_undo_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test filesystem_authority follow_up_retained_undo_ -- --nocapture
```

Expected: FAIL because Follow Up undo still uses the legacy restore/backup-cleanup path.

- [ ] **Step 15: Implement undo through the open-handle helper without cleanup**

Route only completed Follow Up `RestoreBackup` through the helper. Reject destination, acquire/bind both permits, call owner-checked `inspect_s5_for_undo`, replay `recorded_root_accesses` into an opaque roster, then consume both through `admit_s5_undo`; raw `read` cannot service S5 undo. The resulting token owns permits/roots through quota census/reservation, same-parent and open-handle backup validation, no-clobber secure recreation, write/verify, and aggregate `FinalizeUndoSuccess` CAS. Every collision preserves racer bytes and displaced-applied evidence is retained. Undo remains non-cancellable and never accepts a supplied destination.

- [ ] **Step 16: Run undo GREEN and regressions**

```bash
cargo test --manifest-path src-tauri/Cargo.toml undo::tests::follow_up_retained_restore_backup_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml undo::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml leaf_transaction::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test filesystem_authority follow_up_retained_undo_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_log_native -- --nocapture
```

Expected: PASS; S5 retains all evidence and non-Follow-Up undo remains unchanged.

- [ ] **Step 17: Write the localized recovery/retained-locator UI RED test**

In `App.test.tsx`, return a recovery entry with `warningKey: "error.rfi.followUp.recoveryRequired"` and a bounded locator. Assert the localized message and locator display, and assert no absolute journal path is rendered. For a successful S5 result, return `meta.retainedEvidence` as ordered `{ role, locator }` projections and assert `ResultView` localizes each role and displays every bounded locator without paths/hashes/identities, undo authority, or cleanup action. Add quota-preflight UI/error coverage for `error.rfi.followUp.retainedStorageFull`.

- [ ] **Step 18: Observe recovery UI RED**

```bash
npx vitest run --config frontend/vite.config.ts frontend/src/App.test.tsx -t "shows localized Follow Up recovery locator"
```

Expected: FAIL because `RecoveryEntry`, successful `OperationResult.meta.retainedEvidence`, and the current views do not expose/render these additive projections.

- [ ] **Step 19: Implement the additive recovery UI fields**

Add optional `warningKey?: string` and `locator?: string` to `RecoveryEntry` in `types.ts`, plus a typed read-only `RetainedEvidenceProjection { role: string; locator: string }` decoder for `OperationResult.meta.retainedEvidence`. In the existing recovery notice, render `s.t(entry.warningKey)` only when supplied and render the bounded locator. In `ResultView`, render successful retained projections with localized `rfi.followUp.retainedRole.*` labels. Reject malformed/over-limit projections, display no absolute path/hash/identity, and add no automatic cleanup controls; legacy entries/results without fields keep existing rendering.

- [ ] **Step 20: Run recovery UI GREEN**

```bash
npx vitest run --config frontend/vite.config.ts frontend/src/App.test.tsx -t "shows localized Follow Up recovery locator"
npx vitest run --config frontend/vite.config.ts frontend/src/App.test.tsx
```

Expected: PASS.

- [ ] **Step 21: Run the full apply/recovery/undo boundary**

```bash
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::retained_publication_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::windows_directory_sync_uses_flush_file_buffers -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml journal::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml leaf_transaction::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml undo::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test filesystem_authority -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test cloud_metadata -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_log_native -- --nocapture
npx vitest run --config frontend/vite.config.ts frontend/src/App.test.tsx
```

- [ ] **Step 22: Review and commit**

```bash
git diff --check
git add src-tauri/Cargo.toml src-tauri/src/lib.rs src-tauri/src/cap_fs.rs src-tauri/src/plan_store.rs src-tauri/src/paths.rs src-tauri/src/leaf_transaction.rs src-tauri/src/journal.rs src-tauri/src/cloud.rs src-tauri/src/undo.rs src-tauri/src/fs_ops.rs src-tauri/src/native.rs src-tauri/src/rfi_follow_up.rs src-tauri/tests/rfi_follow_up_native.rs src-tauri/tests/filesystem_authority.rs frontend/src/types.ts frontend/src/App.tsx frontend/src/App.test.tsx
git commit -m "[verified] feat: retain Follow Up publication evidence"
```

---

## Task 6: Freeze compatibility and closed-contract regressions

**Files:**
- Modify: `src-tauri/tests/rfi_follow_up_native.rs`
- Modify: `src-tauri/tests/fixtures/rfi_follow_up_compatibility_oracle.json`
- Modify: `src-tauri/tests/rfi_log_native.rs`

**Interfaces:**
- Consumes the implemented public plan/editor projection and existing Slice 6 oracle.
- Produces no production behavior, wire type, backend method, or executable journal kind.

- [ ] **Step 1: Freeze the exact compatibility oracle**

Finalize the JSON cases created in Task 1 with fixed workbook rows and expected public editor projections for:

```text
Completed; Ongoing; already synchronized; blank source; unknown source;
source formula; source merged cell; same-status source duplicate;
conflicting source duplicate; missing source; blank target RFI; invalid target RFI;
target RFI formula; target RFI merged cell; duplicate target RFI; blank target status;
unknown nonblank target status; missing target cell; target status formula; target status merged cell;
row order; deterministic row IDs; selectable/enabled defaults; zero-step warning.
```

Add `preview_matches_the_frozen_follow_up_compatibility_oracle`. Strip only volatile plan/backup UUIDs and fingerprint values; compare every stable operation ID, column, row, reason, full localization-key disposition, proposed value, selection flag, update coordinate, and warning exactly. This is a post-behavior compatibility freeze, not a substitute for the RED→GREEN behavior tests in Tasks 1–5.

- [ ] **Step 2: Run the new oracle and frozen legacy contracts**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native preview_matches_the_frozen_follow_up_compatibility_oracle -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_log_native native_preview_revise_and_table_payload_match_the_frozen_compatibility_oracle -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml registry::tests::canonical_registry_matches_protocol_one_snapshot -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml backend::tests::rust_and_typescript_frontend_method_rosters_match_exactly -- --exact --nocapture
```

Every difference is inspected against the accepted spec. The new oracle is never edited to bless unsafe behavior, and the existing Slice 6 oracle is never rewritten to hide a regression.

- [ ] **Step 3: Run focused complete Follow Up and legacy suites**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_log_native -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test ooxml_fidelity -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test filesystem_authority -- --nocapture
npx vitest run --config frontend/vite.config.ts frontend/src/App.test.tsx -t "RFI Follow Up"
```

- [ ] **Step 4: Review the test-only slice and commit it**

```bash
git diff --check
git add src-tauri/tests/rfi_follow_up_native.rs src-tauri/tests/fixtures/rfi_follow_up_compatibility_oracle.json src-tauri/tests/rfi_log_native.rs
git commit -m "[verified] test: freeze Follow Up sync compatibility"
```

---

## Task 7: Fresh full gates and independent exact-candidate review

**Files:**
- No planned source edit.
- Any reviewer/gate finding starts a new focused RED → GREEN correction in the owning file, followed by every affected/full gate and a new logical commit.

**Candidate base:** `7c33e4beaa7af1ba595ef52eed5e46bac8fd2d14`

- [ ] **Step 1: Confirm complete intended roster and clean ownership**

```bash
git status --short --branch
git diff --name-only 7c33e4beaa7af1ba595ef52eed5e46bac8fd2d14...HEAD
git diff --check 7c33e4beaa7af1ba595ef52eed5e46bac8fd2d14...HEAD
```

Require a clean worktree and only the design/plan plus Task 1–6 paths. Generated `frontend/dist/`, `src-tauri/target/`, local fixture workbooks, Office outputs, credentials, and production QC files must be absent from the candidate.

- [ ] **Step 2: Run focused behavioral gates fresh on the final HEAD**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_log_native -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test ooxml_fidelity -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test filesystem_authority -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test cloud_metadata -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml plan_store::tests::replace_bound_available_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::retained_publication_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::windows_directory_sync_uses_flush_file_buffers -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml journal::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml leaf_transaction::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml undo::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml registry::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml backend::tests -- --nocapture
npx vitest run --config frontend/vite.config.ts frontend/src/App.test.tsx frontend/src/lib/bridge.test.ts frontend/src/lib/frontend-authority.test.ts
```

- [ ] **Step 3: Run every literal repository gate separately**

```bash
node tools/version_manager.mjs check
node tools/package_audit.mjs source
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
npm run audit:licenses
git diff --check 7c33e4beaa7af1ba595ef52eed5e46bac8fd2d14...HEAD
git status --short --branch
```

Record each command's real exit status separately. A broader command does not substitute for a missing literal gate. Remove only generated ignored output, then require the worktree to remain clean.
An unavailable local Windows target/SDK is an environment blocker, not a pass and not a code failure; rerun the exact Windows check in Task 8 before source acceptance.

- [ ] **Step 4: Freeze and fingerprint the exact committed candidate**

```bash
BASE=7c33e4beaa7af1ba595ef52eed5e46bac8fd2d14
CANDIDATE=$(git rev-parse HEAD)
TREE=$(git rev-parse HEAD^{tree})
git diff --binary "$BASE"..."$CANDIDATE" > /tmp/rfi-follow-up-sync-candidate.patch
PATCH_SHA256=$(shasum -a 256 /tmp/rfi-follow-up-sync-candidate.patch | cut -d' ' -f1)
ROSTER_SHA256=$(git diff --name-only "$BASE"..."$CANDIDATE" | LC_ALL=C sort | shasum -a 256 | cut -d' ' -f1)
printf 'base=%s\ncandidate=%s\ntree=%s\npatch_sha256=%s\nroster_sha256=%s\n' \
  "$BASE" "$CANDIDATE" "$TREE" "$PATCH_SHA256" "$ROSTER_SHA256"
git status --porcelain=v1
```

The final line must be empty. Preserve the printed five-value identity with the gate ledger. Do not edit, amend, rebase, merge, or regenerate tracked bytes while that review is in force.

- [ ] **Step 5: Dispatch an independent read-only exact-candidate review**

Give the reviewer only:

- issue `#26` body and acceptance criteria;
- `AGENTS.md`, `src-tauri/AGENTS.md`, product/architecture/tasks/migration canon;
- accepted spec path;
- exact base, candidate commit, candidate tree, patch SHA-256, roster SHA-256;
- the complete binary patch or a detached clean worktree at the exact candidate commit;
- explicit protected behavior: `rfi.log_add`, generic protocol methods, OOXML package fidelity, journal/undo compatibility, cloud metadata-only history, EN/AZ/RU/TR;
- output contract: list P0/P1/P2 findings with file/line/evidence, or exact `PASS (P0/P1/P2-free)`.

Require the reviewer to run before and after inspection:

```bash
git rev-parse HEAD
git rev-parse HEAD^{tree}
git status --porcelain=v1
shasum -a 256 /tmp/rfi-follow-up-sync-candidate.patch
```

A missing, truncated, stale, identity-mismatched, or ambiguous report is not a pass.

- [ ] **Step 6: Remediate every valid finding with a new candidate cycle**

For each current spec-valid finding: reproduce with the smallest failing test, observe focused RED, make the minimal correction, observe focused GREEN, run affected suites, commit one logical correction, rerun Steps 2–4, and request a fresh independent review. Every edit invalidates all prior gate and review evidence.

- [ ] **Step 7: Record the final source/local result honestly**

Report exact changed paths, logical commits, RED/GREEN commands, full gate outputs, candidate identity, and reviewer verdict. State separately that Microsoft Office fixture certification remains external until Task 8 is executed. Do not claim Windows/Office runtime completion from local macOS or synthetic OOXML evidence.

---

## Task 8: External Microsoft Office fixture certification

**Files:**
- Keep working fixtures and Office outputs outside the repository.
- Add release evidence only through the repository's approved release-evidence process after the exact candidate passes; do not store confidential or production QC workbooks.

**Required environment:** clean supported Windows host, clean exact-candidate source checkout with the `x86_64-pc-windows-msvc` Rust target/SDK, exact installed candidate, Microsoft Excel, sanitized Office-authored fixtures, and explicit authorization for the test folder.

- [ ] **Step 1: Bind the installed candidate and environment**

Record candidate commit/tree/installer SHA-256, DocFlow version, Windows build, Excel product/version/bitness, fixture directory, and test operator/date. Verify the installed executable corresponds to the exact independently reviewed candidate.

```bash
cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::windows_directory_sync_uses_flush_file_buffers -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::windows_write_grant_directory_flushes_and_read_grant_cannot -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::file_security_snapshot_windows_complete_round_trip -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::secured_create_windows_descriptor_is_atomic_and_unnamed_stream_zero_share -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::secured_create_windows_restricted_token_gets_no_inherited_access -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml leaf_transaction::tests::retained_publication_authorized_auxiliary_drift_is_recovery_required -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml cap_fs::tests::backup_stream_skip_windows_large_default_small_auxiliary -- --exact --nocapture
cargo test --manifest-path src-tauri/Cargo.toml journal::tests::interprocess_mutation_lock_excludes_second_process -- --exact --nocapture
```

All commands must pass under the exact release account. Prove computed privileges, descriptor-at-create, complete security/EA/ADS capture, exact default-stream skipping, DACL-excluded-principal denial, unnamed-stream sharing, and authorized metadata-drift detection; otherwise S5 is blocked on that host. Also prove real ReadWrite directory flush while Read remains nonmutating.

- [ ] **Step 2: Prepare the sanitized fixture matrix**

Create Office-authored fixtures with narrow DACL/SACL/label/CAP/EA/ADS and >1-MiB default stream. A restricted-token process excluded by the final DACL must never gain inherited access. A separate authorized process mutates ADS/EA/attributes/hard links at controlled barriers; final recapture must reject drift without claiming those accesses are share-fenced.

- [ ] **Step 3: Certify inert preview and revise**

Run the exact installed operation. Verify exact classifications/row IDs/default enablement, manual checkboxes disabled, and both workbook hashes/sizes/mtimes/directory rosters unchanged after preview and after a revise toggle. Confirm no Excel process starts and no artifact directory is created.

- [ ] **Step 4: Certify bounded apply and package fidelity**

Enable a small known set and apply. Require:

- RFI Log exact hash/metadata unchanged;
- backup hash equals original Follow Up hash;
- backup and recreated canonical complete security/stream digests equal the original fixture, including SACL/CAP/label/EA/ADS, while timestamps remain normal new timestamps;
- displaced original identity/hash remains exact at its bounded retained locator after success;
- private output is atomically born owner-only, remains exact at its final recapture, and is never source-name-renamed into canonical;
- only approved target status cells changed semantically;
- target styles, formulas outside targets, validation, tables, comments, drawings/charts, external links, custom XML, and VBA bytes remain intact;
- Excel opens the result without repair, save, password, macro, or link-update prompts;
- the macro sentinel did not execute;
- no unknown nonblank/manual cell changed;
- a controlled broader-parent principal cannot read private output, backup, or empty/partial canonical at any point; an authorized observer can see empty/partial canonical only after exact original access controls are installed. Publication is not called atomic.

- [ ] **Step 5: Certify exact undo and later-edit protection**

Undo once and require the Follow Up SHA-256 and complete normalized security/stream digest to equal the original fixture exactly. Submit native undo once with a `destination` key (including null) and require fail-closed before selection resolution, pending state, cloud marking, or workbook effect; ordinary S5 undo remains non-cancellable after admission. Require the backup, retained original, private output, and retained applied leaf to remain identity/hash exact; S5 must not remove their directory. Apply again, edit/save the target in Excel, then require DocFlow undo to refuse, move the later edit only to the fresh retained-undo locator if displacement won, leave canonical possibly absent, and never restore/delete that mismatched retained edit.

- [ ] **Step 6: Certify negative, cancellation, and recovery cases**

Require protected/signed/ambiguous/over-limit/locked/drift, every missing computed privilege case, unreadable SACL/CAP/ADS, and over-bound metadata to fail before backup. Mutate ACL/SACL/xattr/EA/ADS without changing workbook bytes at each late-revalidation seam and require fail-closed before recreation. Launch two app processes: while A holds the permit, B apply must return in-progress with the exact plan still Available and retryable after A exits; undo/recovery also remain effect-free. Stale revision and stale `WarningOutcomeUnknown` injection must fail CAS. Exercise cancellation before backup, after verified backup/private output, and delivery after displacement; only the pre-displacement checkpoint may return cancellation. In the sanitized test folder, exercise retain/create-new windows: wrong retained identity, racer canonical before create, crash after create before identity journal, partial write, parent/file flush failure, and racer rename/replacement after create. Every racer/unknown leaf must remain untouched; backup/private output/retained original or applied bytes remain at recorded locators; `WarningOutcomeUnknown` must retain localized key+locator/effect state; repeated `recovery/list` must never blind-retry, restore, publish, or delete. Exercise the real Windows `FlushFileBuffers` success and controlled failure path. Power-loss durability remains uncertified unless the host can demonstrate the reviewed failure matrix. External Office evidence supplements but does not replace the automated identity/hash recovery matrix.

- [ ] **Step 7: Close evidence without overclaiming**

Save a sanitized matrix with candidate identity, fixture hashes, observed cells, backup/undo/retained identities and hashes, empty/partial visibility observation, real directory-flush evidence, Excel-open result, macro sentinel result, cancellation/recovery results, and all failures. A failing or unexecuted matrix remains an explicit external blocker; it does not invalidate source/local evidence but prevents Office and Windows power-loss certification claims.

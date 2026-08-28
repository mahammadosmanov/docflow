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
- Canonical RFI matching is whole-value NFC + trim + uppercase against `AZE-SUN-INT-QMS-RFI-[A-Z]{3}-[0-9]{4}` only.
- Map exact case-sensitive `Completed → Completed` and `Ongoing → In Progress` only.
- Keep blank, unknown, unmatched, missing-cell, formula, merged, duplicate-target, conflicting-source, and unknown-target rows manual-only with `enabled=false` and `selectable=false`.
- Discover exactly one `RFI NO.`/`STATUS` header pair within 32 rows × 64 columns; never infer a sheet or neighboring column.
- Enforce 10,000 source data rows per sheet, 10,000 Follow Up data rows, 500 preview rows/updates, and 20,000 Unicode scalar values per cell.
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

**Files:**
- Create: `src-tauri/src/rfi_follow_up.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/ooxml.rs`
- Modify: `src-tauri/src/rfi_log.rs`
- Create: `src-tauri/tests/fixtures/rfi_follow_up_compatibility_oracle.json`
- Test: `src-tauri/tests/rfi_follow_up_native.rs`
- Test: `src-tauri/tests/ooxml_fidelity.rs`

- [ ] **Step 1: Write only the namespace/hierarchy/safety RED tests**

Add these four focused tests before any classification test:

- `worksheet_facts_accept_only_canonical_strict_or_transitional_hierarchy`
- `worksheet_facts_reject_duplicate_coordinates_before_indexing`
- `worksheet_facts_reject_foreign_namespace_cell_shadowing`
- `worksheet_facts_capture_formula_merge_protection_signature_and_visibility`

Fixtures must independently prove:

- direct canonical `worksheet/sheetData/row/c` cells work under exactly the Transitional or Strict SpreadsheetML URI;
- zero/multiple canonical `sheetData` nodes, canonical `c` outside direct `row`, row/c coordinate mismatch, or a repeated canonical coordinate fail closed;
- an element whose local name is `c` and whose `r` parses as a coordinate but whose namespace is foreign is rejected rather than ignored as a shadow;
- merge ranges, formulas, workbook/sheet protection, signed-package relationships, and hidden/veryHidden state are all surfaced before a caller can classify rows;
- duplicate coordinates are represented as ordered XML input and rejected before any `BTreeMap` insertion. Do not write an impossible duplicate-`BTreeMap`-key or duplicate-JSON-object-field test.

- [ ] **Step 2: Run the foundation tests to verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test ooxml_fidelity worksheet_facts_ -- --nocapture
```

Expected: FAIL because the namespace-strict fact APIs/validation do not exist.

- [ ] **Step 3: Implement the minimal namespace-strict facts and handle reader**

In `ooxml.rs`:

```rust
pub fn read_workbook_from_reader<R: Read + Seek>(
    source: &mut R,
) -> Result<WorkbookData, OoxmlError>;
```

Keep `read_workbook(&Path)` as a thin `File::open` adapter. Resolve namespaces from XML declarations/prefixes and accept only:

```rust
const TRANSITIONAL_SPREADSHEETML_NS: &str =
    "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const STRICT_SPREADSHEETML_NS: &str =
    "http://purl.oclc.org/ooxml/spreadsheetml/main";
```

Build worksheet facts only from one canonical `worksheet > sheetData > row > c` chain. Reject repeated canonical coordinates and foreign-namespace cell shadows during ordered event traversal. Preserve existing bounded ZIP/member/XML protections and add reusable facts for formula, merge, protection, signature, and visibility checks. Expose the actual RFI Log projection without duplicating or changing `layout`:

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
- `compatibility_oracle_matches_exact_rows`

Freeze an oracle that includes exact `Completed`, exact `Ongoing`, already synchronized, blank/unknown source status, source formula/merge, same-status duplicate, conflicting duplicate, missing source, invalid/formula/merged/duplicate target RFI, physically present blank status, absent status cell, formula/merged target status, unknown nonblank target status, exact order, deterministic row IDs, enabled/selectable defaults, and no-selection behavior. Preserve real `CIV`/`MEC`/`ELE`/`PRC` worksheet provenance when deduplicating source evidence. No fuzzy/substring/placement test may pass.

- [ ] **Step 6: Run the classification tests to verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native -- --nocapture
```

Expected: FAIL because matching/layout/classification APIs are not implemented. Formula/merge tests are now reachable because Step 4 made their facts real.

- [ ] **Step 7: Implement the minimal pure matcher and bounded scanner**

Implement `rfi_follow_up.rs` DTOs and pure helpers exactly as the design specifies:

- `canonical_rfi` and exact status mapping;
- source evidence aggregation with provenance and formula/merge flags;
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

**Files:**
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

- [ ] **Step 6: Observe anchored-preview RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml native::tests::follow_up_preview_ -- --nocapture
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

Call `materialize_preview(params)` first, then issue exactly `[rfi-parent/Read, follow-up-parent/ReadWrite]` under one scope. Anchor only the two existing workbook leaves, compare identities, and call `preview_from_open_files` in the blocking task. Compute the future backup path lexically; do not create its directory or attempt to anchor that absent leaf during preview. Cache `BackendOrigin::Rust` and cloned grant IDs only after preview succeeds, then consume `commit()`. Any failure revokes provisional grants and leaves no cached plan.

Implement `PathGrantStore::issue_provisional` with a 1–8 bound, ordered issues, explicit prior-ID revocation on error, and existing drop guard. `authorize_capability_for_grant` looks up exact grant ID, rejects expiry, requires complete grant equality/access, and returns capability authorization; `anchor_granted_path` calls `AuthorizedPath::anchor` and never reopens a caller raw path. Add the existing backend preview match branch only; no command/special form/direct React filesystem access.

- [ ] **Step 8: Run preview and route/security GREEN**

```bash
cargo test --manifest-path src-tauri/Cargo.toml native::tests::follow_up_preview_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml native::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml backend::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml registry::tests -- --nocapture
npx vitest run --config frontend/vite.config.ts frontend/src/lib/bridge.test.ts frontend/src/lib/frontend-authority.test.ts
```

- [ ] **Step 9: Review and commit**

```bash
git diff --check
git add src-tauri/src/paths.rs src-tauri/src/fs_ops.rs src-tauri/src/registry.rs src-tauri/src/native.rs src-tauri/src/backend.rs src-tauri/src/smoke.rs src-tauri/tests/fixtures/protocol1_operations.json src-tauri/tests/rfi_follow_up_native.rs frontend/src/lib/bridge.ts frontend/src/lib/bridge.test.ts frontend/src/lib/frontend-authority.test.ts frontend/src/locales/app_en.arb frontend/src/locales/app_az.arb frontend/src/locales/app_ru.arb frontend/src/locales/app_tr.arb docs/RELEASE_SAFETY.md
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

Add separate rejects for empty/more-than-500 maps, absent/wrong/formula/merged selected cells, wrong expected style/value, nonallowed replacement values/update kinds, signed/protected target, foreign namespace shadow, destination collision, and output that would write byte `MAX_PACKAGE_BYTES + 1`. The size failure leaves the partial destination handle/file for journaled retention and never opens or unlinks by path. In `rfi_follow_up_native.rs`, feed two approved update DTOs for the same sheet/coordinate in the ordered `Vec` and require rejection before map construction; do not test duplicate keys in a `BTreeMap`.

- [ ] **Step 2: Observe RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test ooxml_fidelity guarded_follow_up_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test rfi_follow_up_native duplicate_approved_coordinates -- --nocapture
```

Expected: FAIL because guarded patch/delta APIs and Vec-first duplicate validation are missing.

- [ ] **Step 3: Implement the surgical guarded writer and canonical event verifier**

Validate the entire update map before creating the no-clobber destination. Require each selected physical cell and complete expected `CellData`, no formula/merge, and exact approved replacement string. In the target cell preserve every attribute except its text-representation `t` attribute and every non-value child event/subtree. Replace only canonical `<v>` or `<is>` representation with one namespace-correct inline string; insert before canonical `extLst` when no representation exists. Never clear the whole `children` vector, create `<c>`/row, rewrite validation, or touch RFI/header/table placement.

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

**Files:**
- Create: `src-tauri/src/leaf_transaction.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/cap_fs.rs`
- Modify: `src-tauri/src/plan_store.rs`
- Modify: `src-tauri/src/paths.rs`
- Modify: `src-tauri/src/journal.rs`
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

#[serde(rename_all = "snake_case")]
pub enum RetainedPublicationPurpose { Apply, Undo }

#[serde(rename_all = "snake_case")]
pub enum RetainedPublicationPhase {
    Prepared, Displacing, Retained, CreatingCanonical,
    CanonicalSecured, CanonicalAuxiliaryReady, WritingCanonical, VerifyingCanonical,
    Applied, RecoveryRequired,
}

#[serde(rename_all = "snake_case")]
pub enum RetainedArtifactRole {
    Backup, PrivateOutput, DisplacedOriginal, DisplacedApplied,
}

#[serde(rename_all = "snake_case")]
pub enum FollowUpJournalPhase {
    ArtifactParentReady, PrivateOutputReady, BackupReady, PublicationActive,
}

#[serde(rename_all = "snake_case")]
pub enum PersistedRootAccess { ReadOnly, ReadWrite }

#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantRootAccessRecord {
    pub(crate) root: String,
    pub(crate) identity: FilesystemObjectIdentity,
    pub(crate) access: PersistedRootAccess,
}

pub(crate) struct InterprocessMutationPermit { /* held OS lock-file handle */ }

impl JournalStore {
    pub(crate) fn try_acquire_mutation_lock(
        &self,
    ) -> Result<InterprocessMutationPermit, BackendError>;
    pub(crate) fn write_s5_cas(
        &self,
        record: &mut JournalRecord,
        expected_revision: u64,
    ) -> Result<(), BackendError>;
}

// paths.rs
pub(crate) fn root_accesses_for_plan(
    &self,
    grant_ids: &[String],
    caller: &CallerIdentity,
    operation_id: &str,
) -> Result<Vec<GrantRootAccessRecord>, BackendError>;

pub(crate) fn reopen_recorded_roots(
    records: &[GrantRootAccessRecord],
) -> Result<Vec<CapabilityRoot>, BackendError>;

// journal.rs
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingRetainedArtifact {
    pub(crate) role: RetainedArtifactRole,
    pub(crate) path: String,
    pub(crate) locator: String,
    pub(crate) identity: Option<FilesystemObjectIdentity>,
}

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

#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryNotice {
    pub warning_key: String,
    pub locator: String,
}

// JournalRecord additive field:
// revision: u64 (serde default 0; checked-incremented by S5 CAS)

// leaf_transaction.rs
pub(crate) enum PublicationCancellation<'a> {
    Apply(&'a CancellationToken),
    NotCancellable,
}

pub(crate) fn execute_retained_publication<R, V>(
    journals: &JournalStore,
    record: &mut JournalRecord,
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
    record: &mut JournalRecord,
    roots: &[CapabilityRoot],
) -> Result<Option<RecoveryEntry>, BackendError>;

// fs_ops.rs
pub fn recover_pending(
    &self,
    caller: &CallerIdentity,
) -> Result<Vec<RecoveryEntry>, BackendError>;

enum StepFailure {
    NoEffect(String),
    WarningNoEffect(Warning),
    OutcomeUnknown(String),
    WarningOutcomeUnknown(Warning),
    Cancelled,
}

// FilesystemAuthority::apply step-result match
Err(StepFailure::WarningOutcomeUnknown(warning)) => {
    result.failed += 1;
    result.errors.push(warning); // preserve localized key + locator subject
    record.state = Some(JournalState::RecoveryRequired);
    insert_effect_state(&mut result, "outcome_unknown");
    let expected_revision = record.revision;
    self.journals.write_s5_cas(&mut record, expected_revision)?;
    return Ok(result);
}
```

`JournalRecord` gains optional `pending_publication`, optional `pending_retained_artifact`, default-empty `retained_evidence`, optional `recovery_notice`, optional `follow_up_phase`, default-empty `grant_root_accesses`, and default-zero `revision`; `RecoveryEntry` gains optional `warningKey`/`locator`. No step kind or backend method is added.

- [ ] **Step 1: Write capability, security-metadata, preclaim, interprocess-lock, and Windows flush RED tests**

In `cap_fs.rs` and `plan_store.rs` unit tests, plus the named cross-process lock tests in `journal.rs`, prove:

- the Task 4 `create_new` primitive still succeeds only for an absent anchored name, returns the live file handle, and leaves an existing destination untouched with `AlreadyExists`;
- `retain_current_name_no_clobber` moves the current canonical name only to an absent same-parent retained leaf; an existing retained destination wins and both leaves remain untouched;
- a racer replacing canonical before displacement may be moved, but its exact bytes remain at the retained leaf and nothing is overwritten or deleted;
- post-rename directory-sync failure is outcome-unknown and the rename is never retried;
- cross-parent/cross-device retention is rejected with no copy fallback;
- compatibility `CapabilityRoot::open` remains `ReadWrite` for legacy internal mutation; ordinary issuance maps `Read → ReadOnly` and both `Write`/`ReadWrite → ReadWrite`; persisted-read issuance explicitly uses `ReadOnly`; descendants preserve access and no RFI Log grant is upgraded;
- bounded no-follow retained-storage enumeration aggregates both artifact parents, accepts only four exact S5 prefixes, and computes checked logical charge as default length plus canonical auxiliary-record bytes;
- `FileSecuritySnapshot` requires one hard link and round-trips modeled Unix metadata plus Windows owner/group/complete DACL+SACL/CAP/label/EAs/attributes/ADS. Windows parses one unnamed `BACKUP_DATA` header and exact-skips its 64-bit payload with `BackupSeek`; default bytes never enter the 1-MiB auxiliary budget, EA is validated separately, and only named ADS are compared to `FileStreamInformation`;
- S5-only `create_new_secured` atomically supplies the final descriptor. DACL-excluded principals never receive inherited access. Stable injected metadata drift makes the primitive verifier return mismatch/error; lifecycle `RecoveryRequired` persistence is deliberately deferred to Step 9 RED and Step 11 implementation;
- read-only operation inspection leaves plan state unchanged; process-local then OS-wide permits are acquired before `claim_for_apply_bound_under_permit`; contention leaves the exact plan `Available`, makes no journal/quota write, and retry after process exit claims the same plan;
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

Expected: FAIL because retain/sync-parent/access/quota/security/preclaim/OS-lock APIs, the Windows authorization feature, and real Windows flush are absent; the Task 4 create-new regression remains GREEN.

- [ ] **Step 3: Implement only capability/security primitives, preclaim APIs, OS lock, manifest feature, and real Windows flush**

Expose existing `file_identity`. Keep `CapabilityRoot::open` compatibility and add access-aware opening. Preserve Task 4 `AnchoredPath::create_new`; add `sync_parent`, same-parent no-clobber retention, and `AnchoredPath::{capture_parent_security,verify_parent_security}`. Those methods operate internally on private `self.parent: PlatformDirectory`; neither the field nor private type becomes crate-visible. Implement bounded no-follow usage enumeration over both authorized directories and four S5 prefixes.

Implement the security snapshots and non-clone wrapper. Atomic descriptor tests use a restricted-token process excluded by the DACL. Same-owner primitive tests inject stable ADS, EA, attribute, and hard-link drift and assert verifier mismatch only. They do not assert journal state; Step 9 introduces the lifecycle RED that requires persisted `RecoveryRequired`. Document the transient/ABA threat-model boundary and test default-stream/delete-sensitive sharing separately.

For auxiliary capture, inventory named streams, exact-skip one unnamed `BACKUP_DATA`, validate EA independently, and compare only named ADS. Define auxiliary logical charge as canonical record bytes including names/tags/lengths/values. Retained scans measure default plus auxiliary charge and reject overflow/inaccessibility. Test >1-MiB default data with small EA/ADS and exact quota boundaries. Backup lives in canonical parent; private output uses protected owner-only creation.

Quota RED cases all match the Step 2/4 Cargo filter: `directory_access_and_retained_quota_scan_counts_auxiliary_records`, `directory_access_and_retained_quota_apply_reservation_charges_three_auxiliary_copies`, `directory_access_and_retained_quota_undo_reservation_charges_retained_applied_auxiliary`, `directory_access_and_retained_quota_exact_limit_allowed_and_one_byte_over_rejected`, and `directory_access_and_retained_quota_auxiliary_or_arithmetic_overflow_fails_closed`. Step 3 implements only pure checked reservation arithmetic plus census; Step 11/15 consume it. Fixtures vary auxiliary charge at constant ordinary length.

In `plan_store.rs`, implement both Step 1 RED-covered APIs now—not in Task 3. `inspect_available_bound_operation` performs validated same-caller/same-operation bound-plan inspection without state mutation. `claim_for_apply_bound_under_permit` consumes the already-acquired `GlobalMutationPermit`, atomically revalidates the inspected binding/digest/TTL/caller, and only then changes `Available` to the single claimed state. Contention before this call leaves the plan byte-for-byte `Available`; preserve existing `claim_for_apply_bound` behavior for non-S5 callers.

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

- pre-S5 JSON deserializes/reserializes with `pendingPublication`/`pendingRetainedArtifact`/`recoveryNotice`/`followUpPhase` absent and `retainedEvidence`/`grantRootAccesses` empty;
- S5 validation requires one access record per deduplicated legacy root identity, equals the strongest bound grant, maps a distinct source-only root read-only and target/shared parent read-write, and rejects absent/duplicate/conflicting/extra records; non-S5 old journals retain legacy compatibility;
- old JSON defaults `revision=0`; absent journal plus expected zero uses durable O_EXCL create-new and parent sync; existing journal CAS rereads under the OS permit, rejects wrong journal ID/stale revision/overflow without replacement, and checked-increments exactly once on success; legacy recovery cannot rewrite an S5-discriminated record;
- `ArtifactParentReady`, `PrivateOutputReady`, `BackupReady`, and `PublicationActive` allow only their exact pending/completed evidence combinations; every `Applying` S5 record has one representable phase;
- before private-output secure-create, `pendingRetainedArtifact` carries role/path/locator; after creation and before bytes it carries owned-handle identity; crashes preserve any present object and final evidence plus charge/pending-clear is one CAS;
- a valid pending publication contains exact paths/identities/hashes, checked expected-canonical default+auxiliary=logical charge, destination auxiliary charge, pre-effect predicates, optional identities, and one representable phase;
- invalid hashes, duplicate/conflicting paths, retained outside the canonical parent, paths outside persisted roots, overlong locators, impossible phase/identity combinations, or unmatched apply/undo `RestoreBackup` are rejected;
- successful evidence records backup/private-output/displaced-original/displaced-applied roles and stays non-executable; result projection contains only ordered role/localized bounded locator pairs;
- cloud metadata excludes every transaction/evidence/recovery/path/hash field;
- `warning_outcome_unknown_preserves_localized_recovery_warning`: the branch preserves warning/effect state, calls `write_s5_cas` with the current revision, increments exactly once, rejects a stale revision without replacement, and returns immediately; direct `JournalStore::write` is forbidden for this S5 terminal transition.

- [ ] **Step 6: Observe journal/result RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml journal::tests::retained_publication_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml fs_ops::tests::warning_outcome_unknown_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test cloud_metadata -- --nocapture
```

Expected: FAIL because the additive fields, validation, evidence projection, exact StepFailure variant, and mapping do not exist.

- [ ] **Step 7: Implement only the additive journal and result contracts**

Add the exact additive journal/result contracts. The `WarningOutcomeUnknown` S5 handler must call `write_s5_cas(&mut record, expected_revision)` and its focused test must prove increment plus stale rejection; it must never call legacy `write`. Persist root access and the pure reopen helper only; do not wire recovery/undo or publication yet. Preserve legacy behavior only for records with no S5 discriminator.

- [ ] **Step 8: Run journal/result/cloud GREEN**

```bash
cargo test --manifest-path src-tauri/Cargo.toml journal::tests::retained_publication_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml fs_ops::tests::warning_outcome_unknown_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test cloud_metadata -- --nocapture
```

Expected: PASS.

- [ ] **Step 9: Write the complete apply/crash/race/recovery RED matrix before any production helper**

Create crate unit tests in `leaf_transaction.rs`, `fs_ops.rs`, and `journal.rs` **before** implementing `execute_retained_publication` or `reconcile_retained_publication`. Private hook-dependent journal inspection lives only in those crate unit tests; integration tests never name a private `cfg(test)` symbol. If needed, use a private `#[cfg(test)] execute_retained_publication_with_hook` callback accepted only by unit tests.

The RED matrix must programmatically cover:

- exact happy apply: one selected cell changes, RFI Log remains byte/metadata exact, backup equals original, package verifier runs, changed count is exact, and journal order is optional `Mkdir`, `Copy`, `RestoreBackup`;
- lexical backup/private-output authorization before journaled directory creation, anchoring only after the parent exists, and no ownership claim for a pre-existing directory;
- same-size bytes, mtime, identity, workbook predicate, and complete security/stream drift before backup, before displacement, and after displacement on the retained original; any drift prevents canonical recreation;
- `prepared` evidence durably contains original/applied hashes, expected original and backup identities/hashes, exact private-output identity/hash, and unique retained locator while canonical is untouched;
- final cancellation before displacement returns cancellation with canonical unchanged; cancellation after `ArtifactParentReady`, pending/final private output, or backup is `CancelledPartial` even when `steps` is empty, projects retained locators, and is not checked as clean after displacement begins;
- destination-no-clobber displacement moves racer bytes without overwrite, then wrong retained identity/hash persists localized recovery-required and is never restored, published, or deleted even when canonical is absent;
- exact retained original identity/hash is journaled before create-new;
- retained-storage preflight measures each artifact's default stream plus canonical auxiliary-record charge. Apply reserves three files and checked `2 * original_default + MAX_PACKAGE_BYTES + 3 * original_auxiliary_charge`; undo reserves one file and `current_applied_default + current_applied_auxiliary_charge`. Reject overflow or projections above 64/8 GiB; exactly-at-limit remains allowed;
- a second OS process cannot pass admission/quota or write/recover while the first holds the permit; rejected contention leaves the same plan `Available` with unchanged digest/TTL, and retry after release claims it once. Stale CAS cannot replace newer evidence;
- complete original/parent security is late-revalidated; descriptors are atomic. Lifecycle hooks inject stable auxiliary drift and assert persisted `RecoveryRequired`; sequential metadata observation is never called atomic;
- private output is born owner-only with unnamed-stream `ShareAccess=0`; no broader inherited principal gains access before bytes, while same-owner metadata access follows the stated threat model;
- private-output path/locator is journaled before create, open-handle identity before write, and final evidence+pending clear atomically after same-handle verification; crashes at each boundary are discoverable and never trigger path cleanup;
- a racer canonical created before create-new wins untouched; original/backup/private output remain retained;
- crash after atomic secure-create but before identity CAS leaves an unknown restricted canonical that recovery preserves;
- descriptor verification plus identity produce `canonical_secured`; EA/ADS verification produces `canonical_auxiliary_ready`; only then may unnamed bytes begin;
- a DACL-excluded restricted-token process obtains no access; stable same-owner ADS/EA/attribute/hard-link mutation is detected, while transient/ABA authorized mutation is explicitly outside the adversarial guarantee;
- racer rename/replacement after create leaves the foreign canonical untouched and fails final name-to-created-identity validation;
- exact applied canonical identity/hash plus exact retained original/source/backup completes journal classification without deleting any artifact;
- same hash/wrong identity for canonical, retained, backup, or private output; missing/wrong source or backup; access errors misreported as absence; parent-flush uncertainty; and repeated recovery calls all preserve bytes and remain idempotent;
- `WarningOutcomeUnknown` carries the exact warning/effect state and its terminal transition uses revision CAS; stale handler state cannot replace a newer record;
- recovery acquires process-local then journal-root OS-wide permits before enumeration/read and holds both through revision-CAS writes; concurrent same/second-process apply, undo, or recovery returns `operation_in_progress` without touching journals;
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

1. call `inspect_available_bound_operation` without mutation. For S5 acquire process-local then journal-root OS-wide permits, then call `claim_for_apply_bound_under_permit`, consuming the process permit and revalidating all bound-plan predicates. Contention releases permits and leaves the exact plan `Available`. Hold the claim plus OS permit through terminal result. Route every S5 write, including `WarningOutcomeUnknown`, through revision CAS;
2. decode/anchor/revalidate. Call `canonical.capture_parent_security()` and later `canonical.verify_parent_security(&snapshot)` through the pinned private parent handle; never expose `PlatformDirectory`. Capture original security through its identity-matched data/security handles. Aggregate census with `RetainedStorageReservation::for_apply`, reject quota before artifact effect, then journal/create only the private-output parent under CAS;
3. create owner-only private output, record identity, replay/verify EA/ADS, then write/verify OOXML. Unnamed bytes have a publication linearization check; metadata evidence is bounded sequential observation, not a global lock;
4. recapture source, create backup in the canonical parent with original `CreationSecurityDescriptor`, verify its creation security, replay/verify EA/ADS, then copy unnamed bytes. Revalidate everything before displacement;
5. call `execute_retained_publication` with private output, `PublicationCancellation::Apply(token)`, original creation descriptor, and complete snapshot. The helper's sole checkpoint precedes displacement;
6. retain whatever occupies canonical destination-no-clobber, sync the pinned parent, open the retained leaf, and require the exact expected identity/hash. Wrong/unrecorded retained state becomes `RecoveryRequired`; never restore or delete it;
7. persist retained predicates, set `creating_canonical`, and call canonical `create_new_secured` with `ShareAccess=0`. `AlreadyExists` preserves racer;
8. verify atomic creation descriptor, CAS-record identity plus `canonical_secured`, and flush parent. Replay/verify EA/ADS and CAS-set `canonical_auxiliary_ready`;
9. only after `canonical_auxiliary_ready`, set `writing_canonical`, stream unnamed bytes, sync, and verify all predicates through `SecuredCreatedFile`;
10. set `verifying_canonical`, require destination-handle identity/byte/security hash, exact backup/source/retained evidence, and canonical no-follow name resolution to the recorded created identity. A foreign canonical remains untouched;
11. set publication phase `applied`, append completed `RestoreBackup`, de-duplicating by role/path while finalizing permanent backup/displaced-original evidence and preserving the already-recorded private-output evidence, clear pending publication/artifact plus `followUpPhase`, and return changed count. Do not unlink evidence or remove its directory;
12. implement `reconcile_retained_publication` to recognize only exact durable no-effect/applied states from design §11, preserve unknown/unrecorded identities, never restore retained to canonical, never publish during recovery, never delete evidence, and remain idempotent;
13. implement conservative pre-identity backup/private-output and closed interstitial-phase handling. `FilesystemAuthority::recover_pending(caller)` acquires process-local then journal-root OS-wide permits before reading any journal and holds both through revision-CAS writes; concurrent apply/undo/recovery returns `operation_in_progress`. `ArtifactParentReady`, `PrivateOutputReady`, and `BackupReady` preserve exact evidence and become recovery-required without touching canonical; `PublicationActive` requires the publication matrix. `NativeBackend::recovery()` uses this route and delegates only records with every S5 field absent to legacy logic;
14. replace steps-only cancellation classification with `journal_has_effect`: completed steps, pending retained artifact, retained evidence, or any Follow Up phase means `CancelledPartial`; project bounded retained locators.

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

Route only completed Follow Up `RestoreBackup` through the helper. Reject destination, acquire both permits, validate roots and combine census with `RetainedStorageReservation::for_undo` before any undo artifact. Validate the same-parent predicate through `canonical.capture_parent_security()` / `canonical.verify_parent_security(...)` and validate backup through its open identity-matched handles. Undo is non-cancellable. Secure-create replacement, record phases, write/verify, then applied CAS. Every transition uses CAS; preserve evidence.

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
git add src-tauri/Cargo.toml src-tauri/src/lib.rs src-tauri/src/cap_fs.rs src-tauri/src/plan_store.rs src-tauri/src/paths.rs src-tauri/src/leaf_transaction.rs src-tauri/src/journal.rs src-tauri/src/undo.rs src-tauri/src/fs_ops.rs src-tauri/src/native.rs src-tauri/src/rfi_follow_up.rs src-tauri/tests/rfi_follow_up_native.rs src-tauri/tests/filesystem_authority.rs frontend/src/types.ts frontend/src/App.tsx frontend/src/App.test.tsx
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

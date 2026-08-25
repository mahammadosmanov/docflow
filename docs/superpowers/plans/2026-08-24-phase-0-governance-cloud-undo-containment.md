# Phase 0 Governance and Cloud Undo Containment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish GPT/Codex-native project canon and remove cloud-authored filesystem authority while preserving same-device undo through validated local journals.

**Architecture:** This is the first independently releasable slice of the approved strangler migration. It does not yet move document work into Rust. It first reconciles the governing documents, then makes cloud history metadata-only at the current Python/React boundary so later Rust commands inherit a safe contract.

**Tech Stack:** Markdown/AGENTS.md, Python 3.11+ with pytest/mypy, React 19/TypeScript/Vitest, Supabase PostgreSQL RLS, Tauri 2 baseline verification.

**Spec:** `docs/superpowers/specs/2026-08-24-rust-engine-migration-design.md`

## Global Constraints

- Windows and macOS must remain supported throughout migration.
- Every mutation remains preview → explicit approval → apply → journal → undo.
- Apply executes only the approved cached plan and never recalculates it.
- Never overwrite an existing destination.
- Cloud history is audit metadata only; only a validated local journal may authorize undo.
- Forms remain generated from `ParamSchema`.
- User-visible text remains complete in `tr`, `en`, `ru`, and `az`.
- Preserve NFC + uppercase RFI normalization and the strict `-RFI-` anchor.
- Preserve the monochrome UI.
- Do not modify `legacy/` or generated files under `app/build/`.
- Use TDD for behavioral changes. Documentation/configuration edits are reviewed with exact static checks.
- Do not commit, push, publish, or release unless the user explicitly requests it.

---

## File map

### New files

- `AGENTS.md` — canonical GPT/Codex repository instructions.
- `frontend/AGENTS.md` — UI-only authority, localization, accessibility, and frontend verification rules.
- `src-tauri/AGENTS.md` — Rust/Tauri security, side-effect, platform, and verification rules.

### Modified files

- `CLAUDE.md` — removed after its active rules are transferred to `AGENTS.md`.
- `docs/PRODUCT.md` — Rust migration becomes accepted product scope; source-of-truth points to `AGENTS.md`.
- `docs/ARCHITECTURE.md` — target and transitional authority boundaries replace Python-as-permanent-authority wording.
- `docs/MIGRATION.md` — records the superseding staged Rust decision and migration order.
- `docs/RELEASE_SAFETY.md` — adds transitional and final Rust/package gates.
- `docs/VERSIONING.md` — treats Python version fields/tools as transitional until Rust `xtask` replacement.
- `src/docflow/cloud.py` — uploads metadata with an empty compatibility `steps` array; it no longer accepts executable steps from callers.
- `src/docflow/sidecar.py` — merges cloud metadata with local-journal availability and rejects client-supplied undo steps.
- `frontend/src/types.ts` — exposes `undoAvailable`; cloud `steps` remains nullable only for protocol compatibility.
- `frontend/src/App.tsx` — sends only journal identity for undo and disables undo when no local journal exists.
- `frontend/src/App.test.tsx` — proves metadata-only cloud rows do not offer undo.
- `frontend/src/lib/bridge.ts` — mock history data includes local-journal availability.
- `src/docflow/resources/l10n/app_{tr,en,ru,az}.arb` — localized “local journal required” status.
- `docs/supabase_auth.sql` — restricts history insertion/update ownership and permits updates only to `undone`.
- `tests/test_application.py` — backend regression tests for remote-step rejection, metadata-only upload/listing, local undo availability, and SQL privileges.

---

### Task 1: Establish GPT/Codex-native governing instructions

**Files:**
- Create: `AGENTS.md`
- Create: `frontend/AGENTS.md`
- Create: `src-tauri/AGENTS.md`
- Delete: `CLAUDE.md`
- Modify: `docs/PRODUCT.md`
- Modify: `docs/ARCHITECTURE.md`
- Modify: `docs/MIGRATION.md`
- Modify: `docs/RELEASE_SAFETY.md`
- Modify: `docs/VERSIONING.md`

**Interfaces:**
- Consumes: approved design in `docs/superpowers/specs/2026-08-24-rust-engine-migration-design.md`.
- Produces: one root instruction chain recognized natively by Codex, with frontend and Rust overrides.

- [ ] **Step 1: Write root `AGENTS.md`**

Include the current repository layout, canonical-document order, all twelve non-negotiable safety/UI rules adjusted for transitional Rust authority, exact current verification commands, no-commit/push/release default, read-only `legacy/`, and links to the migration design.

The architecture wording must say:

```markdown
- React/Vite owns presentation and transient UI state only.
- Rust/Tauri is the target authority for filesystem, document, network, token, and native-platform work.
- Python remains a temporary fallback during the staged migration; a plan is always applied by the backend that created it.
- The webview never receives tokens or arbitrary filesystem, network, shell, or process authority.
```

- [ ] **Step 2: Write nested instruction files**

`frontend/AGENTS.md` must prohibit filesystem/network/token/process authority, require schema-generated forms and four locales, preserve monochrome semantics, and name `npm test` plus `npm run build`.

`src-tauri/AGENTS.md` must require typed closed commands, bounded inputs, backend authorization, immutable plans, local-journal-only undo, no-clobber operations, write-ahead journaling, cross-platform implementations, and Rust formatting/Clippy/test/check gates.

- [ ] **Step 3: Reconcile product and architecture canon**

Update `docs/PRODUCT.md` so Rust document processing is no longer a non-goal. Replace it with thin-webview and staged-parity constraints. Make `AGENTS.md` source-of-truth item 1.

Rewrite active architecture diagrams and wording to show the target Rust authority plus temporary Python fallback. Preserve the current persistent paths and explicitly record the credential-security exception.

Update `docs/MIGRATION.md` so the previous permanent Python-sidecar selection is historical and superseded by the approved 2026-08-24 staged Rust decision.

- [ ] **Step 4: Reconcile release/version guidance**

Add two release states to `docs/RELEASE_SAFETY.md`:

```text
transitional: Python + Rust gates, packaged sidecar only while a fallback remains
final: Rust/frontend/Tauri gates, package-content and SBOM proof that Python is absent
```

Keep `tools/version_manager.py`, `pyproject.toml`, and Python version fields authoritative during transition. State in `docs/VERSIONING.md` that removal happens only in Phase 9 with an equivalent Rust `xtask` or Node command already verified.

- [ ] **Step 5: Remove `CLAUDE.md` and check references**

Run:

```bash
rg -n 'CLAUDE\.md|No document processing in React, Rust|Python remains the authority|permanent Python' --glob '!node_modules/**' --glob '!src-tauri/target/**' --glob '!app/build/**'
```

Expected: no active canonical reference to `CLAUDE.md`, no active Rust-document-processing prohibition, and historical mentions labeled as historical/superseded.

- [ ] **Step 6: Review checkpoint**

Run `git diff --check` and inspect `git diff -- AGENTS.md frontend/AGENTS.md src-tauri/AGENTS.md docs CLAUDE.md`. Do not commit unless explicitly requested.

---

### Task 2: Make cloud history metadata-only at the backend boundary

**Files:**
- Modify: `tests/test_application.py`
- Modify: `src/docflow/cloud.py`
- Modify: `src/docflow/sidecar.py`

**Interfaces:**
- Consumes: existing `CloudEntry`, `CloudHistory`, `SidecarApplication`, and local `Engine.history()`/`Engine.journal()` APIs.
- Produces: `HistoryRow` JSON with `steps: null` and `undoAvailable: bool`; `engine/undo` accepts only a local `journalId` plus optional destination.

- [ ] **Step 1: Write failing backend tests**

Add these behaviors to `tests/test_application.py`:

```python
def test_cloud_push_never_uploads_executable_steps(...):
    # Arrange the existing fake send_json capture and an allowed AuthService.
    assert cloud.push(
        op_id="rfi.process_scans",
        applied=1,
        failed=0,
        journal_id="journal",
        summary="1/1",
    )
    body = captured["body"]
    assert isinstance(body, dict)
    assert body["steps"] == []


def test_cloud_history_exposes_only_metadata_and_local_undo_availability(...):
    application = SidecarApplication(SettingsStore(tmp_path / "settings.json"))
    application.auth.session = AuthSession(
        "access", "refresh", "user-id", "e@example.com", int(time.time()) + 3600
    )
    application.auth.me = AppUser("user-id", "e@example.com", allowed=True)
    monkeypatch.setattr(
        application.cloud,
        "list",
        lambda _limit: [
            CloudEntry(
                id="cloud-1",
                created_at="2026-08-24T00:00:00Z",
                op_id="rfi.process_scans",
                applied=1,
                failed=0,
                journal_id="local-1",
                summary="1/1",
                steps=[{"kind": "move", "src": "/forged", "dst": "/victim"}],
                undone=False,
                user_email="e@example.com",
            ),
            CloudEntry(
                id="cloud-2",
                created_at="2026-08-23T00:00:00Z",
                op_id="rfi.process_scans",
                applied=1,
                failed=0,
                journal_id="remote-only",
                summary="1/1",
                steps=[{"kind": "move", "src": "/forged", "dst": "/victim"}],
                undone=False,
                user_email="other@example.com",
            ),
        ],
    )
    monkeypatch.setattr(
        application.engine,
        "history",
        lambda _limit=50: [{"journalId": "local-1"}],
    )

    payload = application.dispatch("history/list", {"limit": 50}, lambda _event: None)

    assert payload["rows"][0]["steps"] is None
    assert payload["rows"][0]["undoAvailable"] is True
    assert payload["rows"][1]["steps"] is None
    assert payload["rows"][1]["undoAvailable"] is False
    assert "/victim" not in json.dumps(payload)


def test_sidecar_rejects_client_authored_undo_steps(tmp_path: Path) -> None:
    application = SidecarApplication(SettingsStore(tmp_path / "settings.json"))
    with pytest.raises(ValueError, match="client-authored undo steps"):
        application.dispatch(
            "engine/undo",
            {
                "journalId": "forged",
                "steps": [
                    {"kind": "move", "src": "/forged", "dst": "/victim", "meta": {}}
                ],
            },
            lambda _event: None,
        )
```

Import `CloudEntry` in the test module’s existing cloud-test area.

- [ ] **Step 2: Run RED**

Run:

```bash
.venv/bin/python -m pytest \
  tests/test_application.py::test_cloud_push_never_uploads_executable_steps \
  tests/test_application.py::test_cloud_history_exposes_only_metadata_and_local_undo_availability \
  tests/test_application.py::test_sidecar_rejects_client_authored_undo_steps -v
```

Expected: failures because `CloudHistory.push` still takes/uploads caller steps, history returns cloud steps without `undoAvailable`, and dispatch executes supplied steps.

- [ ] **Step 3: Implement metadata-only cloud push**

Change `CloudHistory.push` to remove the `steps` parameter and always send:

```python
"steps": [],
```

Change `SidecarApplication._sync_result` to stop reading/serializing journal steps for cloud upload. Keep `summary`, counts, `journal_id`, `user_id`, and `user_email` unchanged.

- [ ] **Step 4: Implement local-journal availability**

In `SidecarApplication._history`, build a set from local history before shaping cloud rows:

```python
local_journal_ids = {
    str(entry.get("journalId", ""))
    for entry in self.engine.history(max(limit, 50))
    if entry.get("journalId")
}
```

For every cloud row return:

```python
"steps": None,
"undoAvailable": entry.journal_id in local_journal_ids,
```

For local fallback rows return `"undoAvailable": True`.

- [ ] **Step 5: Reject supplied steps at dispatch**

Before calling `self.engine.undo`, reject the presence of the `steps` key:

```python
if "steps" in params:
    raise ValueError("client-authored undo steps are not accepted")
```

Call `self.engine.undo(journal_id, destination=...)` without `steps`. Keep the lower-level legacy journal reader unchanged in this slice so local tests and rollback compatibility remain intact.

- [ ] **Step 6: Run GREEN and related suite**

Run the exact RED command again, then:

```bash
.venv/bin/python -m pytest tests/test_application.py tests/test_apply.py -v
.venv/bin/python -m mypy src/docflow
```

Expected: all selected tests pass and mypy reports no issues.

- [ ] **Step 7: Review checkpoint**

Inspect `git diff -- tests/test_application.py src/docflow/cloud.py src/docflow/sidecar.py`. Confirm no cloud-provided path reaches `Engine.undo`. Do not commit unless explicitly requested.

---

### Task 3: Disable cross-device undo in the UI without removing audit history

**Files:**
- Modify: `frontend/src/types.ts`
- Modify: `frontend/src/lib/bridge.ts`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/App.test.tsx`
- Modify: `src/docflow/resources/l10n/app_tr.arb`
- Modify: `src/docflow/resources/l10n/app_en.arb`
- Modify: `src/docflow/resources/l10n/app_ru.arb`
- Modify: `src/docflow/resources/l10n/app_az.arb`

**Interfaces:**
- Consumes: backend `HistoryRow` with `undoAvailable` and nullable compatibility `steps`.
- Produces: UI undo request `{ journalId, cloudId?, destination? }`; remote-only history remains visible but non-actionable.

- [ ] **Step 1: Extend `MockBridge` with deterministic history rows**

Add a `history/list` case returning one same-device row and one remote-only row:

```typescript
result = {
  online: true,
  rows: [
    {
      when: "2026-08-24T00:00:00Z",
      opId: "rfi.process_scans",
      moves: 1,
      failed: 0,
      journalId: "local-1",
      steps: null,
      cloudId: "cloud-1",
      undone: false,
      fromCloud: true,
      userEmail: "demo@docflow.local",
      undoAvailable: true,
    },
    {
      when: "2026-08-23T00:00:00Z",
      opId: "rfi.process_scans",
      moves: 1,
      failed: 0,
      journalId: "remote-only",
      steps: null,
      cloudId: "cloud-2",
      undone: false,
      fromCloud: true,
      userEmail: "other@docflow.local",
      undoAvailable: false,
    },
  ],
} satisfies HistoryPayload;
```

Add `undoAvailable: boolean` to `HistoryRow` in `frontend/src/types.ts`.

- [ ] **Step 2: Write the failing frontend test**

Add an App test that signs in, opens history, and asserts exactly one Undo button is available while the remote row shows the localized local-journal requirement:

```typescript
it("never offers undo for cloud history without a local journal", async () => {
  render(<App />);
  await signIn();
  fireEvent.click(
    screen
      .getByText("Yapılan işlemleri gör ve güvenle geri al.")
      .closest("button")!,
  );

  await screen.findByRole("heading", { name: "Geçmiş" });
  expect(screen.getAllByRole("button", { name: "Geri al" })).toHaveLength(1);
  expect(screen.getByText("Geri almak için bu cihazdaki yerel günlük gerekli.")).toBeVisible();
});
```

- [ ] **Step 3: Run RED**

Run:

```bash
npm test -- --run frontend/src/App.test.tsx -t "never offers undo for cloud history without a local journal"
```

Expected: failure because both rows currently offer undo and the new status text is absent.

- [ ] **Step 4: Implement safe UI behavior**

In the undo call remove `steps: row.steps`. Keep `journalId`, optional local recovery destination, and `cloudId` only for marking the matching metadata row after successful local undo.

Render states in this order:

1. `row.undone` → existing undone status.
2. `!row.undoAvailable` → localized non-button status.
3. otherwise → existing Undo button.

Add `history.localJournalRequired` to all four canonical ARB files. Use natural translations, preserving identical key sets:

- Turkish: `Geri almak için bu cihazdaki yerel günlük gerekli.`
- English: `The local journal on this device is required to undo.`
- Russian: `Для отмены требуется локальный журнал на этом устройстве.`
- Azerbaijani: `Geri qaytarmaq üçün bu cihazdakı yerli jurnal tələb olunur.`

- [ ] **Step 5: Run GREEN and related frontend gates**

Run the exact RED selector again, then:

```bash
npm test
npm run build
```

Expected: the focused behavior and complete frontend suite pass; TypeScript/Vite build succeeds.

- [ ] **Step 6: Review checkpoint**

Search:

```bash
rg -n 'steps:\s*row\.steps|engine/undo' frontend/src
```

Expected: no frontend path sends row steps to undo. Inspect the frontend diff. Do not commit unless explicitly requested.

---

### Task 4: Restrict Supabase history mutations to safe owner metadata

**Files:**
- Modify: `tests/test_application.py`
- Modify: `docs/supabase_auth.sql`

**Interfaces:**
- Consumes: authenticated `auth.uid()` and existing `history.user_id`.
- Produces: team-wide read for allowed users, owner-bound inserts, and owner-only update of the `undone` column.

- [ ] **Step 1: Add a failing static security-contract test**

Add:

```python
def test_supabase_history_sql_limits_mutation_to_owned_undone_flag() -> None:
    sql = (Path(__file__).parents[1] / "docs" / "supabase_auth.sql").read_text()
    assert "grant select, insert on public.history to authenticated" in sql
    assert "grant update (undone) on public.history to authenticated" in sql
    assert "grant select, insert, update on public.history to authenticated" not in sql
    assert "and user_id = (select auth.uid())" in sql
    assert "user_id = (select auth.uid())" in sql
```

The test intentionally checks the deployed SQL artifact because there is no local Supabase emulator in the repository.

- [ ] **Step 2: Run RED**

Run:

```bash
.venv/bin/python -m pytest tests/test_application.py::test_supabase_history_sql_limits_mutation_to_owned_undone_flag -v
```

Expected: failure because the current SQL grants broad update and allows `user_id is null`.

- [ ] **Step 3: Harden grants and policies**

Replace broad history privileges with:

```sql
revoke all on public.history from authenticated;
grant select, insert on public.history to authenticated;
grant update (undone) on public.history to authenticated;
```

Keep team-wide select guarded by `public.is_allowed()`. Require inserts to satisfy both allowed status and exact ownership:

```sql
with check (
  (select public.is_allowed())
  and user_id = (select auth.uid())
);
```

Require updates to address only the caller’s row:

```sql
using (
  (select public.is_allowed())
  and user_id = (select auth.uid())
)
with check (
  (select public.is_allowed())
  and user_id = (select auth.uid())
);
```

Column-level privileges ensure that even an owner can patch only `undone`, not `steps`, counts, paths, ownership, or identity.

- [ ] **Step 4: Run GREEN**

Run the exact RED selector again, then all application tests:

```bash
.venv/bin/python -m pytest tests/test_application.py -v
```

Expected: pass.

- [ ] **Step 5: Manual deployment gate**

Do not claim the production Supabase project is hardened merely because the SQL file changed. Record deployment as pending until an authorized operator runs the full idempotent SQL and verifies with two non-admin test users:

- both allowed users can read audit history;
- user A cannot update user B’s row;
- user A cannot update `steps`, `user_id`, or `journal_id` on their own row;
- user A can set `undone=true` on their own row;
- anonymous users have no table access.

- [ ] **Step 6: Review checkpoint**

Inspect `git diff -- docs/supabase_auth.sql tests/test_application.py`. Do not commit or deploy unless explicitly requested.

---

### Task 5: Verify the complete Phase 0 slice

**Files:**
- Verify all paths changed in Tasks 1–4.

**Interfaces:**
- Consumes: completed governance and security changes.
- Produces: exact local evidence; no production-deployment claim.

- [ ] **Step 1: Static consistency checks**

Run:

```bash
python tools/version_manager.py check
rg -n 'CLAUDE\.md|steps:\s*row\.steps' --glob '!node_modules/**' --glob '!src-tauri/target/**' --glob '!app/build/**'
git diff --check
```

Expected: version contract passes; no active `CLAUDE.md` or frontend cloud-step forwarding remains; diff check is clean. Historical references must be reviewed rather than blindly removed.

- [ ] **Step 2: Python gates**

Run:

```bash
.venv/bin/python -m pytest
.venv/bin/python -m mypy src/docflow
```

Expected: all tests pass and mypy reports no issues.

- [ ] **Step 3: Frontend gates**

Run:

```bash
npm test
npm run build
```

Expected: all tests pass and the production frontend build succeeds.

- [ ] **Step 4: Rust baseline gate**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: all commands exit zero. This slice should not yet change Rust runtime behavior.

- [ ] **Step 5: Packaged fallback smoke**

Because Python still exists in Phase 0, run:

```bash
.venv/bin/python tools/build_sidecar.py
.venv/bin/python tools/ci_ocr_smoke.py dist/docflow-sidecar
```

Expected: sidecar builds and the packaged image-only OCR path extracts the canonical RFI code without source/destination mutation.

- [ ] **Step 6: Final scope and diff review**

Run:

```bash
git status --short
git diff --stat
git diff --name-status
git diff
```

Confirm only the planned paths changed, `legacy/` and generated build outputs are untouched, and there are no credentials or document fixtures in the diff.

- [ ] **Step 7: Handoff**

Report exact RED/GREEN and final gate outputs. Explicitly label Supabase deployment as pending until verified against the live project. Do not commit, push, publish, or release unless requested.

---

## Self-review

- Spec coverage: this plan covers approved GPT/Codex governance and the confirmed cloud-undo security blocker. Credential migration and Rust authority are intentionally assigned to later independent plans.
- Scope boundary: no document engine, OCR, PDF, OOXML, printer, or credential-store implementation is included.
- Type consistency: backend and frontend use the same `undoAvailable: bool/boolean`; compatibility `steps` is always `null` in history responses and never accepted by the undo RPC.
- Safety: same-device local journal undo remains available; remote-only history remains visible but cannot mutate local files.
- Deployment honesty: SQL file verification is local evidence only; live Supabase hardening remains a separate authorized gate.

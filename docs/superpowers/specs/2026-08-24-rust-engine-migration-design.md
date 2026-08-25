# Rust Engine Migration Design

**Status:** Approved 2026-08-24

**Goal:** Make DocFlow a lean Tauri 2 application with a thin React/Vite presentation layer and a Rust authority for application, document, filesystem, network, and native-platform work. Python remains only as a temporary, tested fallback and is removed after verified parity.

## Decisions

1. Retain React 19, TypeScript, and Vite as the webview UI.
2. Make Tauri/Rust the sole long-term backend authority.
3. Replace Python one capability at a time; do not perform a big-bang rewrite.
4. Preserve all current capabilities, four locales, UI identity, safety invariants, and supported workflows.
5. Keep Windows and macOS buildable and testable throughout the migration.
6. Preserve existing settings, journals, trash, print queues, operation DTOs, and cloud schemas exactly unless this design explicitly defines a security exception.
7. Import existing plaintext Supabase tokens once into Windows Credential Manager or macOS Keychain, remove them from settings, and require sign-in after downgrading to a pre-migration release.
8. Make `AGENTS.md` the canonical GPT/Codex instruction file and remove active dependence on `CLAUDE.md`.
9. Use measured performance and fidelity to select implementation techniques; do not replace proven native engines merely to maximize pure-Rust code.

## Target architecture

```text
React/Vite webview
  presentation + transient UI state only
          | typed invoke/channel API
          v
Tauri command layer
  closed command set, validation, authorization, path grants
          v
Rust application state
  |- contracts and generated TypeScript DTOs
  |- authentication + secure credentials
  |- Supabase/Gemini network clients
  |- immutable plan store + operation registry
  |- progress/cancellation + one global mutation lock
  |- filesystem/journal/undo safety layer
  |- document-operation modules
  `- native platform adapters
       |- Tesseract and PDF renderer
       |- targeted OOXML patcher
       |- Windows Winspool/GDI
       `- Excel COM host where required
```

Tauri is the application shell and trusted command boundary. “No Python” means no Python interpreter, PyInstaller runtime, Python libraries, or sidecar IPC in the final product. It does not mean reimplementing Tesseract, PDF standards, Office, or operating-system print services from scratch.

### Transitional boundary

The Python sidecar initially moves behind Rust as a `FallbackSupervisor`:

- The webview loses shell spawn, stdin-write, and kill permissions.
- Rust owns backend routing; React cannot choose Python or Rust.
- Every cached plan records its backend origin.
- A Rust plan cannot be applied by Python or vice versa.
- Rust and Python may dual-run read-only preview work on isolated fixtures.
- They never dual-run a real filesystem, cloud, Office, or printer mutation.
- Rust may fall back only before an effect occurs.
- A partial or uncertain effect enters recovery mode; it is never retried through another backend.

## Authoritative operation flow

1. A user-initiated Tauri dialog issues an opaque, request-scoped path grant.
2. Rust validates identity, role, input limits, canonical paths, symlinks, and containment.
3. Preview acquires facts and creates an immutable, backend-cached plan.
4. React receives a reviewable plan DTO, not authority to edit plan steps.
5. Apply accepts an opaque `planId`, never client-authored steps.
6. Rust selects the operation and backend from the stored plan.
7. A pending journal record is durably written before each effect.
8. The approved step executes through atomic no-clobber primitives.
9. Journal state is committed and verified before the next effect.
10. Undo reads a validated local journal, runs inverse steps in reverse, and protects newer user edits with hashes.

Preview is observational. Apply never recalculates. Existing destinations are never overwritten. Undo removes only empty directories created by the operation.

## Security corrections

### Credentials

Current access and refresh tokens are stored in `~/.docflow/settings.json`. The first Rust-owned bootstrap will:

1. Read the compatible legacy session once.
2. Import tokens into Windows Credential Manager or macOS Keychain.
3. Verify secure-store readback.
4. Remove only token fields from the settings file through an atomic write.
5. Keep non-secret preferences and compatibility fields unchanged.
6. Fall back to a signed-out state without deleting the legacy source if secure-store migration cannot be verified.

Downgrading to a pre-migration build requires signing in again.

### Cloud history and undo

Cloud history is audit metadata only. It may not provide executable steps or filesystem paths.

- Local, validated journals are the sole undo authority.
- Cross-device recovery requires selecting local roots and generating a fresh preview.
- Rust rejects cloud-authored paths, plan steps, and journal identities.
- Supabase RLS is tightened so users cannot mutate another user’s history row or executable history fields.
- Rust commands enforce authorization in addition to server-side RLS.

## Persistent compatibility

Rust must read and write existing non-secret formats without an unapproved schema migration:

- `~/.docflow/settings.json`
- `~/.docflow/journal/`
- `~/.docflow/trash/`
- `~/.docflow/print_queue.json`
- protocol-1 request/response casing and nullability
- operation IDs and parameter-schema ordering
- local/cloud history IDs and timestamp shapes
- legacy journal hash fallbacks
- RFI normalization and strict `-RFI-` anchor behavior

Golden fixtures bind field omission versus `null`, ordering where observable, warning/error codes, path rendering, and legacy recovery behavior.

## Migration sequence

### Phase 0 — Contract and evidence freeze

- Supersede the former Python-authority architecture decision.
- Reconcile `AGENTS.md`, product, architecture, migration, release, and security canon.
- Freeze DTO, settings, journal, queue, PDF, workbook, OCR, and operation fixtures.
- Add anonymized representative real-document fixtures.
- Record cold startup, first frame, bootstrap, first OCR, operation p50/p95, peak memory, raw binary size, and installed package size on Windows and macOS.

### Phase 1 — Secure and accelerate the boundary

- Put sidecar supervision and JSON-lines parsing behind Rust.
- Replace broad frontend shell permissions with closed Tauri commands.
- Move bootstrap, settings, auth, credentials, Supabase, Gemini, and cloud metadata to Rust.
- Correct cloud undo and RLS.
- Start Python lazily only for unmigrated operations.

### Phase 2 — Rust domain foundation

- Exact Serde DTOs and generated TypeScript bindings.
- Ordered operation registry and schema-generated forms.
- Immutable one-shot plan store with provenance and digest.
- Request-scoped progress and cancellation.
- Global mutation semaphore.
- Operation-scoped path grants.
- Atomic no-replace filesystem primitives.
- Durable write-ahead journal, crash recovery, exact legacy readers, and hash-protected undo.
- Fault-injection and parity harnesses.

This phase has no production-file authority until sandbox evidence passes.

### Phase 3 — Low-risk functionality

Migrate RFI matching, naming, deterministic planning, preferences, local-history readers, review planning, print-queue planning/recovery, and offline email parsing. Python and Rust previews must agree except for explicitly normalized volatile IDs and timestamps.

### Phase 4 — Controlled mutation canary

Exercise Rust mutation in an isolated sandbox, including destination races, cancellation, crash points, locked files, and undo after later edits. Enable Process Scans copy mode before move mode. Require zero tree, plan, journal, and undo mismatches before expanding authority.

### Phase 5 — OCR and Process Scans

Invoke the bundled Tesseract engine from Rust and preserve the existing rendering/preprocessing/retry contract. Retain the searchable-text fast path. Require at least 95% representative accuracy and zero false RFI labels in the release corpus.

### Phase 6 — Excel/OOXML, RFI Log, email, and clipboard

Port the targeted ZIP/XML patcher, preserving untouched package members exactly. Validate formulas, cached values, data validation, styles, relationships, extensions, and calculation metadata. Preserve deterministic offline email extraction and keep optional Gemini calls behind the Rust network boundary. Rust may supervise the existing Excel COM/PowerShell adapter until direct Rust COM proves more reliable.

### Phase 7 — Signed-page PDF staging

Preserve immutable archive inputs, page geometry, rotation, crop boxes, metadata, attachments, TOC, labels, links, annotations, widgets, encryption, and page count. Revalidate input hashes during apply and publish only validated, fsynced, no-clobber output.

### Phase 8 — Batch printing

Port queue persistence before irreversible printer submission. Use Windows Win32/Winspool/GDI and preserve uncertain-job semantics. Test Microsoft Print to PDF and representative physical/network printers. Never automatically retry an uncertain or partially printed job.

### Phase 9 — Frontend hardening and stabilization

Finish the typed command boundary, path-grant flows, channel-based progress and cancellation, and recovery UI. Run one complete Rust-default stabilization release before removing the fallback.

### Phase 10 — Python and sidecar removal

After one complete Rust-default stabilization release:

- Remove sidecar routing, PyInstaller, Python runtime dependencies, Python application code, and Python tests.
- Port remaining Python repository tools to Rust `xtask` or existing Node tooling.
- Remove Python manifests, sidecar specification/configuration, shell plugin/ACLs, and generated Python artifacts.
- Inspect packaged files and SBOM to prove Python is absent.
- Run clean Windows/macOS install, rollback, and previous-data compatibility tests.

## UI contract

- Preserve the monochrome identity and current information hierarchy.
- Continue generating forms only from `ParamSchema`.
- Keep every user-visible string in Turkish, English, Russian, and Azerbaijani.
- Make source, destination, warnings, collisions, and irreversible effects explicit in preview.
- Use channel-based real progress and cancellation.
- Distinguish completed, safely retryable, partially committed, unknown, and manual-recovery states.
- Provide a recovery center for pending journals and uncertain print jobs.
- Keep migration diagnostics out of normal workflows.
- Test keyboard navigation, focus, labels, scaling, contrast, and screen-reader behavior.
- Do not add operation-specific forms, arbitrary path fields, or frontend domain logic.

## Performance contract

- Normal startup does not start Python or load OCR/PDF engines.
- Heavy adapters load lazily and are reused.
- Blocking filesystem, OCR, PDF, Office, network, and printer work stays off the UI thread.
- Files stream or memory-map where safe; avoid repeated full-file copies and conversions.
- OCR concurrency is bounded by measured CPU and memory behavior.
- Cancellation is checked between expensive stages and before every mutation.
- Progress uses Tauri channels rather than high-frequency polling.
- Large results use summaries and pagination.

Initial budgets:

- Time to interactive: no more than 2 seconds on agreed Windows and Mac reference machines.
- No normal-navigation main-thread task longer than 50 ms.
- No migrated operation p95 regression greater than 5% without an approved fidelity/reliability reason.
- No peak-memory regression for equivalent input.
- The final equivalent installed package is smaller and does not include the measured 92,977,856-byte arm64 sidecar.

Accuracy, no-clobber behavior, journal durability, and format fidelity override speed.

## Failure model

Every backend error includes a stable code, locale key, operation/request ID, effect state, and safe next action:

- `validation`: nothing happened; correct input.
- `no_effect_retryable`: nothing happened; retry is safe.
- `partial_committed`: journaled effects occurred; enter recovery.
- `outcome_unknown`: never retry automatically.
- `cancelled_clean`: no mutation occurred.
- `cancelled_partial`: recover from the journal.
- `auth_required`, `offline`, `permission_denied`: remain distinct from document failures.

Secrets and document contents are redacted from diagnostics. Telemetry is not uploaded by default.

## Verification

Each behavioral slice uses RED → GREEN → REFACTOR and requires:

1. Rust unit/property tests.
2. Exact JSON/Serde/TypeScript snapshots.
3. Python↔Rust differential preview tests.
4. Isolated apply/tree/journal/undo comparison.
5. Crash/fault injection at mutation boundaries.
6. Symlink, traversal, race, forged-ID, malformed-cloud-row, and secret-storage tests.
7. Representative PDF/OOXML/OCR corpus tests.
8. Frontend accessibility, localization, and visual regression tests.
9. Windows and macOS packaged clean-machine tests.
10. Real Office and physical-printer certification where applicable.

Final gates include formatting, Clippy with warnings denied, Rust tests, frontend tests/build, Tauri checks, dependency/license audit, SBOM inspection, installer checksums, rollback installation, previous-data compatibility, and package-content proof that Python is absent.

A phase is complete only when its stated evidence exists on both required platforms.

## GPT/Codex operating standard

- Root `AGENTS.md` is the canonical repository instruction file.
- Nested `AGENTS.md` files carry only subsystem-specific overrides.
- Detailed reasoning remains in canonical documents; agent instructions stay concise and operational.
- Implementation uses small vertical acceptance slices, TDD, constrained workspaces, independent diff review, and exact verification evidence.
- Parallel Codex agents use isolated worktrees and do not share mutation scope.
- Agent completion claims are never accepted without local diff and test verification.
- The repository does not pin a particular GPT version; it targets stable Codex instruction-discovery and execution conventions.

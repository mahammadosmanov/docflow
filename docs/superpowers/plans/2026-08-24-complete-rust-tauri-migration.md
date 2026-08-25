# Complete Rust/Tauri Migration Execution Plan

**Status:** Authorized for unattended execution on 2026-08-24

**Governing design:** `docs/superpowers/specs/2026-08-24-rust-engine-migration-design.md`

**User outcome:** DocFlow ships as a Tauri 2 application with a thin React/Vite webview, a Rust backend, no Python interpreter or PyInstaller sidecar, preserved workflows and safety invariants, and platform-specific macOS/Windows build evidence.

## Baseline evidence

Measured on the macOS arm64 development host before the Rust authority migration:

- packaged Python sidecar: `92,979,744` bytes
- cold `dist/docflow-sidecar --ping`: `38.47` seconds wall time
- Python tests: 145 passing after Phase 0
- frontend tests: 9 passing after Phase 0
- Rust shell: formatting, Clippy, tests, and check passing after Phase 0

The final package must not contain the sidecar or any Python runtime/library.

## Global execution rules

- Preserve all existing uncommitted Phase 0 changes.
- Do not commit, push, release, publish, deploy Supabase SQL, or rewrite history.
- Every mutation remains preview → explicit approval → apply → durable journal → undo.
- Apply consumes a server-cached immutable plan ID and never accepts client-authored steps.
- Existing destinations are never overwritten.
- Cloud history remains metadata-only; local validated journals are the only undo authority.
- React owns presentation and transient state only.
- All user-visible text remains present in `tr`, `en`, `ru`, and `az`.
- Forms remain generated from `ParamSchema`.
- Keep NFC + uppercase RFI canonicalization and the exact `-RFI-` anchor.
- Preserve existing settings, journal, trash, queue, operation IDs, DTO casing/nullability, and paths unless the accepted credential security migration requires removing token fields.
- Windows-only claims require Windows evidence. macOS cannot certify Winspool/GDI, Excel COM, NSIS execution, or physical printers.

## Slice 1 — Rust-owned transitional boundary

### Behavior

- React invokes Rust commands and has no shell/process authority.
- Rust lazily supervises the existing JSON-lines sidecar only for unmigrated methods.
- The Rust allowlist rejects unknown methods before process startup.
- Rust routes progress and errors to the originating request and redacts stderr.
- Child shutdown/crash fails all pending requests deterministically.

### Files

- `src-tauri/src/backend/*`
- `src-tauri/src/lib.rs`
- `src-tauri/Cargo.toml`
- `src-tauri/capabilities/default.json`
- `frontend/src/lib/bridge.ts`
- `package.json`

### Gates

- Rust parser/routing/allowlist/lazy-start tests
- frontend invoke-bridge tests
- no plugin-shell import or frontend shell permission
- Rust and frontend full gates

## Slice 2 — Rust contracts and state foundation

### Behavior

- Exact Serde DTOs reproduce protocol-1 shapes, omission/nullability, parameter order, and stable codes.
- Registry exposes the four operations in canonical order.
- Plan store records origin, digest, creation time, and one-shot apply state.
- Global mutation lock prevents concurrent apply.
- Request cancellation is observable between stages and before effects.
- Settings use atomic writes and filter persisted operation params.

### Modules

- `contracts.rs`
- `registry.rs`
- `settings.rs`
- `plan_store.rs`
- `progress.rs`
- `errors.rs`

### Gates

- JSON snapshots against existing Python protocol fixtures
- forged/expired/reused plan IDs rejected
- concurrent apply rejected
- settings compatibility fixtures and atomic-write tests

## Slice 3 — Rust authentication, credentials, and cloud metadata

### Behavior

- Supabase auth/PostgREST calls run in Rust.
- Access/refresh tokens never enter React, logs, or plain settings after verified migration.
- macOS Keychain and Windows Credential Manager are the secure stores.
- Legacy settings tokens are imported once, read back, then removed atomically.
- Failed secure-store migration leaves the legacy source intact and signs out safely.
- Team audit reads remain; cloud rows never become executable.

### Modules

- `http.rs`
- `credentials.rs`
- `auth.rs`
- `cloud.rs`

### Gates

- fake-server request/response tests
- secret-redaction tests
- legacy credential migration/failure tests
- owner-only cloud mutation contract tests

## Slice 4 — Rust filesystem/journal/undo authority

### Behavior

- request-scoped path grants bind user-selected roots to a request/operation
- canonicalization, symlink, containment, traversal, and limit checks run in Rust
- no-replace move/copy/write primitives handle destination races
- a pending journal is atomically and durably written before every effect
- successful effects gain hashes and committed journal state
- undo runs reverse order, protects later edits, and removes only empty created directories
- legacy journals remain readable
- review delete remains a move to `~/.docflow/trash/`

### Modules

- `paths.rs`
- `fs_ops.rs`
- `journal.rs`
- `undo.rs`
- `review.rs`

### Gates

- port every `tests/test_apply.py` invariant
- traversal/symlink/race/fault-injection tests
- crash-state and legacy-journal fixtures
- isolated apply/tree/journal/undo comparisons

## Slice 5 — Deterministic RFI logic and Process Scans

### Behavior

- exact strict/repaired/fuzzy RFI matcher parity
- deterministic naming and duplicate/revision selection
- searchable PDF text fast path
- first-page upper-band rendering/preprocessing and bounded Tesseract CLI execution
- broken/missing OCR fails closed without moving a whole folder to review
- move and copy plans run only through Rust journaled primitives

### Modules

- `rfi_code.rs`
- `naming.rs`
- `ocr.rs`
- `operations/process_scans.rs`

### Gates

- port RFI/naming/OCR tests and corpus fixtures
- real image-only PDF smoke
- no false RFI labels in representative corpus
- cancellation before every mutation

## Slice 6 — RFI log, OOXML, email, and clipboard

### Behavior

- deterministic offline email extraction and optional Rust Gemini client
- exact workbook lookup/layout/row allocation and editable preview behavior
- targeted ZIP/XML patching preserves untouched package members and namespaces
- formulas/cached values, data validation, calc chain, styles, relationships, and extensions remain valid
- apply writes a backup first, patches a temporary workbook, verifies, then atomically replaces
- undo refuses to replace a workbook changed after apply
- plan table preview/copy remains available; Windows Excel-native copy may use a Rust-managed COM adapter

### Modules

- `email_extract.rs`
- `ooxml.rs`
- `operations/rfi_log.rs`
- `clipboard.rs`

### Gates

- port email, OOXML, RFI log, and clipboard fixtures/tests
- byte comparison for untouched ZIP entries
- Office open/recalculate validation on Windows
- workbook lock and rollback tests

## Slice 7 — Signed-page PDF staging

### Behavior

- scan/archive/review roots must be non-overlapping
- archive inputs remain immutable
- signed page one replaces archive page one while preserving all other pages and document navigation/metadata structures required by canon
- archive hashes are revalidated during apply
- output is written to a same-directory temporary file, validated, fsynced, and published without replacement
- partial output and stale temp cleanup are narrowly scoped

### Modules

- `pdf.rs`
- `operations/stage_signed_pages.rs`

### Gates

- port every signed-page test
- preserve page geometry, rotation, crop boxes, page count, TOC, labels, links, annotations, widgets, attachments, metadata, and encryption behavior
- destination-race/OneDrive-lock tests
- representative real-document corpus

## Slice 8 — Batch printing and queue recovery

### Behavior

- queue state is durable before printer submission
- ordered/collated/per-file settings and revision behavior remain exact
- Windows uses Winspool/GDI or an equally controlled native adapter
- known, failed, pending, cancelled, and uncertain jobs remain distinct
- uncertain/partially submitted jobs are never retried automatically
- macOS provides a controlled native print adapter or an explicit unsupported state without losing queue safety

### Modules

- `print_queue.rs`
- `platform/windows/printing.rs`
- `platform/macos/printing.rs`
- `operations/batch_print.rs`

### Gates

- port print planning/queue tests
- Windows compile and fake-spooler tests
- Microsoft Print to PDF certification
- representative physical/network printer certification remains a hardware gate

## Slice 9 — Frontend hardening and recovery UI

### Behavior

- all backend calls are typed/allowlisted Rust invokes
- dialogs issue opaque path grants rather than passing arbitrary filesystem authority
- progress and cancellation use channels/events, not polling
- recovery center exposes pending journals and uncertain print queues
- statuses distinguish no effect, partial committed, unknown, clean cancellation, and partial cancellation
- no backend/migration mechanics appear in normal workflows

### Gates

- component/accessibility/localization tests
- identical locale key sets
- no shell/fs/network/token authority in capability files or frontend dependencies

## Slice 10 — Python and sidecar removal

### Removal set

- `src/docflow/`
- Python application tests after equivalent Rust tests exist
- `pyproject.toml`, runtime Python requirements, PyInstaller spec and build script
- sidecar binaries/configuration and `externalBin`
- frontend and Rust shell-plugin dependencies/ACLs
- Python-based version/release helpers only after an equivalent Rust `xtask` or Node command is verified
- generated/stale build artifacts containing Python

### Proof

- repository search finds no active runtime Python/PyInstaller/sidecar references
- package/SBOM inspection finds no Python interpreter, stdlib, extension modules, OpenCV Python wheels, PyMuPDF Python package, or sidecar
- startup and package size improve from the recorded baseline

## Final verification matrix

### Host-independent

- version contract replacement
- frontend tests/build
- Rust format
- Rust Clippy with warnings denied
- Rust unit/integration/property tests
- dependency/license audit
- localization key equality
- package-content/SBOM scan
- no-clobber, forged-ID, malformed-cloud-row, secret-redaction, and compatibility fixtures

### macOS host

- release Tauri build (`app` and `dmg` where tooling permits)
- launch/smoke bootstrap, settings, auth boundary, preview/apply/undo sandbox, OCR corpus, PDF/OOXML corpus
- Keychain migration/readback test
- package size/startup measurement
- package proof that Python is absent

### Windows host or trusted Windows CI

- Rust/frontend full gates
- Credential Manager migration/readback
- Winspool/GDI fake and real certification
- Excel/Office open/recalculate and clipboard certification
- image-only OCR smoke with bundled Tesseract
- NSIS build/install/uninstall/rollback/previous-data tests
- PE GUI subsystem verification
- package proof that Python is absent

A Windows installer produced without execution on Windows is not release evidence. If no Windows runner can consume the uncommitted tree, the repository may be made Windows-ready, but the final Windows artifact remains honestly blocked until the same exact tree runs on Windows.

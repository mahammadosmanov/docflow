# Architecture

## Runtime authority

```text
React + TypeScript presentation
        |
closed typed Tauri invoke/channel API
        |
Rust application authority
        |
contracts + auth/cloud + plans + journals + document operations
        |
Tesseract/PDF/Office/OS native adapters
```

React owns presentation and transient screen state only. Rust/Tauri owns the
trusted command boundary and is the sole authority for authentication, operation
schemas, plans, mutations, journals, cloud metadata, OCR, document processing,
Office integration, and printing.

Tesseract, PDF engines, Office, and operating-system print services remain native
dependencies where they are more reliable than reimplementation. Heavy adapters
load only when a requested operation needs them. The application contains no
embedded interpreter, secondary application runtime, or child-process IPC router.

Build-time and repository source audits fail if retired sidecar configuration,
shell-process authority, or removed runtime paths reappear. Package audits reject
interpreter executables/libraries, application sidecars, bytecode, and frozen-runtime
artifacts; macOS package audits also inspect Mach-O dynamic links.

## Security boundary

- The webview receives no Supabase token and has no arbitrary filesystem, network,
  shell, or process authority.
- Tauri dialogs issue opaque operation-scoped path grants backed by pinned directory
  capabilities; Rust resolves and mutates beneath them without following symlinks or
  Windows reparse points.
- Access and refresh tokens live in Windows Credential Manager or macOS Keychain
  after verified one-time import from legacy settings.
- Cloud history is audit metadata only. Cloud/client steps and paths are never
  executable; validated local journals are the sole undo authority.
- Rust commands enforce authorization in addition to Supabase RLS.
- No localhost server, eval, remote script, or service-role key is used.

## Operation contract

```text
Operation.schema -> ParamSchema
Operation.preview(validated params + path grants) -> cached immutable Plan
Operation.apply(planId) -> journaled Result
Operation.undo(localJournalId) -> journaled Result
```

React renders `ParamSchema`; there is no operation-specific form. Apply accepts an
opaque plan identity, not client-authored steps. The DTO shapes remain compatible
through Serde/TypeScript golden fixtures.

## Safety invariants

- Preview never mutates filesystem, cloud, Office, or printer state.
- Apply executes only the cached approved plan; it never recalculates.
- Every step is durably journaled as pending before execution.
- Existing destinations are never overwritten, including race-time collisions.
- Undo runs successful steps in reverse, protects later edits with hashes, and
  removes only empty directories created by the operation.
- Review “delete” moves to recoverable application trash.
- Missing/broken/ambiguous OCR produces a zero-step or review plan, never a guessed label.

At grant issuance Rust opens and pins the authorized directory. Apply, publication,
replacement, removal, and undo then walk existing ancestors without following Unix
symlinks or Windows reparse points and perform each effect relative to pinned parent
handles. No-clobber rename/publication is handle-relative as well. Replacing the
visible root or parent pathname after capability acquisition therefore cannot
redirect an effect outside the originally opened directory; the effect remains on
the pinned directory or fails closed.

## Persistent compatibility

- settings: `~/.docflow/settings.json`
- journals: `~/.docflow/journal/`
- recoverable trash: `~/.docflow/trash/`
- print queue: `~/.docflow/print_queue.json`
- protocol-1 DTO casing/nullability and operation IDs: unchanged
- Supabase `app_users` and `history` schema: compatible, with history steps inert

The explicit compatibility exception is credential storage: legacy plaintext
tokens are imported once into the OS credential store and removed after verified
readback. A downgrade requires sign-in again.

The completed migration rationale and historical phase gates are in
`docs/superpowers/specs/2026-08-24-rust-engine-migration-design.md`.

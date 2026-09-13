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

Tesseract, PDF engines, Office, Outlook desktop COM, and operating-system print services remain native
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

Support diagnostics are collected through a closed, read-only Rust command. The
fixed bundle contains only the DocFlow version, platform, retired-sidecar state,
pathless OCR availability, and at most 20 stable recent error codes. Recursive
denylist, content, path, depth, item, and byte bounds apply before the deterministic
JSON crosses into the webview. Collection does not scan user files or send data;
copying the validated preview to the local clipboard is a separate explicit action.

Device readiness is a separate authenticated, read-only Rust query. It reports
only bounded OCR availability, enumerated-printer count, and pending local
recovery count. It neither scans user folders nor treats a healthy device result
as workbook, OneDrive, Office, or printer-output certification; those checks stay
inside operation preview and platform release gates.

RFI work tracking stores bounded, owner-scoped, append-only JSON events beneath
the application-data directory. Event creation and manual stage transitions use
the same cached Plan, explicit approval, no-clobber write, journal, and undo
boundary as document operations. The home queues are a rebuildable read-only
projection. A stale transition creates a visible review conflict and never
silently replaces the last proven stage. These coordination records cannot
authorize an Excel, PDF, email, print, or replica mutation.

Eligible document journals can authorize only a separate work-item transition
preview. The validator admits committed same-owner RFI Log, successfully filed
Process Scans, or two-workbook System Sync evidence with exact canonical RFI
coverage. The transition remains an independently approved, journaled,
undoable no-clobber write. Journal disappearance, undo, partial state, wrong
operation, wrong stage, or identity mismatch fails closed into rejection or
Needs Review; Outlook draft journals are never lifecycle evidence.

RFI work audit export reads the same bounded owner-scoped event snapshot and
prepares both lossless JSON and Excel-safe CSV outputs. The selected destination
is capability-bound, filenames are unique, and both writes use the ordinary
preview, approval, no-clobber journal, and undo path. Export never prunes or
modifies the append-only store. Queue DTOs expose event count, owned valid-event
bytes, and the fixed limit so operators can act before the store is full.

Signed-page staging performs bounded page-level OCR when a Worley scan contains
multiple RFI first pages. Preview records the exact zero-based source page index,
whole-file fingerprint, archive candidate, and destination in the immutable
Plan. Apply revalidates the source, copies it into private operation storage, and
imports only that approved page into the review copy. It never splits or mutates
the source scan. Missing, low-confidence, duplicate, unsupported, or ambiguous
pages remain warnings and cannot create an output.

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

The shared Plan view derives a compact effect summary only from the immutable
steps already returned by Rust. It counts created outputs, existing-file updates,
moves/renames, print submissions, Outlook drafts, cleanup effects, and warnings.
This is presentation only: it cannot add, remove, or authorize a step, and it is
shown for both record-editor and ordinary previews.

## Safety invariants

- Preview never mutates filesystem, cloud, Office, or printer state.
- Diagnostics preview is local and side-effect-free; there is no telemetry or
  hosted logging path.
- Outlook reply apply revalidates the exact selected MailItem, calls ReplyAll, prepends bounded escaped content, and displays only an unsent draft; it never sends or synthesizes recipients.
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
- RFI work events: `~/.docflow/work-items/events/`
- protocol-1 DTO casing/nullability and operation IDs: unchanged
- Supabase `app_users` and `history` schema: compatible, with history steps inert

Legacy print-queue fields remain readable, but queues written before source/root
filesystem identities were persisted are not automatically resumable. The UI presents a
localized discard-and-reselect migration; trusting their stored path strings would violate
the capability boundary.

The explicit compatibility exception is credential storage: legacy plaintext
tokens are imported once into the OS credential store and removed after verified
readback. A downgrade requires sign-in again.

The completed migration rationale and historical phase gates are in
`docs/superpowers/specs/2026-08-24-rust-engine-migration-design.md`.

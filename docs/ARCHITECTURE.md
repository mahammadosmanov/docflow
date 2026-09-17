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
pathless OCR availability, at most 20 stable recent error codes, and a bounded
pathless snapshot of the latest owned print queue. Recursive
denylist, content, path, depth, item, and byte bounds apply before the deterministic
JSON crosses into the webview. Collection does not scan user files or send data;
copying the validated preview to the local clipboard is a separate explicit action.

Signed-page staging performs bounded page-level OCR when a Worley scan contains
multiple RFI first pages. Preview records the exact zero-based source page index,
whole-file fingerprint, archive candidate, and destination in the immutable
Plan. Apply revalidates the source, copies it into private operation storage, and
imports only that approved page into the review copy. It never splits or mutates
the source scan. Missing, low-confidence, duplicate, unsupported, or ambiguous
pages cannot create a replacement output. Each affected signed source is instead
represented once by a hash-pinned no-clobber Copy step under
`<review root>/_review`, with structured source/page/RFI/reason metadata for
operator review. A multi-page source with several warnings is copied once, not
split or duplicated.

Archive discovery for signed-page staging performs a breadth-by-depth walk of
the complete selected RFI System root. Independent directories at each depth are
enumerated in the bounded Rayon pool, then merged and sorted on the coordinator
thread. This parallelizes network/OneDrive metadata reads without allowing
parallel mutation or nondeterministic candidate order. The Plan records the
maximum worker count and exact directory/entry totals. A single unreadable
directory rejects the whole archive result rather than producing false misses.

For up to 20 accepted RFI rows, Rust may also render the exact signed page and
archive-candidate page one as grayscale PNG thumbnails. Each image is rendered at
144 DPI, reduced to at most 540 by 760 pixels, and rejected above 768 KiB; total
encoded Plan thumbnail data is capped at 12 MiB. The images are inert metadata
keyed by row and opaque candidate token,
not executable steps or filesystem capabilities. React accepts only bounded PNG
data URLs, and selecting an archive candidate displays the image bound to the
same token used by Plan revision. Missing support or exhausted bounds require
manual PDF review and never expand or alter Apply authority.

PDF orientation normalization is a separate Rust operation. Discovery is
restricted to CIV/MEC/ELE/PRC directory trees. A bounded Rayon pool renders and
classifies independent pages with bundled Tesseract OSD; no filesystem mutation
occurs in that phase. The immutable Plan contains exact rotations and hashes.
Apply stays single-threaded to preserve journal order, appends only page
dictionary revisions in private storage, validates structural fidelity, and
publishes review copies with capability-relative no-clobber writes.

## Durable incremental analysis

`scan_index.rs` owns a bundled-SQLite, WAL-backed local index. Its keys include
owner, canonical root, operation ID, algorithm profile, relative file path, and
a platform filesystem fingerprint. `scan_runs` records complete/interrupted
runs, `scan_entries` holds the latest reusable result, and `scan_events` records
new analyses and failures. Each completed PDF is committed independently, so a
later run can resume after application, device, or network interruption.

The index is intentionally outside the mutation authority boundary. It can skip
expensive OCR/rendering during Preview, but it cannot produce an Apply permit,
overwrite a destination, update a workbook, or replace Plan hash/identity
checks. Supabase remains metadata history only; large-system paths and cached
document analysis are local-only.

The same bounded analysis pool is used by native Process Scans OCR, incoming
RFI file name/content inspection, combined
Worley page recognition, signed-input and archive discovery, RFI System
discovery and first-page signature detection, PDF-orientation discovery and
page analysis. Completed indexed PDF analyses
return to one coordinator, which writes each SQLite checkpoint immediately.
The pool preserves deterministic input/result order and is never used for
Office automation, print submission, publication, journals, or Undo, where
parallel effects could create collisions or unknowable outcomes.

Every registered operation emits localized progress phases through the existing
Tauri channel. Long batch phases include bounded completed/total counters and a
current file, folder, page, workbook, printer, or record subject when available.
Progress is presentation evidence only and cannot change Plan authority.

Incoming RFI file grouping is a native Rust operation. Discovery is top-level
and deterministic; independent file inspection is bounded and parallel. The
cached Plan contains the exact source/destination copies. Apply performs those
copies serially through the shared filesystem authority and journal, so parallel
analysis cannot create races or bypass no-clobber checks.

RFI completion-status synchronization is a separate native Rust operation.
It performs a breadth-by-depth bounded parallel walk of the selected RFI System,
accepts only exact canonical `RFI No. <RFI>.pdf` filenames, and never opens or
mutates those PDFs. The coordinator deterministically groups duplicate paths by
RFI and maps them to existing literal `Inspection Status` cells. Apply reuses
the guarded System Sync workbook replacement path, but its typed step kind
permits only `Completed` replacements and has no Follow Up or Worley authority.

RFI System replica publication is one-way and source-authoritative. Preview
fully scans only CIV/MEC/ELE/PRC plus explicitly named root files, rejects
symlinks and overlapping roots, and hashes independent reads in the bounded
worker pool. Apply remains serial and fail-fast. Every replacement first creates
and verifies a retained no-clobber backup, stages a private sibling copy, then
atomically publishes and verifies the approved source hash. Target-only files
are warnings and are never deleted.

The first unattended replica run is still an ordinary explicit Preview and
Apply. A successful Apply stores the exact profile as revocable standing
authorization. While DocFlow is open, a native scheduler creates a fresh cached
Plan at the configured interval and uses the same grants, preflight, backup,
journal, and Undo authority. Any unsafe discovery or Apply result disables the
profile. Windows startup is a separate per-user HKCU Run setting; it grants no
additional filesystem authority.

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

OCR-derived steps carry their backend score in immutable step metadata, signed
page editor rows expose the same evidence through the generic percent display,
and manual Review DTOs include the stored score. React only rounds and bounds
the visible 0–100 label. Plan content is the scrolling region while the action
bar remains reachable outside it; record tables retain independent horizontal
scrolling.

Record-editor previews also expose the same immutable non-directory steps in a
collapsed, height-bounded exact-effects panel. Expanding it renders complete
source and destination paths already present in the Plan; it does not request,
recompute, or mutate backend state.

Warnings use one shared presentation for ordinary and record-editor plans. Five
or more warnings gain local text and type filters while the complete immutable
warning array remains untouched. In signed-page staging, a focus action is shown
only when the warning subject contains an exact bounded ASCII RFI code that also
exists in the current editor rows. It selects the already-rendered comparison;
it cannot revise the Plan, choose an archive candidate, or authorize Apply.

Signed-page Preview streams separate discovery, inspection, OCR, classification,
archive-scan, comparison-preparation, and ready phases. React retains a bounded
ordered phase trail rather than replacing the whole progress display with only
the latest event. The signed-page Plan view puts approved rows first, keeps a
sticky navigator to comparisons/review copies/warnings, and renders structured
warning-copy metadata already present in the immutable Plan.

Guarded RFI System and Follow Up workbook steps already carry the expected and
replacement values that Apply validates. The shared workbook comparison projects
only those exact approved step fields into a collapsed sheet/cell table; it does
not reopen Excel, parse the workbook, or calculate a new update. Coordinates and
text are bounded and structurally validated in the presentation layer, and at
most 1,000 approved cells are displayed. Filtering editor rows changes the step
metadata first, so a revised Plan cannot display a disabled cell as executable.

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
- retired RFI work events: `~/.docflow/work-items/events/` (preserved, inactive)
- scan checkpoints and audit: `~/.docflow/scan-index.sqlite3`
- protocol-1 DTO casing/nullability remains compatible; retired operation IDs
  and commands are rejected
- Supabase `app_users` and `history` schema: compatible, with history steps inert

Legacy print-queue fields remain readable, but queues written before source/root
filesystem identities were persisted are not automatically resumable. The UI presents a
localized discard-and-reselect migration; trusting their stored path strings would violate
the capability boundary.

Windows controlled printing considers a successful `EndDoc` followed by normal
spool-record disappearance a completed handoff because many physical drivers
purge completed entries before the first poll. Deleting/deleted states are
uncertain and non-retryable; generic driver error, paper-out, offline, blocked,
and user-intervention states keep polling within
the approved timeout, and transient observation failures receive only a bounded
read retry, never another print submission. Batch Print's remembered scalar
values are opt-in at the UI; selected files never enter remembered settings.

For native file-association handlers, successful `ShellExecute` is the last
available submission boundary when the application or driver never exposes a
uniquely identifiable spool record. That job becomes `completed_unconfirmed`:
the accepted submission and warning are journaled, no retry is permitted, and
the serial queue advances. Destructive spool deletion and uncertain cancellation do
not use this path and still pause for operator review.
The handler's spool job is matched by title when possible; if it substitutes an
application title, exactly one new post-submission job ID is accepted. Multiple
unmatched new jobs remain ambiguous. Queue enumeration receives the same bounded
observation grace used by job reads.

The same completed-handoff rule applies when a fast-purging driver releases the
job between `GetJob`'s size probe and buffered read. Buffer-growth races retry
the read without resubmission; post-acceptance printer-open, enumeration, or
read loss is completed-unconfirmed, while observed deletion remains uncertain.

The explicit compatibility exception is credential storage: legacy plaintext
tokens are imported once into the OS credential store and removed after verified
readback. A downgrade requires sign-in again. Passive macOS startup reads set
the Security framework's skip-authentication-UI query policy and hold its scoped
legacy-keychain interaction-disable guard: an inaccessible item fails closed to
signed out without deletion instead of blocking the splash screen on an
off-focus Keychain prompt. Dropping the guard restores the process interaction
policy. Explicit sign-in and sign-out remain the only user-driven credential
write/delete paths.

The completed migration rationale and historical phase gates are in
`docs/superpowers/specs/2026-08-24-rust-engine-migration-design.md`.

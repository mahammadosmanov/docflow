# Product requirements

`PRODUCT.md` is the living product definition. This file gives its stable
requirement IDs so code, tests, decisions, and releases can be traced.

## Core requirements

- **DF-001 — Preview first:** every document, workbook, queue, or draft
  mutation is previewed and explicitly approved. DF-025 permits later
  identical-profile replica runs under one revocable standing authorization.
- **DF-002 — Immutable apply:** Apply consumes only the cached approved Plan; it
  does not rediscover files, rows, pages, recipients, or print settings.
- **DF-003 — No clobber:** an existing output is never overwritten. Updates to
  approved workbooks require an exact backup and guarded replacement protocol.
- **DF-004 — Durable recovery:** effects are write-ahead journaled; interruption,
  uncertainty, undo, and retained artifacts remain operator-visible.
- **DF-005 — Exact RFI identity:** persisted RFI codes are exact uppercase ASCII
  `AZE-SUN-INT-QMS-RFI-[A-Z]{3}-[0-9]{4}` values. Ambiguity fails closed.
- **DF-006 — Workbook fidelity:** only approved literal cells may change;
  formulas, macros, relationships, styles, package members, and unrelated cells
  remain intact or the operation is rejected.
- **DF-007 — Read-only archives:** signed-page source scans and the selected RFI
  archive are never modified; results are created beneath a separate review root.
- **DF-008 — Strict print order:** jobs execute serially in approved order;
  post-submission uncertainty is never retried automatically. Transient generic
  driver faults remain under observation; destructive deletion is uncertain.
  A native handler may be bound to its single new spool job without relying on
  an application-controlled document title.
- **DF-009 — Least authority:** React has no arbitrary filesystem, process,
  network, credential, Office, or printer authority. Rust owns every effect.
- **DF-010 — Secret isolation:** access and refresh tokens stay in the native
  credential store and Rust process; service-role secrets are forbidden.
- **DF-011 — Four languages:** all operator strings exist in Turkish, English,
  Russian, and Azerbaijani with equal key sets.
- **DF-012 — Schema-driven UI:** operations use the generic parameter/editor
  contract; operation-specific forms are forbidden.
- **DF-013 — Honest evidence:** local and synthetic success never implies real
  Windows, Office, OneDrive, printer, live-service, signing, or notarization acceptance.
- **DF-014 — Release identity:** `VERSION`, manifests, operator/AI documentation,
  changelog, release catalog, package, checksum, and tag must describe one version.
- **DF-015 — Safe page orientation:** every page below the RFI system's CIV,
  MEC, ELE, and PRC trees is classified from rendered content. Only complete,
  high-confidence PDF decisions may create no-clobber corrected copies under a
  separate review root; the source tree remains read-only.
- **DF-016 — Exact Follow Up projection:** existing CIV/MEC Follow Up sections
  receive only uniquely mapped RFI Log records. Shared-cell status is derived
  from every remaining RFI; Cancelled/Rejected entries are removed; FINAL
  Worley state follows the exact REMARKS marker; excluded sheets never change.
- **DF-017 — Resilient print handoff:** a complete Windows spool submission is
  not failed merely because a driver immediately releases its job record.
  A native-app request accepted by Windows but not exposed as an identifiable
  spool record is completed-unconfirmed, journaled, never retried, and does not
  pause later files. Recoverable device states wait within the approved timeout,
  explicit terminal faults and uncertain cancellation pause without auto-retry,
  and previous task files are never restored automatically. Safe scalar settings
  are reusable only through an explicit operator action.
- **DF-018 — Durable incremental scanning:** repeated large RFI-system analysis
  uses an owner-, root-, operation-, algorithm-, and file-fingerprint-scoped
  local index. Every newly completed file is checkpointed immediately; changed,
  new, failed, or explicitly forced files are analysed again. Cached evidence
  may reduce preview work but can never authorize Apply or replace apply-time
  identity/hash validation.
- **DF-019 — Visible OCR evidence and reachable UI:** OCR-backed preview rows,
  review items, and actionable page-orientation evidence expose a bounded score
  out of 100 without presenting it as a probability or automatic approval. Plan
  content, long warnings, compact editors, dialogs, and final actions remain
  reachable by scrolling at supported small viewport and display-scale bounds.
- **DF-020 — Complete parallel signed-page discovery:** signed-page staging
  searches every depth under the selected read-only RFI System root with a
  bounded multi-core directory walk, matches only exact canonical RFI PDF
  filenames, preserves deterministic results, and records scan evidence in the
  immutable Plan. New signed-scan input recursion defaults on. Symlinks,
  ambiguity, unsupported PDFs, collisions, and unreadable directories fail closed.
- **DF-021 — Actionable signed-page review:** signed-page Preview exposes live,
  ordered discovery/OCR/archive/comparison stages; keeps approved rows, page
  comparisons, warning files, and warnings directly reachable; and identifies
  every warning by source PDF, page when known, exact RFI when known, and reason.
  Apply copies each affected source PDF once beneath `_review`, preserving its
  signed-input subfolder path, without changing the source or overwriting a
  destination, and rejects source drift after Preview.
- **DF-022 — Bounded parallel analysis and visible stages:** independent,
  read-only bulk discovery and analysis use the shared bounded worker pool while
  deterministic results, incremental checkpoints, and source revalidation remain
  intact. Every registered operation reports localized Preview stages and item
  progress; mutation, print submission, publication, journal, and Undo effects
  remain ordered and single-writer.
- **DF-023 — Safe multi-RFI file grouping:** top-level incoming files are
  matched only to exact canonical RFI identifiers found in their names or
  bounded content inspection, including an exact code split by at most two line
  breaks around its hyphens. An RFI is eligible only when a readable EML or MSG
  file also identifies it; otherwise no folder or related copy is planned and a
  missing-email warning names the affected RFI and files. Each eligible source
  is copied into every related RFI folder beneath a separate destination; the
  source is never changed, collisions never overwrite, and unmatched or
  unreadable files are copied to `_review` for manual handling.
- **DF-024 — Exact completion-status synchronization:** a separate operation
  recursively scans the selected read-only RFI System for exact canonical
  `RFI No. <RFI>.pdf` filenames and previews every matched PDF, RFI Log row,
  current `Inspection Status`, and proposed `Completed` value. Apply updates
  only literal blank or `Ongoing` cells in existing CIV/MEC/ELE/PRC rows through
  the guarded workbook backup/delta/journal path. Missing rows/cells, formulas,
  merges, terminal or unknown statuses, malformed names, symlinks, drift, and
  incomplete discovery fail closed; Follow Up and Worley data never change.
- **DF-025 — Source-authoritative replica publication:** an operator selects
  one of three non-overlapping RFI System roots as source, two as targets, and a
  separate retained-backup root. Only CIV/MEC/ELE/PRC trees and explicitly named
  root files are in scope. Preview completes all three scans, hashes independent
  files in the bounded worker pool, exposes each copy/replacement, never deletes
  target extras, and refuses symlinks, overlap, drift, locks, incomplete scans,
  or unstable files. Apply is serial and fail-fast; every replacement is backed
  up and hash-verified before atomic publication, journaled, and undoable.
  Optional unattended mode requires one explicit Apply of the exact profile,
  runs only while DocFlow is open, and disables itself after any unsafe failure.
  Backups are never automatically deleted. Windows startup is per-user and needs
  no administrator permission.
- **DF-026 — Incremental replica comparison:** repeated replica previews reuse
  only owner-, root-, algorithm-, relative-path-, identity-, size-, and
  modification-time-matched SHA-256 results from the durable local scan index.
  Cache hits and fresh hashes remain visible. Every changed or unknown file is
  hashed again, completed hashes are checkpointed immediately, and Apply still
  revalidates the approved live hashes before any publication.
- **DF-027 — Pathless printer evidence:** the local support bundle includes the
  current printer inventory state and latest owned queue's formats, routes,
  options, attempts, outcomes, counts, and bounded error details. It excludes
  document names, document paths, contents, credentials, and tokens; recursive
  redaction and exact copy validation remain mandatory. This evidence diagnoses
  software handoff but never certifies that paper physically exited a printer.

## Scope

The eleven registered operations and application-level history, recovery, review,
settings, access control, and diagnostics surfaces are in scope. Paid
services are optional; no paid dependency is required for core local workflows.

## Release criteria

All source gates in `RELEASE_SAFETY.md` must pass after the final edit. A Windows
release additionally requires `WINDOWS_RUNBOOK.md`. Operator workbook tests use
copies only and never replace the production originals. Physical printer,
Office, and OneDrive acceptance requires the actual target environment.

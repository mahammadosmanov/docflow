# Product requirements

`PRODUCT.md` is the living product definition. This file gives its stable
requirement IDs so code, tests, decisions, and releases can be traced.

## Core requirements

- **DF-001 — Preview first:** every document, workbook, queue, or draft
  mutation is previewed and explicitly approved.
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
  post-submission uncertainty is never retried automatically.
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

## Scope

The nine registered operations and application-level history, recovery, review,
settings, access control, and diagnostics surfaces are in scope. Paid
services are optional; no paid dependency is required for core local workflows.

## Release criteria

All source gates in `RELEASE_SAFETY.md` must pass after the final edit. A Windows
release additionally requires `WINDOWS_RUNBOOK.md`. Operator workbook tests use
copies only and never replace the production originals. Physical printer,
Office, and OneDrive acceptance requires the actual target environment.

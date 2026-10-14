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
  `AZE-SUN-INT-QMS-RFI-[A-Z]{3}-[0-9]{4}` values. Every processing feature
  accepting an RFI candidate normalizes one-to-four ASCII digits only after
  supported RFI context is established, pads to four digits, and rejects longer
  values rather than truncating them. Unrelated stored data is unchanged.
  Ambiguity fails closed.
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
- **DF-015 — Safe page orientation:** PDFs under the CIV, MEC, ELE, and PRC
  trees are classified from rendered content, never page dimensions. The
  standalone normalizer and Fast RFI Scan share canonical `/Rotate`
  normalization and render-relative target selection. A strict exact RFI ID in
  the top-sixth header may resolve otherwise-uncertain direction only when the
  same code and angle agree at 250 and 350 DPI; competing or conflicting
  evidence remains review-only. Preview pins source SHA-256 before inspect or
  detection, verifies it afterward, and requires matching hash plus cache
  profile v5 for reuse. Apply revalidates the planned hash and creates only
  no-clobber corrected copies under a separate review root.
- **DF-016 — Exact Follow Up projection:** supported CIV/MEC source records
  map only to existing prepared destinations. A supported-route record with
  no source context matching any prepared destination is outside the active
  template scope: omit it from Manual Review and leave existing workbook cells
  unchanged. Partial matches remain visible for review, and only independently
  confirmed eligible rows may synchronize. Multiple distinct exact CIV area
  tags may map to their respective distinct existing rows. Any CIV RFI with a
  `Tag No` beginning `PCS-` is outside Follow Up scope, is never added, and is
  removed from existing groups; this includes composite tags such as
  `PCS-19/S1,S2,S3`. For regular MEC and `Signed By Worley MEC`, map source
  `Tag No` to target `Block` and each distinct comma-separated `Area` value
  to its target `Section`; ignore `Inspection Location` on both workbooks.
  Each exact `(Block, Section)` pair maps only to its existing prepared row.
  No rows or sections are created. Duplicate pairs, ambiguous mappings,
  conflicting source values, or unrelated placements cannot authorize
  shared-cell status changes. FINAL destinations are reconciled independently
  by exact prepared row identity: an existing RFI in one destination does not
  suppress another, and partial coverage remains visible for manual review.
  A cached formula status is a no-op only when the RFI is already present and
  that value already matches the projected status. Shared-cell status changes
  only when every represented RFI has an unambiguous source state and exact
  prepared route. Cancelled/Rejected entries are removed; FINAL Worley state
  follows the exact REMARKS marker; excluded sheets never change.
- **DF-034 — Bounded S5 recovery capacity:** retained S5 recovery files are
  limited to 256 files and 8 GiB of logical charge across the workbook parent
  and `.docflow-backups`. Apply and Undo refuse before mutation when the
  projected bound would be exceeded.
- **DF-017 — Resilient print handoff:** a complete Windows spool submission is
  not failed merely because a driver immediately releases its job record.
  A native-app request accepted by Windows but not exposed as an identifiable
  spool record is completed-unconfirmed, journaled, never retried, and does not
  pause later files. Recoverable device states wait within the approved timeout,
  explicit terminal faults and uncertain cancellation pause without auto-retry,
  and previous task files are never restored automatically. Safe scalar settings
  are reusable only through an explicit operator action. Excel worksheet PDFs
  pass bounded private-byte envelope checks, then Windows' native PDF decoder
  must load them and report at least one page before `StartDoc`; the stricter
  mutation parser is not a print-compatibility gate.
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
  bounded multi-core directory walk. It accepts only high-confidence
  consensus-confirmed Strict or approved OcrAlias RFI identities; repaired,
  fuzzy, conflicting, or filename-only identity is not authority. Archive
  lookup requires the exact `RFI No. <full RFI>.pdf` basename (case-insensitive)
  with a one-to-four-digit suffix canonicalized to four digits. A Sketch PDF,
  DOCX, code-bearing folder/other filename, or duplicate canonical-equivalent
  archive candidates cannot authorize replacement. Results and scan evidence
  remain deterministic in the immutable Plan. New signed-scan input recursion
  defaults on. Symlinks, ambiguity, unsupported PDFs, collisions, and
  unreadable directories fail closed.
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
  matched only to exact canonical CIV/MEC/ELE/PRC identifiers found in their
  names or bounded content inspection, including an exact code split by at most
  two line breaks around its hyphens. An RFI is eligible only when a readable
  EML or MSG file also identifies it; otherwise no folder or related copy is
  planned and a missing-email warning names the affected RFI and files. Each
  eligible source is copied to
  `<destination>/<discipline>/<full-code>/<source filename>` for every related
  RFI; sources remain read-only, collisions never overwrite, and unmatched or
  unreadable files are copied to destination-root `_review` for manual handling.
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
  options, attempts, outcomes, counts, and bounded error details. Excel COM
  failures include only the allowlisted member stage and HRESULT. It excludes
  document names, document paths, contents, credentials, and tokens; recursive
  redaction and exact copy validation remain mandatory. This evidence diagnoses
  software handoff but never certifies that paper physically exited a printer.
- **DF-028 — Guarded near-real-time replica publication:** an approved replica
  profile may check once per minute while DocFlow is open. Automatic publication
  defers while any scoped source file is newer than the configured settle window.
  Configured file-effect and total-byte brakes pause standing authorization
  before a mass change; manual Preview remains available for inspection. These
  controls are part of the exact approved profile and do not weaken backup,
  live-hash, journal, Undo, or no-delete rules.
- **DF-029 — Guarded Outlook MSG archive:** Preview binds exactly one selected
  Outlook MailItem identity and fingerprint to a deterministic safe filename.
  Apply revalidates that selection, exports a Unicode MSG to private temporary
  storage, validates the compound-file envelope, then creates one journaled
  no-clobber local copy. It never sends, moves, deletes, or changes mailbox data;
  Undo removes only the exact unchanged created copy.
- **DF-030 — Exact OTHERS-only Worley evidence:** RFI System Sync may classify
  an RFI as Worley-signed only from the exact case-insensitive
  `RFI No. <canonical RFI>.pdf` inside that canonical RFI folder. Page-one
  analysis is restricted to the inner name/signature area of the bottom-right
  `OTHERS` cell. Printed rules and any mark, name, date, note, or signature
  elsewhere are not authority. Missing, unreadable, empty, or insufficient
  evidence remains unsigned and authorizes no workbook change. A detector
  revision invalidates every older cached classification.
- **DF-031 — Guarded issue-to-reply handoff:** only a fully successful,
  owner/session-bound `rfi.log_add` Plan may prepare its exact approved rows as
  a separate unsent Outlook Reply All Plan. The handoff preserves escaped table
  formatting, never recalculates RFI numbers, adds no recipients, sends no mail,
  and requires its own Preview and Apply.
- **DF-032 — Ordered batch RFI scan separation:** Fast RFI Scan in Multi-RFI
  mode may inspect one top-level multi-page PDF as a sequential stack. A page
  with one strict, high-confidence canonical RFI code starts a confirmed cover
  and the next RFI group. Every page after a confirmed cover belongs to that
  group until another confirmed cover appears, even when continuation-page OCR
  is uncertain or unreadable. Pages before the first identified RFI and
  duplicate RFI starts remain Review output. Each output contains only its
  approved pages, retains existing canonical filenames and duplicate/no-clobber
  rules, and places CIV/MEC/ELE/PRC results beneath matching discipline folders.
  Preview pins source hashes and exact page indexes; Apply extracts only those
  pages, verifies fidelity, journals every output, and moves the combined source
  to recoverable trash only after all outputs succeed when Keep original is off.
  Unsupported or drifted PDFs fail closed to Review or Apply refusal; no page is
  silently dropped.
- **DF-035 — Fast RFI Scan orientation:** Multi-RFI orientation correction
  defaults off and uses the same canonical `/Rotate` normalization and
  render-relative correction policy as standalone PDF orientation. Only
  confident right-angle decisions enter the approved output; already-upright
  pages are idempotent, source bytes are never rewritten in place, and
  non-cardinal or uncertain files remain intact in `_review`. If ordinary
  direction evidence remains unresolved, one exact canonical RFI ID in the
  page header can resolve it only when the same ID and direction agree at both
  250 and 350 DPI without conflicting evidence. Analysis and cache reuse are
  bound to source SHA-256; cache profile changes invalidate prior decisions.
  Optional Fast RFI Scan rotation pins source SHA-256 before parallel analysis
  and verifies it afterward, rejecting changed sources before Plan creation.
  Single-RFI reads only page one, keeps the complete PDF, and never splits or
  rotates it.
- **DF-036 — Fast RFI Scan cover recognition and correction:** The active
  `rfi.process_scans_fast_covers` operation confirms a group boundary only when
  one strict RFI identity, the request heading, and a known template mark agree.
  If direction-zero generic OCR words do not confirm a cover, the detector
  continues its remaining cardinal searches; no-code or ambiguous boundaries
  remain in Review. OCR alone may normalize the exact alias `CLV` to `CIV`;
  filenames, searchable text, and manual input do not receive that repair.
  Preview permits only complete canonical CIV/MEC/ELE/PRC code edits; Rust
  rederives targets from trusted settings and rejects invalid, duplicate, or
  existing targets before Apply. Missing discipline/RFI folders are created
  safely. Page extraction preserves XMP metadata; unsupported catalog features
  remain Review cases.
- **DF-033 — Retired Follow Up status remarks:** Superseded by ADR 0028.
  `rfi.follow_up_sync` never writes worksheet Remarks cells. RFI Log REMARKS
  remain read-only source evidence for Worley Signed/Not Signed status.

## Scope

The ten registered operations and application-level history, recovery, review,
settings, access control, and diagnostics surfaces are in scope. Paid
services are optional; no paid dependency is required for core local workflows.

## Release criteria

All source gates in `RELEASE_SAFETY.md` must pass after the final edit. A Windows
release additionally requires `WINDOWS_RUNBOOK.md`. Operator workbook tests use
copies only and never replace the production originals. Physical printer,
Office, and OneDrive acceptance requires the actual target environment.

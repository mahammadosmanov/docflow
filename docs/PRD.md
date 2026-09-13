# Product requirements

`PRODUCT.md` is the living product definition. This file gives its stable
requirement IDs so code, tests, decisions, and releases can be traced.

## Core requirements

- **DF-001 — Preview first:** every document, workbook, queue, draft, or work-item
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

## Scope

The nine registered operations and application-level history, recovery, review,
settings, access control, diagnostics, and readiness surfaces are in scope. Paid
services are optional; no paid dependency is required for core local workflows.

## Release criteria

All source gates in `RELEASE_SAFETY.md` must pass after the final edit. A Windows
release additionally requires `WINDOWS_RUNBOOK.md`. Operator workbook tests use
copies only and never replace the production originals. Physical printer,
Office, and OneDrive acceptance requires the actual target environment.

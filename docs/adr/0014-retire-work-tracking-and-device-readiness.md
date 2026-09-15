# ADR 0014: Retire work tracking and device readiness

- Date: 2026-09-15
- Status: Accepted

## Context

The operator requested removal of RFI Work Tracking, Record RFI Work, Export RFI
Work Audit, and Device Readiness. They added dashboard and maintenance surface
without being required by the active document workflows.

## Decision

Remove both work-item operations from the registry and native preview router.
Remove the work queues, audit controls, readiness panel, and their callable
frontend/native commands. Keep document-specific OCR, printer, workbook, path,
recovery, and source-drift checks inside the workflows that need them.

Do not delete existing application-data work-event files. They are inactive
legacy data and remain available through Git-era builds if recovery is ever
required. Retired operation and command identifiers fail as unknown instead of
silently redirecting to another feature.

## Consequences

- The dashboard is smaller and no longer performs readiness or work-queue calls.
- The registered operation count is nine.
- Existing document history, recovery, journals, review, diagnostics, and Undo
  remain unchanged.
- Reintroducing tracking requires a new product decision and migration plan for
  retained legacy event data.

## Rollback

Revert this decision and its implementation commit. Do not synthesize or migrate
legacy events during rollback; the previous implementation reads its own retained
store.

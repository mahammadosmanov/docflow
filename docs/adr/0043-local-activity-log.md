# ADR 0043: Local activity log for support

- Status: Accepted
- Date: 2026-10-24

## Context

Diagnosing Follow Up and other results required the operator to relay screenshots
and error text one at a time. The support bundle holds only version, platform,
and recent error codes, within a 32 KiB shape-validated cap.

## Decision

The Rust router appends one JSON line per backend call to
`~/.docflow/logs/activity.jsonl` (two files, 1 MiB each, oldest rotated out):
timestamp, DocFlow version, OS, method, operation ID, duration, success, and for
failures the stable error key plus OS cause. For Preview and Revise results it
adds the plan's warning keys, row counts per decision/reason, and up to 400
samples of every non-ready row (RFI, sheet, reason key, and the existing
`mappingDetails` text with ITP/item/tag/area).

Every line passes through the support bundle's recursive redaction: paths, tokens,
credentials, and document-content keys are replaced. A new `support/activityLog`
command returns the newest 768 KiB for the operator to save and share from
Settings > Advanced support. Logging is best effort, never blocks or fails an
operation, and never grants authority.

## Consequences

RFI codes and ITP/item text are included by operator choice so one file explains a
Manual Review. The log is local until the operator shares it. Nothing is uploaded.

## Rollback

Remove the router recording, the command, and the Settings button. No stored
settings, journal, or workbook formats change.

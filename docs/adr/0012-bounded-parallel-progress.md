# ADR 0012: Bounded parallel analysis with visible operation stages

- Date: 2026-09-15
- Status: Accepted

## Context

Large RFI trees contain many independent PDFs and directories. Several Preview
paths still processed those inputs serially or showed only a generic Preview
label, making long runs slower and difficult to distinguish from a stalled run.
Mutation order, print certainty, workbook fidelity, and immediate scan-index
checkpoints must not be weakened for speed.

## Decision

Use the existing Rayon pool for independent, read-only batch work. Keep one CPU
available and cap analysis at four workers. Preserve ordered result assembly.
Route each completed indexed PDF back through one coordinator, which records its
SQLite result and progress before the run continues.

Use this pool for Process Scans OCR, signed-input discovery and PDF inspection,
signed-page OCR/archive discovery, PDF-orientation discovery and page analysis,
RFI System discovery and Worley signature detection. Do not parallelize print submissions, Outlook/Office automation,
workbook writes, Apply publication, journals, recovery, or Undo.

Every registered operation reports specific localized phases through the
existing progress channel. A phase may expose bounded counts and a current
subject, but progress never grants or changes authority.

## Consequences

Large read-only Preview work can use several cores and expose continuous status.
OneDrive/network saturation and native OCR memory remain bounded by the existing
four-worker limit. Small and single-item tasks avoid extra concurrency where it
cannot help. Native Windows throughput and display behavior remain release gates.

Rollback removes the new pool call sites and phase labels. Plan, journal, queue,
workbook, and scan-index formats remain backward compatible.

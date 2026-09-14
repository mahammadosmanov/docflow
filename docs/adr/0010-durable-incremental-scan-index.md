# ADR 0010: Durable incremental scan index

- Date: 2026-09-14
- Status: Accepted

## Context

The RFI system can exceed 20 GB. PDF orientation and Worley-signature scans used
to reopen and reanalyse every discovered PDF on each Preview. A cancelled or
interrupted scan lost all analysis progress, making the next run start again.
The work computer has slow, sometimes unavailable internet and no administrator
access. Source archives and workbooks must remain protected by the existing
preview/apply/journal boundary.

## Options considered

1. Continue full scans. This is simple but repeatedly spends hours on unchanged
   data and loses progress after interruption.
2. Use Supabase as the primary cache. This makes scanning depend on internet,
   exposes sensitive path-derived metadata to a remote service, and creates a
   cloud consistency decision inside a local safety boundary.
3. Use a durable local SQLite index and optionally consider anonymized aggregate
   cloud synchronization later. SQLite is zero-cost, transactional, portable in
   the app bundle, and works offline.

## Decision

Use option 3. The app owns `~/.docflow/scan-index.sqlite3`, configured with WAL,
full synchronous commits, foreign-key enforcement, and a bounded busy timeout.
Keys contain owner, canonical root, operation, algorithm profile, relative path,
and the current file fingerprint. Each newly analysed PDF is committed before
the next completed item is depended upon. Runs remain explicitly completed or
interrupted, and transient failures are audit events rather than reusable cache
entries.

RFI System Sync and PDF orientation use the index first because they repeatedly
analyse the large archive. A task-local Force full rescan option bypasses cached
outcomes and automatically returns to false on the next visit. Preview shows
discovered, reused, freshly scanned, failed, and prior-interruption counts.

The cache is never mutation authority. Existing immutable Plan, exact hash,
capability, no-clobber, workbook fidelity, journal, and Undo checks are unchanged.
No file path, PDF content, fingerprint, or cached result is uploaded to Supabase.
Positive cached Worley detections additionally require a live full-file hash
match before they can contribute a workbook update to Preview.

## Consequences and rollback

Normal repeated scans read directory metadata but avoid PDF rendering/OCR for
unchanged files. New, changed, uncacheable, transiently failed, or force-selected
files are analysed again. A same-metadata content change can make Preview reuse
old non-mutating classification, but an actionable Apply still fails its exact
source-hash check; the operator can force a full rescan when external tools are
known to preserve timestamps.

Rollback removes the index integration and bundled SQLite dependency. The local
database can be retained inertly or deleted by the operator; source documents,
workbooks, journals, and outputs are independent of it.

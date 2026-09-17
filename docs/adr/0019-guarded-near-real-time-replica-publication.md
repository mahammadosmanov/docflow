# ADR 0019: Guarded near-real-time replica publication

## Status

Accepted on 2026-09-17.

## Context

The SharePoint, GlobalProtect, and local RFI Systems can diverge during long
gaps between manual work. A filesystem watcher would add platform-specific
state and still cannot prove that an Office, OneDrive, or network file has
finished writing. Unbounded unattended publication could also distribute a
large accidental change before an operator sees it.

## Decision

Reuse the existing native scheduler and durable incremental hash index. Allow a
one-minute minimum interval. Before an automatic Preview hashes files, require
every scoped source file to be older than the approved settle window. A settle
delay keeps standing authorization enabled and retries on the next due run.

The approved profile also stores maximum changed target-file effects and total
published MiB. Exceeding either limit fails before Apply and pauses automation.
Manual Preview remains available so the operator can inspect the exact effects
and explicitly approve a revised profile. Existing backup, live-hash, no-delete,
journal, and Undo rules remain unchanged.

## Consequences

The design has no new dependency or watcher service and works with the existing
Windows startup option. Publication latency is bounded by the selected interval
plus settle window. Timestamp quality remains an external filesystem property;
missing or future source modification times fail closed during automatic runs.

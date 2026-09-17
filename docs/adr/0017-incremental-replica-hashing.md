# ADR 0017: Incremental replica hashing

Date: 2026-09-17  
Status: Accepted

## Context

Replica publication compares three potentially large RFI System trees. Hashing
every unchanged file on every unattended interval wastes time, network reads,
and OneDrive or GlobalProtect capacity. A pathname-only cache would be unsafe
because files can be replaced in place or the same relative path can exist in a
different root.

## Decision

Use the existing local SQLite scan index for replica SHA-256 results. Scope each
entry by authenticated owner, canonical root, operation, hash algorithm profile,
relative path, filesystem identity, size, and modification time. Record every
fresh successful hash immediately. Reuse only a syntactically valid SHA-256
value with an exact fingerprint match.

The cache is comparison evidence only. Preview still performs a complete,
no-symlink directory discovery. Apply still reopens and hashes the exact approved
source and target before any copy or replacement. Publication, backup, journal,
and Undo remain serial and unchanged.

## Consequences

Unchanged trees avoid repeated content reads and interrupted scans retain
completed hash work. Changed, replaced, unknown, malformed, or unreadable entries
are hashed again. The local index can be deleted without data loss; the next run
performs fresh hashing.

Rollback removes indexed preview wiring and leaves existing index entries inert.
No document or settings migration is required.

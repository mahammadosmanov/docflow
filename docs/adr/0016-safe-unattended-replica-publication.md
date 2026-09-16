# ADR 0016: Safe unattended replica publication

- Date: 2026-09-16
- Status: Accepted

## Context

The operator maintains SharePoint/OneDrive, GlobalProtect/network, and local
copies of the same RFI System. Manual repetition is slow, but a bidirectional
merge or silent overwrite could lose controlled work. The product previously
required explicit approval for every mutation.

## Options considered

1. Keep manual file copying. Rejected because it preserves repetitive work and
   provides no complete scan, backup, hash, journal, or recovery evidence.
2. Bidirectional or newest-file-wins synchronization. Rejected because clocks,
   OneDrive states, and independent edits cannot identify business authority.
3. One-way source-authoritative publication with approval every run. Safe, but
   does not meet the requested automation goal.
4. One-way publication with a narrowly scoped standing authorization. Chosen.

## Decision

`files.replica_publish` stores three fixed roots, one selected source, a separate
backup root, exact scope, and interval. Its first Plan always requires explicit
Apply. If unattended mode is selected, successful Apply stores a revocable
standing authorization for that exact profile.

Every later run still performs a complete no-symlink scan, creates an immutable
Plan, hashes files with bounded read-only workers, revalidates source and target,
backs up every replaced target without overwrite, verifies backup and published
hashes, applies serially, journals exact effects, and supports guarded Undo.
Target-only files are warnings and are never deleted. Any unsafe error pauses
the profile. Backups never expire automatically. Windows startup uses the
current-user Run key and requires no administrator permission.

## Consequences

- Standing authorization is an explicit, narrow exception to per-run approval;
  it does not apply to workbooks, print, OCR filing, Outlook, or other operations.
- “100% guaranteed” is not claimed. Network, disk, OneDrive, and OS failures are
  external; DocFlow fails closed and retains recovery evidence.
- Publication is not a merge. Operators must choose the correct source.
- Windows/OneDrive/GlobalProtect and startup behavior remain native acceptance gates.

## Rollback

Disable unattended sync in Settings. Disable Windows startup separately. The
manual operation, journals, retained backups, and Undo remain available. Revert
the dedicated feature commit to remove the operation; existing backup and journal
files remain ordinary operator data and are never deleted by rollback.

# ADR 0044: Tolerate unsupported directory flush on network shares

- Status: Accepted
- Date: 2026-10-24

## Context

Follow Up Apply on the operator's remote RFI System folder (reached over
GlobalProtect) always ended in recovery-required. The activity log showed
`retain_original: Os { code: 1, "Incorrect function" }` with an unknown outcome:
the rename of the original file had succeeded, then flushing the parent
directory handle (`FlushFileBuffers`) failed. SMB/network filesystems do not
support flushing a directory handle, so every Apply on such a folder stopped
after displacing the original.

## Decision

On Windows, a directory flush that fails with `ERROR_INVALID_FUNCTION` (1) or
`ERROR_NOT_SUPPORTED` (50) is treated as "this filesystem has no directory
flush" and succeeds. Every other error still fails. Nothing else is relaxed:
identity, SHA-256, security-descriptor, backup, journal, no-clobber, and
post-publication verification all still run.

## Consequences

On filesystems without directory flush, the durability of the rename across a
power loss or connection drop is no longer fenced by this call. File content
integrity is unchanged because publication is verified by hash and identity, and
the journal and retained original still allow recovery and Undo. Local NTFS
behavior is unchanged.

## Rollback

Remove `directory_flush_unsupported` and its use in `sync_all`.

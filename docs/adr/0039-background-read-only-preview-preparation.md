# ADR 0039: Background read-only Preview preparation (Action Queue)

- Date: 2026-10-23
- Status: Accepted

## Context

Operators repeat the same start sequence every day: open an operation, choose the saved folders, and request a Preview. The work is waiting in known folders, but DocFlow only notices when the operator asks.

## Decision

- DocFlow may prepare a Preview in the background, without an operator click, for these existing operations only: `rfi.process_scans_fast_covers`, `rfi.group_files`, `rfi.stage_signed_first_pages`, and `rfi.complete_status_sync`.
- Preparation is a Preview only. The watcher has no path to Apply, journal writes, Undo, printing, Outlook, or any filesystem mutation. A source-scan test enforces this for `watch/mod.rs` and `native/watch.rs`.
- Every mutation keeps Preview, explicit user approval, Apply, journal, and Undo. This decision adds no standing authority and does not extend `files.replica_publish`.
- Watching is off by default and is enabled per operation from the Action Queue. It reuses the operation's remembered parameters; it adds no new folder grant, path picker, or operation-specific form.
- Change detection lists metadata only (name, size, modified time), never reads file contents, follows no symlinks, ignores hidden and Office lock files, and fails closed above a bounded entry count. A 60-second settle delay avoids half-copied or syncing files. The default check interval is 5 minutes (60 to 3600 seconds); the first check runs 30 seconds after startup.
- Checks run one at a time and pause while any operator Preview, Apply, Undo, or Review is running.
- A queued Preview is one-shot server-side plan data with the existing 30-minute lifetime. It is marked out of date when the source fingerprint changes or after 25 minutes. Apply continues to revalidate hashes and never recalculates.
- Queue state is in memory and local only. It is rebuilt after restart by evaluating enabled operations again. Cloud history is unchanged.
- The setting is an additive optional `watchQueue` settings section. A settings file without it behaves as before.

## Consequences

Operators open one Action Queue instead of starting each operation, and review the same Preview as before. The cost is background metadata listing and OCR/PDF reading at most once per settled folder change. Windows, OneDrive, and network-path behavior require native acceptance recorded in [KNOWN_ISSUES.md](../KNOWN_ISSUES.md). Extending automation beyond Preview, such as standing authority for `rfi.system_sync` or `rfi.complete_status_sync` Apply, needs a separate accepted ADR.

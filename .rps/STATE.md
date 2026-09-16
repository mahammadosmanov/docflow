# DocFlow project state

Updated: 2026-09-16

## Current release

- Version: 1.2.0
- Branch: `review/rust-tauri-migration`
- Implementation commit: pending release commit
- Objective: safely publish one selected RFI System source to two targets, with
  retained backups, guarded Undo, optional unattended operation, and Windows startup.

## Verified

- Exact scoped discovery, bounded parallel hashing, backup-first replacement,
  hash verification, target-extra preservation, fail-fast Apply, status
  visibility, and Undo pass the local test suite.
- Rust, Node, React, frontend, version, documentation, source, and license checks
  pass. Windows x64 cross-build, package audit, archive integrity, checksums, and
  GUI-subsystem checks also pass as recorded in `docs/VERIFICATION.md`.

## External gate

- Test the 1.2.0 package on native Windows using isolated SharePoint/OneDrive,
  GlobalProtect/network, local, and backup roots. Confirm startup, manual Apply,
  Undo, unattended due runs, safety pause, offline/reconnect, and lock behavior.

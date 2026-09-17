# DocFlow project state

Updated: 2026-09-17

## Current release

- Version: 1.5.0
- Branch: `review/rust-tauri-migration`
- Implementation commit: `18061b1`
- Objective: keep source-authoritative RFI System publication current with a
  guarded one-minute cadence while retaining useful pathless printer evidence.

## Verified

- Durable replica hashes are reused only for the same owner, operation, source
  root, profile, relative path, identity, size, and modified time; Apply still
  live-revalidates source and target hashes.
- Pathless support diagnostics include printer inventory and the latest
  caller-owned queue's route, settings, attempts, outcome, and available error.
- Automatic replica runs defer for recently modified source files and stop
  before configured file-effect or total-MiB mass-change limits are exceeded.
- Rust, Node, React, frontend, version, documentation, source, and license checks
  pass. Windows x64 Rust check and test-target compilation pass as recorded in
  `docs/VERIFICATION.md`.

## External gate

- Build and test the next Windows package using isolated SharePoint/OneDrive,
  GlobalProtect/network, local, and backup roots. Confirm startup, publication,
  one-minute cadence, settle timing, mass-change brakes, interruption, and Undo.
  Export a support report after representative EPSON
  L15160 Series success, partial output, empty-paper signal, and uncertain jobs;
  compare it with actual physical output before changing retry policy.

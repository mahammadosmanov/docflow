# DocFlow project state

Updated: 2026-09-17

## Current release

- Version: 1.5.2
- Branch: `review/rust-tauri-migration`
- Implementation commit: `3c9f9ca`
- Objective: accept Excel worksheet PDFs through Windows' native decoder while
  retaining pre-effect validation and failed-only queue resume.

## Verified

- Durable replica hashes are reused only for the same owner, operation, source
  root, profile, relative path, identity, size, and modified time; Apply still
  live-revalidates source and target hashes.
- Pathless support diagnostics include printer inventory and the latest
  caller-owned queue's route, settings, attempts, outcome, and available error.
- Automatic replica runs defer for recently modified source files and stop
  before configured file-effect or total-MiB mass-change limits are exceeded.
- Excel automation no longer writes global calculation properties before a
  workbook exists, and pathless diagnostics retain COM stage/HRESULT evidence.
- Excel private PDFs receive bounded envelope checks; Windows' native decoder
  owns load and non-zero page-count validation before `StartDoc`.
- The incomplete-queue action explicitly identifies failed-only retry; completed
  and completed-unconfirmed jobs remain disabled in resumed plans.
- Rust, Node, React, frontend, version, documentation, source, and license checks
  pass. Windows x64 Rust check and test-target compilation pass as recorded in
  `docs/VERIFICATION.md`.

## External gate

- Test the 1.5.2 Windows package with isolated copies of both supplied MDR
  workbook shapes, then continue the SharePoint/OneDrive,
  GlobalProtect/network, local, and backup roots. Confirm startup, publication,
  one-minute cadence, settle timing, mass-change brakes, interruption, and Undo.
  Export a support report after representative EPSON
  L15160 Series success, partial output, empty-paper signal, and uncertain jobs;
  compare it with actual physical output before changing retry policy.

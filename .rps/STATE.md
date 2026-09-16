# DocFlow project state

Updated: 2026-09-16

## Current release

- Version: 1.1.0
- Branch: `review/rust-tauri-migration`
- Implementation commit: `37bb37cfc64e3bdc63ddf2b28faeae4e5993b138`
- Objective: recursively discover exact completed RFI PDFs and safely preview
  and apply their `Completed` status to the RFI Log.

## Verified

- Recursive bounded multi-core discovery, exact filename matching, duplicate
  evidence grouping, guarded workbook changes, selection revision, Apply, and
  Undo pass the local test suite.
- Rust, Node, React, frontend, version, documentation, source, and license checks
  pass. Windows x64 cross-build, package audit, archive integrity, checksums, and
  GUI-subsystem checks also pass as recorded in `docs/VERIFICATION.md`.

## External gate

- Test the 1.1.0 package on native Windows/OneDrive using copies of the current
  RFI Log and nested RFI System. Confirm Excel opens the applied and undone
  workbook without repair and that the source PDF tree remains unchanged.

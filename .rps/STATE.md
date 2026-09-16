# DocFlow project state

Updated: 2026-09-16

## Current release

- Version: 1.1.0
- Branch: `review/rust-tauri-migration`
- Implementation commit: pending
- Objective: recursively discover exact completed RFI PDFs and safely preview
  and apply their `Completed` status to the RFI Log.

## Verified

- Recursive bounded multi-core discovery, exact filename matching, duplicate
  evidence grouping, guarded workbook changes, selection revision, Apply, and
  Undo pass the local test suite.
- Rust, Node, React, frontend, version, documentation, source, and license checks
  pass. Windows package evidence is pending the release build.

## External gate

- Test the 1.1.0 package on native Windows/OneDrive using copies of the current
  RFI Log and nested RFI System. Confirm Excel opens the applied and undone
  workbook without repair and that the source PDF tree remains unchanged.

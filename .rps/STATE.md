# DocFlow project state

Updated: 2026-09-15

## Current release

- Version: 1.0.2
- Branch: `review/rust-tauri-migration`
- Implementation commit: `c5d9e3c0daf5a0230a1bd8400462b2f8092c6767`
- Objective: make Windows batch-print spool observation resilient without
  risking duplicate physical output.

## Verified

- Transient generic Windows driver errors remain under observation.
- A unique new native-handler spool job can be followed despite title changes;
  ambiguous new jobs remain unconfirmed.
- Rust, Node, React, frontend, version, documentation, source, license, and
  Windows cross-build/package checks passed as recorded in
  `docs/VERIFICATION.md`.

## External gate

- Test the 1.0.2 package on native Windows with the EPSON L15160 and a mixed
  PDF, Excel, Word, and PowerPoint queue. Cross-build evidence cannot prove that
  paper physically exited.


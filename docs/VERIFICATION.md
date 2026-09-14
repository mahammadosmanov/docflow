# Verification record

This ledger records what was actually exercised for each release increment.
Passing source or cross-build gates never implies native Windows, Office,
OneDrive, Outlook, physical-printer, signing, or notarization acceptance.

## 0.23.0 - 2026-09-14

Passed source evidence after the final implementation edit:

- Clippy with all targets/features and warnings denied;
- 338 Rust unit tests and 210 integration tests; the two operator-workbook
  environment tests remained intentionally ignored;
- durable restart/fingerprint tests plus real cache-hit tests proving unchanged
  PDF orientation and RFI System Sync files avoid repeated analysis;
- 23 Node tests and 52 React tests, including localized incremental-scan counts;
- production TypeScript/Vite build and Cargo check;
- version, documentation, source, and dependency-license audits.

The frontend build retained the known non-failing chunk-size advisory. Native
Windows/OneDrive validation of interruption, resume, file change detection, and
the task-local forced full rescan has not yet run. Cached positive Worley results
are full-hash checked in Rust, but the exact Windows 20 GB system remains an
external acceptance gate.

The Windows x64 cross-build from implementation commit
`a9206c3790deea1e0ed130ae877cf216e8c4d4d0` completed. The OCR-inclusive ZIP
passed the package audit, all generated checksums verified, and both executable
artifacts identify as Windows GUI-subsystem PE files. Cross-linker PDB warnings
were non-failing debug-information warnings. Artifact SHA-256 values:

- portable-with-OCR ZIP: `d609db7432de8fb8f37d3797c019c51276cbcf91ca5999f6b5aa14bafd7593f2`;
- portable EXE: `1ff9a565b497005d10c992801df6d33878361707b5a6587ee8320a94118fabef`;
- per-user installer: `7a61192cc758047046c00591e9e488b45a37714c5f151d0f735f23dbc2af8f8f`.

## 0.22.0 - 2026-09-14

Passed source evidence after the final edit:

- Clippy with all targets/features and warnings denied;
- 336 Rust unit tests and 208 integration tests; the two operator-workbook
  environment tests remained intentionally ignored;
- Windows printing compile-boundary test, including the real adapter source;
- 23 Node tests and 51 React tests, including clean-entry/explicit recall;
- production TypeScript/Vite build and Cargo check;
- version, documentation, source, and dependency-license audits.

The frontend build retained the known non-failing chunk-size advisory. The
Windows x64 cross-build completed, the OCR-inclusive portable ZIP passed the
package audit, all three artifact checksums verified, and both executables were
confirmed as Windows GUI-subsystem PE files. Cross-linker PDB warnings were
non-failing debug-information warnings.

Exact artifact hashes are generated beside each build in `SHA256SUMS`; generated
packages and checksum files are intentionally not committed.

Native Windows EPSON L15160 Series paper-output acceptance is not yet run. The
operator must verify fast spool release, recoverable status waiting, ordering,
cancellation, timeout, clean form entry, and explicit previous-setting restore
with non-sensitive fixtures before this change is considered printer-certified.

# Verification record

This ledger records what was actually exercised for each release increment.
Passing source or cross-build gates never implies native Windows, Office,
OneDrive, Outlook, physical-printer, signing, or notarization acceptance.

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

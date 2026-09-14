# Verification record

This ledger records what was actually exercised for each release increment.
Passing source or cross-build gates never implies native Windows, Office,
OneDrive, Outlook, physical-printer, signing, or notarization acceptance.

## 0.24.1 - 2026-09-14

Passed source evidence after the final implementation edit:

- Clippy with all targets/features and warnings denied;
- 338 Rust unit tests and 210 integration tests; the two operator-workbook
  environment tests remained intentionally ignored;
- Windows status-policy tests cover release on either side of the two-call
  `GetJob` read, buffer-growth retry classification, explicit query failure,
  terminal fault, recoverable status, and cancellation boundaries;
- 23 Node tests and 53 React tests, production TypeScript/Vite build, Cargo
  check, formatting, version, documentation, source, and license audits.

The frontend build retained the known non-failing chunk-size advisory. Native
Windows output on the EPSON L15160 Series remains the decisive acceptance gate:
run at least three mixed files in one Apply and confirm the queue advances
without Review remaining while each page is produced exactly once.

The Windows x64 cross-build from implementation commit
`089c6344e65639e242c07443cbf043ae9e097b23` completed. The OCR-inclusive ZIP
passed the package audit, every generated checksum verified, and both executable
artifacts identify as Windows GUI-subsystem PE files. Cross-linker PDB warnings
were non-failing debug-information warnings. Artifact SHA-256 values:

- portable-with-OCR ZIP: `af967f4b3becf483ae404729643f137588b45be7afba3a914f1ab4f5573dea33`;
- portable EXE: `70e41122724da5df7e8821dc28e98d16c3a9a3b5459d5e5d63b2f4aeeca32ceb`;
- per-user installer: `1c36021a4108a2a418ee4c9038b29ad8c37fd31d2e7abd4529495ebf644e0da4`.

## 0.24.0 - 2026-09-14

Passed source evidence after the final implementation edit:

- Clippy with all targets/features and warnings denied;
- 338 Rust unit tests and 210 integration tests; the two operator-workbook
  environment tests remained intentionally ignored;
- 23 Node tests and 53 React tests, including bounded confidence formatting,
  signed-page editor projection, and the plan scroll/action DOM contract;
- production TypeScript/Vite build, Cargo check, formatting, version,
  documentation, source, and dependency-license audits;
- Playwright at a 680 x 520 viewport showed the OCR 100% badge, zero horizontal
  body overflow, a 1,738 px synthetic long plan inside a 318 px scroll region,
  and the complete action bar fixed within the viewport below that region.

The frontend build retained the known non-failing chunk-size advisory. OCR
scores are intentionally review evidence rather than calibrated probability or
automatic approval. Native Windows/WebView2 display scaling, screen-reader, and
representative low-confidence OCR acceptance have not yet run.

The Windows x64 cross-build from implementation commit
`ef4b6d0f72bd601a5afa86f4e1e46b113c96d0b2` completed. The OCR-inclusive ZIP
passed the package audit, all generated checksums verified, and the installer
and portable executable identify as Windows GUI-subsystem PE files. Cross-linker
PDB warnings were non-failing debug-information warnings. Artifact SHA-256 values:

- portable-with-OCR ZIP: `e5606a209e33cc5ca9bcdffca55980d6e493aa5e8d06f6fc1d28dacc56176e53`;
- portable EXE: `43401700c6220258dc6e0a5b1a060e8e26743ceecebff549c2a5506d76e839e1`;
- per-user installer: `e6d530053b88a69bb7a7acbd7d29b970546775a13178d92661ff93482a560424`.

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

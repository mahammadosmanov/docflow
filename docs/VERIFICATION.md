# Verification record

This ledger records what was actually exercised for each release increment.
Passing source or cross-build gates never implies native Windows, Office,
OneDrive, Outlook, physical-printer, signing, or notarization acceptance.

## 0.26.2 - 2026-09-14

The unused direct Tokio declaration and its DocFlow lockfile edge were removed;
Tokio remains a transitive Tauri dependency. Cargo check passed immediately
after removal. The complete source gates subsequently passed: version,
documentation, source and license audits; 23 Node and 53 React tests; production
frontend build; Rust formatting, Clippy with warnings denied, 338 unit tests,
213 integration tests with two operator-workbook tests intentionally ignored,
and Cargo check.

The Windows x64 cross-build from commit
`e15c3e1` completed. The OCR-inclusive ZIP passed the package audit, all three
checksums verified, and the portable and installer executables identify as
Windows GUI-subsystem PE files. Artifact SHA-256 values:

- portable-with-OCR ZIP: `10b58c287d783ae98d34888c818dd9d7d3f3fdcaf46cc1353c866b3187d3ce0f`;
- portable EXE: `f596fda280c9f13251b332006c140bd550431d82243673a2021925b8c4ade2de`;
- per-user installer: `1c1cc98b775032940995f8dda5200da33623535b4bafd91c5eb88164304649de`.

Cross-build evidence does not establish native Windows, WebView2, Office,
OneDrive, Outlook, or physical-printer acceptance; those gates were not run.

## 0.26.1 - 2026-09-14

The shared domain-independent React controls were extracted from `App.tsx`
without changing their markup, behavior, styling, or imports at call sites.
All 23 Node tests and 53 React tests passed, and the production TypeScript/Vite
build passed with the existing non-failing chunk-size advisory. Windows/native
acceptance was not run because this refactor does not claim platform evidence.

## 0.26.0 - 2026-09-14

Passed source evidence for the signed-page review increment:

- Clippy with all targets/features and warnings denied, Cargo formatting and
  check, documentation/version/source/license audits;
- 338 Rust unit tests and 213 integration tests passed; the two operator-workbook
  environment tests remained intentionally ignored;
- signed-page integration tests prove live OCR/archive/final progress phases,
  one structured no-clobber `_review` copy per warning source, preserved nested
  input layout, and Apply refusal when that source changes after Preview;
- 23 Node tests and 53 React tests passed; the React evidence covers the signed
  section navigator, before/after comparison and enlarged view, exact warning
  PDF/page/RFI/reason presentation, and bounded larger PNG acceptance;
- the production TypeScript/Vite build passed with the known non-failing
  chunk-size advisory.

The Windows x64 cross-build from final candidate commit
`be5250dc148fed0e6aba1207af7ea491ae952ac9` completed. The OCR-inclusive ZIP
passed the package audit, every generated checksum verified, and both executable
artifacts identify as Windows GUI-subsystem PE files. Cross-linker PDB warnings
were non-failing debug-information warnings. Artifact SHA-256 values:

- portable-with-OCR ZIP: `1ae4902992701694aa611061c5b6382cafecda328d96cf7e5763dd50778509f2`;
- portable EXE: `ddfbbd652204a608f0d02090a9350bc1e389662bd5089512957d47a51b673c02`;
- per-user installer: `90b6575d652119d5c3041ef8def18f335ffa1cb1008af1dbedb146747ac448c0`.

Cross-build and browser tests do not establish native Windows/WebView2 or
OneDrive acceptance. The exact 0.26.0 package still needs the signed-page checks
in `WINDOWS_RUNBOOK.md` with non-sensitive copies on the operator machine.

## 0.25.0 - 2026-09-14

Passed source evidence after the final implementation edit:

- Clippy with all targets/features and warnings denied;
- 338 Rust unit tests and 212 integration tests; the two operator-workbook
  environment tests remained intentionally ignored;
- signed-page integration coverage proves exact archive discovery below several
  CIV/MEC/ELE/PRC nesting depths, bounded worker evidence, deterministic exact
  matching, no-work discovery avoidance, and refusal to follow file or directory
  symlinks;
- 23 Node tests and 53 React tests, production TypeScript/Vite build, Cargo
  check, formatting, version, documentation, source, and license audits.

The frontend build retained the known non-failing chunk-size advisory. Native
Windows acceptance is still required on the operator's actual OneDrive/network
RFI System: exercise a representative deep tree, duplicate exact filenames,
an unreadable folder, cancellation, and nested combined scans; verify the archive
hashes remain unchanged and only no-clobber review copies are produced.

The Windows x64 cross-build from implementation commit
`29672facb885b3cbda9594b0ece82d688de7385e` completed. The OCR-inclusive ZIP
passed the package audit, every generated checksum verified, and both executable
artifacts identify as Windows GUI-subsystem PE files. Cross-linker PDB warnings
were non-failing debug-information warnings. Artifact SHA-256 values:

- portable-with-OCR ZIP: `58b87c8f045745013a9fec5c73aa589e0a2b7ad16a3e690b893e2e523b1624bb`;
- portable EXE: `d44302cc097520d7497bc33d53987171aa6973c55092bf827f525d79c48a6186`;
- per-user installer: `faf0fe8d6b3d928b7915a58d5b42b23a4f469144b372f1090ddff37e22943cbe`.

## 0.24.2 - 2026-09-14

Passed source evidence after the final implementation edit:

- Clippy with all targets/features and warnings denied;
- 338 Rust unit tests and 211 integration tests; the two operator-workbook
  environment tests remained intentionally ignored;
- the queue integration test proves an accepted-unconfirmed job is journaled,
  never retried, does not become resumable, and advances to the next file;
- Windows policy tests prove post-acceptance printer-open, enumeration, and job-
  read loss use that continuation state while explicit status faults and
  uncertain cancellation still pause;
- 23 Node tests and 53 React tests, production TypeScript/Vite build, Cargo
  check, formatting, version, documentation, source, and license audits.

The frontend build retained the known non-failing chunk-size advisory. No
software-only signal can prove physical paper output. Native Windows acceptance
on the EPSON L15160 Series remains required: print at least three mixed files in
one Apply and confirm the queue reaches one final result, produces each file
exactly once, and reports any receipt-loss items as sent-unconfirmed instead of
opening Review remaining after every file.

The Windows x64 cross-build from implementation commit
`a4290746bb73812d783db6e89e4d98c13897ccdc` completed. The OCR-inclusive ZIP
passed the package audit, every generated checksum verified, and both executable
artifacts identify as Windows GUI-subsystem PE files. Cross-linker PDB warnings
were non-failing debug-information warnings. Artifact SHA-256 values:

- portable-with-OCR ZIP: `6b52cc75ba95ed4a8b9e4a9468028c8d350439d13f1858e923b98a66c1ed9399`;
- portable EXE: `ac8fd5e53b244503b3b47b22a63c6d34dd0ee6e8a9819ced70b8360ff998e014`;
- per-user installer: `cf8603ae5a7aed1805bed43a8e3f80ce694afbf8d992a0fe6bc01d506d744dce`.

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

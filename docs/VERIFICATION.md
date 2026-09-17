# Verification record

This ledger records what was actually exercised for each release increment.
Passing source or cross-build gates never implies native Windows, Office,
OneDrive, Outlook, physical-printer, signing, or notarization acceptance.

## 1.5.2 - 2026-09-17

Excel PDF handoff passed Rust formatting, Clippy with all targets and features
with warnings denied, Cargo check, 346 unit tests, and 211 integration tests;
three operator-workbook tests remained intentionally ignored. Focused evidence
verifies empty/malformed PDFs stay blocked, bounded bytes are captured before
submission, source substitution remains rejected, Excel-compatible envelopes
reach the platform decoder, retries always re-export, and completed-unconfirmed
native jobs remain excluded from saved-queue retry.

All 23 Node tests and 52 React tests passed. Version, documentation, source, and
dependency-license audits plus the production frontend build passed. The known
non-failing frontend chunk-size advisory remains. Windows x64 Rust check and
complete test-target compilation passed. Windows test executables, desktop
Excel, Windows native PDF decode, and EPSON paper output were not run on this
macOS host.

The reported 1.5.1 result proves Excel created a non-empty file for every one of
the 11 and 4 selected worksheets, but DocFlow rejected each before `StartDoc` at
its mutation-oriented PDF parser. Version 1.5.2 replaces that incompatible gate
with a bounded envelope check followed by Windows native PDF load and non-zero
page-count validation before printer submission. Native Windows must still
print every selected sheet from isolated copies of both supplied MDR workbooks.
Use Retry unprinted only on the incomplete queue so already completed DOCX jobs
are not included in a newly approved batch.

The Windows x64 cross-build from implementation commit `3c9f9ca` completed.
The OCR-inclusive ZIP passed package and archive-integrity audits, all generated
checksums verified, and the installer and portable executable identify as
Windows GUI-subsystem PE files. Cross-linker PDB warnings were non-failing
debug-information warnings. The macOS 27 proc-macro workaround was confined to
the build environment and a temporary macOS 11 bundle target; the repository
configuration was restored to macOS 13 afterward. Artifact SHA-256 values:

- portable-with-OCR ZIP: `ef052202e5023d5ef9f23458c89c3561038e490f14fec91111176a2111ea451b`;
- portable EXE: `c092230e982e494846382d6748e1d1b8d9ac239e6624659c65bd5fb230ec6b07`;
- per-user installer: `ed87b0bba860bd5e6ec192bee2ca07e4a4181eac902bf2bc272e4b9ebd45b43b`.

Operator acceptance on 2026-09-17 passed with the 1.5.2 Windows package and the
reported mixed print set: both supplied DOCX files and every selected worksheet
from the two supplied MDR workbooks printed without an application error or a
missing output. This closes the reported Excel native-PDF compatibility defect.
Paper-out, offline/reconnect, cancellation, concurrent external print jobs, and
other device-fault scenarios remain separate acceptance gates.

## 1.5.1 - 2026-09-17

The Excel pre-open COM fix passed Rust formatting, Clippy with all targets and
features with warnings denied, Cargo check, 345 unit tests, and 211 integration
tests; three operator-workbook tests remained intentionally ignored. Focused
evidence verifies the complete late-bound COM plan no longer contains global
Calculation or CalculateBeforeSave writes, both members are outside the dispatch
allowlist, every remaining property set uses `DISPID_PROPERTYPUT`, and the
localized failure key remains separate from pathless COM stage/HRESULT evidence.

Both supplied MDR workbooks passed ZIP integrity inspection and contained 11 and
4 worksheets respectively, matching the repeated failures reported from 1.2.0.
Neither package exposed workbook protection, external-link, connection, query,
or XLM macro parts in the inspected structure. The two supplied DOCX packages
also passed ZIP integrity inspection; their accepted-unconfirmed results match
the existing non-duplicate native-handler policy and both physically printed.
No supplied user document was modified or committed.

All 23 Node tests and 52 React tests passed. Version, documentation, source, and
dependency-license audits plus the production frontend build passed. The known
non-failing frontend chunk-size advisory remains. The Windows x64 Rust check and
complete test-target compile passed; Windows test executables and desktop Excel
were not run on this macOS host.

The Windows x64 cross-build from implementation commit `4e1e22b` completed. The
OCR-inclusive ZIP passed the package audit, all generated checksums verified,
and the installer and portable executable identify as Windows GUI-subsystem PE
files. Cross-linker PDB warnings were non-failing debug-information warnings.
Artifact SHA-256 values:

- portable-with-OCR ZIP: `46c94d9e479b449eb1725578a4b8ed6faa830c2e89acdbf3a7654b81bef754ca`;
- portable EXE: `4a4662b11240e9ad9d45411a6c95fafe17fbb3b956ac3f32382071eb27fbde6a`;
- per-user installer: `6420b11ec3f53e083308f95a09beb89276e039775ba9c727c85763b3c910681f`.

Native Windows must still print every selected sheet from isolated copies of
both MDR workbook shapes. If a failure remains, the 1.5.1 support report must
identify the exact pathless COM stage and HRESULT before another change is made.

## 1.5.0 - 2026-09-17

Guarded automatic replica publication passed Rust formatting, Clippy with all
targets and features with warnings denied, Cargo check, 344 unit tests, and 211
integration tests; three operator-workbook tests remained intentionally ignored.
Focused evidence verifies that a recently modified source defers an automatic
run and a file-effect limit stops a mass change before Apply. Registry snapshot
evidence covers the generic settle/file/MiB controls, and the established replica
suite still covers backup-first publication, live hash verification, journal,
Undo, and target-extra preservation.

All 23 Node tests and 52 React tests passed. Version, documentation, source, and
dependency-license audits plus the production frontend build passed. The known
non-failing frontend chunk-size advisory remains. The Windows x64 Rust check and
complete test-target compile passed; Windows test executables were not run on
this macOS host.

The Windows x64 cross-build from implementation commit `18061b1` completed. The
OCR-inclusive ZIP passed the package audit, all generated checksums verified,
and the installer and portable executable identify as Windows GUI-subsystem PE
files. Cross-linker PDB warnings were non-failing debug-information warnings.
Artifact SHA-256 values:

- portable-with-OCR ZIP: `34a90a6c96753bf0bb0c6dfa23728e03dff997b87614916cb81263cfe666b303`;
- portable EXE: `7aa8b35e492eb147889a3ed4536ec182f2fc197446424853e7a1c2674ba410ac`;
- per-user installer: `5ae7a8a4534ed294be21041d637f85eea5d03af9e5070b7a34c45c63e6d83386`.

Native Windows testing must still verify one-minute scheduling,
OneDrive/network modification timestamps, file-settle deferral, both
mass-change brakes, startup, interruption, Apply, and Undo on isolated copies of
the three real system types.

## 1.4.0 - 2026-09-17

Pathless printer diagnostics passed Rust formatting, Clippy with all targets and
features with warnings denied, Cargo check, 342 unit tests, and 211 integration
tests; three operator-workbook tests remained intentionally ignored. Focused
evidence verifies that printer inventory state and the latest caller-owned queue
report include the selected route, print options, attempts, outcomes, and
available error details while excluding source document names and paths.

All 23 Node tests and 52 React tests passed. Version, documentation, source, and
dependency-license audits plus the production frontend build passed. The known
non-failing frontend chunk-size advisory remains. The Windows x64 Rust check and
complete test-target compile passed; Windows test executables were not run on
this macOS host.

No Windows package was produced for this checkpoint. Native Windows spooler and
EPSON L15160 Series paper-output acceptance remain external. The diagnostics
make a failed or uncertain run exportable and comparable; they do not prove
physical output and do not authorize automatic retry or possible duplicate
printing.

## 1.3.0 - 2026-09-17

Incremental replica hashing passed Rust formatting, Clippy with all targets and
features with warnings denied, Cargo check, 340 unit tests, and 211 integration
tests; three operator-workbook tests remained intentionally ignored. Focused
evidence verifies first-run hashing, a second preview with all unchanged hashes
reused, root-scoped durable index records, identical planned effects, and the
existing Apply-time source/target hash validation, backup, journal, and Undo path.

All 23 Node tests and 52 React tests passed. Version, documentation, source, and
dependency-license audits plus the production frontend build passed. The known
non-failing frontend chunk-size advisory remains. The Windows x64 Rust check and
complete test-target compile passed; Windows test executables were not run on
this macOS host.

No Windows package was produced for this checkpoint. Native Windows,
OneDrive/SharePoint, GlobalProtect, startup, unattended scheduling, interruption,
20 GB index performance, and operator Apply/Undo acceptance remain external.
Printer hardware behavior was not changed or certified by this release.

## 1.2.0 - 2026-09-16

The source-authoritative replica publication release passed Rust formatting,
Clippy with all targets/features and warnings denied, Cargo check, 339 unit
tests, and 211 integration tests; three operator-workbook tests were
intentionally ignored. Focused evidence covers exact scope, root-overlap and
symlink refusal, parallel hashing, source/target drift checks, retained
backup-first replacement, final hash verification, serial fail-fast Apply,
target-extra preservation, journal, and guarded Undo.

All 23 Node tests and 52 React tests passed. Version, documentation, source,
and dependency-license audits plus the production frontend build passed. The
frontend retained its known non-failing chunk-size advisory. The Windows x64
cross-check passed, including the native current-user startup Registry API and
the closed read-only replica-status command.

The Windows x64 cross-build completed from the final release worktree. The
OCR-inclusive ZIP passed package audit and archive integrity checks, all
generated checksums verified, and both executable artifacts identify as Windows
GUI-subsystem PE files. Cross-linking emitted only known non-failing Microsoft
PDB debug-information warnings. The macOS 27 proc-macro LINKEDIT workaround was
applied only to the build environment/configuration; the repository configuration
was restored afterward. Artifact SHA-256 values:

- portable-with-OCR ZIP: `50ea3e007f053cb3f411b6f98ed533777ec475eba42b5c5dab74a006f804599b`;
- portable EXE: `f478e4f3a0358ef181c8b8e779716fec9fd50a181c15d1e4bb073f171740374e`;
- per-user installer: `0cdef85769bd118222f5c38e9715e6c8c1bd10d56c2494241c2f7e584cc375d9`.

Native Windows startup, WebView2, SharePoint/OneDrive, GlobalProtect/network,
offline/reconnect, lock, restart, unattended scheduling, and operator Apply/Undo
acceptance have not run. The cross-build is not evidence for those external gates.

## 1.1.0 - 2026-09-16

Completion Status Sync at implementation commit
`37bb37cfc64e3bdc63ddf2b28faeae4e5993b138` passed Rust formatting, Clippy
with all targets/features and warnings denied, Cargo check, 336 unit tests, and
210 integration tests; three operator-workbook tests were intentionally ignored
in the complete suite. The new evidence covers recursive exact-name discovery,
numeric-tail rejection, guarded blank/Ongoing-to-Completed changes, terminal and
formula refusal, selection revision, unrelated-formula preservation, Apply,
journal, and Undo. The completion operator test was also run explicitly against
the available prior RFI Log copy and passed. The latest removable-volume copy
was not readable during this session and was not claimed as verified.

All 23 Node tests and 52 React tests passed. Version, documentation, source, and
dependency-license audits plus the production frontend build passed; the build
retained its non-failing chunk-size advisory.

The Windows x64 cross-build completed from that implementation commit. The
OCR-inclusive ZIP passed package audit and archive integrity checks, all
generated checksums verified, and both executable artifacts identify as Windows
GUI-subsystem PE files. Cross-linking emitted only the known non-failing
Microsoft PDB debug-information warnings. The macOS 27 host hit Rust's proc-macro
LINKEDIT alignment defect; the documented macOS 11 deployment-target workaround
was applied only to the build environment/configuration and the repository
configuration was restored before evidence was committed. Artifact SHA-256
values:

- portable-with-OCR ZIP: `49ed1b8397cd65f17deb0b6a4ad5352b7832bc7eb2c546050dd2fc46e64ba4af`;
- portable EXE: `0cf92efa0793bddbae418a4c2ca04524133e6816f64aee9bdeda3c8036669f4b`;
- per-user installer: `3ad34760d8df6a390931dfe2f635792ef52bb44e0cba5bbd2eeea78ac75035c7`.

Cross-build evidence does not establish native Windows, WebView2, OneDrive,
Excel repair-free open/recalculate/save, real-system traversal, locking,
performance, or operator acceptance; those gates were not run.

## 1.0.2 - 2026-09-15

The Windows spool-observation fix at implementation commit
`c5d9e3c0daf5a0230a1bd8400462b2f8092c6767` passed Rust formatting, Clippy with
all targets/features and warnings denied, Cargo check, 336 unit tests, and 208
integration tests; two operator-workbook tests were intentionally ignored. The
new unit evidence verifies that a transient generic driver error remains under
observation, completion wins over a stale generic error, destructive deletion
remains uncertain, a unique renamed native-handler job is correlated, and
ambiguous concurrent jobs are not guessed. One initial parallel full-suite run
hit the existing test-only global mutation-lock collision; its focused rerun and
the complete serial suite passed. All 23 Node tests and 51 React tests passed.
Version, documentation, source, dependency-license, and production frontend
build checks passed; the frontend build retained its non-failing chunk-size
advisory.

The Windows x64 cross-build completed from that implementation commit. The
OCR-inclusive ZIP passed package audit and archive integrity checks, all
generated checksums verified, and both executable artifacts identify as Windows
GUI-subsystem PE files. Cross-linking emitted only the known non-failing
Microsoft PDB debug-information warnings. Artifact SHA-256 values:

- portable-with-OCR ZIP: `42a81f288163e82b89baa0e0e0c9c9bd1423e76a8dda981014aba71348f0670d`;
- portable EXE: `0db289bca98aa7cda4db7f06ef2144476db4b5268fcd6e5c72d3e56bf607fd8d`;
- per-user installer: `731fc71abcaec10ae173160fe8fa6a09f633cc2a7c2bf6042c479e12fbf78e48`.

Cross-build evidence does not establish physical output or native EPSON L15160,
Windows spooler, Office handler, OneDrive, or operator acceptance; those gates
were not run. In particular, software cannot infer that paper exited merely
from a spool completion/release signal.

## 1.0.1 - 2026-09-15

The Word checklist boundary fix at implementation commit
`abfa47f2359978995a590c4295b7d14ea4d85854` passed Rust formatting, Clippy with
all targets/features and warnings denied, Cargo check, 335 unit tests, and 208
integration tests; two operator-workbook tests were intentionally ignored. The
new regression first failed against the previous extractor, then passed and
proves that a wrapped `AZE-SUN-INT-QMS-RFI-` / `MEC-0765` in a Word table is
recognized without joining the following `MOS No.` cell. All 23 Node tests and
51 React tests passed. Version, documentation, source, and dependency-license
audits plus the production frontend build passed; the build retained its
non-failing chunk-size advisory.

The Windows x64 cross-build completed from that implementation commit. The
OCR-inclusive ZIP passed package audit and archive integrity checks, all
generated checksums verified, and both executable artifacts identify as Windows
GUI-subsystem PE files. Cross-linking emitted only the known non-failing
Microsoft PDB debug-information warnings. Artifact SHA-256 values:

- portable-with-OCR ZIP: `d3c471e1a26aada800228c27c748482926a3de40cddd0253700226cef9a57134`;
- portable EXE: `1f0df7a0eea89cddbfbc9ad293144c42f0062b7bf9113ecd003bee1557604961`;
- per-user installer: `e4512783e2a331c0838257f9301d4c6038d94be52b9e7718062501658e3aba05`.

Cross-build evidence does not establish native Windows, WebView2, real Word
checklist, OneDrive, Outlook, or operator acceptance; those gates were not run.

## 1.0.0 - 2026-09-15

The breaking retirement of RFI Work Tracking, Device Readiness, Record RFI
Work, and Export RFI Work Audit at implementation commit
`d04e2306941d83b319afe186b43286d049cb604b` passed Rust formatting, Clippy with
all targets/features and warnings denied, Cargo check, 335 unit tests, and 207
integration tests; two operator-workbook tests were intentionally ignored. All
23 Node tests and 51 React tests passed. Version, documentation, source, and
dependency-license audits plus the production frontend build passed; the build
retained its non-failing chunk-size advisory. Registry/package evidence confirms
that exactly nine document operations remain and the retired support/list
routes are rejected.

The Windows x64 cross-build completed from that implementation commit. The
OCR-inclusive ZIP passed package audit and archive integrity checks, all
generated checksums verified, and both executable artifacts identify as Windows
GUI-subsystem PE files. Artifact SHA-256 values:

- portable-with-OCR ZIP: `6d7e0c7930a243304ef6c0f84b08ced59fe4ba7a4bb98c2c902a712c56207e1e`;
- portable EXE: `aeec5416e02bd3863cd63e14449c2d97f7e531e2a06c8bc6dab226fa227436e9`;
- per-user installer: `7ef5ec06a47bc6a1d242b0fd83647550b553923727f73e52ad16e4b8a6d8dfce`.

Cross-build evidence does not establish native Windows, WebView2, Office,
OneDrive, Outlook, physical-printer, or operator acceptance; those gates were
not run. Existing local work-event files were deliberately left untouched as
inactive legacy data.

## 0.28.1 - 2026-09-15

The per-RFI email prerequisite and bounded wrapped-identifier fix at implementation
commit `85928a6a70487534cdba9712df5b5d94437137c5` passed Rust formatting,
Clippy with all targets/features and warnings denied, Cargo check, 339 unit tests,
and 220 integration tests; two operator-workbook tests were intentionally ignored.
Focused tests prove the screenshot form `AZE-SUN-INT-QMS-RFI-\nMEC-0765`, bounded
line-wrap rejection, complete refusal when mail is absent, independent eligibility
inside a multi-RFI file, source preservation, and journaled Apply/Undo with mail.
All 23 Node tests and 53 React tests passed. Version, documentation, source, and
dependency-license audits plus the production frontend build passed; the build
retained its non-failing chunk-size advisory.

The Windows x64 cross-build completed from that implementation commit. The
OCR-inclusive ZIP passed package audit, all generated checksums verified, and the
portable and installer executables identify as Windows GUI-subsystem PE files.
Cross-linking emitted only the known non-failing Microsoft PDB debug-information
warnings. Artifact SHA-256 values:

- portable-with-OCR ZIP: `0defab62dfa142314a9d04fc8ab49eb6247c604dd959457c78aed6c67231e969`;
- portable EXE: `ca1795071a847a71427a94f8dee639069c9572fa0b366328c7323e6e809ac8fd`;
- per-user installer: `8838fdcf55cb1333dde8a53fb59ebc64f30fb2d842eb37df372b843590867189`.

Cross-build evidence does not establish native Windows, OneDrive, Outlook
MSG/EML, WebView2, or live sync-lock acceptance; those gates were not run.

## 0.28.0 - 2026-09-15

The safe multi-RFI file-grouping implementation at commit `f3dec8d` passed Rust formatting, Clippy
with all targets/features and warnings denied, Cargo check, 339 unit tests, and
217 integration tests; two operator-workbook tests were intentionally ignored.
Focused evidence covers canonical filename matches, content-only Office files,
UTF-8 EML and UTF-16 MSG multi-RFI fan-out, ignored source subfolders, review
copies for unmatched/unreadable files, strict rejection of decorated identifiers,
separate-root enforcement, revise filtering, source-preserving Apply, journal,
destination-collision handling, and Undo. All 23 Node tests and 53 React tests passed. Version, documentation,
source, and dependency-license audits plus the production frontend build passed;
the build retained its non-failing chunk-size advisory.

The Windows x64 cross-build completed from `f3dec8d`. The OCR-inclusive ZIP
passed package audit, generated checksums verified, and both executable artifacts
identify as Windows GUI-subsystem PE files. Cross-linking emitted only the known
non-failing Microsoft PDB debug-information warnings. Artifact SHA-256 values:

- portable-with-OCR ZIP: `787aa47d101f7a1e23d3776cd252c936a2dcd37b44ecada242bd9c1a29a7e693`;
- portable EXE: `7dbbb900c4c04d2a6e199dacdbe09347331c41ef8b619d2ff8c20516278ea120`;
- per-user installer: `e921a1eb501b84a885ba564bfe50194c266c6313c66a597d719ee4ca9909e38b`.

Cross-build evidence does not establish native Windows, OneDrive, Outlook
MSG/EML, WebView2, or live sync-lock acceptance; those gates were not run.

## 0.27.0 - 2026-09-15

Bounded parallel discovery and analysis plus operation-wide progress stages were
verified from implementation commit `540e352ddc96b701638d5bfc2c41312c113d9a79`.
Rust formatting, Clippy with all targets/features and warnings denied, Cargo
check, 338 unit tests, and 213 integration tests passed; two operator-workbook
tests were intentionally ignored because they require supplied workbook copies.
The version, documentation, source, package, and dependency-license audits
passed, as did 23 Node tests, 53 React tests, and the production frontend build.
The frontend retained its non-failing chunk-size advisory. Local Rust commands
also reported the host's unaccepted Xcode license, but direct toolchain paths
allowed the checked commands to complete.

The Windows x64 cross-build completed from that commit. The OCR-inclusive ZIP
and installer passed package audit, all generated checksums verified, and both
executables identify as Windows GUI-subsystem PE files. Cross-linking emitted
non-failing Microsoft PDB debug-information warnings. Artifact SHA-256 values:

- portable-with-OCR ZIP: `83c45a14f6a27c623ddb0a44cbd47566f96c9ab3173b8ec2b3f9a41a5c739718`;
- portable EXE: `20309814c9c92fe4012f65b3eb0f720946545c4b209416bcbfc4159bb013d5ed`;
- per-user installer: `b1e71a03d6c7a5942859b86929c796704214738282a337314e09be7692c19f35`.

Cross-build evidence does not establish native Windows, WebView2, Office,
OneDrive, Outlook, or physical-printer acceptance; those gates were not run.

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

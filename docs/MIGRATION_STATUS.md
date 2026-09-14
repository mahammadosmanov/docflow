# Rust migration execution status

Updated by verified implementation runs. A slice checkbox records source/local verification on the
current worktree; external Windows-host, Office, printer, signing, notarization, and live-service
certification remain separate unchecked gates and are never implied by a source slice.

- [x] Version 0.21.0: exact RFI Log to Follow Up projection
  - uniquely mapped CIV/MEC records update only existing CIV, MEC, PCU, and Road
    RFI/status pairs; absent sections and ambiguous routes fail closed
  - grouped identifiers support exact compact and bounded range forms;
    Cancelled/Rejected tokens are removed and shared status is recalculated
  - existing FINAL Worley CIV/MEC tables follow only an exact `Signed by Worley`
    REMARKS line; POT test, STAT, Changelog, ELE, and PRC remain untouched
  - macOS source verification passed: 334 Rust unit tests and 208 integration
    tests serially, 23 Node tests, 51 React tests, Clippy with warnings denied,
    frontend build, version/docs/source audits, and dependency-license audit
  - a separate read-only run against operator-supplied workbook copies passed
    preview, revise, private temporary patch, and closed OOXML delta verification
  - Windows cross-package and Excel/OneDrive native acceptance are recorded
    separately and are not implied by these source and package-fidelity checks

- [x] Version 0.20.0: safe multi-core PDF orientation normalization
  - recursively inspects PDFs only below CIV, MEC, ELE, and PRC trees and uses
    content-based Tesseract OSD to approve an exact rotation for every page
  - the selected RFI system stays read-only; approved copies are published into
    a separate no-clobber review tree with the same relative folder layout
  - one uncertain page rejects the whole PDF, while encrypted, signed, malformed,
    changed, and colliding inputs fail closed before publication
  - Process Scans and combined Worley recognition share the bounded Rust worker
    pool; mutation, journaling, Undo, Office work, and printing remain serial
  - macOS source verification passed: 331 Rust unit tests and 208 integration
    tests, with one operator-workbook test intentionally ignored; warnings-denied
    Clippy, fmt, check, 23 Node tests, 51 React tests, production frontend build,
    source/version/docs audits, and the 287-Cargo/173-npm/60-OCR license audit
  - Windows package, native Tesseract OSD, OneDrive review-tree, and current
    production-document acceptance remain external gates and were not run here

- [x] Version 0.19.0: machine-checked maintainer and AI onboarding canon
  - ordered documentation now covers requirements, all ten operations, exact
    RFI rules, mutation/recovery state, generic editors, traceability, known
    limitations, and Windows build/operator acceptance
  - active Tauri command authority is documented before the retained historical
    sidecar protocol, and both `AGENTS.md` and `CLAUDE.md` route new models to the
    same source-of-truth order
  - a Node audit fails on missing guides, broken map links, undocumented or
    unregistered operations, and requirements without traceability owners
  - this documentation/source gate does not claim the still-external Windows,
    Office, OneDrive, printer, live-service, signing, or notarization evidence

- [x] Version 0.18.3: current-version documentation enforcement
  - the version tool atomically updates runtime manifests plus the README,
    product definition, and roadmap current-version markers
  - repository validation rejects stale human-facing markers, including the
    previously stale 0.13 roadmap heading
  - Node contract tests cover automatic updates and fail-closed drift detection;
    external Windows, Office, OneDrive, and printer gates are unchanged

- [x] Version 0.18.2: non-interactive macOS credential restore
  - passive startup requests the exact Keychain item with authentication UI
    disabled, so the splash screen cannot wait on a hidden authorization prompt
  - unavailable or interaction-protected credentials remain untouched and fail
    closed to signed out; native-only token storage is unchanged
  - Windows behavior, explicit credential writes, and explicit deletes are
    unchanged
  - macOS source verification passed: 329 Rust unit tests and 200 integration
    tests, with one operator-workbook test intentionally ignored; warnings-denied
    Clippy, fmt, check, 20 Node tests, 51 React tests, production frontend build,
    source/version audits, and the 282-Cargo/173-npm/60-OCR license audit
  - the unsigned 0.18.2 package passed tree audit and packaged OCR/apply/journal/
    undo smoke (17 ms bootstrap/settings, 300 ms first OCR, 382 ms operation)
  - process sampling after normal launch confirmed bootstrap was no longer
    blocked in Keychain restore; final visual GUI inspection remained pending
    only because the macOS session locked during verification

- [x] Version 0.18.1: mutation-free readiness and stale-root recovery listing
  - dashboard readiness uses the same bounded journal parser without acquiring
    mutation permits, reopening workbook roots, reconciling effects, or writing
    journal state
  - finalized Follow Up recovery notices are returned directly from their
    validated local record even when the former document root identity is stale
  - pending and uncertain records remain visible and are never retried or removed
    by startup inspection
  - macOS source verification passed: 329 Rust unit tests and 199 integration
    tests, with one operator-workbook test intentionally ignored; warnings-denied
    Clippy, fmt, check, 20 Node tests, 51 React tests, production frontend build,
    source/version audits, and the 282-Cargo/173-npm/60-OCR license audit
  - the unsigned 0.18.1 package passed tree audit and packaged OCR/apply/journal/
    undo smoke (22 ms bootstrap/settings, 307 ms first OCR, 410 ms operation),
    but normal GUI launch exposed a separate macOS Keychain read stall; this is
    not counted as a successful normal-launch gate and is addressed separately

- [x] Version 0.18.0: exact existing-cell workbook comparison
  - guarded RFI System and Follow Up steps expose workbook, sheet/cell, canonical
    RFI, expected current value, and exact replacement value before Apply
  - the bounded view derives only from approved step metadata and performs no
    workbook read, recalculation, or mutation
  - invalid or oversized display fields fail closed and at most 1,000 approved
    cell changes cross the presentation boundary
  - macOS source verification passed: 328 Rust unit tests and 199 integration
    tests, with one operator-workbook test intentionally ignored; warnings-denied
    Clippy, fmt, check, 20 Node tests, 51 React tests, production frontend build,
    source/version audits, and the 282-Cargo/173-npm/60-OCR license audit

- [x] Version 0.17.0: warning-focused review
  - previews with five or more warnings provide local text search, type filtering,
    result counts, and a compact scroll region without mutating Plan warnings
  - signed-page warnings focus an existing comparison only after exact canonical
    ASCII RFI matching against a current editor row
  - filtering and focus do not revise candidate choices, steps, or Apply authority
  - macOS source verification passed: 328 Rust unit tests and 199 integration
    tests, with one operator-workbook test intentionally ignored; warnings-denied
    Clippy, fmt, check, 20 Node tests, 50 React tests, production frontend build,
    source/version audits, and the 282-Cargo/173-npm/60-OCR license audit

- [x] Version 0.16.0: bounded Worley first-page comparison
  - signed-page staging shows the pinned signed page beside the selected archive
    candidate's first page before Apply
  - grayscale PNG previews are read-only, row/token-bound, and capped by row,
    dimensions, per-image bytes, and total Plan bytes
  - unsupported or exhausted rendering omits the image without changing steps,
    editor validity, or Apply authority
  - macOS source verification passed: 328 Rust unit tests and 199 integration
    tests, with one operator-workbook test intentionally ignored; warnings-denied
    Clippy, fmt, check, 20 Node tests, 49 React tests, production frontend build,
    source/version audits, and the 282-Cargo/173-npm/60-OCR license audit
  - packaged Windows PDF rendering and representative OneDrive acceptance remain
    pending and are not implied by macOS source verification

- [x] Version 0.15.0: exact approved effects in editable previews
  - record editors expose a collapsed bounded list of immutable non-directory
    Plan steps with complete source and destination paths
  - the disclosure performs no backend read, recomputation, or mutation and does
    not change Apply authority
  - 20 Node tests, 48 React tests, production frontend build, source/version
    audits, warnings-denied Clippy, fmt, check, and the full Rust suite passed

- [x] Version 0.14.0: shared Apply-impact summary
  - ordinary and schema-driven editor previews display compact localized counts
    for outputs, updates, moves, printing, drafts, cleanup, and warnings
  - counts are presentation-only projections of immutable approved steps and
    cannot expand Plan authority
  - macOS source verification passed: 327 Rust unit tests and 199 integration
    tests, with one operator-workbook test intentionally ignored; warnings-denied
    Clippy, fmt, check, 20 Node tests, 48 React tests, production frontend build,
    source/version audits, the 282-Cargo/173-npm/60-OCR license audit, and a
    real-browser accessibility snapshot

- [x] Version 0.13.1: responsive combined-scan preview
  - current page identity is emitted during recognition and cancellation is
    checked before every page, with zero document effects during preview
  - focused cancellation regression, warnings-denied Clippy, fmt/check, Node and
    React tests, frontend build, and version/source audits passed on macOS
  - packaged Windows responsiveness remains part of the pending 0.13 acceptance

- [x] Version 0.13.0: combined Worley scan page staging
  - every page in a combined signed PDF is recognized independently and shown as
    a separate review candidate with its exact page number
  - approved Plans pin the source fingerprint and page index; apply imports only
    that page and never splits or mutates the source scan or archive
  - unreadable, low-confidence, duplicate, unsupported, missing, and ambiguous
    pages fail closed, and total scan work is bounded to 20,000 pages
  - macOS source verification passed: 327 Rust unit tests and 198 integration
    tests, with one operator-workbook test intentionally ignored; warnings-denied
    Clippy, fmt, check, 20 Node tests, 47 React tests, production frontend build,
    source/version audits, and the 282-Cargo/173-npm/60-OCR license audit
  - Windows packaged OCR and OneDrive-backed acceptance remains pending and is
    not implied by macOS source verification

- [x] Version 0.12.0: portable RFI work audit and capacity visibility
  - owner-scoped valid events export together as lossless JSON and Excel-safe
    CSV through preview, approval, no-clobber writes, journal, and undo
  - invalid and foreign-owner events are never copied into the audit; formula-like
    CSV text is neutralized and each output is bounded to 16 MiB
  - the home queue reports valid event count, owned bytes, and the 10,000-event
    ceiling without automatically deleting or changing history
  - macOS source verification passed: 327 Rust unit tests and every integration
    suite, with one operator-workbook test intentionally ignored; warnings-denied
    Clippy, fmt, check, 20 Node tests, 47 React tests, production frontend build,
    source/version audits, and the 282-Cargo/173-npm/60-OCR license audit
  - Windows/OneDrive destination acceptance remains pending and is not implied
    by macOS source verification

- [x] Version 0.11.0: journal-linked RFI work transitions
  - eligible work cards launch RFI Log, Process Scans, or two-workbook Worley
    System Sync and offer a separate explicitly approved tracking transition
    only after a fully committed same-owner journal proves exact RFI coverage
  - partial, cancelled, failed, missing, foreign, undone, mismatched, review-only,
    single-workbook, and Outlook-draft evidence fail closed
  - macOS source verification passed: 326 Rust unit tests plus 192 integration
    tests, with one operator-workbook test intentionally ignored; warnings-denied
    Clippy, fmt, check, 20 Node tests, 47 React tests, production frontend build,
    source/version audits, and the 282-Cargo/173-npm/60-OCR license audit
  - Windows/Office/OneDrive runtime acceptance remains pending and is not implied
    by the macOS source result

- [x] Phase 0: GPT/Codex canon and cloud-undo containment
  - 145 Python tests, mypy, 9 frontend tests/build, Rust fmt/Clippy/tests/check
  - packaged OCR and forged-undo sentinel probe passed
  - live Supabase SQL deployment remains external/pending
- [x] Slice 1: Rust-owned transitional backend boundary
  - React uses Tauri invoke/channels only; webview shell permissions removed
  - Rust allowlist, lazy supervisor, JSON-lines framing, progress, failure, and shutdown tests pass
- [x] Slice 2: Rust contracts, registry, settings, plan store, progress/cancellation
  - protocol-1 Serde snapshots and the full Python operation-registry fixture pass
  - atomic compatible settings, opaque one-shot plans, expiry/forgery/reuse rejection,
    lock-before-claim concurrency, and request cancellation tests pass
  - 18 Rust tests plus fmt/Clippy/check, 145 Python tests plus mypy/version check,
    and 10 frontend tests plus build passed on macOS
- [x] Slice 3: Rust Supabase auth, secure credentials, Gemini, cloud metadata
  - Rust owns protocol auth/PostgREST/cloud routing; migrated calls do not start Python
  - Supabase sessions use macOS Keychain or Windows Credential Manager, with verified
    one-time plaintext import and downgrade-safe public metadata
  - cloud history requests metadata fields only; local journals alone enable undo
  - bounded Supabase/Gemini HTTP and offline fixtures cover stable errors and secret handling
  - 35 Rust tests plus fmt/Clippy/check, 147 Python tests plus mypy/version check,
    and 10 frontend tests plus build passed on macOS
  - the Windows credential module passed an offline `x86_64-pc-windows-msvc` compile check
- [x] Slice 4: Rust filesystem grants, no-clobber primitives, journal, recovery, undo
  - request/owner/session-bound grants, immutable exact-plan apply, atomic no-clobber
    move/copy/write, write-ahead journals, restart recovery, cancellation states, and
    hash-protected reverse undo pass isolated adversarial fixtures
  - legacy Python journal omissions/bytes remain readable and recoverable review delete
    is a Rust Plan → apply → journal → trusted-local undo move to application trash
  - 61 Rust tests (15 filesystem-authority/fault tests) plus fmt/Clippy/check, 147 Python
    compatibility tests plus mypy/version check, and 10 frontend tests plus build passed
    on macOS; Windows runtime semantics remain a later host gate
- [x] Slice 5: Rust RFI matching, naming, OCR, Process Scans, review
  - one-pass NFC+uppercase canonicalization, exact `-RFI-` anchor behavior, deterministic
    naming/revision selection, and shared Rust/Python ambiguity/decoy fixtures pass
  - searchable-text and image-only PDF paths, bounded preprocessing/Tesseract execution,
    fail-closed OCR, native Process Scans/review routing, immutable approved plans,
    progress/cancellation, no-clobber apply, journal, and undo pass isolated fixtures
  - 74 Rust tests (16 focused Slice 5 tests) plus fmt/Clippy/check, 150 Python
    compatibility tests plus mypy/version check, and 10 frontend tests plus build passed
    on macOS
  - an unsigned macOS application bundle passed real Rust-path image-only PDF OCR using
    its packaged relocatable Tesseract; all 60 resource files are checksum-covered, English
    tessdata and 43 license-evidence files are present, and declared/concluded license checks
    found no AGPL, Affero, or commercial component
  - the Windows PDF/OCR adapter passed an offline `x86_64-pc-windows-msvc` compile check;
    Windows packaged/runtime OCR remains a Windows-host gate
- [x] Slice 6: Rust email extraction, OOXML/RFI log, clipboard
  - native offline/Gemini extraction, RFI Log preview/revise/apply/table routes,
    Rust-owned clipboard output, targeted ZIP/XML mutation, verified backup,
    atomic replacement, recovery, and hash-protected undo are implemented
  - frozen Python-oracle preview parity and deterministic XLSX/XLSM corpus hashes pass;
    untouched sheets, styles, formulas, relationships, defined names, VBA, drawings,
    comments, external links, metadata, ZIP order/comments/properties, and compressed
    bytes are asserted where applicable
  - 85 Rust tests (15 focused Slice 6 tests) plus fmt/Clippy/check, 150 Python
    compatibility tests plus mypy/version check, and 10 frontend tests plus build passed
    on macOS
  - externally pending: Windows Office open/recalculate, Excel/CF_HTML clipboard,
    real workbook-lock/retry, and runtime path certification remain required host gates;
    the offline Windows cross-check could not resolve an uncached target dependency
- [x] Slice 7: Rust signed-page PDF staging
  - native preview/revise/apply routes use strict canonical RFI matching, immutable approved
    plans, bounded read/write grants, same-directory no-clobber publication, write-ahead
    journals, recovery, hash-protected undo, and narrowly authorized stale-link cleanup;
    at this gate Python fallback routing was limited to batch printing
  - incremental object mutation retains the archive as an exact byte prefix and explicitly
    preserves later-page objects, metadata/XMP, attachments, outlines/bookmarks, labels,
    forms, annotations, links, page boxes/rotation, object streams, and unrelated catalog
    structures; encrypted, digitally signed, tagged, malformed, page-one-widget, and other
    unsupported fidelity cases fail closed with reviewable warnings
  - archive discovery accepts only case-insensitive exact `RFI No. <canonical-code>.pdf`
    basenames; multiple valid matches require an archive-root-relative user selection whose
    precomputed path and hashes are rebound into the revised plan and revalidated during apply
  - 102 Rust tests (14 focused signed-page corpus tests) plus fmt/Clippy/check, 150 Python
    compatibility tests plus mypy/version check, and 10 frontend tests plus build passed on
    macOS; a rendered 144-DPI output visually confirmed the signed rotated/cropped first page
  - the maintained compression dependencies and the 175-package npm graph passed declared
    license checks with no AGPL, Affero, commercial, or missing npm license declaration
  - the actual PDF parser/publication modules passed an offline warnings-denied
    `x86_64-pc-windows-msvc` compile check; the full Tauri cross-check remains unavailable
    because its pre-existing `windows-sys 0.60.2` target dependency is not cached, and
    Windows packaged/runtime behavior remains a Windows-host gate
- [x] Slice 8: Rust batch printing and queue recovery
  - `files.batch_print` preview/revise/apply/cancel and print-queue latest/resume/discard are
    Rust-routed; current user operation flows no longer require the Python fallback
  - immutable approved plans, path grants, durable pre-submission queue state, local audit
    journals, progress/cancellation, retry/timeout/on-error handling, exact job ordering,
    partial results, and explicit uncertain-job recovery pass fake-adapter fixtures
  - macOS uses bounded direct CUPS commands with closed arguments and enumerated destinations;
    Windows source uses EnumPrinters/Winspool/GDI and built-in GDI+ codecs for controlled PDF/image
    jobs and registered native print/printto handlers for Office and other associated formats with
    limited-setting warnings
  - 114 Rust tests (8 focused queue/fake-adapter tests and 4 macOS adapter tests), fmt,
    warnings-denied Clippy, and check; 150 Python oracle tests plus mypy/version check; and
    10 frontend tests plus build passed on macOS without submitting a printer job
  - the Windows printing adapter and tests plus the Windows PDF renderer passed offline
    `x86_64-pc-windows-msvc` compile checks; the full Tauri cross-check remains unavailable
    because its pre-existing `windows-sys 0.60.2` target dependency is not cached
  - 123 npm packages, 288 macOS Rust third-party packages, and the 15-package Windows printing
    checker had declared licenses with no AGPL, Affero, or commercial match
  - externally pending: Windows-host runtime and Microsoft Print to PDF, Microsoft Office,
    isolated/physical/network printers, packaged installers, signing, and notarization
- [x] Slice 9: frontend typed commands, grants, progress, recovery UI
  - React uses a closed 28-method TypeScript command map and the sole `frontend_call` Tauri
    command; Rust and TypeScript roster-drift tests reject unknown, missing, internal, and legacy
    methods, and every React-reachable method is Rust-owned
  - all path selection and reveal requests are Rust-mediated, schema-scoped, caller-bound opaque
    selections; the webview has only `core:default` capability and no dialog, opener, shell,
    process, filesystem, network, storage, or credential authority
  - application bootstrap is native and cannot start or call the quarantined Python fallback;
    progress channels, cancellation, stable errors, immutable-plan apply, history/local-journal
    undo, uncertain print-queue recovery, and all `ParamSchema`-generated forms remain wired
  - the recovery center and explicit effect-state UI pass in Turkish, English, Russian, and
    Azerbaijani; no Python, sidecar, PyInstaller, or packaging artifact was removed because Slice
    10 owns that atomic deletion
  - 112 Rust tests plus fmt, warnings-denied Clippy, test, and check; 150 Python oracle tests plus
    mypy/version check; and 14 frontend tests plus production build passed on macOS
  - the Windows credential, OCR, PDF-staging, and printing source checkers passed offline
    `x86_64-pc-windows-msvc` checks; the complete Tauri application later passed the same
    release cross-check after the pinned CRT and dependency cache became available
  - externally pending: Windows-host runtime/installer, Office, physical/network printer,
    live-service, signing, and notarization certification remain separate gates
- [x] Slice 10: remove the retired runtime/packager/sidecar and replace its tooling/tests
  - all 28 React-reachable methods and all four operations are Rust-owned; router,
    manifest, frontend-roster, final-build guard, and repository/package audits
    reject a second backend, shell plugin, external binary, or removed runtime path
  - the retired application sources, manifests, tests, build output, binary staging,
    and package hooks are removed after the final 150-test compatibility handoff;
    behavior-to-Rust evidence is indexed in `docs/PARITY_CLOSURE.md`
  - Node version/checksum/source/package/license tools, Rust packaged smoke, and
    no-runtime Windows/macOS scripts replace every canonical repository command
  - 18 Node tooling tests, 19 frontend tests/build, and a maintained 168-test
    cross-platform Rust inventory (the canonical macOS run executes 164; target-specific
    tests are certified separately), with fmt, warnings-denied Clippy, check, source audit, and a
    281-Cargo/173-npm/60-native-resource host license audit passing without the retired runtime
  - an unsigned arm64 `.app` builds and its full tree/Mach-O audit passes; the identical
    release executable with bundled OCR resources passes bootstrap/settings plus real
    image-only OCR and Process Scans apply/journal/undo
  - independently rerun after the final source changes: every canonical source gate,
    no-runtime audit, package audit, release build, Rust package smoke, and ZIP integrity/
    checksum gate passed; LaunchServices registration and DMG creation remain host blockers,
    not source-migration blockers
- [x] macOS unsigned release build, launch/smoke, package inspection, size/startup comparison
  - the fresh arm64 `.app` is 31 MiB and the verified unsigned ZIP is 11 MiB versus the
    retired 92,979,744-byte sidecar; package smoke recorded 17 ms bootstrap/settings,
    308 ms first OCR, and 359 ms representative operation time
  - the packaged executable survived a direct normal launch for 20 seconds with a
    98,560 KiB RSS sample; full tree/Mach-O/no-runtime audit and real image-only OCR plus
    Process Scans apply/journal/undo smoke passed
  - DMG creation is blocked because this host's `hdiutil` reports `Device not configured`;
    Developer ID signing/notarization and clean-machine Gatekeeper certification remain pending
- [x] experimental Windows x64 cross-build from macOS
  - all focused Windows adapter crates and the complete application pass the
    `x86_64-pc-windows-msvc` release check after `cargo-xwin` acquired the pinned MSVC CRT
    and previously uncached `windows-sys 0.60.2` dependency
  - a 16 MiB PE32+ x86-64 GUI executable was produced, package-name audited, checksummed,
    and preserved as `artifacts/DocFlow_0.6.0_x64-windows-experimental.exe`
  - this compile artifact is not an NSIS installer and has not executed on Windows; native
    OCR resources, clean install/startup/uninstall, credentials, Office, and printing remain
    Windows-host gates
- [ ] Windows-host CI/runtime/installer/Office/printer certification
  - the Windows workflow builds/tests/packages on `windows-2022` and automates clean
    install, startup, OCR, apply/undo, installed-content audit, and uninstall
  - Office/Excel COM/native clipboard, representative physical/network printers,
    live service, signing, and notarization remain explicit separate gates

## Current measured baseline

- arm64 PyInstaller sidecar: 92,979,744 bytes
- cold sidecar `--ping`: 38.47 seconds wall time

## Hard blockers that cannot be waived

- A macOS cross-build is not Windows execution evidence.
- Physical/network printer certification needs a Windows machine and representative printers.
- Office recalculation/COM/Excel-native clipboard certification needs Windows with Microsoft Excel.
- Live Supabase RLS changes require authorized deployment and two-user verification.
- Signing/notarization requires the corresponding certificates and credentials.

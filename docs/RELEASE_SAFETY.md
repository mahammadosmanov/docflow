# Release safety

## Release invariants

1. `.github/workflows/windows-build.yml` defines the Windows build; it runs
   `flutter build windows --release` on a real Windows runner and uploads the zip.
2. Flutter SDK (3.47.7 in CI), `pubspec.lock`, and direct dependencies remain pinned.
3. Tesseract is bundled and user `PATH` is not trusted by installed packages.
4. Preview does not mutate files; packaged smoke verifies source/destination state.
5. Bootstrap, the exact thirteen-operation registry, settings, real image-only OCR,
   apply, journal, and undo are exercised beyond a process-start check.
6. A release tag equals `v` plus exact canonical `VERSION`.
7. Existing destinations, source archives, and unrelated workbook package members
   are never overwritten.
8. Source audits reject retired application runtimes, sidecars, and shell/external-binary
   configuration; the Dart engine is the only backend.

## Required source gates

```bash
node tools/version_manager.mjs check
node tools/docs_audit.mjs
cd flutter/app
flutter analyze
flutter test
```

`flutter analyze` must report zero errors. `flutter test` must be green with the
skip count unchanged or explained in the commit.

## Package gates

For the Windows package:

1. Build from the verified source and locked dependencies:
   `flutter build windows --release` (workflow `.github/workflows/windows-build.yml`).
2. Upload the release zip as a workflow artifact and record its SHA-256.
3. Launch the packaged executable on a real Windows host and verify it stays alive
   through bootstrap.
4. Record package size, cold startup, and first OCR timing against any recorded baseline.
5. Preserve the dependency-license report and OCR checksum/license evidence.

The macOS build cannot produce or certify the Windows package. See
`flutter/WINDOWS_TEST.md` for the manual Windows test steps.

## Regression matrix

| Area | Mandatory evidence | Risk controlled |
|---|---|---|
| Version contract | version check (`tools/version_manager.mjs`) | Mixed UI/installer versions |
| Documentation contract | Required-map and requirement-ID audit (`tools/docs_audit.mjs`) | New operator or coding agent follows missing/stale product guidance |
| Fast RFI Scan | Multi-/Single-RFI OCR and page behavior, unsupported-PDF review, OCR-only CLV alias, Dart revision/collision/folder tests, actual eligible-RFI cover fixtures baked at 0°/90°/180°/270° with matching Fast/standalone targets, parallel-analysis source-replacement rejection, rendered-output inspection, and Apply/Undo | OCR drift, wrong identity, unsafe separation, stale analysis, path tampering, missing engine/data, page loss, or hidden cross-RFI content |
| Signed pages | rotated/cropped/fidelity PDF tests | Archive mutation, wrong page geometry, partial output |
| PDF orientation | Canonical `/Rotate` normalization and shared decision policy; strict exact-RFI-header consensus/conflict tests; same-fingerprint source replacement and cache-hash/profile invalidation; non-cardinal/uncertain review; actual cardinal Fast RFI Scan parity; extracted-page rotation mapping with Apply, rendered-output inspection, source-hash preservation and Undo; no-clobber and fidelity tests | Wrong rotation, stale evidence/cache, source mutation, unrelated-tree scan, or damaged PDF graph |
| RFI Log | workbook/OOXML fixtures | Formula, validation, style, lock, and repair damage |
| RFI Follow Up | exact grouped-ID/mapping/status/removal tests; distinct-context CIV/MEC lifecycle and FINAL fan-out; source-order permutation, partial prepared-context review, duplicate/ambiguous/unrelated placement and shared-cell conflict refusal; cached-formula no-op and unsafe formula refusal; preview/revise/apply/undo; retained-storage exhaustion before mutation; current-workbook temporary-copy closed-delta check | Wrong-row updates, missing contexts silently skipped, conflicted source authorizing shared-cell writes, terminal records retained, excluded-sheet mutation, formula/Remarks damage, package damage, or recovery ambiguity |
| RFI System/Worley sync | exact-canonical-PDF and OTHERS-only detector regressions + shared-formula fixture + operator-supplied signed/unsigned PDFs and current-template guarded-delta test | Arbitrary-PDF fallback, signatures outside OTHERS, Folder Link formula overwrite, wrong control-table mapping, stale cached detection, stale apply, shared-formula or calc-chain repair |
| RFI completion-status sync | deep exact-name discovery, terminal/formula refusal, preview/revise/apply/undo, and current-RFI-Log temporary-copy closed-delta test | False completion, wrong-row update, source-tree mutation, workbook repair, or unrecoverable replacement |
| Outlook reply draft | fake-adapter identity/call-order/conditional-undo tests + Windows compile | Wrong message, recipient synthesis, accidental send, or deletion of a changed draft |
| Outlook MSG archive | fake-adapter selection/fingerprint/single-use authority tests, no-clobber Apply/Undo tests, registry/locale parity, and Windows compile | Wrong message, mailbox mutation, overwrite, private data in Plan, or unguarded archive deletion |
| Batch Print | fake-adapter queue/spooler tests + Excel pre-open COM-plan and PDF envelope/native-decoder delegation contracts + Windows test-target compile | Order loss, duplicated jobs, uncertain recovery, every worksheet failing before Open, or valid Excel PDFs being rejected before native decoding |
| Printer diagnostics | latest-owned-queue path redaction, bounded job roster, canonical copy validation, Windows test-target compile | Undiagnosable handler/driver failures, private path leakage, or logs becoming retry authority |
| Operation registry | Dart roster-drift test against the frozen `flutter/app/test/fixtures/frontend_commands.ts` | Omitted or unowned operation |
| Localization | four-catalog key equality | Raw keys or missing language strings |
| Security boundary | command/grant/RLS/credential tests | Tokens, cloud steps, or authority crossing into the UI layer |
| Credential startup | native-store contract test + normal packaged GUI launch | Hidden Keychain prompt, indefinite splash, token deletion, or UI-layer exposure |
| Package | `flutter build windows --release` on Windows CI + zip artifact + packaged launch | Source/package mismatch or missing runtime files |
| Incremental scanning | durable-index restart/fingerprint tests + orientation and System Sync cache-hit tests + generic Preview summary UI test | Full rescans after interruption, stale cross-owner/root/profile reuse, transient-failure caching, or cached evidence becoming mutation authority |
| Incremental replica comparison | first-run/follow-up cache evidence + Windows test-target compile + Apply-time hash revalidation tests | Rehashing an unchanged 20 GB tree, cross-root reuse, stale comparison, or cache-authorized mutation |
| Guarded automatic replica cadence | settle-window and mass-change-brake tests + registry/locale parity + Windows test-target compile | Publishing a file still being written, exceeding an approved run limit, silently revoking on an ordinary settle delay, or bypassing manual Preview |

## External certification gates

These are never implied by source, macOS, or cross-compile success:

- Windows clean-machine startup of the release zip (build runs in CI via
  `windows-build.yml`, but execution on a real host is still pending)
- Microsoft Office open/recalculate, Excel COM, and Excel-native clipboard
- Microsoft Outlook current-selection, Reply All, preserved signature/quote,
  auto-save, unchanged-draft undo, Unicode MSG export, collision refusal, and
  hash-guarded MSG undo
- Microsoft Print to PDF and representative physical/network printers
- live Supabase RLS deployment and two-user verification
- signing and macOS notarization with authorized credentials

## Release and rollback

Start from a clean reviewed commit, complete version/catalog notes, run all
applicable gates after the final edit, and never reuse a published tag. Verify
checksums before installation. Keep the previous verified release available until
the new package passes its host gates. Roll back code by reinstalling the prior
immutable tag and documents through their journals; never substitute source-code
rollback for document undo.

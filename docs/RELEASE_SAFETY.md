# Release safety

## Release invariants

1. `.github/workflows/windows-release.yml` and `tools/build_windows.ps1` define
   the distributable Windows build.
2. Node, Rust lockfiles, Tesseract, and direct dependencies remain pinned.
3. Tesseract is bundled and user `PATH` is not trusted by installed packages.
4. Preview does not mutate files; packaged smoke verifies source/destination state.
5. Bootstrap, the exact twelve-operation registry, settings, real image-only OCR,
   apply, journal, and undo are exercised beyond a process-start check.
6. A release tag equals `v` plus exact canonical `VERSION`.
7. Existing destinations, source archives, and unrelated workbook package members
   are never overwritten.
8. Source and package audits reject retired application runtimes, sidecars,
   bytecode, frozen-runtime artifacts, and shell/external-binary configuration.

## Required source gates

```bash
node tools/version_manager.mjs check
node tools/docs_audit.mjs
node tools/package_audit.mjs source
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
npm run audit:licenses
```

## Package gates

For each host package:

1. Build from the verified source and locked dependencies.
2. Audit the complete installed/bundle tree with
   `node tools/package_audit.mjs package <path>`.
3. Run `<packaged-executable> --package-smoke <new-report.json>`.
4. Launch the normal GUI and verify it remains alive through bootstrap.
5. Record package size, cold startup, peak memory, bootstrap/settings, first OCR,
   and representative operation timing against any available recorded baseline.
6. Create SHA-256 files with `node tools/checksum.mjs <artifact...>`.
7. Preserve the dependency-license report and OCR checksum/license evidence.

On Windows the canonical command is `./tools/build_windows.ps1`; the Actions
workflow then performs silent clean install, packaged smoke, installed-tree audit,
and silent uninstall on a real Windows runner.

## Regression matrix

| Area | Mandatory evidence | Risk controlled |
|---|---|---|
| Version contract | Node version check | Mixed UI/installer versions |
| Documentation contract | Required-map, requirement-ID, and operation-roster audit | New operator or coding agent follows missing/stale product guidance |
| Process Scans | parser/OCR tests + image-only packaged smoke | OCR drift, false labels, missing engine/data |
| Signed pages | rotated/cropped/fidelity PDF tests | Archive mutation, wrong page geometry, partial output |
| PDF orientation | discipline-only discovery, confidence fail-closed, incremental fidelity, no-clobber apply/undo tests | Wrong rotation, unrelated-tree scan, source mutation, damaged PDF graph |
| RFI Log | workbook/OOXML fixtures | Formula, validation, style, lock, and repair damage |
| RFI Follow Up | exact grouped-ID/mapping/status/removal tests, preview/revise/apply/undo, current-workbook temporary-copy closed-delta check | Wrong-row updates, terminal records retained, excluded-sheet mutation, package damage, or recovery ambiguity |
| RFI System/Worley sync | exact-canonical-PDF and OTHERS-only detector regressions + shared-formula fixture + operator-supplied signed/unsigned PDFs and current-template guarded-delta test | Arbitrary-PDF fallback, signatures outside OTHERS, Folder Link formula overwrite, wrong control-table mapping, stale cached detection, stale apply, shared-formula or calc-chain repair |
| RFI completion-status sync | deep exact-name discovery, terminal/formula refusal, preview/revise/apply/undo, and current-RFI-Log temporary-copy closed-delta test | False completion, wrong-row update, source-tree mutation, workbook repair, or unrecoverable replacement |
| Outlook reply draft | fake-adapter identity/call-order/conditional-undo tests + Windows compile | Wrong message, recipient synthesis, accidental send, or deletion of a changed draft |
| Outlook MSG archive | fake-adapter selection/fingerprint/single-use authority tests, no-clobber Apply/Undo tests, registry/locale parity, and Windows compile | Wrong message, mailbox mutation, overwrite, private data in Plan, or unguarded archive deletion |
| Batch Print | fake-adapter queue/spooler tests + Excel pre-open COM-plan and PDF envelope/native-decoder delegation contracts + Windows test-target compile | Order loss, duplicated jobs, uncertain recovery, every worksheet failing before Open, or valid Excel PDFs being rejected before native decoding |
| Printer diagnostics | latest-owned-queue path redaction, bounded job roster, canonical copy validation, Windows test-target compile | Undiagnosable handler/driver failures, private path leakage, or logs becoming retry authority |
| Operation registry | Rust/TypeScript roster-drift tests | Omitted or unowned operation |
| Localization | four-catalog key equality | Raw keys or missing language strings |
| Security boundary | command/grant/RLS/credential tests | Tokens, cloud steps, or authority crossing into webview |
| Credential startup | native-store contract test + normal packaged GUI launch | Hidden Keychain prompt, indefinite splash, token deletion, or webview exposure |
| Package | full tree audit + packaged smoke | Source/package mismatch or retired runtime inclusion |
| Incremental scanning | durable-index restart/fingerprint tests + orientation and System Sync cache-hit tests + generic Preview summary UI test | Full rescans after interruption, stale cross-owner/root/profile reuse, transient-failure caching, or cached evidence becoming mutation authority |
| Incremental replica comparison | first-run/follow-up cache evidence + Windows test-target compile + Apply-time hash revalidation tests | Rehashing an unchanged 20 GB tree, cross-root reuse, stale comparison, or cache-authorized mutation |
| Guarded automatic replica cadence | settle-window and mass-change-brake tests + registry/locale parity + Windows test-target compile | Publishing a file still being written, exceeding an approved run limit, silently revoking on an ordinary settle delay, or bypassing manual Preview |

## External certification gates

These are never implied by source, macOS, or cross-compile success:

- Windows clean-machine installer/startup/uninstall execution (automated in CI,
  but still pending until a workflow run from the exact source succeeds)
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

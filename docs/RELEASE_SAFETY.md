# Release safety

## Release invariants

1. `.github/workflows/windows-release.yml` and `tools/build_windows.ps1` define
   the distributable Windows build.
2. Node, Rust lockfiles, Tesseract, and direct dependencies remain pinned.
3. Tesseract is bundled and user `PATH` is not trusted by installed packages.
4. Preview does not mutate files; packaged smoke verifies source/destination state.
5. Bootstrap, the exact eight-operation registry, settings, real image-only OCR,
   apply, journal, and undo are exercised beyond a process-start check.
6. A release tag equals `v` plus exact canonical `VERSION`.
7. Existing destinations, source archives, and unrelated workbook package members
   are never overwritten.
8. Source and package audits reject retired application runtimes, sidecars,
   bytecode, frozen-runtime artifacts, and shell/external-binary configuration.

## Required source gates

```bash
node tools/version_manager.mjs check
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
| Process Scans | parser/OCR tests + image-only packaged smoke | OCR drift, false labels, missing engine/data |
| Signed pages | rotated/cropped/fidelity PDF tests | Archive mutation, wrong page geometry, partial output |
| RFI Log | workbook/OOXML fixtures | Formula, validation, style, lock, and repair damage |
| RFI Follow Up | exact-match preview/revise/apply/undo and package-fidelity tests | Wrong-row updates, unsafe status projection, package damage, or recovery ambiguity |
| RFI System/Worley sync | shared-formula fixture + operator-supplied current-template guarded-delta test | Folder Link formula overwrite, wrong control-table mapping, stale apply, shared-formula or calc-chain repair |
| Outlook reply draft | fake-adapter identity/call-order/conditional-undo tests + Windows compile | Wrong message, recipient synthesis, accidental send, or deletion of a changed draft |
| Batch Print | fake-adapter queue/spooler tests | Order loss, duplicated jobs, uncertain recovery |
| Operation registry | Rust/TypeScript roster-drift tests | Omitted or unowned operation |
| Localization | four-catalog key equality | Raw keys or missing language strings |
| Security boundary | command/grant/RLS/credential tests | Tokens, cloud steps, or authority crossing into webview |
| Package | full tree audit + packaged smoke | Source/package mismatch or retired runtime inclusion |
| Device readiness | pure fact classification + typed command/UI tests | Hidden OCR/printer failure or overlooked interrupted work |
| RFI work tracking | strict event validation + conflict projection + native preview/apply/list/undo + typed queue UI | Lost resume state, cross-user exposure, silent transition conflict, or coordination metadata authorizing document mutation |

## External certification gates

These are never implied by source, macOS, or cross-compile success:

- Windows clean-machine installer/startup/uninstall execution (automated in CI,
  but still pending until a workflow run from the exact source succeeds)
- Microsoft Office open/recalculate, Excel COM, and Excel-native clipboard
- Microsoft Outlook current-selection, Reply All, preserved signature/quote, auto-save, and unchanged-draft undo
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

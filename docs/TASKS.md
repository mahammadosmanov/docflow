# Project tasks

## Verification on every change

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

Project scope and priority are tracked in `PRODUCT.md` and `ROADMAP.md`.
Implementation tasks live in GitHub Issues; completed versions live in
`../releases.json` and `../CHANGELOG.md`.

For a local unsigned macOS package:

```bash
npm run tauri build -- --bundles app,dmg
node tools/package_audit.mjs package src-tauri/target/release/bundle/macos/DocFlow.app
src-tauri/target/release/bundle/macos/DocFlow.app/Contents/MacOS/docflow \
  --package-smoke artifacts/macos-package-smoke.json
```

On Windows use `./tools/build_windows.ps1`, followed by
`./tools/smoke_windows_installer.ps1 -Installer <path>` on a clean test runner.

## Printing Order v1

Source/local implementation is split into independently reviewed commits:

- `490d17c`: typed, operation-scoped print-setting presets;
- `6c89ae6`: generic preset UI and closed command parity;
- `6da70fe`: per-file canonical page ranges in preview, revise, queue, and recovery;
- `3742218`: exact selected-page execution in Windows PDF and macOS CUPS adapters;
- `80372a3`: browser/test protocol parity for the per-file page-range editor.

Presets contain print settings only. File selection, file order, worksheet selection,
and page ranges are order-local and are never persisted in a preset. PDF ranges are
fail-closed and exact; images use single-page semantics; native Office handlers reject
non-`all` ranges.

The following remain external certification gates and must not be inferred from local
or cross-target checks:

- exact installed Windows candidate behavior;
- representative multi-page PDF output and strict mixed-file printer order;
- macOS CUPS runtime output;
- physical/virtual printer cancellation and post-submission uncertainty;
- installer signing and publication.

## OCR calibration

Use isolated Rust fixtures and representative consented scans. Only change the
OCR cascade after measuring representative documents. Maintain at least 95%
exact coverage with zero wrong automatic labels and keep the strict `-RFI-`
anchor. The recursive RFI System audit accepts only exact cover filenames inside
their matching canonical folder as strong labels, never modifies the corpus,
and exits with status 2 for every OCR/reference mismatch. Folder and filename
agreement is reference evidence, not ground truth; visually resolve every
mismatch before making a precision claim:

```bash
DOCFLOW_OPERATOR_TESSERACT=<tesseract> cargo run \
  --manifest-path src-tauri/Cargo.toml --release \
  --example ocr_corpus_audit -- <RFI-System> 0 <outside-corpus-report.json> 4
```

## Adding a document operation

1. Add and register the Rust operation under `src-tauri/src/operations/`.
2. Declare its generic `ParamSchema`.
3. Add keys to all four catalogs in `frontend/src/locales/`.
4. Add pure behavior, filesystem safety, backend roster, packaged-smoke, and UI tests.

Do not add an operation-specific React form. Extend the generic schema renderer
only for a genuinely new parameter kind.

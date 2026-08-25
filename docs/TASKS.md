# Project tasks

## Verification on every change

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

## OCR calibration

Use isolated Rust fixtures and representative consented scans. Only change the
OCR cascade after measuring representative documents. Maintain at least 95%
extraction accuracy and keep the strict `-RFI-` anchor.

## Adding a document operation

1. Add and register the Rust operation under `src-tauri/src/operations/`.
2. Declare its generic `ParamSchema`.
3. Add keys to all four catalogs in `frontend/src/locales/`.
4. Add pure behavior, filesystem safety, backend roster, packaged-smoke, and UI tests.

Do not add an operation-specific React form. Extend the generic schema renderer
only for a genuinely new parameter kind.

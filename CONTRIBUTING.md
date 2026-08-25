# Contributing to DocFlow

## Before changing code

1. Read the root and nearest nested `AGENTS.md`, `docs/PRODUCT.md`, and the
   relevant architecture/design document.
2. Create or link a GitHub Issue for user-visible work.
3. Branch from current `main` using `feat/`, `fix/`, `docs/`, `test/`, or `chore/`.
4. Never use real/private RFI, workbook, token, or credential data as a fixture.

## Development discipline

- Write a failing behavior test before production behavior.
- Keep presentation in React and all trusted/domain authority in Rust.
- Preserve preview → approval → apply → journal → undo.
- Add user-visible strings to all four catalogs.
- Do not add a dependency without documenting why the current stack is insufficient.
- Do not add an interpreter, application sidecar, or second mutation backend.
- Use Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `ci:`, `chore:`).

## Required checks

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

Packaging rules and release gates are in `docs/RELEASE_SAFETY.md`.

## Pull requests

A PR must state the user-visible outcome, safety/data impact, exact tests and
results, version/changelog impact, and rollback/undo behavior. Include screenshots
only when the UI changed. Do not merge a draft or failing PR.

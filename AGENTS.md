# DocFlow agent instructions

## Architecture

DocFlow is a Tauri 2 desktop application:

- `frontend/`: React 19, TypeScript, and Vite presentation layer
- `src-tauri/`: Rust/Tauri trusted application authority and packaging
- `src-tauri/tests/`: compatibility, safety, and operation parity fixtures
- `tools/`: Node/shell/PowerShell verification and packaging tools
- `docs/`: product, architecture, migration, security, and release canon

React owns presentation and transient UI state only. Rust/Tauri is the sole
authority for filesystem, document, network, token, and native-platform work.

## Canon

Read sources in this order:

1. This `AGENTS.md` and the closest nested `AGENTS.md`
2. `docs/PRODUCT.md` and accepted architecture decisions
3. `docs/superpowers/specs/2026-08-24-rust-engine-migration-design.md`
4. Protocol/contracts and safety tests
5. `docs/VERSIONING.md`, `docs/RELEASE_SAFETY.md`, and CI evidence
6. Roadmap and GitHub Issues

When active sources disagree, stop and reconcile documents and tests in the same
change before moving authority.

## Non-negotiable rules

1. Every mutation is preview → explicit user approval → apply → journal → undo.
2. Apply executes only the approved cached `Plan`; it never recalculates.
3. Never overwrite an existing destination.
4. Undo reverses successful steps and removes only empty directories created by the operation.
5. Cloud history is audit metadata only. Only a validated local journal may authorize undo.
6. The webview never receives Supabase tokens or arbitrary filesystem, network, shell, or process authority.
7. All user-visible text exists in `tr`, `en`, `ru`, and `az`.
8. Forms are generated from `ParamSchema`; never add an operation-specific form.
9. Canonical RFI codes are normalized to NFC and uppercase once; never lowercase them.
10. Keep the strict `-RFI-` anchor. A miss is recoverable; a false label is silent corruption.
11. The UI is monochrome. Meaning comes from controls, text, icons, and border weight.
12. OCR, filesystem, network, Office, and printer work stays outside the webview.
13. Never expose a Supabase service-role or secret key in a desktop client.
14. Windows and macOS remain supported.
15. Preserve existing non-secret settings, journal, trash, queue, DTO, and cloud formats unless an accepted decision explicitly changes one.
16. Every completed feature or improvement slice must increment the project version across all manifests (`node tools/version_manager.mjs set <next-version>`) and finish with a dedicated git commit describing the change, so history is granular, traceable, and reversible.
17. Classify every completed slice with `node tools/version_manager.mjs next`: `fix` increments PATCH, `feature` increments MINOR, and `breaking` increments MAJOR. Do not extend a prerelease train unless an explicit release-candidate plan is active.

## Development workflow

- Inspect the worktree and relevant canon before editing.
- Protect pre-existing user changes; do not mix unrelated cleanup.
- Use vertical RED → GREEN → REFACTOR slices for behavior changes.
- Keep the frontend thin and commands closed, typed, bounded, and authorized in Rust.
- Never add an interpreter, application sidecar, shell/process authority, or a second mutation backend.
- Do not modify `legacy/` or generated output under `app/build/`, `frontend/dist/`, `src-tauri/target/`, or local environment directories.
- Advance the project version across all manifests (`node tools/version_manager.mjs set <next-version>`) and create a focused git commit at the end of each completed work item.
- Do not push, publish, deploy, or release unless the user explicitly requests it.

## Verification

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

Packaging requires the platform gates in `docs/RELEASE_SAFETY.md`, an audited
package, the packaged executable smoke, and checksums. A task is not complete
until focused tests, affected full suites, static checks/builds, and final
diff/status inspection have fresh evidence. macOS success cannot certify Windows
runtime, Office, or printer behavior.

The pre-migration Flutter and Qt sources under `legacy/` are read-only rollback material.

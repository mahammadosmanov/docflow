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
2. `docs/README.md` and its ordered documentation map
3. `docs/PRODUCT.md`, `docs/PRD.md`, and accepted architecture decisions
4. `docs/superpowers/specs/2026-08-24-rust-engine-migration-design.md`
5. Protocol/contracts and safety tests
6. `docs/VERSIONING.md`, `docs/RELEASE_SAFETY.md`, and CI evidence
7. Roadmap and GitHub Issues

When active sources disagree, stop and reconcile documents and tests in the same
change before moving authority.

## Non-negotiable rules

1. **MANDATORY VERSION BUMP ON EVERY CHANGE (FOR ALL AI MODELS - GEMINI, CLAUDE, ETC.):**
   Every new modification, fix, feature, or release MUST ALWAYS bump the version number across all manifests (`node tools/version_manager.mjs set <next-version>`), update `releases.json` and `CHANGELOG.md`, and verify with `node tools/version_manager.mjs check`. Never deliver a build, commit, or claim completion under an existing version number. Gemini, Claude, and any other AI model working on this codebase must read this rule and bump the version.
2. Every mutation is preview → explicit user approval → apply → journal → undo.
   The sole exception is an enabled `files.replica_publish` profile: its first
   exact profile Plan requires explicit Apply, which grants standing authority
   for later identical-profile runs. Each run still builds an immutable Plan,
   revalidates hashes, backs up replacements, journals effects, and pauses on
   ambiguity or failure.
3. Apply executes only the approved cached `Plan`; it never recalculates.
4. Never overwrite an existing destination.
5. Undo reverses successful steps and removes only empty directories created by the operation.
6. Cloud history is audit metadata only. Only a validated local journal may authorize undo.
7. The webview never receives Supabase tokens or arbitrary filesystem, network, shell, or process authority.
8. All user-visible text exists in `tr`, `en`, `ru`, and `az`.
9. Forms are generated from `ParamSchema`; never add an operation-specific form.
10. OCR candidates may be normalized to NFC and uppercase once. Persisted RFI
   codes must already match exact uppercase ASCII; never silently repair them.
11. Keep the strict `-RFI-` anchor. A miss is recoverable; a false label is silent corruption.
12. The UI is monochrome. Meaning comes from controls, text, icons, and border weight.
13. OCR, filesystem, network, Office, and printer work stays outside the webview.
14. Never expose a Supabase service-role or secret key in a desktop client.
15. Windows and macOS remain supported.
16. Preserve existing non-secret settings, journal, trash, queue, DTO, and cloud formats unless an accepted decision explicitly changes one.
17. Every completed feature or improvement slice must increment the project version across all manifests (`node tools/version_manager.mjs set <next-version>`) and finish with a dedicated git commit describing the change, so history is granular, traceable, and reversible. After applicable checks pass, push that commit to the canonical GitHub branch and publish its immutable version tag and GitHub Release; this repository has standing owner authorization for those backup actions. Never include secrets, operator documents, generated caches, or unverified package claims.
18. Classify every completed slice with `node tools/version_manager.mjs next`: `fix` increments PATCH, `feature` increments MINOR, and `breaking` increments MAJOR. Do not extend a prerelease train unless an explicit release-candidate plan is active.
19. Multi-core execution is allowed for independent read-only discovery or
    analysis. Apply, publication, journal order, and Undo remain deterministic;
    never parallelize effects that could collide or become uncertain.

## Development workflow

- Inspect the worktree and relevant canon before editing.
- Protect pre-existing user changes; do not mix unrelated cleanup.
- Follow Up route inclusions/exclusions require an explicit domain decision in
  `docs/RFI_DOMAIN.md` and an accepted ADR before route code changes. An
  unmapped pair is not approval to add a route; confirm scope first.
- Use vertical RED → GREEN → REFACTOR slices for behavior changes.
- Keep the frontend thin and commands closed, typed, bounded, and authorized in Rust.
- Never add an interpreter, application sidecar, shell/process authority, or a second mutation backend.
- Do not modify `legacy/` or generated output under `app/build/`, `frontend/dist/`, `src-tauri/target/`, or local environment directories.
- Advance the project version across all manifests (`node tools/version_manager.mjs set <next-version>`), create a focused git commit, push the canonical branch, and publish its immutable tag and GitHub Release at the end of each completed work item.
- Standing authorization covers DocFlow source commits, version tags, GitHub Releases, and verified release assets in the canonical private repository. It does not cover deployment, production-document mutation, secrets, spending, or other external publication.

## Verification

```bash
node tools/version_manager.mjs check
node tools/docs_audit.mjs
node tools/package_audit.mjs source
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run audit:licenses
```

Packaging requires the platform gates in `docs/RELEASE_SAFETY.md`, an audited
package, the packaged executable smoke, and checksums. A task is not complete
until focused tests, affected full suites, static checks/builds, and final
diff/status inspection have fresh evidence. macOS success cannot certify Windows
runtime, Office, or printer behavior.

The pre-migration Flutter and Qt sources under `legacy/` are read-only rollback material.

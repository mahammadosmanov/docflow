# DocFlow agent instructions

## Architecture

DocFlow is a Dart/Flutter desktop application. The Dart engine is the sole
application and mutation authority.

- `flutter/app/lib/engine/`: Dart engine, the trusted authority for filesystem,
  document, network, token, OCR, print, and native-platform work
- `flutter/app/lib/ui/` and `flutter/app/lib/bridge/`: presentation only
- `flutter/app/lib/l10n/`: generated catalog (`generate_catalog.dart`)
- `flutter/app/tool/locales/`: ARB source catalogs for `tr`, `en`, `ru`, `az`
- `flutter/app/test/`: engine, UI, and foundation tests; `test/fixtures/` holds frozen synthetic oracles
- `tools/`: `version_manager.mjs` and `docs_audit.mjs`
- `docs/`: product, safety, release, and decision canon

The UI owns presentation and transient state only. The Dart engine is the sole
authority for filesystem, document, network, token, and native-platform work.

## Canon

Read sources in this order:

1. This `AGENTS.md`
2. `docs/README.md` and its ordered documentation map
3. `docs/PRODUCT.md`, `docs/PRD.md`, and accepted decisions, especially
   `docs/adr/0039-dart-engine-supersedes-no-second-backend.md`
4. Protocol/contracts and safety tests
5. `docs/VERSIONING.md`, `docs/RELEASE_SAFETY.md`, and CI evidence
6. Roadmap and GitHub Issues

When active sources disagree, stop and reconcile documents and tests in the same
change before moving authority.

## Non-negotiable rules

1. **MANDATORY VERSION BUMP ON EVERY CHANGE (FOR ALL AI MODELS):**
   Every new modification, fix, feature, or release MUST ALWAYS bump the version number across all manifests (`node tools/version_manager.mjs set <next-version>`), update `releases.json` and `CHANGELOG.md`, and verify with `node tools/version_manager.mjs check`. Never deliver a build, commit, or claim completion under an existing version number.
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
7. The UI layer never receives Supabase tokens or arbitrary filesystem, network, shell, or process authority.
8. All user-visible text exists in `tr`, `en`, `ru`, and `az`.
9. Forms are generated from `ParamSchema`; never add an operation-specific form.
10. OCR candidates may be normalized to NFC and uppercase once. Persisted RFI
   codes must already match exact uppercase ASCII; never silently repair them.
11. Keep the strict `-RFI-` anchor. A miss is recoverable; a false label is silent corruption.
12. The UI is monochrome. Meaning comes from controls, text, icons, and border weight.
13. OCR, filesystem, network, Office, and printer work stays in the engine, never in the UI layer.
14. Never expose a Supabase service-role or secret key in a desktop client.
15. Windows and macOS remain supported.
16. Preserve existing non-secret settings, journal, trash, queue, DTO, and cloud formats unless an accepted decision explicitly changes one.
17. Every completed feature or improvement slice must increment the project version across all manifests (`node tools/version_manager.mjs set <next-version>`) and finish with a dedicated git commit describing the change, so history is granular, traceable, and reversible. Never push, tag, publish, or create a release without explicit user authorization in chat. Never include secrets, operator documents, generated caches, or unverified package claims.
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
- Keep the UI thin; commands stay closed, typed, bounded, and authorized in the Dart engine.
- Never add an interpreter, application sidecar, shell/process authority, or a second mutation backend.
- Do not modify generated output under `flutter/app/build/` or local environment directories.
- Advance the project version across all manifests (`node tools/version_manager.mjs set <next-version>`) and create a focused git commit at the end of each completed work item. Do not push, tag, or publish without explicit user authorization.

## Verification

```bash
node tools/version_manager.mjs check
node tools/docs_audit.mjs
cd flutter/app && flutter analyze && flutter test
```

Packaging requires the platform gates in `docs/RELEASE_SAFETY.md`, the
Windows build workflow in `.github/workflows/windows-build.yml`, and checksums.
A task is not complete until focused tests, affected full suites, static checks,
and final diff/status inspection have fresh evidence. macOS success cannot
certify Windows runtime, Office, or printer behavior.

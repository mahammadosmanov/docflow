# Contributing to DocFlow

## Before changing code

1. Read the root and nearest nested `AGENTS.md`, `docs/PRODUCT.md`, and the
   relevant architecture/design document.
2. Create or link a GitHub Issue for user-visible work.
3. Branch from current `main` using `feat/`, `fix/`, `docs/`, `test/`, or `chore/`.
4. Never use real/private RFI, workbook, token, or credential data as a fixture.

## Development discipline

- Write a failing behavior test before production behavior.
- Keep presentation in the Flutter UI and all trusted/domain authority in the Dart engine.
- Preserve preview → approval → apply → journal → undo.
- Add user-visible strings to all four catalogs.
- Do not add a dependency without documenting why the current stack is insufficient.
- Do not add an interpreter, application sidecar, or second mutation backend.
- Use Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `ci:`, `chore:`).

## Required checks

```bash
node tools/version_manager.mjs check
node tools/docs_audit.mjs
cd flutter/app
flutter analyze
flutter test
```

Packaging rules and release gates are in `docs/RELEASE_SAFETY.md`.

## Pull requests

A PR must state the user-visible outcome, safety/data impact, exact tests and
results, version/changelog impact, and rollback/undo behavior. Include screenshots
only when the UI changed. Do not merge a draft or failing PR.

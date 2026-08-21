# Contributing to DocFlow

## Before changing code

1. Read `CLAUDE.md`, `docs/PRODUCT.md`, and the relevant architecture document.
2. Create or link a GitHub Issue for user-visible work.
3. Branch from current `main` using `feat/`, `fix/`, `docs/`, `test/`, or `chore/`.
4. Never use real/private RFI, workbook, token, or credential data as a fixture.

## Development discipline

- Write a failing behavior test before production behavior.
- Keep document logic in Python and presentation in React.
- Preserve preview → approval → apply → journal → undo.
- Add user-visible strings to all four catalogs.
- Do not add a dependency without documenting why the standard library/current
  stack is insufficient.
- Use Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `ci:`, `chore:`).

## Required checks

```bash
python tools/version_manager.py check
.venv/bin/python -m pytest
.venv/bin/python -m mypy src/docflow
npm test
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

Packaging rules and release gates are in `docs/RELEASE_SAFETY.md`.

## Pull requests

A PR must state:

- user-visible outcome
- safety/data impact
- exact tests run and results
- version/changelog impact
- screenshots only when UI changed
- rollback/undo behavior for filesystem or workbook changes

Do not merge a draft or failing PR. Squash only when preserving individual
commits adds no useful investigation/release history.

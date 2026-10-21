## Summary

Makes the Dart engine under `flutter/app` the only DocFlow backend and removes
the Tauri/Rust/React application and its Node tooling.

## What changed

- Deleted: `src-tauri/`, `frontend/`, `package.json`/`package-lock.json`,
  Rust-only and Tauri-only tools and workflows, and the Rust-era docs
  (migration status, parity closure, verification ledger, IPC protocol,
  architecture, Windows runbook, tasks, `docs/superpowers/`).
- Moved: ARB locale sources to `flutter/app/tool/locales/`; frozen synthetic
  oracles to `flutter/app/test/fixtures/`. Paths updated in code and tests.
- Adapted: `tools/version_manager.mjs` (manifest is `VERSION` plus
  `docflowVersion` in `flutter/app/lib/engine/backend.dart`), `tools/docs_audit.mjs`,
  `quality.yml` (Flutter analyze/test), `dependabot.yml` (pub and actions),
  `AGENTS.md`, `CLAUDE.md`, `README.md`, `CONTRIBUTING.md`, PR template,
  `docs/README.md`, `docs/RELEASE_SAFETY.md`, `flutter/HANDOFF.md`.
- Safety rules text is kept unchanged. Push and tag rules now require explicit
  user authorization.

## Windows build

- Added manual workflow `.github/workflows/windows-build.yml`: `flutter build windows --release`
  on `windows-latest` with Flutter 3.47.7; uploads a zip and SHA-256.
- Guide: `flutter/WINDOWS_TEST.md`. Not yet run from this branch.

## Test plan

- [x] `node tools/version_manager.mjs check`
- [x] `node tools/docs_audit.mjs`
- [x] `node --test tools/version_manager.test.mjs tools/docs_audit.test.mjs`
- [x] `flutter analyze` (0 errors)
- [x] `flutter test` (844 pass, 16 skipped)

## UNVERIFIED gaps (not run on a real host)

- Windows runtime, installer, and packaged startup.
- Office, Excel COM, Outlook, printers, and Print to PDF.
- Live Supabase RLS deployment and two-user verification.
- Signing and notarization.
- The GitHub `quality.yml` run has not executed on this branch.

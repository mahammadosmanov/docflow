# ADR 0039: Dart engine supersedes the no-second-backend decision

- Date: 2026-10-20
- Status: Accepted

## Context

`docs/MIGRATION.md` rejected "Flutter plus a second backend runtime" because
it would duplicate contracts and add a heavier UI toolchain. The Flutter
migration now ports the Rust backend (`src-tauri/src`) to Dart under
`flutter/app/lib/engine`, including the native command router
(`NativeBackend`), the filesystem authority, OOXML, PDF, print and Excel
modules.

## Decision

- The Dart engine is the single backend for the Flutter application. It is
  not a second runtime beside Rust: the Rust crate stays the reference until
  the migration's release gate passes.
- The earlier "no second backend" rejection is superseded for the migration
  period. Contracts are kept in step through the shared port tests
  (`flutter/app/test/engine`) and the Rust tests they mirror.
- Rust remains the authority for behaviour that is not yet ported. Ported
  behaviour is authoritative in Dart once its tests pass.

## Options considered

- Keep the Rust backend and a second UI shell: rejected earlier; still
  duplicates UI contracts.
- Keep Rust and rewrite only the UI: rejected here, because the native
  router and authority need one implementation to guarantee the apply and
  undo invariants.
- Port to Dart (chosen): one engine in the app's own toolchain.

## Consequences

- Platform adapters that cannot be verified here (Windows COM, Win32
  printing, PDFKit, the Windows PDF engine) stay marked UNVERIFIED until
  they run on real hosts.
- Rollback: the Rust crate and the React frontend history remain in git, so
  reverting the migration commits restores the earlier backend.

## Index

Add this ADR to the decision index in `docs/DECISIONS.md`.

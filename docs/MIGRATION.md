# Stack migration decision

## Decision

DocFlow uses a thin React/Vite presentation layer and a Rust/Tauri trusted
application authority. The staged migration approved on 2026-08-24 is source-complete:
all React-reachable commands, plans, mutations, journals, document operations,
credentials, cloud calls, and platform adapters are Rust-owned.

The former application-sidecar runtime, child-process router, shell plugin,
external-binary packaging, manifests, application sources, and repository tools
were retired only after operation parity and safety gates had passed. Compatibility
fixtures remain under `src-tauri/tests/fixtures/` as inert test data.

## Historical alternatives

| Option | Decision |
|---|---|
| Flutter plus a second backend runtime | Rejected: duplicated contracts and heavier UI toolchain |
| PySide6 | Retired: robust widgets, slower product-UI iteration |
| Electron + React | Rejected: larger runtime and memory floor |
| Rust-native UI | Rejected: unnecessary React rewrite and document-engine coupling |
| Tauri + React + permanent application sidecar | Superseded 2026-08-24: startup/package overhead and duplicated backend maintenance |
| Tauri + React + Rust authority | Accepted and implemented |

## Completed migration sequence

1. Froze canon, contracts, representative fixtures, and performance baselines.
2. Moved routing, bootstrap, authentication, cloud, and security boundaries to Rust.
3. Added immutable plans, grants, journals, recovery, and compatibility harnesses.
4. Migrated deterministic parsers, preferences, history, review, and queues.
5. Proved filesystem mutations in isolated adversarial fixtures.
6. Migrated OCR and Process Scans.
7. Migrated OOXML/RFI Log, email extraction, and clipboard output.
8. Migrated signed-page PDF staging.
9. Migrated printer submission and queue recovery.
10. Removed the retired runtime and replaced version, release, checksum, license,
    source-audit, package-audit, and packaged-smoke tooling without it.

## Compatibility and certification boundary

- Settings, journals, trash, print queues, protocol-1 DTOs, operation IDs, and
  Supabase schemas remain compatible.
- Cloud history remains metadata-only; local validated journals alone authorize undo.
- macOS local/package evidence and Windows cross-compile evidence cannot certify
  Windows installer execution, Office/Excel COM, or physical/network printers.
- Live Supabase policy, signing, and notarization require external credentials and
  are explicit separate gates.

The detailed approved design and historical gates remain in
`docs/superpowers/specs/2026-08-24-rust-engine-migration-design.md`. The Flutter and
Qt sources under ignored `legacy/` remain read-only rollback material.

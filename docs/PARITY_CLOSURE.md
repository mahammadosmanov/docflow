# Rust parity closure

The retired application runtime was deleted only after its behavior had a Rust
test, a frozen compatibility fixture, or an explicit platform certification gate.
All fixtures are synthetic and inert; none is executable or loaded by production.

| Retired behavior area | Rust evidence |
|---|---|
| Protocol DTOs, operation schemas, settings | `src-tauri/src/contracts.rs`, `registry.rs`, `settings.rs`; `fixtures/protocol1_operations.json` |
| Auth, secure credential import, cloud metadata | `src-tauri/tests/auth_http.rs`, `cloud_metadata.rs`, `gemini_http.rs` |
| Exact plans, path grants, no-clobber, journal/recovery/undo | `src-tauri/tests/filesystem_authority.rs` |
| RFI normalization/naming, OCR, Process Scans, review | `src-tauri/tests/process_scans.rs`; `fixtures/slice5_rfi_naming.json` |
| Email parsing and bounded model extraction | `src-tauri/tests/email_extract.rs`, `gemini_http.rs` |
| OOXML/RFI Log preview, revise, apply, backup, fidelity, undo | `src-tauri/tests/rfi_log_native.rs`, `ooxml_fidelity.rs`; `fixtures/slice6_rfi_compatibility_oracle.json` |
| Signed-page PDF fidelity and safe publication | `src-tauri/tests/signed_pages.rs`; `fixtures/slice7_signed_pages_oracle.json` |
| Ordered print queue, recovery, retry/cancel semantics | `src-tauri/tests/batch_print.rs` plus target-specific adapter tests |
| React-reachable command ownership | `src-tauri/src/backend.rs` and `frontend/src/lib/frontend-authority.test.ts` |
| Normal startup/build cannot route to a retired backend | Rust build guard in `src-tauri/build.rs`, router/manifests tests in `backend.rs`, and `tools/package_audit.mjs source` |
| Packaged bootstrap/settings/OCR/apply/journal/undo | `src-tauri/src/smoke.rs` executed from the packaged binary |

The original pre-removal suite passed immediately before deletion as the final
compatibility handoff. After deletion, canonical verification is Node, React,
Rust, Tauri, native OCR, and platform packaging only.

Host-only evidence remains separate: Windows installer execution, Office/Excel
COM and native clipboard, Microsoft Print to PDF, representative physical/network
printers, live Supabase policies, signing, and notarization.

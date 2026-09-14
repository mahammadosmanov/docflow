# Requirement traceability

This index identifies primary owners. Focused tests are authoritative; the full
evidence matrix is in `RELEASE_SAFETY.md` and `PARITY_CLOSURE.md`.

| Requirement | Primary implementation | Primary evidence |
|---|---|---|
| DF-001, DF-002 | `plan_store.rs`, `native.rs`, `fs_ops.rs` | `filesystem_authority.rs`, backend plan tests |
| DF-003, DF-004 | `cap_fs.rs`, `journal.rs`, `undo.rs`, `leaf_transaction.rs` | filesystem fault/recovery tests |
| DF-005 | `rfi.rs`, `ocr.rs`, RFI operations | `process_scans.rs`, `rfi_follow_up_native.rs`, `work_items.rs` |
| DF-006 | `ooxml.rs`, `rfi_log.rs`, `rfi_follow_up.rs`, `rfi_system_sync.rs` | `ooxml_fidelity.rs`, `rfi_log_native.rs`, `rfi_system_sync_native.rs` |
| DF-007 | `operations/stage_signed_pages.rs`, `pdf.rs` | `signed_pages.rs` integration suite |
| DF-008 | `print_queue.rs`, `platform/*/printing.rs`, `platform/excel/` | `batch_print.rs`, controlled-submit tests |
| DF-009 | `backend.rs`, `paths.rs`, Tauri capabilities | backend roster and `frontend-authority.test.ts` |
| DF-010 | `auth.rs`, `credentials.rs`, `cloud.rs` | `auth_http.rs`, `cloud_metadata.rs` |
| DF-011 | `frontend/src/locales/*.arb` | frontend catalog-equality tests |
| DF-012 | `registry.rs`, `contracts.rs`, generic `App.tsx` renderer | registry snapshots and `App.test.tsx` |
| DF-013 | docs, Windows workflow, package audit | `windows_workflow.test.mjs`, release records |
| DF-014 | `version_manager.mjs`, manifests, release catalog | `version_manager.test.mjs`, package audit/smoke |
| DF-015 | `operations/pdf_orientation.rs`, `ocr.rs`, `pdf.rs`, `fs_ops.rs` | `pdf_orientation.rs`, `signed_pages.rs`, registry and localization tests |
| DF-016 | `rfi_follow_up.rs`, `ooxml.rs`, `fs_ops.rs` | `rfi_follow_up_native.rs`, `rfi_follow_up_current.rs`, `ooxml_fidelity.rs` |
| DF-017 | `print_queue.rs`, `platform/mod.rs`, `platform/windows/printing.rs`, `registry.rs`, generic `App.tsx` form | `batch_print.rs` accepted-unconfirmed continuation test, Windows status/query-race policy tests, `native.rs` settings tests, `App.test.tsx` explicit-restore test |
| DF-018 | `scan_index.rs`, `operations/pdf_orientation.rs`, `rfi_system_sync.rs`, generic `App.tsx` summary | scan-index unit tests, `pdf_orientation.rs`, `rfi_system_sync_native.rs`, `App.test.tsx` |
| DF-019 | OCR operation metadata, `native.rs` review DTO, generic `App.tsx` percent renderer and responsive CSS | `process_scans.rs`, `signed_pages.rs`, `App.test.tsx`, small-viewport Playwright evidence |
| DF-020 | `operations/stage_signed_pages.rs`, `parallel.rs`, `registry.rs` | `signed_pages.rs` deep-recursion/parallel-evidence and symlink tests, registry protocol snapshot |

Paths in the table are relative to `src-tauri/src/`, `src-tauri/tests/`, or the
repository root as context requires. When an owner or acceptance path moves,
update this table in the same commit. Requirement IDs are never reused.

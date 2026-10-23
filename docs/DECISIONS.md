# Architecture decision index

DocFlow keeps decisions as repository Markdown so they remain free, reviewable,
and versioned with the code they govern.

| Decision | Status | Document |
|---|---|---|
| Tauri 2 + React/TypeScript + permanent Python sidecar | Superseded | [`MIGRATION.md`](MIGRATION.md) |
| Tauri 2 + React/TypeScript + Rust authority | Accepted | [`MIGRATION.md`](MIGRATION.md) |
| Private JSON-lines child-process protocol | Retired after Rust migration | [`IPC_PROTOCOL.md`](IPC_PROTOCOL.md) |
| Preview/apply/journal operation contract | Accepted | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| Repository-native SemVer and release catalog | Accepted | [`VERSIONING.md`](VERSIONING.md) |
| Current RFI workbook header discovery and guarded Worley synchronization | Accepted | [`adr/0001-guarded-current-rfi-workbooks.md`](adr/0001-guarded-current-rfi-workbooks.md) |
| Offline-first RFI work-item lifecycle and evidence boundaries | Superseded | [`adr/0002-rfi-work-item-lifecycle.md`](adr/0002-rfi-work-item-lifecycle.md) |
| Exact-page staging for combined Worley scans | Accepted | [`adr/0003-combined-worley-scan-staging.md`](adr/0003-combined-worley-scan-staging.md) |
| Bounded signed-page thumbnails carried by the approved Plan | Accepted | [`adr/0004-bounded-plan-page-previews.md`](adr/0004-bounded-plan-page-previews.md) |
| Non-interactive passive macOS credential restore | Accepted | [`adr/0005-noninteractive-keychain-restore.md`](adr/0005-noninteractive-keychain-restore.md) |
| Machine-checked maintainer and AI onboarding canon | Accepted | [`adr/0006-machine-checked-onboarding-canon.md`](adr/0006-machine-checked-onboarding-canon.md) |
| Content-based PDF orientation with read-only source and bounded parallel preview | Accepted | [`adr/0007-safe-parallel-pdf-orientation.md`](adr/0007-safe-parallel-pdf-orientation.md) |
| Exact RFI Log to existing Follow Up projection with grouped status/removal rules | Accepted | [`adr/0008-exact-rfi-log-follow-up-projection.md`](adr/0008-exact-rfi-log-follow-up-projection.md) |
| Windows spool release semantics and opt-in Batch Print setting recall | Accepted | [`adr/0009-resilient-print-handoff-and-opt-in-recall.md`](adr/0009-resilient-print-handoff-and-opt-in-recall.md) |
| Local durable incremental scan index with cloud-independent authority | Accepted | [`adr/0010-durable-incremental-scan-index.md`](adr/0010-durable-incremental-scan-index.md) |
| Actionable signed-page warnings as hash-pinned no-clobber Review copies | Accepted | [`adr/0011-actionable-signed-page-warning-review.md`](adr/0011-actionable-signed-page-warning-review.md) |
| Shared bounded read-only parallelism with operation-wide progress phases | Accepted | [`adr/0012-bounded-parallel-progress.md`](adr/0012-bounded-parallel-progress.md) |
| Exact no-clobber grouping of incoming files into multiple RFI folders | Accepted | [`adr/0013-safe-multi-rfi-file-grouping.md`](adr/0013-safe-multi-rfi-file-grouping.md) |
| Retire RFI work tracking, audit export, and device readiness | Accepted | [`adr/0014-retire-work-tracking-and-device-readiness.md`](adr/0014-retire-work-tracking-and-device-readiness.md) |
| Exact completed-PDF evidence as a separate guarded RFI Log status operation | Accepted | [`adr/0015-exact-completed-pdf-status-sync.md`](adr/0015-exact-completed-pdf-status-sync.md) |
| Per-RFI mixed-status remarks on CIV and PCU Follow Up sheets | Superseded | [`adr/0026-mixed-rfi-status-remarks.md`](adr/0026-mixed-rfi-status-remarks.md) |
| Keep Follow Up Sync away from worksheet Remarks cells | Accepted | [`adr/0028-remove-follow-up-status-remarks.md`](adr/0028-remove-follow-up-status-remarks.md) |
| Exact multi-area CIV Follow Up fan-out | Accepted | [`adr/0029-exact-civ-multi-area-follow-up.md`](adr/0029-exact-civ-multi-area-follow-up.md) |
| Larger bounded S5 retained-file capacity | Accepted | [`adr/0030-bounded-s5-retained-file-capacity.md`](adr/0030-bounded-s5-retained-file-capacity.md) |
| Distinct-context MEC Follow Up fan-out (matching key superseded) | Partially superseded | [`adr/0031-exact-mec-context-follow-up.md`](adr/0031-exact-mec-context-follow-up.md) |
| Exact PCS and composite Follow Up context matching | Partially superseded | [`adr/0032-exact-pcs-and-composite-follow-up-contexts.md`](adr/0032-exact-pcs-and-composite-follow-up-contexts.md) |
| Exclude CIV PCS-tagged RFI Log records from Follow Up | Accepted | [`adr/0033-exclude-civ-pcs-rfis-from-follow-up.md`](adr/0033-exclude-civ-pcs-rfis-from-follow-up.md) |
| Skip supported Follow Up routes with no existing target row | Superseded | [`adr/0027-follow-up-no-match-scope.md`](adr/0027-follow-up-no-match-scope.md) |
| Confirmed ITP/item pairs intentionally excluded from Follow Up | Accepted | [`adr/0035-intentional-follow-up-scope-exclusions.md`](adr/0035-intentional-follow-up-scope-exclusions.md) |
| Follow Up context-scope filtering and actionable partial-match diagnostics | Accepted | [`adr/0036-follow-up-context-scope-and-diagnostics.md`](adr/0036-follow-up-context-scope-and-diagnostics.md) |
| MEC Follow Up Section/Block matching and multi-section fan-out | Accepted | [`adr/0037-mec-follow-up-section-block-matching.md`](adr/0037-mec-follow-up-section-block-matching.md) |
| CIV Follow Up Tag No token fan-out and partial-coverage diagnostics | Accepted | [`adr/0038-civ-follow-up-tag-token-coverage.md`](adr/0038-civ-follow-up-tag-token-coverage.md) |
| Background read-only Preview preparation (Action Queue) | Accepted | [`adr/0039-background-read-only-preview-preparation.md`](adr/0039-background-read-only-preview-preparation.md) |
| Evidence truthfulness and fail-closed automation | Accepted | [`adr/0024-evidence-truthfulness.md`](adr/0024-evidence-truthfulness.md) |
| Ordered multi-RFI scan separation and discipline folders | Superseded | [`adr/0025-ordered-batch-rfi-scans.md`](adr/0025-ordered-batch-rfi-scans.md) |
| Confirmed-cover grouping for batch Process Scans | Accepted | [`adr/0034-confirmed-cover-batch-scan-grouping.md`](adr/0034-confirmed-cover-batch-scan-grouping.md) |
| One-way backup-first three-root publication with revocable standing authorization | Accepted | [`adr/0016-safe-unattended-replica-publication.md`](adr/0016-safe-unattended-replica-publication.md) |
| Durable fingerprint-scoped hash reuse for replica comparison | Accepted | [`adr/0017-incremental-replica-hashing.md`](adr/0017-incremental-replica-hashing.md) |
| Local pathless printer evidence in the redacted support bundle | Accepted | [`adr/0018-pathless-printer-diagnostics.md`](adr/0018-pathless-printer-diagnostics.md) |
| Guarded near-real-time replica publication | Accepted | [`adr/0019-guarded-near-real-time-replica-publication.md`](adr/0019-guarded-near-real-time-replica-publication.md) |
| Exact selected Outlook message archived as a guarded native Unicode MSG copy | Accepted | [`adr/0020-guarded-selected-outlook-msg-archive.md`](adr/0020-guarded-selected-outlook-msg-archive.md) |
| Exact canonical PDF and bottom-right OTHERS cell as sole Worley authority | Accepted | [`adr/0021-exact-others-worley-evidence.md`](adr/0021-exact-others-worley-evidence.md) |
| Existing PCU FINAL RFI as authority for its separate Worley status cell | Accepted | [`adr/0022-pcu-final-worley-projection.md`](adr/0022-pcu-final-worley-projection.md) |
| Successful guarded RFI Log plan as source for a separate unsent Outlook reply plan | Accepted | [`adr/0023-rfi-log-to-outlook-reply-handoff.md`](adr/0023-rfi-log-to-outlook-reply-handoff.md) |

## Adding a decision

Create `docs/adr/NNNN-short-title.md` with:

- context and problem
- options considered, including doing nothing
- decision and rationale
- consequences and rollback/migration plan
- date and status (`Proposed`, `Accepted`, `Superseded`)

Add it to this index. Superseded decisions remain in history and point to their
replacement; they are never silently deleted.

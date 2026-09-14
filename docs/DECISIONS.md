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
| Offline-first RFI work-item lifecycle and evidence boundaries | Accepted | [`adr/0002-rfi-work-item-lifecycle.md`](adr/0002-rfi-work-item-lifecycle.md) |
| Exact-page staging for combined Worley scans | Accepted | [`adr/0003-combined-worley-scan-staging.md`](adr/0003-combined-worley-scan-staging.md) |
| Bounded signed-page thumbnails carried by the approved Plan | Accepted | [`adr/0004-bounded-plan-page-previews.md`](adr/0004-bounded-plan-page-previews.md) |
| Non-interactive passive macOS credential restore | Accepted | [`adr/0005-noninteractive-keychain-restore.md`](adr/0005-noninteractive-keychain-restore.md) |
| Machine-checked maintainer and AI onboarding canon | Accepted | [`adr/0006-machine-checked-onboarding-canon.md`](adr/0006-machine-checked-onboarding-canon.md) |
| Content-based PDF orientation with read-only source and bounded parallel preview | Accepted | [`adr/0007-safe-parallel-pdf-orientation.md`](adr/0007-safe-parallel-pdf-orientation.md) |

## Adding a decision

Create `docs/adr/NNNN-short-title.md` with:

- context and problem
- options considered, including doing nothing
- decision and rationale
- consequences and rollback/migration plan
- date and status (`Proposed`, `Accepted`, `Superseded`)

Add it to this index. Superseded decisions remain in history and point to their
replacement; they are never silently deleted.

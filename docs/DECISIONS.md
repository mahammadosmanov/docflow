# Architecture decision index

DocFlow keeps decisions as repository Markdown so they remain free, reviewable,
and versioned with the code they govern.

| Decision | Status | Document |
|---|---|---|
| Tauri 2 + React/TypeScript + Python sidecar | Accepted | [`MIGRATION.md`](MIGRATION.md) |
| Private JSON-lines child-process protocol | Accepted | [`IPC_PROTOCOL.md`](IPC_PROTOCOL.md) |
| Preview/apply/journal operation contract | Accepted | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| Repository-native SemVer and release catalog | Accepted | [`VERSIONING.md`](VERSIONING.md) |

## Adding a decision

Create `docs/adr/NNNN-short-title.md` with:

- context and problem
- options considered, including doing nothing
- decision and rationale
- consequences and rollback/migration plan
- date and status (`Proposed`, `Accepted`, `Superseded`)

Add it to this index. Superseded decisions remain in history and point to their
replacement; they are never silently deleted.

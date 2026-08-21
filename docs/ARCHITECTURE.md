# Architecture

```text
Tauri 2 native shell
        |
React + TypeScript presentation
        |
scoped stdin/stdout JSON-lines channel
        |
Python application facade
        |
engine + auth + cloud + OCR + safe filesystem
```

Tauri owns only the window, OS dialogs, opening a selected path, and sidecar
lifecycle. React owns presentation and transient screen state. Python remains
the authority for authentication, operation schemas, plans, mutations,
journals, cloud history, and OCR.

## Security boundary

- The shell capability permits spawning only `binaries/docflow-sidecar`.
- The webview has no Supabase URL/key/token and makes no external requests.
- Access and refresh tokens stay in the Python process and compatible settings
  file.
- No localhost port, HTTP server, eval, remote scripts, or service-role key is
  used.
- Native file selection is scoped to user-initiated Tauri dialogs.

## Operation contract

Every document feature implements the same framework-independent contract:

```python
class Operation:
    schema: ParamSchema

    def preview(self, params: dict) -> Plan: ...
    def apply(self, plan: Plan) -> Result: ...
```

React renders `ParamSchema`; there is no operation-specific form. Add a module
under `src/docflow/ops/` and translations to all four ARB catalogs for a new
operation.

## Safety invariants

- Preview never mutates the filesystem.
- Apply executes only `plan.steps`; it never recalculates a plan.
- Every step is journaled as pending before execution.
- Existing destinations are never overwritten.
- Undo runs steps in reverse and removes only empty created directories.
- Review “delete” moves to recoverable application trash.
- Missing/broken OCR produces a zero-step plan.

## Persistent compatibility

- settings/session: `~/.docflow/settings.json`
- journals: `~/.docflow/journal/`
- recoverable trash: `~/.docflow/trash/`
- Supabase `app_users` and `history` contracts: unchanged

The Python sidecar is frozen as one executable with PyInstaller. Tauri bundles
it using the required target-triple filename and packages the final native app.

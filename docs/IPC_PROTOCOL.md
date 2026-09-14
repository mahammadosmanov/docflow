# Application command protocol

The current UI uses one closed typed Tauri command, `frontend_call`, whose method
roster is mirrored in Rust and TypeScript and rejected on drift. React sends a
typed method plus bounded DTO; Rust authenticates and authorizes it, resolves
opaque path-selection capabilities, and returns typed data or a stable localized
error. Long operations use a Tauri channel for bounded progress and a typed
cancellation command. Apply accepts only an opaque cached Plan identity, never
client-authored steps. Tokens, raw credential values, native handles, and
arbitrary file/network/process authority never cross into the webview.

Canonical owners are `src-tauri/src/backend.rs`, `src-tauri/src/native.rs`,
`frontend/src/lib/bridge.ts`, and their roster/golden tests.

`rfi.normalize_pdf_orientation` accepts only opaque selections for the RFI
system and a separate review root. Preview returns ordinary `mkdir` and
`pdf_page_rotate` Plan steps containing approved page indexes, target rotations,
confidence evidence, and source hashes. Apply accepts only the cached plan ID;
React cannot supply or change a rotation step directly.

## Retired private sidecar protocol (historical)

This protocol is retained only to document compatibility history. No shipping or
test path starts a sidecar or uses child-process IPC.

DocFlow used one long-lived, line-delimited JSON channel over the child
process's stdin/stdout. There is no listening socket or network server.

Request:

```json
{"id":"12","method":"engine/preview","params":{"opId":"rfi.process_scans","values":{}}}
```

Progress and response:

```json
{"type":"progress","requestId":"12","event":{"phase":"scan","done":3,"total":8}}
{"type":"response","id":"12","result":{"planId":"...","steps":[],"warnings":[]}}
```

Errors were returned as `{kind,message}`. Backend tracebacks went only to stderr.
Every accepted request produces exactly one final response.

Method families:

- `app/*`: bootstrap data
- `settings/*`: preferences and remembered operation parameters
- `auth/*`: sign-in/up/out, approval profile, admin controls
- `engine/*`: operations, preview, apply, review, undo
- `history/*`: cloud history with local fallback
- `system/*`: ping and diagnostics

Authentication responses expose only user/profile state; Supabase access and
refresh tokens never cross into the webview.

# Retired private sidecar protocol (historical)

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

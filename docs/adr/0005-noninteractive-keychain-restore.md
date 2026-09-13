# ADR 0005: Non-interactive passive macOS credential restore

- Date: 2026-09-13
- Status: Accepted

## Context

DocFlow restores an optional Supabase session from macOS Keychain during
bootstrap. A rebuilt or unsigned application can encounter a Keychain item whose
access control requires authentication UI. The synchronous Security framework
read then waits for an OS prompt while the webview remains on the splash screen;
the prompt may not be visible or focused. Moving tokens to settings, deleting an
inaccessible item, or placing a detached timeout around a blocked secret-reading
thread would weaken security or leak unbounded work.

## Decision

Passive macOS startup queries the exact generic-password service and account
through `ItemSearchOptions`, requests only one data result, and sets
`skip_authenticated_items(true)`. For legacy macOS keychain ACLs, the read also
holds Security Framework's process interaction-disable guard; its RAII drop
restores the prior interaction policy immediately after the query. The Security
framework therefore cannot show authentication UI for the startup read. Missing,
inaccessible, malformed, or unexpected results fail closed through the existing
signed-out bootstrap path. The Keychain item is not deleted when the passive read
is unavailable.

Explicit credential writes and deletes keep their existing native Keychain
implementation. Tokens remain native-only and are never returned to React.
Windows Credential Manager behavior is unchanged.

## Consequences

- Normal startup cannot wait indefinitely for a hidden Keychain authorization
  prompt.
- An item requiring interaction is preserved, but the operator must sign in
  again before authenticated work.
- No background thread, timeout leak, alternate token store, or webview secret
  path is introduced.
- The exact behavior is verified by the macOS source contract and a normal launch
  of the packaged application; Windows remains covered by its native credential
  and installer gates.

## Rollback

Restoring the former generic-password helper restores automatic interactive
Keychain reads but also restores the indefinite splash risk. No credential or
settings migration is needed because the service, account, and serialized value
are unchanged.

# Security policy

DocFlow processes private local documents and uses a Supabase-backed optional
account/history service. Security reports should avoid attaching real documents,
access tokens, passwords, or production workbook contents.

## Reporting

Use the repository's private GitHub Security Advisory flow when available, or
contact the repository owner privately. Include:

- affected DocFlow version and Git commit/tag
- operating system and package type
- minimal synthetic reproduction
- whether source documents, destination files, credentials, or undo journals are at risk

Do not open a public issue containing secrets or customer documents.

## Supported version

The entry marked `current` in `releases.json` is the supported version. Older
entries are historical unless explicitly marked `supported`.

## Security boundaries

- Supabase service-role/secret keys are forbidden in desktop code.
- Access/refresh tokens remain in Windows Credential Manager or macOS Keychain and
  never cross to the webview.
- Local authentication is a UI gate; server RLS is the cloud authorization boundary.
- A verified one-time legacy import removes plaintext token fields after secure-store
  readback; downgrade requires sign-in again.
- Filesystem mutations require approved plans and journals; existing destinations
  are never overwritten.

A security fix follows the normal version and release gates. Urgency does not
permit skipping tests, version identity, or no-overwrite guarantees.

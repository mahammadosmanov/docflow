# Signed in-app updates from immutable GitHub Releases

Status: Accepted

## Decision

- The first update-enabled distribution is the Windows x64 NSIS package. macOS
  remains outside the updater channel until it has a verified package and
  release pipeline; the application must not claim an update is available on
  an unsupported target.
- Rust owns update checks, signature verification, download, and installation.
  The webview receives only bounded metadata and progress through the existing
  closed frontend bridge. It never receives updater plugin IPC permissions.
- Update checks are public so users can learn about releases before signing in.
  Installation requires an explicit click and consumes the exact update object
  returned by the preceding signed check once.
- Every release manifest points to a version-tagged GitHub asset. Stable users
  read `latest.json` from GitHub's latest stable release. Versioned artifact
  signatures and trusted version comments remain mandatory.
- The public verification key is supplied to the release build through the
  repository variable `DOCFLOW_UPDATER_PUBLIC_KEY`. The private signing key and
  optional password exist only as GitHub Actions secrets. Missing key material
  fails the package/release job closed.
- Existing installations need one manual install of an update-enabled package
  before they can use in-app updates. That bootstrap release cannot update
  binaries that do not contain the updater code and pinned public key.

## Consequences

- Windows x64 users can check from the app, inspect release notes, and install
  a signed update without opening GitHub.
- Releases remain immutable. A stable release is published only with its
  installer, updater archive, signature, checksums, package evidence, and
  `latest.json` together.
- Adding macOS requires a separately verified macOS updater bundle and release
  manifest entry. A Windows-only manifest must never be treated as universal.

## Verification

- Manifest tests reject version/tag mismatch and bind the signature to an
  immutable release URL.
- Rust authorization tests keep update commands public and Rust-owned.
- Frontend tests verify notification and user-triggered installation.
- The Windows release runner must perform signed bundle generation, clean
  install/startup/uninstall smoke, and exact asset verification.

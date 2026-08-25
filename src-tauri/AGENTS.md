# Rust/Tauri agent instructions

These rules extend the root `AGENTS.md` for `src-tauri/`.

- Rust/Tauri is the sole trusted backend. Expose only closed, typed commands with bounded inputs and stable error codes.
- Enforce identity, role, path grants, canonicalization, symlink policy, and containment in Rust; frontend checks are not authorization.
- Store plans server-side as immutable one-shot values with backend provenance. Apply accepts only an opaque plan identity and never recalculates.
- Only the Rust authority may mutate real state. Partial or unknown effects enter recovery and are never blindly retried.
- Existing destinations are never overwritten. Use atomic no-replace primitives and re-check races at execution time.
- Write a pending journal record durably before every effect, then commit successful state. Undo trusts only validated local journals, runs in reverse, protects later edits with hashes, and removes only empty created directories.
- Keep Supabase access/refresh tokens in Windows Credential Manager or macOS Keychain after the approved migration; never send them to the webview or logs.
- Keep cloud history metadata-only. Never deserialize cloud/client paths into executable steps.
- Blocking filesystem, OCR, PDF, Office, network, and printer work must not run on the UI thread.
- Preserve Windows and macOS behavior. Platform-specific adapters require target-specific tests and packaged verification.
- Native engines such as Tesseract, PDF renderers, Office, and OS print services are allowed when they are more reliable than reimplementation; load heavy adapters lazily.

Behavior changes use Rust RED → GREEN → REFACTOR. Verify with:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

Do not add an application sidecar, interpreter, shell plugin, external binary, or
second mutation backend. Platform-specific adapters remain behind bounded Rust APIs.

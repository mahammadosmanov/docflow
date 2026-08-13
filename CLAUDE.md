# DocFlow engineering rules

DocFlow is a Tauri 2 desktop application:

- shell: Rust/Tauri 2 under `src-tauri/`
- UI: React 19 + TypeScript + Vite under `frontend/`
- engine: a PyInstaller Python sidecar backed by `src/docflow/`
- OCR: PyMuPDF + OpenCV + Tesseract
- cloud: Supabase Auth/PostgREST over HTTPS
- packaging: Tauri bundler + PyInstaller sidecar

## Non-negotiable rules

1. Every filesystem mutation is preview → user approval → apply → journal.
2. `apply()` executes only the approved `Plan`; it does not recalculate.
3. Never overwrite an existing destination.
4. Undo reverses steps and removes only empty directories.
5. User-visible text must remain available in all four locales (`tr`, `en`, `ru`, `az`).
6. Forms are generated from `ParamSchema`; never create an operation-specific form.
7. Canonical RFI codes are NFC + uppercase once; never lowercase them.
8. Keep the strict `-RFI-` anchor. A miss is recoverable; a false label is silent corruption.
9. The UI is monochrome. Meaning comes from controls, text, icons, and border weight.
10. OCR/filesystem/network work remains in the Python sidecar, never in the webview.
11. Never expose a Supabase service-role or secret key in the desktop client.
12. Supabase access and refresh tokens remain inside the sidecar process.

## Verification

```bash
.venv/bin/python -m pytest
.venv/bin/python -m mypy src/docflow
npm test
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

Run `.venv/bin/python tools/build_sidecar.py`, then
`npm run tauri build` for packaging. Use
`.venv/bin/python tools/ci_ocr_smoke.py <sidecar-executable>` for the real OCR
path.

The pre-migration Flutter and Qt UI sources under `legacy/` are read-only
rollback material. Do not make active changes there.

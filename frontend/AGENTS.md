# Frontend agent instructions

These rules extend the root `AGENTS.md` for `frontend/`.

- React is presentation and transient UI state only.
- Do not add filesystem, document, OCR, network, credential, shell, process, Office, or printer authority to the webview.
- Use only the closed Rust-owned Tauri command map. Do not introduce shell/process
  access, a direct alternate backend bridge, or untyped command names.
- Render operation forms only from `ParamSchema`; do not create operation-specific forms.
- Preserve the monochrome visual language. Use text, controls, icons, and border weight for meaning.
- Add every user-visible string to all four canonical ARB catalogs: `tr`, `en`, `ru`, and `az`.
- Keep keyboard navigation, focus, accessible names, scaling, contrast, responsive layout, and screen-reader behavior intact.
- Treat plans as review data. The UI may submit only opaque plan/journal/request identities and approved user decisions, never executable filesystem steps.
- Use Tauri channels for long-running progress/cancellation when the Rust boundary is available; do not poll at high frequency.
- Do not show migration/backend mechanics in normal workflows. Put diagnostics behind an advanced surface.

Verify frontend changes with:

```bash
npm test
npm run build
```

Behavior changes require a focused Vitest RED run before production edits, the same selector GREEN afterward, and then the complete frontend gates.

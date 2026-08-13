# Project tasks

## Verification on every change

```bash
python -m pytest
python -m mypy src/docflow
npm test
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

For a release:

```bash
python tools/build_sidecar.py
python tools/ci_ocr_smoke.py dist/docflow-sidecar
npm run tauri build
```

## OCR calibration

Run the read-only probe on representative scans:

```bash
python tools/rfi_probe.py /path/to/scans --sweep
```

Only change the OCR cascade after measuring representative documents. Maintain
at least 95% extraction accuracy and keep the strict `-RFI-` anchor.

## Adding a document operation

1. Add and register a module under `src/docflow/ops/`.
2. Declare its generic `ParamSchema`.
3. Add keys to all four ARB catalogs.
4. Add pure behavior, filesystem safety, bridge, and UI tests.

Do not add an operation-specific React form. Extend the generic schema renderer
only for a genuinely new parameter kind.

# Release safety

This document defines the gates that prevent one DocFlow feature or packaging
change from silently breaking another.

## Release invariants

1. Only `.github/workflows/windows-release.yml` and `tools/build_windows.ps1`
   define a distributable Windows build.
2. Python, Node, Rust lockfiles, Tesseract, and direct application dependencies
   remain pinned. Dependency upgrades are separate reviewed changes.
3. Tesseract and the Python sidecar are bundled; user `PATH` is not trusted.
4. Preview must not mutate files. Packaged OCR smoke verifies source/destination
   state as well as extraction output.
5. `ping` alone is insufficient: tests verify the complete operation ID set,
   version contract, and real image-only OCR path.
6. A release tag must equal `v` plus the exact canonical `VERSION`.
7. Existing destinations, source archives, and unrelated workbook package members
   are never overwritten.

## Required gates

```bash
python tools/version_manager.py check
.venv/bin/python -m pytest
.venv/bin/python -m mypy src/docflow
npm test
npm run build
python tools/build_sidecar.py
cargo check --manifest-path src-tauri/Cargo.toml
```

On Windows, the canonical release command is:

```powershell
./tools/build_windows.ps1
```

It additionally stages pinned Tesseract, freezes and smoke-tests the sidecar,
runs real packaged OCR, builds the NSIS installer, and verifies that the app is
a GUI executable without a companion console window.

## Regression matrix

| Area | Mandatory evidence | Risk controlled |
|---|---|---|
| Version contract | `version_manager.py check` | Mixed UI/sidecar/installer versions |
| Process Scans | parser/OCR unit tests + image-only smoke | OCR drift, false labels, missing engine/data |
| Signed pages | rotated/cropped PDF tests | Archive mutation, wrong page geometry, partial output |
| RFI Log | workbook/OOXML tests | Formula, validation, style, lock, and repair damage |
| Batch Print | queue/spooler tests | Order loss, duplicated jobs, unrecoverable resume state |
| Operation registry | exact operation ID test | Feature omitted from frozen sidecar |
| Localization | four-catalog key equality | Raw keys or missing language strings |
| Security boundary | sidecar/bridge tests | Tokens or authority crossing into webview |
| Package | sidecar smoke + OCR smoke + NSIS build | Source/package behavior mismatch |

## Release procedure

1. Start from a clean, reviewed commit on `main`.
2. Complete the version/changelog/catalog steps in `docs/VERSIONING.md`.
3. Run all local gates after the final edit.
4. Tag the exact commit `v$VERSION`; never move or reuse a published tag.
5. Push the tag and inspect the Windows workflow result.
6. Confirm the release contains both the installer and its generated `.sha256`
   asset; verify the checksum before running the installer.
7. Keep the previous GitHub Release available until the new installer has been
   manually opened and the critical workflow smoke-tested on Windows.
8. If any gate fails, do not publish or relabel the build as stable.

## Rollback

- Code rollback: redeploy/reinstall the last verified immutable tag.
- Document rollback: use the operation journal; never replace it with a source-code rollback.
- Release metadata rollback: mark the bad release superseded/yanked in notes;
  do not delete history or retarget its tag.
- Partial local builds are staged separately and never copied over a working install.

## Budget boundary

The safety model does not require paid infrastructure. Local scripts provide the
same checks as CI. GitHub Actions and Releases are convenient mirrors; when free
quota is exhausted, releases wait rather than skipping gates or incurring cost.

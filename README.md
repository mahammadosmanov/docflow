# DocFlow

DocFlow 0.6.0 keeps all three original RFI workflow operations and adds a safe
signed-page staging workflow:

- Process Scans: OCR, rename, organize into optional RFI folders, and review
- Add RFI Log entries: multi-sheet Excel scheduling, editable preview, safe
  workbook backup/verification, email extraction, and Excel-native mail copy
- Batch Print: one strictly ordered Windows print queue for mixed file types
- Stage signed RFI first pages: identify signed scans, keep the archive strictly
  read-only, and create review copies in the original folder structure with only
  page one replaced

The active desktop stack is intentionally small at the product layer:

- Tauri 2 for the native window, lifecycle, dialogs, and packaging
- React 19 + TypeScript for the schema-driven interface
- the proven Python engine for PDF/OCR, filesystem safety, Supabase, and undo
- a private JSON-lines sidecar channel; no local server and no exposed port

The webview never receives Supabase access/refresh tokens and cannot directly
touch arbitrary files. All document changes remain preview-first and journaled.

## Development

Requirements: Python 3.12+, Node 22+, stable Rust, and Tesseract.

```bash
python3.12 -m venv .venv
source .venv/bin/activate
python -m pip install -r requirements-dev.txt
python -m pip install -e .
npm ci

python tools/version_manager.py check
python -m pytest
python -m mypy src/docflow
npm test
npm run build
python tools/build_sidecar.py
npm run tauri dev
```

## Project and version tracking

The complete product/release record lives in the repository and requires no
paid service:

- [`docs/PRODUCT.md`](docs/PRODUCT.md) — living product requirements
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — Now / Next / Later outcomes
- [`docs/DECISIONS.md`](docs/DECISIONS.md) — architecture decision index
- [`docs/VERSIONING.md`](docs/VERSIONING.md) — SemVer and release workflow
- [`releases.json`](releases.json) — every known version in machine-readable form
- [`CHANGELOG.md`](CHANGELOG.md) — user-visible changes and safety notes

Run `npm run versions` to list every release and `npm run version:check` to
detect version drift before a build.

On macOS, `./tools/run_macos.sh` prepares missing local dependencies, stages
the sidecar, and starts the Tauri application.

## Release build

```bash
python tools/build_sidecar.py
npm run tauri build
```

Tauri produces native installers under `src-tauri/target/release/bundle/`.
The Windows workflow also bundles Tesseract and runs OCR against a real
image-only PDF before producing an NSIS installer.

## Windows installer

Stable installers are published under the repository's **Releases** page. To
produce an unversioned test build without a prepared Windows machine, open
**Actions → Windows Release → Run workflow**. When free Actions artifact storage
is available, download the `DocFlow-windows-x64` artifact. A test build never
replaces a stable or preview release asset.

For a local Windows build, install Python 3.12+, Node 22+, stable Rust, Visual
Studio C++ Build Tools, and Tesseract. Then run:

```powershell
npm run build:windows
```

That single command installs pinned project dependencies, runs every active
test and type check, stages Tesseract, freezes and smoke-tests the Python
sidecar, verifies real image-only PDF OCR, and creates the installer under
`src-tauri\target\release\bundle\nsis\`.

Building the Windows artifact on Windows or in the supplied GitHub Actions
runner is recommended. Although Tauri can cross-compile some Windows bundles,
the native Python/OpenCV sidecar also has to be frozen for Windows.

## Safety model

1. `preview()` reads inputs and creates an immutable `Plan`.
2. The user reviews and approves that exact plan.
3. `apply()` executes only approved steps and journals each mutation.
4. `undo()` reverses journaled steps and never overwrites an existing file.

The signed-page staging operation additionally requires three non-overlapping
roots: new scans, the read-only archive, and a separate review output. Duplicate
scan numbers, missing/ambiguous archive matches, and existing outputs are
reported without selecting or overwriting anything.

Unknown or low-confidence documents go to `_review/`; delete from review is a
recoverable move to `~/.docflow/trash/`.

See [architecture](docs/ARCHITECTURE.md), [migration decision](docs/MIGRATION.md),
and [Supabase schema](docs/supabase_auth.sql).

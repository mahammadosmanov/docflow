# DocFlow

DocFlow 1.33.80 is a Tauri 2 desktop application with nine registered document
workflows:

- Fast RFI Scan: separate confirmed multi-RFI scans or read only page one for
  a single RFI while keeping the complete PDF; review and correct canonical
  RFI numbers before Apply.
- Group Files by RFI: require matching EML/MSG evidence, then copy each related
  incoming file into exact RFI folders while preserving the source
- Add RFI Log entries: multi-sheet Excel scheduling, editable preview, safe
  workbook backup/verification, email extraction, and Excel-native mail copy
- Batch Print: one strictly ordered mixed-file Windows queue with resilient
  physical-printer observation, explicit recovery, named presets, and optional
  one-click recall of previous safe settings without retaining old files
- Stage signed RFI first pages: identify every page in a combined Worley scan,
  search every nested folder below the selected read-only RFI System root with
  bounded multi-core discovery, and create review copies with only the exact
  matched page replacing page one, with a bounded old/new page comparison
- Correct RFI PDF page orientation: scan every page below CIV, MEC, ELE, and PRC
  with bounded multi-core content analysis and create fidelity-checked copies in
  a separate review tree without modifying the RFI system
- Synchronize RFI Follow Up: add/update uniquely mapped CIV/MEC groups, remove
  Cancelled/Rejected entries, and project FINAL Worley evidence, including the
  current PCU FINAL `Signed by Worley` column
- Analyze RFI System: detect Worley signatures and guard RFI Log/Follow Up updates.
  Insufficient signature evidence remains unsigned; it cannot promote status to
  `Signed`, may set a safe existing status to `Not Signed`, and never downgrades
  an existing `Signed` value.
- Mark completed RFIs: recursively find exact completed RFI PDFs and preview
  guarded RFI Log Inspection Status updates to Completed

Authenticated users can reach Home, Review, History, Recovery, Preferences,
and (when authorized) Admin from persistent navigation. Fast RFI Scan is the
primary home task; all other registered workflows remain searchable and
category-filtered.

Large RFI-system scans and replica comparisons use a durable local incremental
index. Completed work survives interruption, unchanged files skip repeated
rendering, OCR, or hashing, and Preview shows reuse/fresh/failure counts. Cached
analysis never replaces Apply-time hash and workbook guards.

File-specific workbook, OneDrive, lock, and fingerprint checks remain part of
each operation preview. Every operation preview also shows one compact impact summary for new outputs,
updated files, moves, print jobs, drafts, cleanup actions, and warnings before
Apply becomes available. Editable previews provide an optional bounded list of
the approved effects with their full source and destination paths. Worley
signed-page previews also show bounded thumbnails of the selected archive page
and exact signed source page without exposing local file access to React.
Large warning sets stay compact and can be searched or filtered by type; exact
Worley RFI warnings can focus their related page comparison directly.
Existing-cell RFI Log and Follow Up updates can also be expanded into an exact
sheet/cell, current-value, and after-Apply comparison.

Settings support diagnostics includes the latest owned print queue's pathless
printer, format, route, option, attempt, outcome, count, and redacted-error
evidence. It never includes document names, paths, contents, credentials, or
tokens and never claims that software status proves physical paper output.

The shipping stack is Tauri 2 plus a Rust authority and a thin React 19 /
TypeScript presentation layer. Tesseract and operating-system document/print
adapters are loaded only when needed. There is no application sidecar, embedded
interpreter, or local server.

The webview never receives Supabase tokens and cannot directly touch arbitrary
files. All document changes remain preview-first, explicitly approved, journaled,
and undoable.

## Development

Requirements: Node 22+, stable Rust, the platform Tauri prerequisites, and
Tesseract 5 for OCR development.

```bash
npm ci
node tools/version_manager.mjs check
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
npm run audit:licenses
npm run tauri dev
```

`npm run build` includes the final-source audit, which fails if retired runtime,
sidecar, shell-plugin, or packaging configuration is reintroduced. Rust builds
apply the same fail-fast guard.

## Project and version tracking

- [`docs/PRODUCT.md`](docs/PRODUCT.md) — living product requirements
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — Now / Next / Later outcomes
- [`docs/DECISIONS.md`](docs/DECISIONS.md) — architecture decision index
- [`docs/VERSIONING.md`](docs/VERSIONING.md) — SemVer and release workflow
- [`releases.json`](releases.json) — machine-readable release history
- [`CHANGELOG.md`](CHANGELOG.md) — user-visible changes and safety notes

Run `npm run versions` to list releases and `npm run version:check` to detect
version drift. On macOS, `./tools/run_macos.sh` verifies the source contract,
stages the native OCR resources if needed, and starts the application.

## Unsigned local packages

```bash
npm run tauri build
```

Tauri writes native packages under `src-tauri/target/release/bundle/`. Audit a
bundle before use:

```bash
node tools/package_audit.mjs package src-tauri/target/release/bundle/macos/DocFlow.app
node tools/checksum.mjs path/to/package
```

The packaged executable exposes an isolated, non-GUI verification mode used by
release automation:

```bash
path/to/docflow --package-smoke artifacts/package-smoke.json
```

It round-trips bootstrap/settings, performs real image-only OCR, and applies then
undoes a representative Fast RFI Scan fixture in a temporary directory.

## Windows installer

On a Windows host with Node 22+, stable Rust, Visual Studio C++ Build Tools, and
Tesseract 5, run:

```powershell
npm run build:windows
```

The command runs all source gates, stages Tesseract, audits dependency licenses,
builds the NSIS installer, exercises the unpackaged release executable, audits
the artifact, and writes a SHA-256 file. The Windows Actions workflow additionally
performs a silent clean install, packaged startup/OCR/apply/undo smoke, content
audit, and uninstall. Office/Excel COM and representative physical/network printer
certification remain explicit manual Windows-host gates.

## Safety model

1. `preview()` reads inputs and creates an immutable `Plan`.
2. The user reviews and approves that exact plan.
3. `apply()` executes only approved steps and journals each mutation.
4. `undo()` reverses journaled steps and never overwrites an existing file.

Unknown or low-confidence documents go to `_review/`; delete from review is a
recoverable move to `~/.docflow/trash/`.

See [architecture](docs/ARCHITECTURE.md), [migration decision](docs/MIGRATION.md),
and [Supabase schema](docs/supabase_auth.sql).

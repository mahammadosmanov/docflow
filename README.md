# DocFlow

DocFlow 1.20.16 is a Flutter desktop application with a Dart engine and nine
registered document workflows:

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
and exact signed source page without exposing local file access to the UI layer.
Large warning sets stay compact and can be searched or filtered by type; exact
Worley RFI warnings can focus their related page comparison directly.
Existing-cell RFI Log and Follow Up updates can also be expanded into an exact
sheet/cell, current-value, and after-Apply comparison.

Settings support diagnostics includes the latest owned print queue's pathless
printer, format, route, option, attempt, outcome, count, and redacted-error
evidence. It never includes document names, paths, contents, credentials, or
tokens and never claims that software status proves physical paper output.

The stack is Flutter with a Dart engine under `flutter/app/lib/engine`. The
engine is the sole application and mutation authority; the Flutter UI is a thin
presentation layer. Tesseract and operating-system document/print adapters are
loaded only when needed. There is no application sidecar, embedded interpreter,
or local server.

The UI layer never receives Supabase tokens and cannot directly touch arbitrary
files. All document changes remain preview-first, explicitly approved, journaled,
and undoable.

## Development

Requirements: Flutter 3.47.7 (stable) with the desktop target for your host.
Tesseract 5 is needed for OCR development.

```bash
node tools/version_manager.mjs check
node tools/docs_audit.mjs
cd flutter/app
flutter pub get
flutter analyze
flutter test
flutter run -d macos   # or -d windows on a Windows host
```

`flutter analyze` must report zero errors and `flutter test` must be green.

## Project and version tracking

- [`docs/PRODUCT.md`](docs/PRODUCT.md) — living product requirements
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — Now / Next / Later outcomes
- [`docs/DECISIONS.md`](docs/DECISIONS.md) — architecture decision index
- [`docs/VERSIONING.md`](docs/VERSIONING.md) — SemVer and release workflow
- [`releases.json`](releases.json) — machine-readable release history
- [`CHANGELOG.md`](CHANGELOG.md) — user-visible changes and safety notes

Run `node tools/version_manager.mjs list` to list releases and
`node tools/version_manager.mjs check` to detect version drift.

## Windows build

Windows packages are built on a real Windows host with `flutter build windows
--release` from `flutter/app`. The packaged executable has not been verified on
Windows from this macOS workspace; Office, Excel COM, Outlook, printer, and
signing checks remain explicit manual Windows-host gates.

## Safety model

1. `preview()` reads inputs and creates an immutable `Plan`.
2. The user reviews and approves that exact plan.
3. `apply()` executes only approved steps and journals each mutation.
4. `undo()` reverses journaled steps and never overwrites an existing file.

Unknown or low-confidence documents go to `_review/`; delete from review is a
recoverable move to `~/.docflow/trash/`.

See the [Dart engine decision](docs/adr/0039-dart-engine-supersedes-no-second-backend.md)
and the [Supabase schema](docs/supabase_auth.sql).

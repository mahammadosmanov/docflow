# Worker brief (read fully)

Repo: /home/claude/docflow, branch flutter-migration. Do NOT switch branches, commit, or push; the orchestrator commits. Do not edit files outside your assigned paths. Do not edit anything outside /home/claude/docflow/flutter/app (except reading).
Goal: faithful 1:1 Dart port with FULL parity (no functionality dropped, no stubs, no TODO placeholders; if something truly cannot be ported on Linux, implement it behind an interface and write a clear `// UNVERIFIED:` comment plus list it in your final report).
Source: Rust in /home/claude/docflow/src-tauri/src (engine), React in /home/claude/docflow/frontend/src (UI). Read AGENTS.md, docs/ARCHITECTURE.md and relevant docs/*.md and docs/adr/* for behavior. Inline-test fixtures: src-tauri/tests/fixtures/*.json and src-tauri/tests/*.rs; port those tests to Dart `flutter/app/test/engine/...` and replay the JSON fixtures.
Layout: engine code in flutter/app/lib/engine/<module>.dart mirroring Rust file names (snake_case); UI in flutter/app/lib/ui/...; models in lib/models/.
Toolchain: `export PATH=/opt/flutter/bin:$PATH PUB_CACHE=/opt/pub-cache FLUTTER_ALLOW_ROOT=1 CI=true; cd /home/claude/docflow/flutter/app`. Run `dart analyze <your files>` and `flutter test <your tests>`; fix until clean. Only add pub dependencies you need via editing pubspec.yaml sparingly (note them in your report; prefer pure-Dart: crypto, archive, xml, path, http, sqlite3, intl, uuid, characters). Another worker may be adding deps concurrently; do `flutter pub get` and ignore unrelated analyzer errors in other files.
Cross-module deps: if you need a type from a module owned by another worker, import it by the mirrored path (lib/engine/<name>.dart); if not present yet, define a minimal abstract interface in your own file and say so in your report.
Final report (≤200 words): files written, tests passing count, deviations/gaps, unverified items.
Your assignment:

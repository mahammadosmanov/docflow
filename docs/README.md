# Documentation map

This directory is the onboarding entry point for operators, maintainers, and
coding agents. The Dart engine under `flutter/app/lib/engine` is the sole
application and mutation authority. Runtime behavior and executable tests win if
prose ever drifts.

## Read in this order

1. [`PRD.md`](PRD.md) — requirement IDs, scope, and acceptance boundary.
2. [`FEATURES.md`](FEATURES.md) — complete operator-visible behavior.
3. [`RFI_DOMAIN.md`](RFI_DOMAIN.md) — exact identifiers, workbook, scan, mail,
   and lifecycle rules.
4. [`SAFETY_AND_STATE.md`](SAFETY_AND_STATE.md) — preview/apply/recovery/undo
   invariants and persistent state.
5. [`EDITOR_CONTRACT.md`](EDITOR_CONTRACT.md) — reusable schema and table UI.
6. [`TRACEABILITY.md`](TRACEABILITY.md) — requirements to implementation and tests.
7. [`DECISIONS.md`](DECISIONS.md), [`KNOWN_ISSUES.md`](KNOWN_ISSUES.md), and
   [`RELEASE_SAFETY.md`](RELEASE_SAFETY.md) — accepted choices, honest debt, gates.
8. [`adr/0039-dart-engine-supersedes-no-second-backend.md`](adr/0039-dart-engine-supersedes-no-second-backend.md)
   — why the Dart engine is the single backend.
9. [`FEATURE_DEVELOPMENT.md`](FEATURE_DEVELOPMENT.md) — slice, measure, verify,
   and accept every feature consistently.
10. [`VERSIONING.md`](VERSIONING.md) and [`ROADMAP.md`](ROADMAP.md) — version
    rules and planned work.

## Source-of-truth order

1. `AGENTS.md` and accepted ADRs
2. `PRODUCT.md` plus `PRD.md`
3. Dart engine code under `flutter/app/lib/engine` and its tests
4. Executable tests and frozen synthetic fixtures under `flutter/app/test/fixtures`
5. Release/version metadata (`VERSION`, `releases.json`, `CHANGELOG.md`)
6. Roadmap prose

Never use a private workbook, scan, email, secret, or production path as a
committed fixture. Windows/Office/printer results must be reported separately
from macOS, source, synthetic, package-smoke, or cross-compile evidence.

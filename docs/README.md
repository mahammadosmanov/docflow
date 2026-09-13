# Documentation map

This directory is the onboarding entry point for operators, maintainers, and
coding agents. Runtime behavior and executable tests win if prose ever drifts.

## Read in this order

1. [`PRD.md`](PRD.md) — requirement IDs, scope, and acceptance boundary.
2. [`FEATURES.md`](FEATURES.md) — complete operator-visible behavior.
3. [`RFI_DOMAIN.md`](RFI_DOMAIN.md) — exact identifiers, workbook, scan, mail,
   and lifecycle rules.
4. [`SAFETY_AND_STATE.md`](SAFETY_AND_STATE.md) — preview/apply/recovery/undo
   invariants and persistent state.
5. [`EDITOR_CONTRACT.md`](EDITOR_CONTRACT.md) — reusable schema and table UI.
6. [`ARCHITECTURE.md`](ARCHITECTURE.md) and
   [`IPC_PROTOCOL.md`](IPC_PROTOCOL.md) — runtime authority and command boundary.
7. [`TRACEABILITY.md`](TRACEABILITY.md) — requirements to implementation and tests.
8. [`DECISIONS.md`](DECISIONS.md), [`KNOWN_ISSUES.md`](KNOWN_ISSUES.md), and
   [`RELEASE_SAFETY.md`](RELEASE_SAFETY.md) — accepted choices, honest debt, gates.
9. [`WINDOWS_RUNBOOK.md`](WINDOWS_RUNBOOK.md) — build and native acceptance.

## Source-of-truth order

1. `AGENTS.md` and accepted ADRs
2. `PRODUCT.md` plus `PRD.md`
3. Rust contracts, registry, and operation code
4. Executable tests and frozen synthetic fixtures
5. Release/version metadata
6. Roadmap prose

Never use a private workbook, scan, email, secret, or production path as a
committed fixture. Windows/Office/printer results must be reported separately
from macOS, source, synthetic, package-smoke, or cross-compile evidence.

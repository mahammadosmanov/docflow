# ADR 0006: Machine-checked onboarding canon

- Date: 2026-09-13
- Status: Accepted

## Context

The Rust migration had accurate product and architecture documents, but no
single ordered map, stable requirement identifiers, full feature ledger,
domain/state/editor contracts, traceability index, known-issue ledger, or native
Windows runbook. A new maintainer or coding model could therefore infer behavior
from stale history and miss a non-negotiable safety boundary.

## Options considered

1. Keep relying on source discovery and migration history.
2. Add prose only, without an executable drift check.
3. Add a bounded documentation canon and audit it against required files,
   requirement IDs, map links, and the live Rust operation roster.

## Decision

Use option 3. `docs/README.md` defines the order. `PRD.md` owns stable `DF-###`
IDs, `TRACEABILITY.md` owns primary code/test links, and `FEATURES.md` mirrors the
registered operation roster. `tools/docs_audit.mjs` is a canonical release gate.

## Consequences

Documentation changes have a small maintenance cost, but missing and obvious
roster/ownership drift fail early. The audit intentionally checks structure,
not semantic truth; focused code review and executable tests remain authoritative.
External platform acceptance remains separate and cannot be satisfied by prose.

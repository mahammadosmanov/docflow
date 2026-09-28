# ADR 0033: Exclude CIV PCS-tagged RFI Log records from Follow Up

## Status

Accepted; supersedes the CIV PCS routing behavior in ADR 0032.

## Context

The operator confirmed that CIV RFI Log records whose `Tag No` begins with
`PCS-` do not yet have a Follow Up destination. This includes composite values
such as `PCS-19/S1,S2,S3`. Previous routing could place those RFIs in PCU or
CIV Follow Up sections based on ITP, item, block, and area matching.

## Decision

For records on the RFI Log CIV sheet, any `Tag No` value containing a normalized
token beginning with `PCS-` is outside current Follow Up scope. Never add it to
Follow Up. During the next approved sync, remove its identifier from existing
Follow Up groups and clear the corresponding status when that group becomes
empty. If a group retains other RFIs, preserve those identifiers and derive
the status from the remaining source records. Existing preview, explicit Apply,
backup, journal, and Undo safeguards continue to govern these removals.

## Consequences

Exact CIV area matching remains available for non-PCS tags. MEC context matching
is unchanged. PCU FINAL lifecycle or Worley rows sourced from CIV PCS-tagged
records are also excluded. No rows or sections are created. Formula-backed,
merged, or otherwise unsafe existing cells remain in manual review.

## Verification

Native tests cover ordinary and composite PCS tags, skipping new Follow Up
entries, removing existing entries, clearing empty-group status, and keeping
the remaining grouped-RFI behavior guarded.

## Propagation

- DF-016 in `docs/PRD.md`: define CIV PCS exclusion and cleanup behavior.
- `docs/RFI_DOMAIN.md` and `docs/FEATURES.md`: describe the current scope.
- `docs/TRACEABILITY.md`: link the exclusion and removal regressions.
- `src-tauri/src/rfi_follow_up.rs`: mark PCS-tagged CIV records for cleanup and
  block their normal and FINAL projection.
- `src-tauri/tests/rfi_follow_up_native.rs`: cover add prevention and cleanup.

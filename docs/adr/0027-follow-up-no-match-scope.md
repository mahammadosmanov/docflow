# ADR 0027: Skip supported Follow Up routes with no existing target row

- Date: 2026-09-26
- Status: Superseded

## Historical context

This ADR originally treated supported CIV/MEC routes with no existing target
row as outside template scope and omitted them from manual review. That
zero-match omission is no longer current policy.

## Superseding decision

The earlier decision to omit zero-match records from Preview is superseded.
Supported CIV/MEC records, including FINAL records, that lack a matching
prepared Follow Up row must remain visible in manual review and contribute to
the operator warning. The sync must not automatically add a row, create a
section, or synchronize a record without its required prepared destination.
Ambiguous or otherwise unsafe mappings also remain reviewable. This policy
preserves visibility without changing the existing no-row-creation boundary.

This supersedes only ADR 0027's zero-match handling. CIV records whose `Tag No`
starts with `PCS-` remain explicitly excluded by [ADR 0033](0033-exclude-civ-pcs-rfis-from-follow-up.md);
that exclusion is not a zero-match warning case.

## Historical alternatives and consequences

- Keeping zero-match records in manual review preserves visibility but may
  increase Preview warnings; this is now the required behavior.
- Creating rows or sections remains out of scope and is not permitted.

The original implementation omitted zero-match records from warnings. The
current policy instead surfaces those unmatched records for review while
continuing to synchronize only records with valid prepared destinations.

## Propagation

- `docs/PRD.md`: DF-016 zero-match and multiple-match rules.
- `docs/PRODUCT.md`, `docs/FEATURES.md`, and `docs/RFI_DOMAIN.md`: operator
  behavior and scope.
- `docs/TRACEABILITY.md`: DF-016 regression coverage.
- `src-tauri/src/rfi_follow_up.rs`: classify unmatched supported standard and
  FINAL routes for visible review, without creating destinations.
- `src-tauri/tests/rfi_follow_up_native.rs`: standard and FINAL zero-match
  warning/manual-review plus ambiguous-row coverage.

No rows or sections are created. CIV records with `PCS-` Tag No values remain
excluded under ADR 0033 and do not enter zero-match review.

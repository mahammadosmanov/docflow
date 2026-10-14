# ADR 0027: Skip supported Follow Up routes with no existing target row

- Date: 2026-09-26
- Status: Superseded by [ADR 0036](0036-follow-up-context-scope-and-diagnostics.md)

## Historical context

This ADR originally required every supported CIV/MEC route without an exact
prepared target row to remain visible in Manual Review. The policy created
warnings for source locations absent from the Follow Up template, even when
those locations were not intended to be tracked there.

## Superseding decision

[ADR 0036](0036-follow-up-context-scope-and-diagnostics.md) replaces that
blanket zero-match warning rule. If no source context matches any prepared row
in the intended destination, omit the record from Manual Review regardless of
unrelated existing placements; do not propose RFI/status changes and leave any
existing cells untouched. Keep warnings when a source context is represented
but a target is unsafe, when only some source contexts match, or when an
otherwise in-scope RFI has an unrelated placement. Continue to prohibit
automatic row or section creation.

CIV RFIs whose `Tag No` starts with `PCS-` remain explicitly excluded by
[ADR 0033](0033-exclude-civ-pcs-rfis-from-follow-up.md); that separate
exclusion is not a zero-match warning case.

## Historical alternatives and consequences

- At the time this ADR was active, keeping zero-match records in Manual Review
  preserved visibility but increased Preview warnings.
- Creating rows or sections remained out of scope.

The original implementation omitted zero-match records from warnings. This ADR
then changed that behavior to surface them; ADR 0036 later narrowed it to
actionable template-scope mismatches and partial-context cases.

## Historical propagation

- `docs/PRD.md`: DF-016 zero-match and multiple-match rules.
- `docs/PRODUCT.md`, `docs/FEATURES.md`, and `docs/RFI_DOMAIN.md`: operator
  behavior and scope at the time of this ADR.
- `docs/TRACEABILITY.md`: DF-016 regression coverage.
- `src-tauri/src/rfi_follow_up.rs`: previously classified supported standard
  and FINAL zero-match routes for visible review without creating destinations.
- `src-tauri/tests/rfi_follow_up_native.rs`: standard and FINAL zero-match
  warning/manual-review plus ambiguous-row coverage.

No rows or sections are created. CIV records with `PCS-` Tag No values remain
excluded under ADR 0033 and do not enter zero-match review.

# ADR 0027: Skip supported Follow Up routes with no existing target row

- Date: 2026-09-26
- Status: Accepted

## Context

RFI Log may contain supported CIV/MEC ITP clauses that are intentionally absent
from a particular Inspection Status Follow Up template. Reporting every such
record as unmapped made a large preview warning even when no corresponding
Follow Up section or row was expected. The operator still needs a clear signal
when one source record could map to multiple existing Follow Up rows.

## Decision

For the existing Follow Up projection, a source record with a recognized
CIV/MEC route and zero matching rows in the current template is outside the
template's scope. This includes FINAL records whose item has no prepared row.
Skip it without adding a manual-review row or incrementing the unmapped-record
warning. A source record matching exactly one existing row continues through
the current guarded sync. A record matching multiple rows remains unselected in
manual review and contributes to the warning count, except for the exact CIV
multi-area case added by [ADR 0029](0029-exact-civ-multi-area-follow-up.md).

This rule does not create sections, rows, or cells. Formula, merged, protected,
and other unsafe matched targets continue through existing manual review.

## Alternatives

- Keep every zero-match source record in manual review: preserves visibility of
  unused ITP items but obscures meaningful ambiguity in large previews.
- Create rows or sections automatically: changes the workbook template and is
  outside the existing projection's mutation authority.

## Consequences and rollback

Preview warnings now count only records that need operator attention for
non-unique mapping. Existing exact matches still synchronize; duplicate matches
remain visible for manual review. Reverting the source change restores the old
preview classification without changing workbook data.

## Propagation

- `docs/PRD.md`: DF-016 zero-match and multiple-match rules.
- `docs/PRODUCT.md`, `docs/FEATURES.md`, and `docs/RFI_DOMAIN.md`: operator
  behavior and scope.
- `docs/TRACEABILITY.md`: DF-016 regression coverage.
- `src-tauri/src/rfi_follow_up.rs`: classify unmatched supported standard and
  FINAL routes.
- `src-tauri/tests/rfi_follow_up_native.rs`: standard and FINAL zero-match skip
  plus ambiguous-row manual-review coverage.
- No stories, glossary, locale, UI, or override/void marker changes are needed.

No prior decision is voided; this clarifies the existing template boundary in
ADR 0008.

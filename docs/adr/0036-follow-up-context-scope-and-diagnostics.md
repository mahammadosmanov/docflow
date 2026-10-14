# ADR 0036: Follow Up context scope and actionable mapping diagnostics

- Date: 2026-10-14
- Status: Accepted

## Context

The RFI Log contains many supported-route records whose `Tag No`, `Area`, or `Inspection Location` does not identify any prepared Follow Up destination. These can describe locations or work outside the Follow Up workbook, such as quarries, laydown areas, and other areas absent from its prepared rows. Treating every such record as a missing mapping creates noise and makes it difficult to identify records that need source-log correction.

Some records have multiple contexts: a subset may match prepared rows while another subset does not. These cases need a useful operator warning instead of an unexplained `Manual Review` label.

## Decision

- For a supported route, compare each source context to prepared rows in its intended destination using the existing exact route-specific matching rules.
- If no source context matches any prepared destination, treat the record as outside the active Follow Up template scope, regardless of unrelated existing placements. Omit it from Manual Review, propose no RFI or status update, and leave existing cells untouched.
- If a source context matches a prepared row but its target cell is unsafe, keep the record in Manual Review with the specific target-safety reason.
- If some contexts match and others do not, keep the record in Manual Review while synchronizing only independently confirmed eligible destinations. The warning must list each matched source context with its target row and each unmatched context. It must ask the operator to verify the source `Tag No`, `Area`, and `Inspection Location` in the RFI Log; DocFlow must not edit the source log.
- An existing placement that does not match the source context remains a Manual Review safety warning when at least one source context is represented by the intended Follow Up destination. Do not silently move or clear unrelated placements.
- Keep the 21 exact ITP/item exclusions in [ADR 0035](0035-intentional-follow-up-scope-exclusions.md) out of route classification and Manual Review. Existing occurrences are removed only through the normal approved sync. Other unsupported ITP/item pairs remain visible until their scope is explicitly decided.
- Preserve the separate [ADR 0033](0033-exclude-civ-pcs-rfis-from-follow-up.md) rule for every CIV RFI Log record whose `Tag No` starts with `PCS-`.

## Consequences

This supersedes [ADR 0027](0027-follow-up-no-match-scope.md)'s requirement to review every supported zero-match record. A missing destination for a context that is not represented in the Follow Up template is not, by itself, an actionable mapping defect. Partial matches remain visible and provide the exact evidence needed for an operator to decide whether to correct the source RFI Log.

No Follow Up row or section is created. The source RFI Log and Follow Up workbook remain unchanged until the operator reviews and applies an approved plan.

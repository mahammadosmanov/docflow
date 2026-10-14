# ADR 0037: MEC Follow Up Section and Block matching

- Date: 2026-10-14
- Status: Accepted

## Context

MEC Follow Up records were previously required to match `Inspection Location` in addition to the source `Tag No` and `Area`. This prevented valid mappings when the Follow Up workbook represented the same work by Section and Block but used a different or absent inspection-location value. A single source RFI Log record can also list several sections in its `Area` field, each represented by a separate prepared Follow Up row.

## Decision

- For regular MEC Follow Up and `Signed By Worley MEC`, map source `Tag No` to target `Block` and each source `Area` value to target `Section`.
- Ignore source and target `Inspection Location` when deciding MEC destination eligibility, matching prepared rows, reconciling signed destinations, or assessing multi-section coverage.
- Split comma-separated `Area` values into section contexts. Fan out one source RFI to each distinct exact `(Block, Section)` pair that has a corresponding existing prepared row.
- Never create rows or sections. Do not guess when multiple prepared rows represent the same `(Block, Section)` pair; keep the mapping in Manual Review.
- For partial coverage, synchronize only independently confirmed prepared pairs and show unmatched `Tag No`/`Area` contexts so the operator can correct the source RFI Log. DocFlow does not edit that source log.
- Preserve all other Follow Up scope, safety, lifecycle, signature-evidence, and status-write rules.

## Consequences

A location mismatch or blank location no longer blocks a MEC Follow Up match. Missing or ambiguous Section/Block pairs remain reviewable or outside template scope under the existing zero-match/partial-match policy. [RFI_DOMAIN.md](../RFI_DOMAIN.md) is the canonical domain contract.

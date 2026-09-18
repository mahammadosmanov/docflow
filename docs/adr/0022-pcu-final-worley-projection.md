# ADR 0022: Existing PCU FINAL RFI owns its separate Worley status cell

- Status: Accepted
- Date: 2026-09-18

## Context

The current Inspection Status Follow Up workbook adds a standalone `Signed by
Worley` column to the PCU table. The FINAL RFI remains in column P, lifecycle
status remains in Q, and the new Worley state is in R. The workbook also adds a
protected `POT test` sheet and MEC calculated/manual remark fields that are not
RFI Log projection targets.

## Decision

Discover the PCU Worley target only when the exact PCU FINAL RFI/status headers
and standalone `Signed by Worley` header are present. For an existing FINAL RFI,
project `Signed` only from the exact `Signed by Worley` RFI Log REMARKS line;
otherwise project `Not Signed`.

Do not couple a new PCU FINAL lifecycle insertion and its Worley status in one
independently selectable preview. Insert the missing FINAL RFI/status first and
offer the Worley status on the next preview. Never authorize `POT test`, `STAT`,
`Changelog`, calculated remark, manual override, formula, merged, or ambiguous
cells.

## Consequences

Existing PCU FINAL rows synchronize in one guarded Apply. A newly prepared PCU
FINAL row needs a second preview for Worley state, avoiding a partial-selection
dependency. Existing backup, fingerprint, closed OOXML-delta, journal, and Undo
rules remain unchanged. Rollback removes the standalone PCU layout discovery
and route while leaving earlier Signed By behavior intact.

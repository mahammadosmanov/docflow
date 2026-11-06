# ADR 0053: ELE Follow Up sync routes clause 1.6

- Date: 2026-11-06
- Status: Accepted
- Amends: [ADR 0051](0051-ele-follow-up-sync-existing-entries.md) and [ADR 0052](0052-ele-fill-prepared-empty-slots.md) route lists

## Context

The operator confirmed that the prepared ELE `(1.6)` column (`Verify cable drum
insulation resistance and continuity`, columns B:C) now needs syncing. In the
RFI Log it corresponds to ITP `ELE-0005` / item `ELE-5.1.6`. ADR 0051/0052
excluded it while the section was blank.

## Decision

Add one ELE route: ITP `ELE-0005` / item `ELE-5.1.6` to the prepared `1.6`
RFI/status pair. All ADR 0051/0052 rules apply unchanged: exactly one source
`PCS-` Tag No matched to the target `Block`; update an existing RFI or fill a
safe empty pair in an existing prepared row; never create rows or sections;
never change Remarks; formula, merge, ambiguity, and conflict safeguards hold.
Other ELE pairs (including `ELE-5.1.4`, `ELE-5.1.5`) stay out of scope.

The operator also decided (2026-11-06) that every 1.6 RFI must sync, not only
`PCS-` ones. For 1.6 only, the whole normalized source Tag No (for example `A4`,
`A12/1`) is matched exactly to the target `Block`. A composite tag such as
`A4, A12/1` matches no row and stays outside Follow Up. Routes `2.5, 2.6` and
`2.1, 2.4, 2.5.1, 2.5.3` keep the single-`PCS-` rule.

## Consequences

`ELE-0005` / `ELE-5.1.6` records whose tag equals a prepared Block sync through the existing
Preview, Plan, guarded Apply, journal, and Undo flow.

## Rollback

Remove the `ELE-5.1.6` arms in source filtering, clause routing, and default
column. Applied workbook changes remain reversible through Undo.

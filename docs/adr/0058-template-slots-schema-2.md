# ADR 0058: Template field map schema 2 (slots), read first

- Date: 2026-11-25
- Status: Accepted
- Evolves: [ADR 0055](0055-shared-document-templates.md)
- Design: [`docs/superpowers/specs/2026-11-25-template-system-v2-design.md`](../superpowers/specs/2026-11-25-template-system-v2-design.md)

## Context

Templates today mark editable text with `{{placeholders}}` and describe them
in a schema 1 `field_map` (`fields`). The operator wants to choose, per
template, exactly which Word text, table cells and rows, and which Excel
cells, ranges, rows, and sheets are editable, and to change that choice later.
A scan of 91 operator files showed forms that are mostly fixed text with 1 to
15 variable cells, variable data in Word headers, three checkbox kinds,
merge-heavy Excel sheets, and mirror formulas across sheets. Those need
anchors other than placeholders.

## Decision

- Schema 2 stores the editable parts as `slots`: `key`, `label`, `type`,
  `anchor`, `source` (`manual`, `fixed`, or an RFI Log `log` token),
  `options`. File name pattern and presets are unchanged.
- **Two-step rollout.** Release 1.55.0 only reads schema 2. Rust converts it to
  the schema 1 `FieldMap` all renderers use. DocFlow keeps writing schema 1.
  A later release starts writing schema 2 once the readers have shipped, so a
  DocFlow install that has not updated never meets a map it cannot read. (An
  older install rejects a schema 2 manifest entry and lists the template as
  failed.)
- 1.55.0 accepts only placeholder anchors and `text`, `multiline`, and
  `choice` slot types. Any other anchor, type, or source is refused with "The
  template needs a newer DocFlow version"; nothing is half applied.
- The manifest check accepts schema 1 (`fields`) and schema 2 (`slots`), both
  capped at 500. Any other schema is rejected as before.
- Published versions stay immutable. No stored data, journal, cloud, or DTO
  format changes (AGENTS rule 16).

### Cell anchors (1.56.0)

- Anchor `{"kind":"cell", ...}`: Excel uses `sheet` and `address`; Word uses
  zero-based `table`, `row`, `cell` of body tables. Mixed or incomplete
  shapes are invalid. Two slots cannot share one cell.
- Excel writer (`ooxml::patch_template_cells`): plain text, style kept,
  first cell of a merged range allowed (operator forms are merge-heavy; the
  guarded Follow Up writer refuses merged cells and is not used). Refused:
  protected or signed workbook, protected sheet, formula cell, a merged cell
  that is not the first, a missing sheet. The workbook recalculates on open.
- Word writer: replaces the cell's content with one paragraph that keeps the
  cell properties, paragraph properties, and the first run's formatting.
  Refused: a vertical-merge continuation cell, nested tables, and cells with
  drawings, content controls, form fields, symbols, or fields.
- Placeholders and anchored cells fill in one pass (`render_package`).
- Nothing writes an anchor yet. Anchored templates come from editing the
  `field_map` by hand until the mark-editable screen ships.

## Consequences

No operator-visible change in 1.55.0. Later releases add cell, range, and row
anchors, a mark-editable editor, file replacement, optional sheets, free mode,
and computed parts, each in its own release.

## Rollback

Revert the release. Schema 1 maps were never rewritten, so every stored
template still loads.

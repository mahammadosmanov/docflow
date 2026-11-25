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

### Mark cells (1.57.0)

- The editor's Cells section shows the draft file's Word tables or Excel
  sheets (`templates/cells`, read only, bounded: 30 sheets, 300 rows, 80
  columns, 6,000 cells, 60 tables, 120 characters per cell). Selecting a cell
  adds a field with a cell anchor to the draft's map; "Save draft" stores it.
  A cell that cannot be written (formula, merge continuation, protection,
  unsafe Word content) is shown locked with the reason.
- Rust dry-runs every anchored field on the draft file when saving. A cell
  that cannot be written is refused then, before anything is stored.
- A cell holding an RFI code is suggested first; the start value keeps the
  cell's own label (`RFI No: {rfiNo}`). The operator confirms; nothing is
  written or repaired silently (AGENTS rules 10 and 11).
- **Storage.** A map with no anchors is stored exactly as schema 1, as before.
  A map with at least one anchor is stored as schema 2 (`FieldMap::to_value`),
  so an older install lists the template as failed instead of silently
  skipping the cell. The shared store has no check on `field_map`'s schema,
  so no SQL change is needed.
- Not in this release: suggest from filled copies, repeating rows, optional
  sheets, free mode, computed parts.

### Suggest from other copies (1.58.0)

- `templates/suggestFromCopies` opens Rust's own file dialog (up to 10 Word or
  Excel files of the template's kind, each within the template size limit).
  Rust compares them with the draft file cell by cell: a Word cell by table,
  row, and cell position; an Excel cell by sheet name and address. Copies are
  only read here. They are never uploaded, stored, or sent to the webview
  beyond up to three short sample values per suggested cell.
- A cell is suggested when its text differs between the files and it can be
  written in the draft file (no formula, merge tail, protected sheet, or unsafe
  Word content). Cells empty in the draft but filled in a copy count. The label
  is the nearest filled cell to the left that reads the same in every file.
- When every filled value holds an exact RFI code, the suggestion is marked
  RFI and its start value writes the code as `{rfiNo}`. The operator still
  confirms each suggestion or uses "Make all editable"; nothing is applied
  silently (AGENTS rules 10 and 11).

### Edit later (1.59.0)

- **Replace the file.** After the new file is stored, Rust checks every marked
  cell against it (`templates/replaceFile` now returns `checks`). A value cell
  changes from copy to copy, so a cell is matched by its label, the nearest
  filled cell to its left in the same row. Same label beside the same place:
  found. The label elsewhere, exactly once, and the target writable: moved.
  Anything else, including a label that appears twice: missing. The editor
  lists moved and missing cells; the operator moves, keeps, or removes each
  one, and Publish stays disabled until none is left. Rust's own backstop:
  publishing refuses a draft whose marked cell cannot be written in its file.
  A kept cell whose label changed but whose place is still writable is the
  operator's decision; Rust cannot tell it from a correct one.
- **Version history.** `templates/versions` lists every version, newest first.
  `templates/restoreVersion` copies an earlier published or retired version's
  file and field map into a new draft after the latest version, or into the
  open draft (the editor warns first). Earlier versions are never changed, so
  documents already generated stay traceable.
- **Marked cells list.** The Cells section shows the marked cells beside the
  file view (below it on narrow windows) with a remove button each.

### Optional sheets (1.60.0)

- Schema 2 adds `optionalSheets`: names of Excel sheets a document may leave
  out. Such a template is stored as schema 2, so an older install lists it as
  failed instead of ignoring the list. The editor shows a checkbox per sheet
  (workbooks with more than one sheet); saving checks each optional sheet on
  its own and all of them together against the draft file, and publishing
  checks again. A Word template with optional sheets is refused.
- Generation adds one checkbox column per optional sheet (record table kind
  `bool`, all on). An unticked sheet is removed whole by
  `templates::sheets::remove_sheets`: the sheet, its relationships, its
  drawing and comments, pictures no kept part uses, names scoped to it, and
  the calculation chain (Excel rebuilds it). Names after it shift their sheet
  index; the open tab resets to the first sheet.
- Refused: a sheet that a formula on a kept sheet uses, a workbook name that
  is not scoped to the sheet and uses it, a protected or signed workbook, an
  unknown sheet, and removing every visible sheet. References from conditional
  formatting, data validation, charts, or pivot tables to a removed sheet are
  not checked; the damage workbooks in the operator's sample have none, and
  Windows Excel acceptance must confirm a generated file opens without repair.
- The picked choice is part of the Preview rows and the journal like every
  other edited value; Undo removes the generated file as usual.

### Repeating Word rows (1.61.0)

- A cell can be marked as the start of a repeating row: anchor kind `rows`
  with `table`, `row`, `cell`, and `repeatColumns` (1 to 20). Everything else in
  the row is cloned unchanged. In Preview the field is one multiline value:
  each line becomes a row, a Tab separates the columns (rows copied from Excel
  paste as they are). An empty value is one empty row; trailing empty lines are
  dropped; at most 200 rows; a line with more values than columns is refused.
- Rendering writes single cells first, then expands repeating rows from the
  lowest row up, so a row above is not moved by rows added below it. Each cell
  keeps its cell, paragraph, and first-run formatting; a cell left empty keeps
  its paragraph properties but holds no run.
- Refused: a row with any vertically merged cell, a row with fewer cells than
  `from + count`, a mapped cell with a drawing, control, form field, symbol, or
  field, and any other marked cell in the same table row. Saving tries the row
  with two lines on the draft file; publishing checks again.
- Stored as schema 2 (`rows` is unknown to 1.55.0 to 1.60.0, which refuse the
  template with the usual message).

### Repeating Excel rows (1.62.0)

- The same `rows` anchor works in Excel with `sheet`, `address` (the first mapped
  column of the template row), and `repeatColumns`. The value format is the same
  (a line per row, Tab between columns).
- `ooxml::repeat_template_rows` writes the first line into the template row, then
  inserts the other rows with the engine built for Follow Up (rows below,
  formulas, ranges, conditional formatting, and defined names move as Excel's
  Insert Row moves them; unmapped cells are cloned and their formulas
  translated; the calculation chain is dropped). That engine does not clone
  merged ranges, so the template row's single-row merges are cloned onto each new
  row afterwards. Rows lower on a sheet are expanded first.
- Refused: a missing template row, a protected sheet, a row crossed by a
  vertical merge, a mapped cell that holds a formula or sits inside a merge but is
  not its first cell, and any other marked cell in the same row.
- Not handled: data validation, hyperlinks, and drawings anchored to the
  template row are not cloned; images anchored below the row keep their position.

### Categories (1.63.0)

- `doc_templates.category` is a text column (default empty) holding a folder path:
  up to three levels, each 1 to 40 characters, no `\ : * ? " < > |` or control
  characters, no `.` or `..` level. Rust (`normalize_category`) trims the levels,
  joins them with `/`, and refuses the rest; a SQL check repeats the rule. The
  manifest view returns it as the last column, so older clients ignore it.
- `discipline` stays: it is still required and drives RFI number shorthand. The
  library shows and groups by `category`, falling back to `discipline`. The
  output file name pattern already exists (`fileName`) and is unchanged.
- A manifest entry with a category that does not normalize to itself is skipped
  like any other invalid entry.

## Consequences

No operator-visible change in 1.55.0. Later releases add cell, range, and row
anchors, a mark-editable editor, file replacement, optional sheets, free mode,
and computed parts, each in its own release.

## Rollback

Revert the release. Schema 1 maps were never rewritten, so every stored
template still loads.

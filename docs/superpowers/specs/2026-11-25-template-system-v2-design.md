# Template system v2 design

- Date: 2026-11-25
- Status: Draft for operator review (no code changed)
- Replaces over time: ADR 0055 field-map schema 1 and the hard-coded
  `concrete_documents/` cell maps. A new ADR (0058) records acceptance.

## Operator requirements (2026-11-25)

1. Operator names every template and every output file. No fixed naming scheme.
2. Systematic structure for the library, so many different files stay ordered.
3. Word and Excel now; other file types may come later.
4. Per template, the operator chooses exactly which parts are editable:
   Word text, table cells and rows; Excel cells, ranges, and rows.
5. Images and signatures are out of scope (stay untouched in the file).
6. Generate from RFI Log rows and also without an RFI (free mode).

## What stays from ADR 0055

Supabase store, immutable versions, SHA-256 offline cache, Rust-only file
access, editor roles, preview → approval → apply → journal → undo, no
overwrite, Windows-safe file names. The shared editor stays a `ParamSchema`
form plus the generic record table (AGENTS rule 9).

## Sample analysis (91 operator files, 2026-11-25)

Method: read-only structural scan of the OOXML packages in the operator's
Downloads folder (scripts in `/tmp`, nothing copied into the repo). These are
production documents: never commit them or their text; fixtures stay
synthetic. Numbers below are counts from that scan.

Mix: 49 `.xlsx`, 41 `.docx`, 1 legacy `.doc`. Families seen: RFI covers (8),
CON-CIV (002, 004, 009, 010, 011, 012, 013, 014), CON-MEC (002, 003), CON-ELE
(002, 009, 010, 017), CON-INS-002, CON-MT-001, CON-COA-001, Materials
handing over (2), Material receiving report (2), BT RFI sheets (3), and 32
damage/MDR workbooks.

What the samples show, and what the design does about it:

1. **They are filled copies, not blank forms.** Inside one family only 1 to 15
   cells differ between copies (for example CON-CIV-014: 2 of 181; CON-MT-001:
   10 of 92). Most of every form is fixed text. Design: marking editable parts
   is a small job, and the editor can **suggest parts from copies**: upload
   two or more filled copies of one form and Rust proposes the cells that
   differ. With one copy, the current text becomes each part's starting value.
2. **Every form carries its RFI code** (`-RFI-` text in 16 of 17 families).
   Design: suggest an RFI part first, as a suggestion the operator confirms.
   Never write or repair an RFI code silently (AGENTS rules 10, 11).
3. **Word forms are one large table** with heavy cell merging (up to 174
   `gridSpan` and 20 `vMerge` per form), no nested tables, no text boxes, no
   tracked changes. Design: anchor by table, row, and cell position; merged
   cells are one slot.
4. **Variable data also sits in Word headers and footers** (CON-MEC-002 has 4
   header variants across 4 copies). Design: `text` and `cell` anchors cover
   headers and footers, as placeholders already do.
5. **Checkboxes come in three kinds**: legacy form-field checkboxes (16 in 8
   files), content-control checkboxes (1 file), and symbol or ☐ characters
   (10 files). Design: new slot type `checkbox` (checked or not), written the
   way the file already stores it. Unknown shapes are rejected at save.
6. **Excel forms are merge-heavy** (up to 342 merged ranges), single-sheet
   forms mostly, with a logo image. 24 files have defined names (print
   areas), 3 conditional formatting, 1 sheet protection. No macros, no data
   validation, no hidden sheets, no external links. Design: a slot anchors at
   the top-left cell of a merge; defined names and formatting are preserved;
   a protected sheet is flagged at save so the operator knows Excel may block
   edits.
7. **Formulas appear in 15 files**, all damage workbooks: `IF(sheet!cell="","",
   ...)` mirror cells that copy an input from a photo sheet to a summary
   sheet. Design: input cells are slots, formula cells are never slots (as
   already planned); the written workbook is marked to recalculate on open,
   so mirrored values never stay stale. Excel and Windows acceptance of this
   is not claimable from macOS tests.
8. **Damage/MDR workbooks are not one form.** 32 files, 2 to 12 sheets, up to
   20+ photos each, and no shared cell positions between copies. Design: no
   family assumption anywhere. Every file is its own template. Photos are
   kept and untouched. Some copies differ only by photo sheet count; see
   Optional sheets.
9. **Row counts vary** (CON-CIV-010: 23 cells exist in only some copies).
   This confirms repeating `rows` parts.
10. **Dates are plain text in the files** (46 date strings in Excel text,
    none as date-formatted numbers in Word). Design: `date` slots write text
    in the format the operator picks per slot (default `dd.MM.yyyy`).
11. **One legacy `.doc` file** (CON-CIV-014 old copy). Rust cannot read the
    binary `.doc` format and will not add a converter or interpreter.
    Design: upload stops with "Save this file as .docx in Word, then upload
    again."
12. **A small share of the content is unusual**: no existing `{{ }}`
    placeholders in any sample, so every sample needs marking. Supports the
    upload, mark, save flow.

## Model

### Template identity

- `code`: free text chosen by the operator. Unique, case-insensitive, 1-80
  chars, no path characters. No required prefix.
- `title`: free text.
- `category`: operator-defined folder path such as `Concrete/Civil`. This is the
  systematic structure. It replaces the fixed ELE/MEC/CIV/PRC discipline.
  Existing discipline values migrate to categories of the same name.
- `fileKind`: `docx`, `xlsx` now. A closed Rust `TemplateFormat` trait
  (`scan`, `read_slot`, `write_slot`, `extension`) lets a new kind be added
  by code, never by script (AGENTS: no interpreter).

### Output file name

Per template pattern, set by the operator: free text with `{{slot}}` parts,
for example `{{rfiNo}}_Concrete Pour`. Rust sanitizes it (`safe_file_stem`),
and a name that already exists on disk or twice in one batch is a Preview
conflict. It is never overwritten and never silently renamed.

### Slots (the editable parts)

Everything not marked as a slot is locked and copied byte-for-byte.

| Anchor | Word | Excel |
| --- | --- | --- |
| `text` | selected text span (as in 1.48.0) | cell text part |
| `cell` | table cell | one cell: sheet + address |
| `range` | table block | rectangle: sheet + `A1:C4` |
| `rows` | repeating table row | repeating row band |

Excel anchors are stored in the field map and applied at render time, so the
original form file is not edited. The editor adds click-to-map on a sheet
preview (the item ADR 0055 left unbuilt).

Word anchors work in the body, headers, and footers. Merged cells are one
slot. A slot never sits on a formula cell or an image.

Slot value types: `text`, `multiline`, `number`, `date` (output format set per
slot), `choice` (list), `checkbox` (checked or not), and `computed`.

`computed` is a closed Rust enum of operations (`sum`, `difference`, `offset`,
`concat`, `copy`, `round`) over other slots. No expressions, no scripting.
Example: ambient temperature = plant temperature + offset, now done by hand
in `concrete_documents/`.

Slot source (starting value): manual, fixed default, RFI Log column
(`{token}`), or same as another slot. `rows` slots also define columns and
min/max row count; the operator adds and removes rows in Preview.

Presets stay: named sets of slot values.

### Optional sheets (Excel, decided 2026-11-25)

For workbooks with several sheets (damage reports: 2 to 12), the operator
marks each sheet in the editor as `fixed` (default) or `optional`. At
generation, Preview shows a checklist of the optional sheets, all on; the
operator unticks the ones to leave out of that document.

Rust removes an unticked sheet whole: its worksheet part, relationships,
content-types entry, drawing and photos only it uses, and defined names
scoped to it. Safety rules:

- A sheet that any kept formula refers to cannot be removed. The checklist
  shows it locked with the reason ("Used by sheet X"). This protects the
  mirror formulas found in the damage workbooks.
- At least one sheet stays.
- The editor checks at save time that each optional sheet can be removed on
  its own, and shows the result next to the sheet.
- The choice is part of the Preview plan and the journal. Undo removes the
  generated file as usual; nothing else changes.

Word has no sheets, so this applies to Excel only. A `rows` part covers
repeated Word and Excel rows.

### Field map schema 2

```json
{
  "schema": 2,
  "fileName": "{{rfiNo}}_Concrete Pour",
  "slots": [
    {"key": "rfiNo", "label": "RFI No.", "type": "text",
     "anchor": {"kind": "cell", "sheet": "Form", "address": "C3"},
     "source": {"kind": "log", "token": "rfiNo"}}
  ],
  "presets": []
}
```

Schema 1 maps load through an in-memory converter (placeholders become `text`
slots). Published versions stay immutable; only new versions write schema 2.
Rust validates every map on save, publish, and use, with count and size caps.

## Operator flow and UX (added 2026-11-25)

Principle: the simple path is three actions; everything else is reachable
later and never blocks the simple path. The UI stays monochrome (AGENTS rule
12) and reuses the existing Templates screen, `Modal`, `ActionBar`, and
`recordTable` parts. Meaning comes from controls, text, icons, and border
weight, not color.

### Create: upload, mark, save

```
 1 Upload file      -> Rust opens its own file dialog (no path in React)
 2 Mark editable    -> one screen: document preview + side list
 3 Name and save    -> name, category, output file name
```

Step 2 is one split view. Left: the file as a read-only preview (Word text
and tables, Excel sheet grid). Right: the list of editable parts, empty at
first with the line "Select text or cells in the file to make them
editable."

- Select text, a cell, a range, or a table row in the preview. A small
  toolbar offers one action: "Make editable". Marked parts get a heavy
  border in the preview and a row in the list.
- Each list row shows label, type, and where it sits. Defaults are quiet:
  type `text`, starting value empty. "More options" on a row opens type,
  choices, source (RFI Log column, same as another part), and computed.
- Click a row to jump to its place in the file. Remove returns the part to
  locked.
- Nothing else is on screen. Presets, naming pattern, and category sit in the
  Save step, not in the marking step.

### Edit later: everything stays changeable

Every template can be reopened and changed in full. Each change is a new
draft version; published versions stay immutable and can be restored as a new
draft ("Go back to version 3").

| Change | Behavior |
| --- | --- |
| Add, remove, move a marked part | Same split view as creation |
| Rename template, category, output name | Metadata only; history keeps old values |
| Replace the file | See below |
| Change a part's type, choices, source | Row options |
| Archive, restore from archive | Library action |

**Replace the file.** The operator picks a new file. Rust re-checks every
marked part against it and shows one result list:

- Found: text or cell still matches. Kept.
- Moved: same text found at another place. Offered with one click.
- Missing: part has no match. The operator points at the new place in the
  preview, or removes it.

Draft cannot be published while a part is Missing. Old versions keep their
own file, so documents already generated stay traceable.

### Using a template

Library list: search box, category folders, one row per template with a
single "Create documents" action. Create screen: pick mode (RFI Log or
free), fill the parameters, Preview, Apply. No template setup appears here.

### Copy and states

Plain verbs, sentence case, same word for the same action everywhere
("Make editable", "Save draft", "Publish"). Empty and error states say what
happened and the next step, for example "No editable parts yet. Select text
or cells in the file." All text exists in `tr`, `en`, `ru`, `az`. Keyboard
focus is visible, and every action works without a mouse.

## Generation

One operation `templates.generate_document`, parameters from `ParamSchema`:

- `mode`: `rfi` (RFI Log + numbers, as today) or `free` (operator enters how
  many documents; rows start empty or from defaults).
- Preview: editable record table per document; `rows` slots expand in a detail
  panel. Preset column and choice suggestions as today.
- Apply: new files only, journaled, template version recorded, Undo as today.
  A template update between Preview and Apply stops the run (as today).

## Migration of concrete forms

CON-CIV-010/011/013/014 move onto templates using `computed` slots. Legacy
code stays until golden-output parity tests on synthetic fixtures pass for
each form, then each form is switched in its own release.

## Releases (each its own SemVer commit, `feature` = MINOR)

1. 1.55.0 schema 2 read support (done). `TemplateFormat` trait moved to the
   release that first needs it.
2. 1.56.0 cell anchors: Word table cells and Excel cells (merged first cell
   included) are read from schema 2 and written at generation. Still read
   only, no editor; DocFlow still writes schema 1.
3. 1.57.0 mark-cells screen (Word tables, Excel grid), RFI part suggestion,
   and the first release that writes schema 2, only when a template uses a
   cell anchor (done). Ship it after 1.55.0 and 1.56.0 have reached every
   install, because older installs list a schema 2 template as failed.
4. 1.58.0 "suggest from other copies": the operator picks other filled
   copies, Rust proposes the cells that differ (done). The side-by-side file
   preview moves to 1.59.0.
5. 1.59.0 edit later: version history and restore as draft, replace file
   with Found/Moved/Missing check, marked-cells list beside the file view
   (done).
6. 1.60.0 Excel optional sheets with the reference check (done).
7. 1.61.0 Word repeating rows (done).
8. 1.62.0 Excel repeating rows with cloned merges (done).
9. 1.63.0 categories, library search (done; the output name pattern already existed).
10. 1.64.0 `free` mode, `number`/`date`/`choice`/`checkbox` types.
11. 1.65.0 `computed` parts; then one release per migrated concrete form.

Every release: four locale keys, traceability/decisions/issues updates, all
`docs/RELEASE_SAFETY.md` gates. Supabase needs one SQL migration (category
column); an admin runs it, as with ADR 0055.

## Risks and limits

- Excel merged cells, shared strings, and formulas need fixtures for every
  anchor kind; a slot on a formula cell is rejected.
- Word tables with merged cells limit `rows` anchors; unsupported shapes are
  rejected at save with a clear message.
- Windows, Excel, and Word acceptance cannot be claimed from macOS tests;
  report it separately in `ACCEPTANCE.md`.
- Images and signatures inside the file are preserved but not editable.

## Open questions

1. Is `Category/Subcategory` the right shape for ordering, or do you also
   want tags?
2. Which computed operations beyond the six listed do your forms need?
   (Can wait until release 1.61.0.)
3. Sample files: received 91 (analysis above). Fixtures must be synthetic,
   never production documents. I will build them from the scan; no new files
   needed unless you want a specific layout covered.
4. Decided: unused photo sheets can be left out per document (see
   Optional sheets). Remaining question: should removal also work when only
   the photos inside a sheet are unused?
5. Copies may be filled by hand for years. Should "suggest from filled
   copies" also compare against copies already generated by DocFlow?

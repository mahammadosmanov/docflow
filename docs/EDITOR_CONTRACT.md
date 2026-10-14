# Schema and record editor contract

Rust `OperationDef` and `ParamSchema` own operation forms. React renders generic
parameter kinds and must not contain a separate operation-specific form. Add a
new parameter kind only when the shared contract genuinely lacks reusable input
semantics.

## Parameter contract

- Rust validates required values, types, choices, bounds, and visibility.
- File/folder values are opaque caller- and parameter-scoped selection tokens;
  React must not manufacture trusted paths.
- Remembered values are filtered through schema. Secrets and operation identity
  fields are transient.
- Presets contain reusable operation settings only; selected files, order,
  worksheets, RFI identities, and row-specific ranges remain task-local.

## Editable preview contract

An editor is a view over rows already proposed by the immutable Plan. Revision
may enable/disable rows, reorder supported rows, apply a field to selected/all
rows, or replace values allowed by column metadata. It cannot introduce a new
row, path, RFI, worksheet, page, archive candidate, or effect.

Compact display transforms (RFI suffix, short ITP/item, day/month) never alter
the stored full value. Detail fields remain reachable and editable. Apply uses
the latest successfully revised Plan, not React table state.

Readonly columns may declare `display: "percent"` for a backend fraction in the
closed interval 0–1. React clamps and rounds the presentation to 0–100%, while
the immutable Plan retains the original value. This display transform grants no
authority and does not change confidence thresholds.

## Shared preview surfaces

- Impact counts derive from approved steps only.
- Exact effects show complete approved source/destination paths without reopening files.
- Warning filtering is presentation-only and never removes Plan warnings.
- Workbook comparison shows approved sheet/cell/current/replacement values only.
- Signed-page thumbnails are bounded inert PNG metadata tied to row/candidate
  tokens. Signed-page warning-review metadata names only sources/destinations and
  reasons already authorized by Plan steps; it cannot create a copy by itself.
- The shared stage shell allocates content below the actual header and gives each
  stage one vertical scroll owner; its primary action bar remains reachable.
  Content-region heights and table columns support pointer and keyboard
  adjustment with Fit/Reset. These controls change presentation only and cannot
  revise Plan authority or application preferences.
- Fit does not overlay stage summaries on data. Escape closes an open adjustment
  dialog without discarding its pending presentation changes; with no dialog
  open, the existing Escape-to-Configure behavior remains.
- Wide record tables keep readable minimum column widths and own horizontal
  scrolling; they must not be compressed to fit. Dialogs remain vertically
  scrollable within the viewport.
- Signed-page plans put the approved editor before comparisons and warnings and
  provide an in-flow navigator so section controls never cover review rows.
  Comparison controls change only the visible row/image; warning filters and
  navigation never revise Plan authority.

Any editor interaction change requires a frontend test and all four locale keys.

# Schema and record editor contract

Rust `OperationDef` and `ParamSchema` own operation forms. React renders generic
parameter kinds and must not contain a separate operation-specific form. Add a
new parameter kind only when the shared contract genuinely lacks reusable input
semantics.

## Parameter contract

- Rust validates required values, types, choices, bounds, and visibility.
- File/folder values are opaque caller- and parameter-scoped selection tokens;
  React must not manufacture trusted paths.
- Remembered values are filtered through schema. Secrets and work-item identity
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
- Signed-page thumbnails are bounded inert PNG metadata tied to row/candidate tokens.
- The Plan content region owns vertical scrolling; its final action bar remains
  outside that region. Wide record tables own horizontal scrolling, and dialogs
  remain vertically scrollable within the viewport.

Any editor interaction change requires a frontend test and all four locale keys.

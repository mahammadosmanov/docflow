# ADR 0047: Follow Up generated row remarks

- Status: Accepted
- Date: 2026-10-24
- Supersedes: [ADR 0028](0028-remove-follow-up-status-remarks.md)

## Context

ADR 0028 stopped Follow Up Sync from touching worksheet `Remarks` at the owner's
request because the previous generated text was unwanted. On 2026-10-24 the owner
asked for a different, precisely specified remark: a row that contains a cell with
several RFIs of which at least one is not Completed shows which RFIs are Completed
and which are not, and a generated remark disappears once everything is Completed.

## Decision

On the `CIV`, `PCU`, and `Road` sheets, Follow Up Preview adds a separate review
row for each physical row whose `Remarks` cell should change:

- The text lists every real (non-placeholder) lifecycle RFI of the row from left
  to right with its Log state. Completed RFIs come first, then In Progress, then
  Pending; consecutive same-discipline numbers are grouped:
  `CIV-0021,0022 Completed CIV-0043 In Progress`.
- The Completed segment is written bold, underlined, in theme Accent 5 (Plum); the
  other segments are plain. All runs use the cell's own font.
- A remark is written only if some cell in the row holds two or more RFIs and at
  least one of them is not Completed. Otherwise a generated remark is cleared.
- DocFlow replaces or clears only text matching this exact generated format.
  Operator notes, the old `RFI Status Sync:` text, formula cells, and merged cells
  are never changed; they appear as unselectable Manual Review rows.
- A row with an RFI of unknown Log status, or with an unsafe RFI/status cell, gets
  no remark change.

Apply uses the existing guarded pipeline with a rich-text cell replacement whose
output is verified against the exact approved runs and font. Every remark change
has its own approval row, backup, journal entry, and Undo.

## Consequences

Remark rows are independent of the status rows of the same sync. Deselecting a
status row but keeping its remark can leave a remark that describes the Log rather
than the Follow Up cell; the next sync recomputes both. Rich text is a new guarded
cell representation limited to this operation.

## Rollback

Stop producing remark rows. Written remarks remain ordinary cell content and Undo
restores the exact original bytes.

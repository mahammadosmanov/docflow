# Guarded RFI Log to Outlook reply handoff

Date: 2026-09-19
Status: Accepted

## Context

After assigning RFI numbers, the operator copies the approved Excel rows into a
Reply All message. The guarded RFI Log and Outlook draft operations already own
those effects, but reopening the second operation and recreating the table adds
work and can select the wrong rows.

## Decision

After a fully successful `rfi.log_add` Apply, DocFlow may prepare a separate
`rfi.outlook_reply_draft` Plan from the exact owner/session-bound RFI Plan. Rust
rebuilds the approved table for every affected discipline, preserves its escaped
HTML formatting, and binds the new Plan to the one currently selected classic
Outlook message.

The handoff does not recalculate RFI numbers, mutate the workbook, add
recipients, invent standard prose, or send mail. The operator reviews and
explicitly applies the separate Outlook Plan. Failed or partial workbook results
do not offer the handoff.

## Consequences

- The source workbook journal and Outlook draft journal remain separate and can
  be recovered independently.
- Selection drift and unavailable or oversized plans fail closed.
- Exact official greeting and closing text remain operator-editable until a
  representative approved template is supplied.

## Rollback

Remove the result action and the bounded handoff command. Both original
operations remain independently usable.

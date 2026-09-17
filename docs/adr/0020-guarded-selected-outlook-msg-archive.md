# Guarded selected Outlook MSG archive

Date: 2026-09-17  
Status: Accepted

## Context

Publication mail must be retained with its native Outlook content and later
copied into every related RFI folder. Automatic mail parsing cannot decide
whether an irregular message or attachment is professionally correct, and
automatic send or mailbox mutation is outside DocFlow's authority.

## Options considered

1. Do nothing and keep manual Save As for every message.
2. Reconstruct EML from fields read through Outlook COM.
3. Ask classic Outlook to save the exact selected MailItem as Unicode MSG, then
   guard the resulting local file with DocFlow's normal Plan and journal rules.

Reconstructed EML risks losing Outlook-specific formatting, attachments, and
metadata. Manual Save As repeats work and gives no Plan, collision, journal, or
Undo evidence.

## Decision

DocFlow provides `rfi.outlook_archive_selected`. Preview requires exactly one
classic Outlook MailItem and stores its identity and bounded fingerprint only in
private native authority. The executable Plan contains the visible subject and
safe destination, but no body, recipients, attachments, or Outlook IDs.

Apply consumes the authority once, requires the same sole selection and
fingerprint, calls Outlook `SaveAs` with `olMSGUnicode` into a private temporary
directory, validates the compound-file header and size, then gives the bytes to
the capability-scoped no-clobber writer. The mailbox is never sent, moved,
deleted, marked, or changed. The local copy is journaled and Undo removes it only
when its hash is unchanged.

## Consequences

- The operation intentionally supports classic Outlook on Windows; new Outlook
  has no compatible COM surface.
- An existing deterministic destination is a reviewable collision, never an
  overwrite or hidden rename.
- Selection drift requires a fresh Preview.
- The existing Group Files by RFI operation remains responsible for extracting
  exact RFI numbers and copying one MSG into several RFI folders.

## Rollback

Disable or remove the registered operation. Existing MSG files remain ordinary
user documents; completed copies can be removed through their guarded journal
Undo when unchanged.

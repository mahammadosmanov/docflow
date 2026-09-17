# Exact OTHERS-only Worley evidence

Date: 2026-09-17  
Status: Accepted

## Context

The prior RFI System detector measured a wide lower-right page region and chose
the first available PDF when no `RFI No.` file was present. Printed form lines,
dates, notes, or signatures in other roles could therefore create a false
Worley result and authorize an incorrect workbook update.

## Options considered

1. Keep the broad pixel detector and raise its threshold.
2. Use OCR to infer the signer or role from the whole page.
3. Require the exact canonical RFI PDF and measure only the approved OTHERS
   name/signature cell, removing printed table lines and failing closed.

A higher broad threshold still treats unrelated marks as authority. Whole-page
OCR adds ambiguity and can silently confuse names, roles, and scan artifacts.

## Decision

Only a case-insensitive exact `RFI No. <canonical RFI>.pdf` in the matching RFI
folder is eligible. The first page is rendered and analysis is limited to the
inner name/signature area of the bottom-right OTHERS cell. Long printed rows and
columns are excluded before a conservative residual-ink threshold is applied.
All marks outside this cell are ignored. Missing, unreadable, empty, and weak
evidence remains unsigned and non-actionable.

The scan-index profile changes whenever this authority or algorithm changes, so
old positive and negative classifications cannot cross the boundary.

## Consequences

- False positives from other signatures and arbitrary PDFs are blocked.
- A faint or materially different form can become a safe false negative and
  require operator review rather than mutate a workbook incorrectly.
- Private production examples remain external test inputs and are never added
  to Git.

## Rollback

Revert the detector and filename selection together and assign another new
algorithm profile. Never restore old cache compatibility across detector rules.

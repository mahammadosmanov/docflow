# ADR 0007: Safe parallel PDF orientation

- Date: 2026-09-14
- Status: Accepted
- Amended: 2026-09-18 — multi-resolution OSD and conservative direction scoring

## Context

RFI PDFs may contain a mixture of portrait and landscape page sizes while the
printed content itself is sideways or upside down. Page dimensions cannot prove
reading direction. Editing the production RFI tree directly would make a false
classification or interrupted write difficult to recover, and thousands of
independent pages make serial OCR unnecessarily slow.

## Options considered

1. Infer orientation from page width and height. Rejected because legitimate
   landscape documents are not necessarily sideways.
2. Rewrite source PDFs in place. Rejected because it increases corruption,
   collision, sync, and recovery risk.
3. Use content-based orientation detection in parallel during preview, then
   publish validated incremental copies serially. Accepted.

## Decision

Rust discovers PDFs only below folders named CIV, MEC, ELE, or PRC and uses a
bounded worker pool of at most four threads. Each page is first rendered at 250
DPI and classified by bundled Tesseract OSD. Unresolved pages receive a 350-DPI
OSD pass; agreeing evidence may combine. If OSD remains uncertain, the 250-DPI
page is compared in all four right-angle directions with English OCR. That
fallback accepts only at least 20 recognized words, average confidence at least
55, a winner score at least 1.45 times the runner-up, and an average-confidence
margin of at least 8. A complete PDF becomes actionable only if every page has
a valid right-angle result with normalized confidence at least 10.0.

Preview records source hashes and exact page rotations. Apply does not repeat
OCR. It copies the approved source to private storage, appends only revised page
dictionaries, verifies document and page fidelity, and publishes under a
separate review root without overwrite. Apply and journaling remain serial.
Encrypted and digitally signed PDFs are never changed automatically.

## Consequences and rollback

Multi-core work is limited to independent read-only analysis, so deterministic
mutation order and recovery remain unchanged. The higher-resolution and
four-direction fallbacks cost more CPU only after the fast pass is uncertain.
Sparse-text, drawing-only, close 0/180-degree, or otherwise uncertain pages
require manual review rather than risking a wrong rotation. Rollback is ordinary
guarded Undo of generated review copies; removing the registered operation and
its step executor restores the prior product without migrating user data.

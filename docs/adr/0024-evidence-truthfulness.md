# ADR 0024: Evidence truthfulness and fail-closed automation

## Status

Accepted.

## Decision

DocFlow must describe only what its evidence proves. A heuristic score is not
an accuracy probability. A platform handoff is not physical completion. A
plausible OCR token is not an approved RFI identity.

- Deterministic byte, hash, workbook-cell, and journal checks may be described
  as verified only within their explicit boundary.
- OCR and extraction scores are labelled as heuristic evidence. They never
  mean 100% correctness or automatic approval.
- Process Scans requires one conflict-free visible-page OCR code across at
  least three render resolutions and at least two exact reads. Searchable PDF
  text may corroborate or veto that result, but may never authorize it alone.
- A page or text layer containing multiple exact RFI codes is ambiguous.
- Signed-page replacement accepts only the same exact conflict-free identity;
  repaired or fuzzy codes remain manual review.
- Printer states distinguish rejected, submitted, completed by the spooler,
  and submitted without a retained receipt. DocFlow never claims that paper
  physically exited a printer without operator evidence.
- Unknown, conflicting, incomplete, unavailable, or externally unverifiable
  evidence stops automatic mutation and remains visible for review.

## Consequences

Safe files with weak evidence can require review, and visible-page OCR can take
longer than trusting embedded text. This is intentional: a recoverable miss is
preferred to a silently wrong business record.

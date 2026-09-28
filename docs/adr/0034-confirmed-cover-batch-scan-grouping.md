# ADR-0034: Confirmed-cover grouping for batch Process Scans

- Status: Accepted
- Date: 2026-09-28
- Supersedes: [ADR-0025](0025-ordered-batch-rfi-scans.md), uncertain-page boundary rule

## Context

Process Scans splits ordered multi-RFI PDFs. Its earlier rule sent any page with
ambiguous or unreadable OCR to `_review` and ended the current RFI group. In a
batch scan, those pages are often continuation pages of the RFI whose cover was
already identified. This created separate Review output for pages that belong
to an identified RFI.

## Decision

- A page starts an RFI only when OCR returns one strict, canonical,
  high-confidence RFI identity with confidence `>= 0.8`. This is the confirmed
  cover marker; native OCR requires its existing multi-resolution consensus
  before returning a strict identity.
- After a confirmed cover, every page belongs to that RFI until the next
  confirmed cover. Ambiguous, low-confidence, non-matching, and unreadable
  continuation pages do not break the group and do not create Review output.
- Pages before the first confirmed cover remain in `_review` because no RFI
  owns them.
- A repeated RFI code after another confirmed cover remains Review under the
  existing duplicate rule. Its following pages stay with that Review group
  until another confirmed cover.
- The detector inspects pages to find later covers. It does not require every
  continuation page to yield an RFI identity or become a separate review item.
- Existing immutable Plan, source-hash validation, page-only extraction,
  orientation policy, no-clobber Apply, journal, and Undo rules remain intact.

For example, page 1 with one confirmed code and pages 2–23 with no confirmed
cover form one output. A confirmed cover on page 24 starts the next output.

## Rationale and trade-offs

Only a strict, canonical, high-confidence identity can split a combined scan.
Uncertain continuation OCR cannot strand pages from an already identified RFI.
Leading pages remain reviewable, and duplicate confirmed codes remain excluded
from automatic placement.

## Alternatives considered

- Review each page whose OCR is uncertain: rejected because continuation OCR
  is not needed to retain the active RFI boundary.
- Let any candidate code start a group: rejected because repaired, fuzzy, or
  ambiguous OCR cannot safely establish a new RFI identity.

## Consequences and rollback

Update DF-032, Process Scans operator guidance, and traceability tests in the
same change. No protocol or persisted-data migration is required. Rollback is a
source release rollback; already created outputs remain governed by their
existing journals.

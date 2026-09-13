# Roadmap

This file describes product outcomes, not day-to-day task status. GitHub Issues
and pull requests are the free execution tracker; `releases.json` and tags are
the historical tracker.

## Now — 0.9 unified RFI work foundation

- Keep the read-only device-readiness surface accurate and actionable.
- Extend the implemented append-only RFI work items and Today, Resume, and Needs
  Review queues with one-click prefilled transitions and journal-linked
  automatic evidence from successful operations.
- Certify the exact Windows candidate against Excel open/recalculate, Outlook,
  OneDrive-backed folders, Microsoft Print to PDF, and representative printers.
- Expand structural fixtures for every current production workbook family
  without committing private documents.
- Calibrate OCR against consented representative scans and retain at least 95%
  exact canonical-RFI recognition.
- Keep version, changelog, installer, checksums, and release metadata synchronized.

## Next — deeper RFI automation

- Add per-operation safety summaries and exact before/after workbook/PDF previews.
- Split a multi-RFI Worley scan into reviewable page groups without touching the
  source scan.
- Prepare consistent Outlook drafts and archive reviewed publication mail as EML
  without automatically sending it.

## Then — controlled system synchronization

- Compare the original, local, and INTEC/OneDrive RFI systems using hashes and
  explicit scope boundaries.
- Present same, missing, newer, and conflicting files before any transfer.
- Require an explicit authority choice for every conflict; never use silent
  last-writer-wins behavior.
- Stage no-clobber transfers with retained backups, resumable low-bandwidth
  queues, and no automatic retry for uncertain outcomes.

## Later — document automation breadth

- Printer capability discovery, preflight thumbnails, test-page gates, and
  spooler receipts
- RFI status extraction and filing
- PDF merge/split and quality repair
- DOCX find/replace
- Additional register/export operations driven by generic `ParamSchema`
- Internal module decomposition, parser fuzzing, and fault-injection expansion

## Intake rule

Every roadmap item becomes a GitHub Issue before implementation. The issue must
state user outcome, acceptance criteria, safety impact, affected operation, and
intended release. Large architecture decisions receive a document linked from
`docs/DECISIONS.md`.

## Prioritization

1. Data-loss, workbook-repair, duplicate-print, or false-label risk
2. Broken release/package and recovery paths
3. Repetitive user effort in an existing RFI lifecycle
4. New document operations
5. Cosmetic work

No roadmap date is a promise unless a release issue explicitly records it.

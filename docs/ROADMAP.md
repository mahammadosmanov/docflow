# Roadmap

This file describes product outcomes, not day-to-day task status. GitHub Issues
and pull requests are the free execution tracker; `releases.json` and tags are
the historical tracker.

## Now — 2.0.0 reliability and Windows certification

- Keep human-facing current-version markers synchronized with `VERSION` through
  the tested release tool, alongside changelog and release metadata.
- Keep combined multi-RFI Worley scan page identification and exact-page staging
  covered by representative Windows OCR and OneDrive acceptance evidence.
- Certify the exact Windows candidate against Excel open/recalculate, Outlook,
  OneDrive-backed folders, Microsoft Print to PDF, and representative printers.
- Expand structural fixtures for every current production workbook family
  without committing private documents.
- Calibrate OCR against consented representative scans and retain at least 95%
  exact canonical-RFI recognition.
- Benchmark Fast RFI Scan, signed-page staging, and PDF orientation on the same
  Windows and OneDrive fixtures, with cold and warm caches. Record Preview phase
  times, exact OCR coverage, incorrect labels, incorrect split boundaries,
  manual reviews, and recovery after interruption.
## Next — deeper RFI automation

- Keep the implemented exact existing-cell workbook comparison aligned with
  guarded Follow Up step metadata as workbook families expand.
- Keep the implemented bounded Worley first-page thumbnails covered by packaged
  Windows and representative combined-scan acceptance evidence.
- Keep the implemented warning search, type filtering, and exact-RFI page focus
  covered by large combined-scan usability checks.
## Later — document automation breadth

- Printer capability discovery, preflight thumbnails, test-page gates, and
  spooler receipts
- Additional RFI lifecycle extraction beyond exact completed-PDF evidence
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

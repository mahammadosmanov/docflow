# Roadmap

This file describes product outcomes, not day-to-day task status. GitHub Issues
and pull requests are the free execution tracker; `releases.json` and tags are
the historical tracker.

## Now — 0.6.x reliability

- Ship the Tauri 2 production baseline as `v0.6.0`.
- Calibrate OCR against representative real scans and retain at least 95% accuracy.
- Exercise the signed-page workflow on OneDrive-backed Windows folders.
- Keep version, changelog, installer, and release metadata synchronized.

## Next — safe operational maturity

- Add a read-only in-app diagnostics/export bundle for support reports.
- Add checksums to every published installer and surface them in release notes.
- Expand fixture coverage for every production workbook family without committing
  private documents.
- Define a free/manual Windows code-signing plan; do not claim signing until a
  trustworthy certificate path exists.

## Later — document automation breadth

- Multi-RFI PDF splitting
- RFI status extraction and filing
- PDF merge/split
- DOCX find/replace
- Additional register/export operations driven by generic `ParamSchema`

## Intake rule

Every roadmap item becomes a GitHub Issue before implementation. The issue must
state user outcome, acceptance criteria, safety impact, affected operation, and
intended release. Large architecture decisions receive a document linked from
`docs/DECISIONS.md`.

## Prioritization

1. Data-loss or false-label risk
2. Broken release/package paths
3. Repetitive user effort in an existing workflow
4. New document operations
5. Cosmetic work

No roadmap date is a promise unless a release issue explicitly records it.

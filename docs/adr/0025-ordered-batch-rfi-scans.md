# ADR-0025: Ordered batch RFI scan separation

- Status: Accepted
- Date: 2026-09-25

## Context

Process Scans currently recognizes one RFI per top-level PDF. Operators may
scan a stack of consecutive RFIs into one PDF and need the usual exact naming,
folder placement, duplicate handling, Review fallback, and recoverable Apply.
Copying the full stack into each RFI folder would disclose unrelated pages and
would not separate the documents.

## Decision

Process Scans inspects every page of a top-level PDF in order, subject to the
existing bounded PDF size and a 20,000-page operation limit. One strict,
high-confidence exact RFI code starts a group. A page with no RFI candidate
continues the current group. Pages before the first exact RFI, unreadable pages,
and pages with repaired, fuzzy, low-confidence, or conflicting evidence go to
Review and break the current group. A repeated RFI start after another group is
also Review; it is not silently appended to the earlier RFI.

Apply uses page indexes and source SHA-256 values frozen in the immutable Plan.
Each generated PDF contains only its approved pages and their reachable page
objects; unsupported signatures, encryption, document-level structures,
widgets, annotations, cross-page references, parser failures, drift, and output
collisions fail closed. Each output is journaled as a no-clobber copy. When
Keep original is off, the combined source moves to application trash only after
all of its extracted outputs succeed. The source remains if extraction fails.

CIV, MEC, ELE, and PRC outputs always use a top-level discipline folder.
Organized outputs use `<destination>/<discipline>/<RFI>/<canonical filename>`;
flat outputs use `<destination>/<discipline>/<canonical filename>`. Other
disciplines retain the existing Process Scans layout. Review outputs remain in
`_review`.

## Rationale and trade-offs

Page-only PDF reconstruction prevents unrelated pages or hidden page objects
from remaining accessible in an RFI output. Conservative Review boundaries
protect against mistaking uncertain pages for continuations. Files containing
unsupported document-level features remain whole in Review rather than losing
those features during extraction. This favors fidelity over automatic coverage.

## Alternatives considered

- Keep the full combined PDF under every detected RFI: rejected because every
  output would contain other RFIs' pages.
- Rasterize pages to new image PDFs: rejected because it can reduce color,
  searchability, vector fidelity, and print quality.
- Add an external PDF utility or runtime dependency: rejected because the
  existing Rust PDF parser/importer can safely provide the required operation.
- Treat every page without an exact code as Review: rejected because ordinary
  continuation pages often omit the RFI number; these continue the current
  exact group until uncertain evidence or a new exact group appears.

## Consequences and rollback

The Rust step contract gains an additive page-extraction action. Protocol
version remains unchanged. Four locale catalogs need no new strings because
the generic Plan renders the existing renamed/review notes and approved paths.
Undo removes exact generated outputs and restores any source moved to application
trash. Rollback is a source release rollback; no document migration is needed.

## Propagation

- `docs/PRD.md`: DF-032.
- `docs/PRODUCT.md` and `docs/FEATURES.md`: ordered batch handling and
  discipline folders.
- `docs/TRACEABILITY.md`: implementation and regression evidence owner.
- `docs/KNOWN_ISSUES.md` and `docs/RELEASE_SAFETY.md`: unsupported-page and
  packaged acceptance boundaries.
- `src-tauri/src/contracts.rs`, `pdf.rs`, `operations/process_scans.rs`,
  `fs_ops.rs`, and `native.rs`: page-only extraction, plan pinning, and
  recoverable mutation.

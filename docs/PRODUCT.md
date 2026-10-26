# Product definition (living PRD)

## Product

DocFlow is a desktop document-automation application for repetitive, high-risk
RFI, PDF, Excel, and printing workflows. It is designed for operators who need
speed without allowing silent document loss or misclassification.

Current product version: **1.33.13**. `VERSION` is authoritative.

## Problem

Manual construction-document work is repetitive and error-prone: scans must be
identified, files renamed and organized, Excel registers updated without breaking
workbook behavior, print batches kept in order, and every change recoverable.

## Users

- Document controllers processing RFI scans and signed pages
- Site/office operators maintaining RFI registers
- Administrators approving access and auditing shared history

## Product contract

1. Every mutation is preview → explicit approval → apply → journal → undo.
   A fixed replica profile may receive revocable standing authorization after
   its first explicit Apply; every later run still plans, verifies, journals,
   backs up, and fails closed.
2. Apply executes the approved plan without recalculating it.
3. Existing destinations are never overwritten.
4. A missed OCR match is reviewable; a guessed label is forbidden.
5. The archive used by signed-page staging is read-only.
6. The webview never receives Supabase tokens or arbitrary filesystem/network authority.
7. All user-visible strings exist in Turkish, English, Russian, and Azerbaijani;
   schema-generated switches expose localized accessible names.
8. The UI is monochrome; meaning is conveyed by words, icons, controls, and border weight.
9. Cloud history is audit metadata only; only a validated local journal can authorize undo.
10. Every preview presents a shared, localized count of its approved effect types before Apply;
    editable previews can reveal the exact approved source and destination paths.
11. Warning-heavy previews remain searchable and type-filterable without hiding
    unmatched records from the immutable Plan.
12. Existing-cell workbook mutations expose the exact approved sheet, cell,
    current value, and replacement value before Apply.
13. A machine-checked documentation map, stable requirement IDs, traceability
    owners, known limitations, and Windows runbook keep human and AI handoffs
    aligned with the registered Rust operation roster.
14. PDF orientation comes from rendered content, not page dimensions. OSD and
    directional OCR remain fail-closed; a narrow fallback accepts only the
    same exact canonical RFI header ID and direction at both 250/350 DPI.
    Ambiguous or conflicting evidence stays for review, and source PDFs remain
    read-only; only approved copies enter the separate review tree.
15. Incoming RFI grouping requires readable EML/MSG evidence for each exact RFI;
    missing mail blocks that RFI's folder and all related copies.
16. Large repeated RFI scans checkpoint each completed PDF in a local,
    owner-scoped index; reuse is visible and never substitutes for Apply guards.
    Orientation decisions pin source SHA-256 before inspection/detection, recheck
    after analysis and cache reuse, and use a versioned cache profile.
17. Independent bulk discovery and analysis use bounded multi-core workers;
    every operation reports localized stages and item progress while all effects
    that can collide or become uncertain remain ordered.
18. Incoming top-level files can be grouped by exact CIV, MEC, ELE, or PRC RFI
    identity from filenames or bounded content inspection; eligible approved
    copies go under `<destination>/<discipline>/<full-code>/<source filename>`.
    Multi-RFI mail fans out to every eligible matching folder while source files
    remain read-only; collisions never overwrite.
19. Exact completed-RFI PDF filenames anywhere below the read-only RFI System
    can drive an explicit, reviewable RFI Log `Inspection Status = Completed`
    update without touching Follow Up or Worley data.
20. One selected RFI System is the sole source for one-way publication to two
    targets. Replacements retain verified backups; target-only files are never
    deleted; unattended publication supports one-minute checks, defers for
    recently modified source files, and pauses before approved mass-change
    limits are exceeded.
21. Repeated replica comparisons reuse only exact fingerprint-scoped local
    SHA-256 evidence. Completed hash work survives interruption, while Apply
    always validates live files and never trusts the index as mutation authority.
22. Local support diagnostics include bounded, pathless printer inventory and
    latest-queue evidence so real Windows/EPSON outcomes can be compared without
    exposing document names, contents, paths, credentials, or tokens.
23. Authenticated users can reach primary tasks and recovery destinations through
    persistent, responsive navigation. Fast RFI Scan is prominent without
    removing access to any registered operation.

## Current capabilities

| Capability | User outcome | Safety boundary |
|---|---|---|
| Fast RFI Scan | Identify known RFI covers, split confirmed multi-RFI scans, or read only page one for a single RFI while preserving its complete PDF | Multi-RFI requires high-confidence consensus on a Strict or approved OCR-alias identity plus visible cover evidence; a narrow independent rescue may resolve an otherwise unresolved strong-vote candidate but never overrides a stable text-layer identity or conflict. Exact OCR-only `CLV` normalizes to `CIV`. Preview accepts supported full-code candidates with one-to-four ASCII digits and pads the suffix to four; ITP, dates, and other numbers are not normalized. With optional page rotation on, direction-zero generic OCR words alone do not stop later cardinal cover searches; unresolved/no-code pages retain unchanged-copy review handling. That path pins each source SHA-256 before parallel OCR/orientation analysis and rejects source changes before Plan creation. Rust rederives target paths; source/page selections stay fixed and Apply never overwrites |
| RFI file grouping | Copy PDFs, Office documents, EML, MSG, and other bounded files into `<destination>/<discipline>/<full-code>/<source filename>` | Only CIV/MEC/ELE/PRC; top-level source is read-only; readable EML/MSG evidence is required per RFI; exact multi-RFI fan-out is previewed; no overwrite; unmatched/unreadable files go to root `_review` |
| RFI Log Add | Prepare and apply multi-sheet register updates | Backup, workbook fingerprint, post-write verification, undo |
| RFI System/Worley sync | Detect signed first pages and synchronize RFI Log remarks plus CIV/MEC Worley control tables | Bounded parallel discovery/signature analysis with serial per-PDF checkpoints; exact header discovery, literal-cell guards, closed OOXML delta verification, backup, and undo |
| RFI Follow Up sync | Project uniquely mapped RFI Log statuses into existing Follow Up groups and Worley tables | Exclude CIV source RFIs with `PCS-` Tag No values, including removing them from existing groups; fan comma-separated CIV `Tag No` tags only to distinct, exact prepared rows and show matched/unmatched tags in unselectable partial-coverage diagnostics; match MEC by exact source `Tag No`→target Block and source `Area`→target Section, including comma-separated multi-section fan-out; ignore MEC Inspection Location; supported zero-match records stay outside template scope; never create rows or sections or change worksheet Remarks; ambiguous matches remain manual review; immutable Plan, guarded Apply, retained backup, and exact-identity failed-Apply Undo |
| RFI completion-status sync | Detect exact completed RFI PDFs at every nesting depth and mark approved RFI Log rows Completed | Read-only bounded parallel discovery; exact filenames and rows; conflicting/formula/merged targets fail closed; backup, closed OOXML delta verification, journal, and undo |
| Outlook reply draft | Prepare a Reply All response from the one selected message | Windows-only, exact-selection revalidation, unsent display, no recipient synthesis |
| Batch Print | Keep mixed-file jobs in an approved order without false failure on normal fast spool release; optionally find exact First Page `.docx` and Sketch `.pdf` documents in the selected RFI System by canonical code, discipline-number input, or four-digit number | Persistent resumable queue, journaled approved steps, recoverable status waiting, no retry of explicit uncertainty, opt-in recall of safe settings without old files; finder is read-only, presents missing types/multiple versions, and requires explicit choice where bare numbers span disciplines |
| Signed-page staging | Identify individual RFI pages in single or combined Worley scans, search the complete nested RFI System, visually compare old/new first pages, and replace page one in review copies | High-confidence consensus-confirmed Strict or approved OcrAlias identities only; repaired, fuzzy, conflicting, and filename-only identities remain review-only. Archive matching requires the exact case-insensitive RFI PDF basename; one-to-four-digit suffixes canonicalize to four digits, while Sketch/DOCX and folder/other-name matches do not count. Equivalent short/canonical duplicate PDFs remain ambiguous; archive and signed scan stay read-only, and existing review destinations never overwrite |
| PDF orientation normalization | Make every confidently classifiable PDF page under CIV/MEC/ELE/PRC readable without manual rotation | Shares the canonical `/Rotate` and correction policy with Fast RFI Scan; the exact-header-ID fallback requires one canonical ID and direction at both 250/350 DPI. Source SHA-256 is pinned before analysis and checked after analysis and cache reuse; durable per-PDF checkpoints, bounded multi-core preview, separate no-clobber review tree, structural fidelity verification, and whole-file review for unresolved cases |
| Review manager | Keep, place, or recoverably delete exceptions | Delete is a move to application trash |
| History and undo | Audit and reverse completed work | Local journal is authoritative; cloud is optional |
| Access control | Sign in, approve users, administer access | Supabase RLS is the server-side authority |
| Support diagnostics | Preview, copy, or download a redacted local bundle including latest pathless print-queue evidence | Bounded Rust DTO, recursive redaction, no upload, document path, or user-file scan |

## Non-goals

- No paid SaaS dependency is required to build, track, or release the project.
- No document processing, filesystem authority, credentials, arbitrary network access,
  shell access, or process control in React or the webview.
- No embedded interpreter, application sidecar, or second mutation backend.
- No telemetry, hosted logging, or paid support service.
- No silent auto-correction when source evidence is ambiguous.
- No destructive delete or destination overwrite.
- No unsupported claim that authentication prevents local binary modification.

## Success criteria

- Representative consented OCR calibration retains at least 95% exact coverage
  with zero wrong automatic RFI labels; ambiguous evidence remains Review.
- Preview never changes source or destination state.
- Apply/undo returns the test fixture to its original hash when no later user edit exists.
- Source, frontend, Rust, license, package-content, packaged operation, and
  platform packaging gates pass after the final edit.
- Package inspection proves that no retired application runtime, library,
  frozen-runtime artifact, bytecode, or sidecar is bundled.
- Windows host, Office, printer, live-service, signing, and notarization evidence
  is reported separately and never inferred from a macOS or cross-compile result.
- Every release is discoverable through `releases.json`, `CHANGELOG.md`, a Git tag,
  and (for distributed builds) a GitHub Release.
- Required onboarding documents, requirement ownership, and the documented
  operation roster pass `node tools/docs_audit.mjs`.

## Source-of-truth order

1. Root and nearest nested `AGENTS.md` engineering rules
2. This product contract and accepted architecture decisions
3. `docs/superpowers/specs/2026-08-24-rust-engine-migration-design.md`
4. Protocol/contracts and safety tests
5. Release/version metadata and CI evidence
6. Roadmap and GitHub Issues

When sources disagree, stop the change and reconcile documents and tests in the
same pull request.

# Product definition (living PRD)

## Product

DocFlow is a desktop document-automation application for repetitive, high-risk
RFI, PDF, Excel, and printing workflows. It is designed for operators who need
speed without allowing silent document loss or misclassification.

Current product version: **1.10.2**. `VERSION` is authoritative.

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
7. All user-visible strings exist in Turkish, English, Russian, and Azerbaijani.
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
14. PDF orientation is inferred from rendered content, never page dimensions;
    the RFI system remains read-only and only complete high-confidence decisions
    may create separately reviewed copies.
15. Incoming RFI grouping requires readable EML/MSG evidence for each exact RFI;
    missing mail blocks that RFI's folder and all related copies.
16. Large repeated RFI scans checkpoint each completed PDF in a local,
    owner-scoped index; reuse is visible and never substitutes for Apply guards.
17. Independent bulk discovery and analysis use bounded multi-core workers;
    every operation reports localized stages and item progress while all effects
    that can collide or become uncertain remain ordered.
18. Incoming top-level files can be grouped by exact RFI identity from filenames
    or bounded content inspection; multi-RFI mail is copied to every matching
    folder while source files remain untouched.
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

## Current capabilities

| Capability | User outcome | Safety boundary |
|---|---|---|
| Process Scans | Identify, split ordered multi-RFI scan PDFs, rename, organize, and review RFI pages | Exact high-confidence RFI starts; blank pages continue the current RFI; uncertain pages go to `_review`; outputs contain only approved pages; CIV/MEC/ELE/PRC results use matching discipline folders; no-overwrite and recoverable source handling |
| RFI file grouping | Copy PDFs, Office documents, EML, MSG, and other bounded files into exact per-RFI folders | Top-level source is read-only; exact ASCII identifiers only; multi-RFI fan-out is previewed; collisions do not overwrite; unmatched/unreadable files go to `_review` |
| RFI Log Add | Prepare and apply multi-sheet register updates | Backup, workbook fingerprint, post-write verification, undo |
| RFI System/Worley sync | Detect signed first pages and synchronize RFI Log remarks plus CIV/MEC Worley control tables | Bounded parallel discovery/signature analysis with serial per-PDF checkpoints; exact header discovery, literal-cell guards, closed OOXML delta verification, backup, and undo |
| RFI completion-status sync | Detect exact completed RFI PDFs at every nesting depth and mark approved RFI Log rows Completed | Read-only bounded parallel discovery; exact filenames and rows; conflicting/formula/merged targets fail closed; backup, closed OOXML delta verification, journal, and undo |
| RFI System replica publication | Publish CIV/MEC/ELE/PRC and selected root files from one chosen source to two targets manually or under a standing authorization | Complete no-symlink scans, durable fingerprint-scoped incremental hashing, retained version backup before replacement, serial fail-fast Apply, live hash verification, journal, Undo, no automatic deletion |
| Outlook reply draft | Prepare a Reply All response from the one selected message | Windows-only, exact-selection revalidation, unsent display, no recipient synthesis |
| Batch Print | Keep mixed-file jobs in an approved order without false failure on normal fast spool release | Persistent resumable queue, journaled approved steps, recoverable status waiting, no retry of explicit uncertainty, and opt-in recall of safe settings without old files |
| Signed-page staging | Identify individual RFI pages in single or combined Worley scans, search the complete nested RFI System, visually compare old/new first pages, and replace page one in review copies | Bounded multi-core archive discovery uses exact filenames; exact page index and candidate are pinned; thumbnails are inert and bounded; archive and source scan stay read-only; unreadable, duplicate, ambiguous, and unsupported pages are rejected |
| PDF orientation normalization | Make every confidently classifiable PDF page under CIV/MEC/ELE/PRC readable without manual rotation | Durable per-PDF checkpoints; bounded multi-core preview; exact page rotations and source hashes; separate no-clobber review tree; structural fidelity verification; uncertain whole PDFs stay unapplied |
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

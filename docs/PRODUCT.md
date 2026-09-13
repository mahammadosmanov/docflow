# Product definition (living PRD)

## Product

DocFlow is a desktop document-automation application for repetitive, high-risk
RFI, PDF, Excel, and printing workflows. It is designed for operators who need
speed without allowing silent document loss or misclassification.

Current product version: **0.9.0**. `VERSION` is authoritative.

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
2. Apply executes the approved plan without recalculating it.
3. Existing destinations are never overwritten.
4. A missed OCR match is reviewable; a guessed label is forbidden.
5. The archive used by signed-page staging is read-only.
6. The webview never receives Supabase tokens or arbitrary filesystem/network authority.
7. All user-visible strings exist in Turkish, English, Russian, and Azerbaijani.
8. The UI is monochrome; meaning is conveyed by words, icons, controls, and border weight.
9. Cloud history is audit metadata only; only a validated local journal can authorize undo.

## Current capabilities

| Capability | User outcome | Safety boundary |
|---|---|---|
| RFI work tracking | Resume multi-day RFI work from Today, Resume, and Needs Review queues | Owner-scoped append-only events; exact RFI codes; concurrent transitions fail into review; official documents remain authoritative |
| Process Scans | Identify, rename, organize, and review RFI PDFs | Strict `-RFI-` anchor, no-overwrite, `_review/` fallback |
| RFI Log Add | Prepare and apply multi-sheet register updates | Backup, workbook fingerprint, post-write verification, undo |
| RFI System/Worley sync | Detect signed first pages and synchronize RFI Log remarks plus CIV/MEC Worley control tables | Exact header discovery, literal-cell guards, closed OOXML delta verification, backup, and undo |
| Outlook reply draft | Prepare a Reply All response from the one selected message | Windows-only, exact-selection revalidation, unsent display, no recipient synthesis |
| Batch Print | Keep mixed-file jobs in an approved order | Persistent resumable queue and journaled approved steps |
| Signed-page staging | Replace page one in review copies | Archive read-only, separate roots, ambiguous matches rejected |
| Review manager | Keep, place, or recoverably delete exceptions | Delete is a move to application trash |
| History and undo | Audit and reverse completed work | Local journal is authoritative; cloud is optional |
| Access control | Sign in, approve users, administer access | Supabase RLS is the server-side authority |
| Support diagnostics | Preview and copy a redacted local support bundle | Bounded Rust DTO, recursive redaction, no upload or user-file scan |
| Device readiness | See OCR, printer, and interrupted-work health before starting | Read-only bounded checks; document-specific checks remain in preview |

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

- Representative OCR calibration remains at least 95% accurate.
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

## Source-of-truth order

1. Root and nearest nested `AGENTS.md` engineering rules
2. This product contract and accepted architecture decisions
3. `docs/superpowers/specs/2026-08-24-rust-engine-migration-design.md`
4. Protocol/contracts and safety tests
5. Release/version metadata and CI evidence
6. Roadmap and GitHub Issues

When sources disagree, stop the change and reconcile documents and tests in the
same pull request.

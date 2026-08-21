# Product definition (living PRD)

## Product

DocFlow is a desktop document-automation application for repetitive, high-risk
RFI, PDF, Excel, and printing workflows. It is designed for operators who need
speed without allowing silent document loss or misclassification.

Current product version: **0.6.0**. `VERSION` is authoritative.

## Problem

Manual construction-document work is repetitive and error-prone: scans must be
identified, files renamed and organized, Excel registers updated without
breaking workbook behavior, print batches kept in order, and every change made
recoverable. Existing general-purpose tools do not preserve one reviewable plan
and one undo trail across those operations.

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

## Current capabilities

| Capability | User outcome | Safety boundary |
|---|---|---|
| Process Scans | Identify, rename, organize, and review RFI PDFs | Strict `-RFI-` anchor, no-overwrite, `_review/` fallback |
| RFI Log Add | Prepare and apply multi-sheet register updates | Backup, workbook fingerprint, post-write verification, undo |
| Batch Print | Keep mixed-file jobs in an approved order | Persistent resumable queue and journaled approved steps |
| Signed-page staging | Replace page one in review copies | Archive read-only, separate roots, ambiguous matches rejected |
| Review manager | Keep, place, or recoverably delete exceptions | Delete is a move to application trash |
| History and undo | Audit and reverse completed work | Local journal is authoritative; cloud is optional |
| Access control | Sign in, approve users, administer access | Supabase RLS is the server-side authority |

## Non-goals

- No paid SaaS dependency is required to build, track, or release the project.
- No document processing in React, Rust, or the webview.
- No silent auto-correction when source evidence is ambiguous.
- No destructive delete or destination overwrite.
- No unsupported claim that authentication prevents local binary modification.

## Success criteria

- Representative OCR calibration remains at least 95% accurate.
- Preview never changes source or destination state.
- Apply/undo returns the test fixture to its original hash when no later user edit exists.
- Every release passes Python tests/mypy, frontend tests/build, Rust check, sidecar smoke,
  and packaged real-OCR smoke on Windows.
- Every release is discoverable through `releases.json`, `CHANGELOG.md`, a Git tag,
  and (for distributed builds) a GitHub Release.

## Source-of-truth order

1. `CLAUDE.md` non-negotiable engineering rules
2. This product contract and accepted architecture decisions
3. Protocol/contracts and safety tests
4. Release/version metadata and CI evidence
5. Roadmap and GitHub Issues

When sources disagree, stop the change and reconcile the documents and tests in
the same pull request.

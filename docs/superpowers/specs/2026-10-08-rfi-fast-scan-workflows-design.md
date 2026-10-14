# RFI Grouping and Fast Scan Workflows Design

**Status:** Approved by the user; implementation plans reviewed and execution authorized.

## Goal and constraints

Make incoming RFI filing and scan processing reliable for the four supported disciplines—CIV, MEC, ELE, and PRC—and usable both for a combined multi-RFI scan and for one complete RFI PDF at a time. Remove the redundant standalone Process Scans operation while preserving the scan engine that Fast RFI Scan uses. Improve the app's shared task, form, and preview UI without changing its monochrome identity.

Ventoy's original RFI database remains read-only. Tests and manual scenarios use isolated copies. Group Files sources remain read-only. Fast RFI Scan retains its existing `keep_original`/trash transfer behavior; this change does not alter source-retention semantics. Every mutation remains Preview → explicit Apply → journal → guarded Undo; no-clobber remains mandatory. Rust remains the authority for identifiers, plans, paths, and filesystem effects. React remains presentation-only. All user-visible strings are localized in EN/TR/RU/AZ.

## Approved operator behavior

### Group Files

`rfi.group_files` copies each approved file to:

```text
<destination>/<CIV|MEC|ELE|PRC>/<full canonical RFI code>/<source filename>
```

For example, a CIV document for `AZE-SUN-INT-QMS-RFI-CIV-0765` goes beneath `<destination>/CIV/AZE-SUN-INT-QMS-RFI-CIV-0765/`. Preserve current input scope, mail-evidence requirement, multi-RFI fanout, source-read-only behavior, preview/revision flow, and no-overwrite behavior. A file matched to several eligible RFIs is copied once under each matching code. A target collision never overwrites; it remains a visible non-applied/review outcome according to the existing fail-closed Plan behavior.

### Fast RFI Scan modes

Keep one user-facing operation, Fast RFI Scan, with a schema-driven `scan_mode` choice whose wire values are `multi_rfi` and `single_rfi`. Default to `multi_rfi` so existing Fast RFI Scan behavior is unchanged; settings that lack the new field deserialize as `multi_rfi`.

1. **Separate multiple RFIs:** retain the current Fast Cover recognition and page grouping behavior. Only a confirmed cover starts a new RFI group; the identity must be high-confidence consensus-confirmed Strict or an approved OCR alias. The independent cover-confirmed rescue is limited to unresolved strong-vote candidates and cannot override a stable text-layer identity or conflict; uncertain pages remain review-only.
2. **One RFI per PDF:** render/OCR only page 1 for the RFI identity. Use the complete input PDF bytes as one unsplit output; do not extract pages, rotate, or rewrite PDF metadata in this mode. Honor the existing `keep_original` setting and its current copy/move-to-output behavior. Derive the canonical output name and discipline from the validated code. When `organize_into_rfi_folders` is enabled, use `<destination>/<discipline>/<full-code>/RFI No. <full-code>.pdf`; when disabled, keep the existing flat layout under `<destination>/<discipline>/RFI No. <full-code>.pdf`.

Both modes retain the existing source-transfer/keep-original behavior. A mode-specific, explicit selection prevents the operator from having to infer whether a PDF will be split. When first-page identity is absent, ambiguous, malformed, unsupported, or below the current acceptance threshold, the source follows the existing review/transfer behavior rather than a guessed output.

### Discipline recognition and correction

The supported persisted RFI disciplines are exactly CIV, MEC, ELE, and PRC. Remove general edit-distance discipline recovery from code recognition: it can turn unsupported text into a false RFI. The sole automatic OCR alias is an exact OCR-recognized `CLV` discipline segment, normalized to `CIV`; this is OCR-only and does not make CLV valid in filenames, imported text, or manual identifiers. Do not fuzzy-repair other segment values. Unsupported or uncertain identities stay in review.

When OCR applies the CLV alias, expose that correction in the preview as evidence (detected CLV → proposed CIV), alongside the canonical code and confidence. The operator still reviews and explicitly applies the plan. A manual preview edit accepts a full code from one of the four supported disciplines with a one-to-four-ASCII-digit suffix, padding it to four digits for canonical output; longer suffixes are rejected without truncation. Editing is limited to the RFI code; the output filename and destination are derived again from that code by Rust. The UI cannot submit a destination path or filesystem action. Invalid edits, stale plans, or target collisions fail closed without modifying the source or replacing an existing destination.

### Retiring standalone Process Scans

Remove `rfi.process_scans` from the active operation registry and new-workflow UI. Keep Fast RFI Scan (`rfi.process_scans_fast_covers`) as the single registered scan operation, and retain only the shared page-splitting/orientation internals it uses. Existing journals, settings, cloud history, and operation identifiers are persistent compatibility data: do not delete or rewrite them. Keep legacy Process Scans history renderable, but do not expose the retired operation as a way to create a new plan. Preserve any low-level authorization needed to safely recover existing journaled work; no new preview may be created under the retired operation ID.

## UI direction

Use a workflow-first refresh across shared shell, operation setup, and preview components rather than a navigation rebuild or isolated scanner-only restyle. Keep the current monochrome light/dark tokens, native system font, and semantic status colors only. Strengthen hierarchy and legibility: 14px minimum body text, clear 16–24px headings, and comfortable control targets. Make Fast RFI Scan the primary task; keep other tasks searchable and grouped. Reduce duplicated dashboard utility presentation. Reuse a consistent compact step header, focused form width, readable preview, and pinned primary-action area. At desktop sizes, remove avoidable empty space; at narrow/window-scaled sizes, preserve readable content and keyboard access. Honor high-contrast, focus-visible, reduced-motion, and responsive behavior. Do not add a UI framework or decorative gradients/color system.

The Fast RFI Scan form exposes the two modes with concise descriptions. The preview groups planned outputs by discipline/RFI and shows the source, detected/corrected code evidence, exact derived target, review state, and collision/warning outcome. Manual code correction is a focused inline control with visible validation; changing the code recalculates the target through Rust. Existing Preview → Apply controls and no-overwrite warnings remain prominent. UI changes must not weaken the separation between preview and effect.

## Architecture and data flow

- `registry.rs` defines the active operation list and the Fast RFI Scan mode parameter. Remove only the standalone Process Scans definition.
- `backend.rs` and `native.rs` accept new previews only for the active Fast RFI Scan operation. Shared engine code remains behind that operation; legacy journal/recovery paths remain narrowly compatible.
- `rfi_code.rs` provides exact canonical parsing for the four supported disciplines. OCR normalization is a separate, explicit recognition step that maps only CLV to CIV, records evidence, and does not affect filenames or other user-authored text.
- `operations/process_scans.rs` selects first-page-only whole-file output or existing multi-RFI separation from a typed scan mode. It constructs paths and output names in Rust, rejects invalid corrections, and never trusts client-provided target paths.
- `operations/rfi_file_group.rs` adds the discipline directory above each canonical RFI folder without changing discovery or eligibility rules.
- The Plan revision path for Fast RFI Scan validates edited codes and regenerates related directory/copy steps from the original approved source, destination root, and revised code. It rechecks the same no-clobber and identifier constraints. It must not permit edits to source paths, destination roots, page indexes, or arbitrary Plan steps.
- `frontend/src/App.tsx` and shared plan-editor components render the modes and correction affordance from schemas/Plan editor descriptors; no operation-specific filesystem logic is added to React. The demo bridge must mirror the visible contract without becoming authority.
- Update all affected translations, product/feature/domain docs, traceability and tests. Retired operation strings may remain only where required to display legacy records or document retirement.

## Failure handling and safety

Malformed/unsupported RFI, failed OCR, ambiguous cover, invalid correction, source drift, overlapping roots, symlink/reparse point, unsafe output name, or occupied output must fail closed. Preview may report review outcomes, but must not claim a file was filed if no safe output is planned. Apply uses only the immutable approved Plan and revalidates pinned sources. An output collision never overwrites or silently renames an existing file. Undo may remove only the exact files/directories created by the operation and must preserve later user changes. The single-RFI mode's output bytes must equal the complete source PDF bytes; existing `keep_original` transfer semantics remain unchanged.

## Verification and acceptance

1. Rust tests prove all four disciplines route to their matching top-level folder, the mail gate and multi-RFI fanout still work, unmatched material remains reviewable, and existing target collisions never overwrite.
2. Fast RFI Scan tests prove: first-page OCR is invoked without later-page OCR in one-RFI mode; the output SHA-256 equals the input's; source retention/movement follows the existing `keep_original` setting; the existing multi-RFI cover path still separates multiple confirmed covers; uncertain/missing/unsupported codes go to review; exact OCR CLV becomes CIV with visible evidence; other unsupported disciplines are not repaired; and only a valid manually edited code regenerates the canonical output path. Test malformed edits and collisions.
3. Registry/backend tests prove the retired operation is absent from active discovery and cannot create a new plan, while legacy records remain renderable and safe journal recovery is not broken.
4. Frontend tests cover accessible mode selection, localized scan/edit states, validation feedback, visible planned target, and no UI path authority. Check all EN/TR/RU/AZ keys.
5. Run affected Rust/frontend suites, version/docs/package audits, formatting and quality gates. Exercise one feature scenario at a time against private copies of representative Ventoy RFI PDFs and mail/doc samples. Record hashes before and after; original Ventoy sources and database remain untouched.
6. After all source behaviors pass, build a Windows trial package and report its artifact/checksum and any Windows-host acceptance still requiring the user's machine. A macOS build does not certify Windows runtime behavior.

## Out of scope

No changes to Ventoy's original data, database schema, or RFI system. No change to unrelated operations, cloud formats, credentials, PDF trust policy, workbook mapping, or release publication. No full navigation redesign, new UI dependency, automatic overwrite/numbering, or fuzzy correction beyond OCR-only CLV → CIV.

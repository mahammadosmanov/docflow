# Printing Order v1 Design

**Date:** 2026-08-26
**Status:** Approved for implementation
**Base candidate:** `a439d95b468e354c5b99dd929c61beb4f9a4064e`
**Operation:** `files.batch_print`

## 1. Goal

Complete the existing Batch Print workflow with:

1. named, device-local presets for stable print settings; and
2. exact, per-file page ranges for controlled PDF printing.

The feature must preserve DocFlow's existing strict queue order, preview-before-apply contract, resumable queue semantics, native Rust authority, and compatibility with previously persisted print queues.

## 2. Confirmed product decisions

- A preset stores print settings only.
- A preset never stores file paths, file order, or page ranges.
- Files are selected and ordered for every new print run.
- Page range is edited per file in Preview.
- PDF page ranges are exact.
- Supported images accept only `all` or `1`.
- Office/native-handler files accept only `all`; any other range is rejected before apply.
- Office COM is not part of Printing Order v1. It belongs to the later Excel worksheet-selection subproject.
- Settings remain device-local because installed printer identities are device-specific.
- Existing queues that do not contain `pageRange` behave as `all`.

## 3. Non-goals

Printing Order v1 does not:

- save complete print orders;
- synchronize presets to cloud accounts;
- store per-file overrides in presets;
- print selected Excel worksheets;
- use Word or Excel COM automation;
- submit a physical print job during automated tests;
- silently downgrade an unsupported page range to `all`;
- add an operation-specific frontend form.

## 4. Architecture

### 4.1 Generic operation-preset capability

`OperationDef` gains a generic `supportsPresets: bool` capability with a backward-compatible default of `false`. The registry enables it only for `files.batch_print` in this release.

The frontend renders one generic preset toolbar from this capability. It must not branch on `operation.id` to create a print-specific form.

### 4.2 Preset contract

A preset has this protocol shape:

```json
{
  "id": "opaque UUID",
  "name": "A4 duplex",
  "values": {
    "printer": "Installed printer name",
    "paper": "A4",
    "orientation": "auto",
    "scaling": "fit",
    "color": "printer",
    "duplex": "long",
    "copies": 1,
    "collate": true,
    "retries": 0,
    "on_error": "pause",
    "timeout_minutes": 30
  }
}
```

Rules:

- Name is trimmed and contains 1–64 Unicode scalar values.
- Names are unique per operation using case-insensitive comparison.
- At most 20 presets may exist for one operation.
- New presets receive an opaque UUID.
- A duplicate name is rejected unless the request explicitly sets `replace: true`.
- Replacement preserves the existing preset ID.
- Listing is deterministic: case-insensitive name, then ID.
- Deletion requires the exact operation ID and preset ID.
- Unknown operation IDs, operations without preset support, unknown preset IDs, malformed names, and capacity overflow fail closed.

### 4.3 Preset value allowlist

The Rust backend derives the allowed value set from the operation registry. Only `ParamKind::Bool`, `ParamKind::Choice`, and `ParamKind::Int` fields are persisted.

The following are always excluded even if supplied by a forged frontend request:

- `Folder`, `File`, and `Files`;
- `Text` and `LongText`;
- `Secret`;
- `SheetCounts`;
- unknown fields;
- arrays and objects.

Values are validated against the registry's choices and the operation's existing numeric validation at preview time. Saving a preset does not grant filesystem authority and does not materialize path selections.

### 4.4 Storage

Presets live in the existing atomically written `~/.docflow/settings.json` under an `operationPresets` object keyed by operation ID.

The store follows the existing clone → validate → atomic write → memory swap pattern. A failure before persistence leaves both the on-disk object and in-memory state unchanged; after persistence, a parent-directory sync failure is accepted as committed success only when an exact readback of the settings path matches the serialized new bytes. Existing unknown top-level settings fields remain intact.

### 4.5 Commands

The Rust router and typed frontend command map add:

- `settings/presets` with `{ opId }`, returning `OperationPreset[]`;
- `settings/savePreset` with `{ opId, name, values, replace? }`, returning the saved `OperationPreset`;
- `settings/deletePreset` with `{ opId, presetId }`, returning `{ ok: true }`.

All three methods require the same signed-in and allowed state as other operation settings commands. Rust/frontend method parity remains exact.

### 4.6 Generic frontend behavior

On an operation whose registry definition enables presets, the form stage shows a generic toolbar that can:

- list and select a preset;
- merge the selected preset's values into current form values;
- save current preset-capable values under a new name;
- explicitly replace a duplicate after confirmation;
- delete the selected preset after confirmation.

Applying a preset merges only its allowlisted values. It must preserve all current path selections and all non-preset-capable values. Static invalid choices are scrubbed from returned and persisted preset data. A `files.batch_print.printer` value is the sole dynamic-choice exception: a non-empty, control-free printer name of at most 512 Unicode scalar values survives temporary printer disconnection or inventory failure. It is never silently replaced; current-inventory validation still rejects Preview/apply until the exact printer becomes available again.

## 5. Page-range domain

### 5.1 Syntax

Accepted input is either:

```text
all
```

or a comma-separated list of positive, 1-based pages and inclusive ranges:

```text
1-3,5,8-10
```

Whitespace around tokens and range endpoints is permitted. Parsing is case-insensitive for `all`.

Rejected input includes:

- empty input;
- page `0`;
- negative values;
- empty comma segments;
- more than one hyphen in a token;
- descending ranges such as `5-3`;
- mixed `all` and numeric tokens;
- non-numeric tokens;
- numeric overflow;
- a resolved page above a known PDF page count.

Numeric selections are expanded, deduplicated, sorted ascending, coalesced into maximal consecutive ranges, and serialized without whitespace. Single pages remain single values. Example: `3, 1-2,2,7` becomes `1-3,7`.

### 5.2 Queue and plan representation

`PrintJob` gains a serde-defaulted `pageRange` field whose default is `all`. `PrintSettings`, preview editor rows, `Step.meta`, queue state, resume rows, and revised plans carry the canonical string.

The field participates in job validation. Apply revalidates it even when a forged or legacy plan bypasses the normal UI path.

### 5.3 Format behavior

- **PDF / controlled:** numeric ranges are accepted. On Windows, `windows_pdf::page_count` resolves the upper bound, and only the selected zero-based page indexes are rendered between `StartDocW` and `EndDoc`. On macOS, the canonical numeric range is sent to CUPS as `page-ranges=<range>`.
- **Supported image / controlled:** only `all` and `1` are accepted. They are normalized to `all` for queue execution because the supported static image route is one printable document page.
- **Office or another native application handler:** only `all` is accepted. Numeric page ranges return a validation error during preview or revise.
- **Unsupported file:** existing unsupported-format behavior remains authoritative.

No platform path may catch a page-range validation error and print all pages instead.

### 5.4 Preview and revise

Initial preview rows use `pageRange: "all"`. The generic record-table editor adds one text column for page range.

`engine/revise` is the authoritative point for user-edited row validation. It:

1. validates row identity and existing print settings;
2. parses and canonicalizes `pageRange`;
3. applies the format restrictions above;
4. re-inspects controlled/native support;
5. rebuilds jobs and plan metadata only after every enabled row is valid.

If any enabled row is invalid, no revised plan replaces the prior cached plan.

### 5.5 Resume compatibility

Persisted queue records without `pageRange` deserialize as `all`. Resume copies the saved canonical range into each row. Uncertain, failed, cancelled, and pending jobs retain their page ranges when resumed.

The queue version does not need to change because serde defaults make the added field backward-compatible and no old field changes meaning.

## 6. Error handling and localization

New user-facing failures use localization keys in all supported locales (`tr`, `en`, `ru`, `az`) for:

- invalid range syntax;
- range beyond PDF page count;
- image range unsupported;
- native/Office range unsupported;
- preset name invalid;
- duplicate preset requiring replacement;
- preset limit reached;
- preset missing or stale;
- preset settings write failure.

The frontend shows backend errors through the existing `humanError` boundary. It does not infer a fallback range or fallback printer.

## 7. Security and privacy

- Presets cannot contain paths, file-selection IDs, email text, workbook data, or credentials.
- Backend filtering is authoritative; frontend filtering is defense in depth only.
- Preset operations do not create or extend filesystem grants.
- Preset names are rendered as text, never HTML.
- Page-range input is parsed as data and never interpolated into a shell command.
- macOS CUPS arguments remain separate `OsString` arguments with the existing `--` path delimiter.
- No physical print submission occurs in automated verification.

## 8. TDD slices

1. **Preset storage:** atomic round-trip, filtering, deterministic list, duplicate/replace, capacity, and failed-write rollback.
2. **Command boundary:** authorization, operation capability, forged value filtering, and Rust/frontend method parity.
3. **Generic UI:** load/apply/save/replace/delete while preserving selected files.
4. **Page-range parser:** valid normalization and every rejected syntax/boundary class.
5. **Plan and queue:** preview default, revise canonicalization, image/Office fail-closed behavior, step/job metadata, legacy queue resume.
6. **Platform adapters:** Windows selected index resolution and macOS CUPS `page-ranges` argument generation without physical submission.
7. **Regression:** focused Rust tests, frontend tests/typecheck, formatting/clippy, offline gates, and Windows target check.

Every production behavior is introduced after an observed focused RED and is followed by focused GREEN plus the affected suite.

## 9. Acceptance criteria

- A user can create, apply, explicitly replace, and delete a named Batch Print settings preset.
- Applying a preset never changes selected files or stores file paths.
- A forged preset request cannot persist non-scalar or path-bearing values.
- A user can set a different page range for each preview row.
- Valid PDF ranges are canonicalized and only those pages reach the controlled print renderer.
- Invalid or out-of-bounds PDF ranges block the revised plan.
- Images reject any range other than `all` or `1`.
- Office/native files reject every numeric range before apply.
- Existing queues without `pageRange` resume as `all`.
- Queue order, copies/collation, retries, cancellation, journal behavior, and file-signature validation remain unchanged.
- All supported locales contain the new user-visible keys.
- Required local gates pass with no physical printer side effects.

## 10. External certification

Source/local completion does not certify physical output. Final external gates remain:

- Windows installed-app smoke;
- representative multi-page PDF page-range output on a physical or controlled virtual printer;
- strict mixed-file order under the target printer driver;
- macOS CUPS runtime behavior if macOS becomes a supported shipping target.

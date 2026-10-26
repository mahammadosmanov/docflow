# Inbox Watcher and Action Queue Design

**Status:** Approved by the user (scope, UI sketch, `complete_status_sync` in first slice, 5-minute poll, 60-second settle). Implementation authorized on the feature branch only.
**Branch:** `feature/inbox-watcher-action-queue` (all work and tests stay here; merge to `main` only on explicit approval).
**Target release:** 1.21.0 (`feature`, MINOR). Version bump, changelog, and `releases.json` are part of the implementation slice, not this spec.

## Goal

Reduce the operator's repeated manual starts. DocFlow notices when work is waiting, prepares the existing operations' read-only Previews in the background, and lists them in one Action Queue. The operator opens a queue item, reviews the normal Preview, and explicitly applies it.

Nothing about authority changes: **the watcher never applies, writes, moves, prints, or sends.** It only prepares Previews. Every mutation stays Preview → explicit approval → Apply → journal → Undo (AGENTS.md rule 2). No standing authority is added; `files.replica_publish` remains the only exception.

## Source scope

- Operator workflow, from the Obsidian note "RFI Assistant Numbering v1 Tasarımı" and `Kariyer.md`: RFI numbering, scan filing, signed second-page insertion, three-system sync, mixed printing.
- Current DocFlow canon read for this spec: `docs/FEATURES.md`, `ROADMAP.md`, `KNOWN_ISSUES.md`, `RFI_DOMAIN.md`, `adr/` list, `src-tauri/AGENTS.md`, `frontend/AGENTS.md`.
- Omitted: operator documents, real workbooks, and the code of each operation beyond its public contract. Implementation must verify the assumptions marked **[verify]**.

## Scope v1

### In

1. **Watch profiles**, off by default. Each profile is only an operation ID plus an enabled flag. The watcher reuses that operation's *remembered parameters* (the same saved values the operator used last, re-issued through the existing path-selection grants), so no new folder picker, path grant, or form is introduced. A profile with no remembered parameters stays idle and says why. Supported operations:
   - `rfi.process_scans_fast_covers` (scan inbox)
   - `rfi.group_files` (incoming files)
   - `rfi.stage_signed_first_pages` (signed scan input)
   - `rfi.complete_status_sync` (exact completed PDFs vs. RFI Log status)
2. **Cheap change detection:** polling of directory metadata (name, size, mtime) with a settle delay so partially copied or OneDrive-syncing files are not read. No filesystem-event dependency, because network and OneDrive paths are unreliable. Default cadence is several minutes, bounded; "Check now" is always available.
3. **Background Preview preparation** for a changed profile, one profile at a time, through the existing Rust Preview path. Cancelled immediately if the operator starts or applies any operation. Heavy OCR profiles run only after the settle delay, never in parallel with an operator task.
4. **Action Queue** screen plus a count badge in navigation. Each item shows: operation, source folder, number of files and effects in its Preview, warnings and manual-review count, age, and a staleness state. Actions: **Open Preview**, **Dismiss**, **Re-prepare**.
5. **Staleness:** a queued Preview is invalid when any pinned source hash or folder fingerprint changed. Opening a stale item re-prepares it. Apply keeps its existing hash revalidation; the queue adds no new trust.
6. **Clear states:** idle, waiting for files to settle, preparing, ready, stale, failed (with stable error code), paused (operator task running).

### Out (explicitly not v1)

- Any automatic Apply, standing authority for `system_sync` or `complete_status_sync`, or auto-approval from OCR confidence.
- Outlook inbox monitoring or auto-reading mail. (Candidate for a later slice: selected-mail prefill of `rfi.log_add`.)
- Chained multi-operation workflow sessions.
- Enabling `files.replica_publish`.
- Background service, daemon, tray-only mode, localhost server, or sidecar. The watcher is an in-app Rust task that runs only while DocFlow is open; the existing per-user startup option can launch the app.
- Uploading queue contents to Supabase. Queue state is local; cloud history stays audit metadata.

## Architecture

- **Rust owns everything.** A new bounded module `src-tauri/src/watch/` holds profile validation, the polling scheduler, the settle check, and queue state. It calls the same Preview entry points the UI uses, so plans are produced by the existing authority with the same provenance. **[verify]** that `plan_store` can store a plan prepared without a UI request and what its lifetime is.
- **Plans remain one-shot, server-side, opaque.** The queue holds only an opaque plan identity, never filesystem steps. The webview submits only that identity and the operator's approval.
- **Settings:** an additive, optional `watch_profiles` section. Missing = no profiles = current behavior (rule 16). Profile paths pass the same path-selection grants and canonicalization as operation parameters; symlinks and overlapping roots are rejected.
- **Bounds:** maximum profiles, maximum directory entries per poll, minimum interval, maximum queue length, maximum retained prepared plans. Exceeding a bound fails closed with a stable code.
- **Concurrency (rule 19):** discovery is read-only and may use the existing bounded worker pool. Preparation is serial per profile. Apply, journal order, and Undo are untouched.
- **Network/OneDrive cost:** polling lists metadata only and never reads file contents. Content is read only inside Preview preparation, after settle, at most once per unchanged fingerprint. A failed or cancelled preparation backs off and does not loop.
- **Restart behavior:** plans prepared in memory are not assumed to survive restart. On startup the queue is rebuilt by re-evaluating enabled profiles. **[verify]** against `plan_store` persistence.
- **Frontend:** a new Action Queue screen and navigation badge. Watch profiles are configured through a `ParamSchema`-generated form in Settings (rule 9); no operation-specific form. Strings exist in `tr`, `en`, `ru`, `az`. Monochrome: state is carried by text, icon, and border weight. Keyboard, focus, and accessible names follow `frontend/AGENTS.md`. Progress and state changes use Tauri channels/events, not high-frequency polling from React.

## UI sketch (needs approval)

```text
Action Queue                                      [Check now]
------------------------------------------------------------
[Scan inbox]  Fast RFI Scan        7 PDFs · 4 outputs · 1 review
              ready · 3 min ago                 [Open Preview] [Dismiss]
[Incoming]    Group Files          12 files · 5 RFIs · 2 warnings
              ready · 11 min ago                [Open Preview] [Dismiss]
[RFI System]  Completion Status    3 rows would become Completed
              stale (source changed)            [Re-prepare]
------------------------------------------------------------
Paused: an operation is running.
```

## Safety analysis

| Risk | Control |
|---|---|
| Watcher acts without approval | No Apply path exists in `watch/`; enforced by a test that the module cannot reach Apply entry points. |
| Reading half-copied or syncing files | Settle delay on stable size+mtime; skip files that fail to open for read. |
| Stale queued plan applied | Opaque identity + existing Apply hash revalidation; queue marks drift and re-prepares. |
| Background OCR starves the operator or slow network | One job at a time, paused during operator tasks, minimum interval, off by default, metadata-only polling. |
| Wrong folder watched (e.g., source tree overlaps destination) | Same grant, canonicalization, overlap and symlink checks as operations. |
| Queue grows without bound | Hard caps; oldest dismissed items are dropped. |
| Silent behavior change | Default off; additive settings; first enable shows the exact profile for confirmation. |

## Decisions required

- **ADR 0039 (to be written with the implementation):** "Background read-only Preview preparation." It records that preparing a Preview without an operator click is allowed because it has no effect, and that this does not extend `replica_publish` standing authority. `docs/DECISIONS.md` links it.
- Whether `complete_status_sync` should be in the first slice or follow after the three file-based profiles. Recommendation: include it; it is read-only and has the highest repeat value.
- Default polling interval (recommend 5 minutes) and settle delay (recommend 60 seconds).

## Test plan (RED → GREEN → REFACTOR, on the feature branch)

Rust:
- Profile validation: unknown operation, overlapping roots, symlink, over-bound entry counts.
- Poll diff: new, changed, removed, unchanged; settle delay; unreadable file.
- Scheduler: serial preparation, pause on operator task, backoff after failure, cancellation.
- Queue: staleness on hash or fingerprint change, dismissal, caps, restart rebuild.
- Safety: `watch/` has no call path to Apply, journal writes, or filesystem mutation; source tree byte-identical after a full cycle.
- Settings: file without `watch_profiles` loads unchanged; round trip preserves existing keys.

Frontend (focused Vitest RED before production edits):
- Queue states, badge count, Open Preview hand-off by opaque identity, stale → Re-prepare, empty state, keyboard and accessible names, four-locale key parity.

Gates before merge: all commands in `AGENTS.md` Verification (version check, docs audit, package audit, `npm test`, `npm run build`, `cargo fmt/clippy/test/check`, license audit).

**Windows is not certified by macOS results.** The watcher on OneDrive/network paths, settle behavior, and Excel-backed `complete_status_sync` need the Windows acceptance already tracked in `KNOWN_ISSUES.md`; an entry will be added there with this slice.

## Delivery

1. Approve this spec (scope, UI sketch, open decisions).
2. Implement on `feature/inbox-watcher-action-queue` in vertical slices: settings + validation → poll/diff → scheduler + preparation → queue state → UI.
3. Bump to 1.21.0 (`node tools/version_manager.mjs set 1.21.0`), update `CHANGELOG.md`, `releases.json`, docs, ADR 0039; run all gates.
4. Stop. Merge to `main`, push, tag, and GitHub Release happen only after the operator confirms the branch result.

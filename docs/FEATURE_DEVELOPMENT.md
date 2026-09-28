# Feature development workflow

Use this workflow for every DocFlow feature and improvement. It shortens
feedback while preserving the Preview, approval, Apply, journal, and Undo
contract.

## 1. Define and split the work

- Start from a GitHub Issue with one user outcome, observable acceptance
  criteria, affected operation/layer, safety impact, and platform acceptance.
- Split broad work into independently reviewable vertical slices. Each slice
  should deliver one visible behavior through its required Rust authority,
  generic UI, and focused regression evidence.
- Reuse existing operation contracts, `ParamSchema`, Plan, progress, bounded
  analysis, scan index, journal, and Undo paths. Add shared machinery only when
  more than one real feature needs it.

## 2. Measure before optimizing

For performance-sensitive changes, record before and after on the same
representative synthetic or consented input and machine. Capture total Preview
time and the expensive stages that apply: discovery, OCR/rendering, workbook
read/write, Apply, and Undo. Also record item count, cache hit/reuse counts,
worker limit, platform, and whether the run was warm or cold. Do not use private
operator documents as fixtures or claim a speedup from one noisy run.

If the feature is not performance-sensitive, record the focused-test and full
verification elapsed times when they affect iteration cost. Change one likely
bottleneck at a time, then repeat the same measurement. Keep caches fingerprint
and algorithm-profile scoped; cached analysis never authorizes Apply. Parallelize
independent read-only work only. Keep mutations, journals, and Undo ordered.

## 3. Run checks in a fast-to-slow loop

1. Run the narrowest relevant Rust integration test or Node/Vitest test while
   implementing the slice, for example:

   ```bash
   cargo test --manifest-path src-tauri/Cargo.toml --test <target> <filter>
   node --test tools/<file>.test.mjs
   npx vitest run --config frontend/vite.config.ts <file>
   ```

2. Run formatting, type/build checks, and affected module tests.
3. Before completion, run every applicable source gate in
   [`RELEASE_SAFETY.md`](RELEASE_SAFETY.md). Record exact commands, elapsed
   times, results, and skipped platform gates in the PR.

Targeted checks shorten feedback; they do not replace required final gates.
Keep Rust build concurrency at its established bounded setting unless measured
CI evidence supports changing it. CI runs frontend, Rust format/lint, and Rust
tests/checks independently so a long linker step does not wait behind unrelated
jobs and lint artifacts do not accumulate in the test runner. Dependency caches
may speed clean runners, but must not contain secrets or be treated as trusted
mutation evidence.

## 4. Accept on the real platform

Add acceptance steps when the feature issue is opened, not after coding. Mark
which evidence is synthetic, source-level, packaged, or native. For Windows
workflows, name the needed Excel, Outlook, OneDrive, OCR, printer, or physical
device checks and the operator-owned test copy. Use
[`WINDOWS_RUNBOOK.md`](WINDOWS_RUNBOOK.md) and keep pending native acceptance
visible in the issue/release record.

## 5. Close the slice

Update the feature's docs and traceability with the implementation. Classify
the change with `node tools/version_manager.mjs next <fix|feature|breaking>`;
follow the repository release and commit rules for a completed slice. Report
what became faster using the recorded measurements, what checks passed, and
which platform gates remain pending.

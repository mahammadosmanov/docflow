# DocFlow Workflow UI Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make DocFlow's shared dashboard, operation setup, and Plan preview clearer and more efficient while keeping the existing monochrome identity and safety workflow.

**Architecture:** Rework shared shell and task hierarchy using current React components and CSS tokens. Keep forms schema-driven and Plan changes Rust-authorized. Make Fast RFI Scan the most visible workflow, remove duplicate utility promotion, and apply readable type/spacing/focus patterns consistently without adding a dependency.

**Tech Stack:** React 19, TypeScript, Vite, CSS custom properties, ARB localization, Vitest/Testing Library, browser visual smoke.

**Spec:** `docs/superpowers/specs/2026-10-08-rfi-fast-scan-workflows-design.md`

## Global Constraints

- Keep the current monochrome light/dark tokens, native font stack, and semantic status colors only.
- Preserve the Fast RFI Scan mode and code-edit contracts from the Fast RFI Scan plan.
- Preserve Preview → explicit Apply → journal → guarded Undo; React remains presentation-only.
- Keep all user-visible strings in EN, TR, RU, and AZ; add no UI framework or visual dependency.
- Use readable 14px body text, clear 16–24px headings, comfortable control targets, keyboard access, visible focus, and responsive layouts.
- Do not change Ventoy data, operation authority, or persistent data formats.

## Review Focus

- The home view must foreground Fast RFI Scan without hiding searchable access to other operations.
- Existing users must still reach Review, History, Settings, and Admin; removing duplicated cards must not remove destinations.
- `multi_rfi` and `single_rfi` labels/help must remain distinct and selectable by keyboard in all locales.
- Plan preview must show exact derived target and OCR correction evidence without allowing path edits.
- Narrow window sizes, high contrast, system dark mode, and reduced-motion preferences must remain usable.

---

## File Map

- `frontend/src/App.tsx`: shared home/dashboard hierarchy, `OperationScreen`, `SchemaField`, and `PlanView` presentation.
- `frontend/src/styles.css`: existing color tokens, dashboard grid/cards, form/preview typography and responsive/focus styles.
- `frontend/src/App.test.tsx`: behavior/accessibility assertions for dashboard, forms, preview, and history.
- `frontend/src/locales/app_{en,tr,ru,az}.arb`: revised concise text and mode-specific guidance.


## Execution order

Run after the Group Files and Fast RFI Scan plans. The dashboard uses their final operation roster and preview/edit contracts.

### Task 1: Simplify the task-first dashboard

**Files:**
- Modify: `frontend/src/App.tsx`, `frontend/src/styles.css`
- Test: `frontend/src/App.test.tsx`

**Interfaces:**
- Consumes: active operation list from the Rust bootstrap and the shared navigation routes.
- Produces: Fast RFI Scan as the primary home action; searchable/category-filtered access to every other active operation; Review and Recovery remain reachable as compact actions, while duplicate History/Settings/Admin cards are removed only where the header already provides those routes.

- [ ] **Step 1: Add failing dashboard behavior tests.** Add `home_prioritizes_fast_scan_and_keeps_all_tasks_searchable` and `home_keeps_review_history_settings_admin_and_recovery_reachable`. Assert the primary scan action is first/prominent, the retired Process Scans operation is absent, other operations remain discoverable through search/category controls, and Review/History/Settings/Admin/Recovery routes remain accessible.
- [ ] **Step 2: Run focused tests and observe failure.** Run `npm test -- --run frontend/src/App.test.tsx -t 'home_prioritizes_fast_scan|home_keeps_review'`. Expected: current dashboard gives all operations equal two-column weight and repeats utilities in the side panel.
- [ ] **Step 3: Reorganize the shared dashboard.** In `App.tsx`, add a task-first Fast RFI Scan action from the active bootstrap operation, keep the searchable grouped operation list, preserve Review and Recovery actions because the header does not replace those routes, and remove only duplicate History/Settings/Admin cards. Do not hardcode operation authority in React.
- [ ] **Step 4: Style the dashboard using current tokens.** In `styles.css`, establish consistent content alignment, compact hierarchy, responsive layout, visible keyboard focus, and hit targets without adding brand colors or changing the palette.
- [ ] **Step 5: Re-run focused tests.** Run the tests from Step 2. Expected: key task is prominent and all existing operations/utilities remain reachable.

### Task 2: Refine shared form and Plan preview surfaces

**Files:**
- Modify: `frontend/src/App.tsx`, `frontend/src/styles.css`
- Test: `frontend/src/App.test.tsx`

**Interfaces:**
- Consumes: `ParamSchema` and immutable Plans from Rust; Fast Scan `scan_mode`, evidence, and canonical code-edit rows.
- Produces: a shared focused setup layout, clear form/preview/apply step hierarchy, readable planned targets/warnings, and generic editable Plan fields that remain limited by Rust validation.

- [ ] **Step 1: Add failing UI assertions.** Add `scan_modes_are_named_and_keyboard_selectable` and `plan_preview_shows_corrected_rfi_evidence_and_derived_target`. Assert mode names are distinct, keyboard selection works, the CLV → CIV evidence and exact path are visible, and no source/destination path input appears in the preview editor.
- [ ] **Step 2: Run focused tests and observe failure.** Run `npm test -- --run frontend/src/App.test.tsx -t 'scan_modes_are_named|plan_preview_shows_corrected'`. Expected: the mode/editor contracts are not yet represented in the active form/preview.
- [ ] **Step 3: Refine shared form and preview layout.** In `OperationScreen`, `SchemaField`, and `PlanView`, keep labels/help adjacent to controls, group sources/destinations and review evidence, use a focused maximum content width, keep the primary Preview/Apply action visible, and show output paths with readable wrapping. Use the existing generic `EditorColumn` input behavior; do not add scanner-only filesystem logic.
- [ ] **Step 4: Apply shared type, spacing, and responsive rules.** Ensure body text is at least 14px, headings follow the 16–24px hierarchy, table/metadata text remains legible, and layout remains usable under narrow widths and Windows scaling. Honor visible focus, system dark mode, high contrast, and reduced motion.
- [ ] **Step 5: Re-run focused tests.** Run the tests from Step 2 plus existing `PlanView` and `OperationScreen` tests. Expected: keyboard and output evidence remain visible without changing Rust authority or Apply semantics.

### Task 3: Localize, visually smoke-test, and package the final trial

**Files:**
- Modify: `frontend/src/locales/app_en.arb`, `app_tr.arb`, `app_ru.arb`, `app_az.arb`
- Test: `frontend/src/App.test.tsx`
- Modify: `CHANGELOG.md`, `releases.json`, `VERSION`, runtime manifests

**Interfaces:**
- Consumes: the shared layout from Tasks 1–2 and the finalized contracts from the Group Files and Fast RFI Scan plans.
- Produces: fully localized, visually exercised UX and an isolated Windows trial artifact with checksums; Windows-host runtime remains a separate acceptance boundary.

- [ ] **Step 1: Review and update all four catalogs.** Keep keys identical; update home, operation, mode, preview, error, and accessibility labels in natural EN/TR/RU/AZ. Run `npm test -- --run frontend/src/lib/frontend-authority.test.ts -t 'user-facing locale key'` to verify locale key parity.
- [ ] **Step 2: Run frontend tests and build.** Run `npm test -- --run frontend/src/App.test.tsx`, `npm run build:frontend`, and `node tools/docs_audit.mjs`. Expected: tests/build/docs audit pass without new dependencies.
- [ ] **Step 3: Visually smoke the actual UI.** Launch the Vite preview on localhost and use a real browser at desktop and narrow widths. Check light/dark/system themes, operation search, Fast RFI Scan modes, code correction, Plan targets, keyboard focus/tab order, pinned actions, and scroll behavior. Capture the observed result; do not claim Windows native behavior from this browser run.
- [ ] **Step 4: Bump version and close source gates.** Run `node tools/version_manager.mjs next fix`, set the reported version with `node tools/version_manager.mjs set <version>`, update `CHANGELOG.md` and `releases.json`, then run the complete source gates listed in `docs/RELEASE_SAFETY.md`. Do not suppress warnings or alter unrelated existing user changes.
- [ ] **Step 5: Commit the UI slice.** Stage only shared UI, locales, tests, and version/release files. Commit as `ref(ui): Simplify DocFlow workflow navigation and review`.
- [ ] **Step 6: Build the Windows trial in an isolated worktree.** Use `using-git-worktrees` to create a disposable worktree from the final reviewed commit; run `npm run build:windows:cross` only inside that worktree because it temporarily replaces `src-tauri/resources/tesseract` and writes to `artifacts/windows-<version>`. Do not run it in the user's primary worktree or against Ventoy folders. Run `node tools/package_audit.mjs package artifacts/windows-<version>`, verify the generated `SHA256SUMS`, and record installer/portable artifact sizes. The Windows installer/portable executable cannot be runtime-accepted on macOS; report Windows-host install, OCR, Office, Outlook, printer, smoke, and uninstall checks that still require a Windows host.

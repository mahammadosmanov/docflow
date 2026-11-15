# DocFlow UI/UX Quality Pass Implementation Plan

**Goal:** Raise the visual and interaction quality of the existing React UI
without changing behavior, authority, layout structure, or the monochrome
identity. Four small frontend-only slices, one release commit each.

**Basis:** Browser MockBridge audit on 2026-11-15 (1280x800 and 760x800, light
and dark) of Home, Operation configure/preview/result, Review, Queue,
Templates, History, Recovery, Settings, Admin. The shell, navigation, stage
flow, and responsive rail are sound and stay as they are.

## Global constraints

- Frontend only: `frontend/src/**`. No Rust, no IPC, no `ParamSchema` change,
  no new dependency, no new operation-specific form.
- Monochrome stays. Do not add a hue. Meaning comes from text, icons, controls,
  border weight.
- No new user-visible strings unless a slice says so. Any new string needs all
  four locales (`app_en/tr/ru/az.arb`) and must pass
  `frontend/src/locales/catalogs.test.ts`. Do not delete locale keys: some are
  pinned by `frontend/src/lib/frontend-authority.test.ts`.
- `styles.css` import order is intentional; `layout-refresh.css` is the last
  override layer. Edit declarations where they live; do not reorder imports or
  merge files.
- Do not touch `legacy/`, `frontend/dist/`, `src-tauri/**` sources, or files
  that are already modified/untracked in the worktree and are not yours.
- One slice = one version (`fix` => PATCH) = one commit. Stage only the files
  you changed, by explicit path. Do not push, tag, or publish.

## Per-slice release procedure

1. `node tools/version_manager.mjs next fix` -> prints the next version `V`.
2. Make the slice change with its test (RED -> GREEN where a test is named).
3. `node tools/version_manager.mjs set V`.
4. Add a `## V - 2026-11-15` section with a `### Fixed` bullet to
   `CHANGELOG.md` (below `## Unreleased`), and the matching entry in
   `releases.json`, copying the shape of the 1.51.3 entries.
5. `npm run gate` must pass. If a cargo gate fails for a reason unrelated to
   the slice, stop and report the exact output; do not fix Rust.
6. Commit: `fix(ui): <summary> (V)` with the repository attribution trailer.

## Slice 1 — Legible type floor

**Problem:** 25 declarations set text below 12px (down to `0.58rem` = 9.3px) in
`flow-review.css`, `results-admin.css`, `home.css`, `layout-refresh.css`,
`shell.css`. Row metadata, badges, and step labels are hard to read at normal
Windows display scaling.

**Change:**

- Add to `:root` in `frontend/src/styles/tokens.css`:
  `--text-xs: 0.75rem; --text-sm: 0.8125rem;`
- Find every offender:
  `grep -n -E 'font-size:\s*(0\.[0-6][0-9]*rem|[0-9](\.[0-9]+)?px|1[01](\.[0-9]+)?px)' frontend/src/styles/*.css frontend/src/components/*.css`
- Replace each matched value with `var(--text-xs)`. Keep any existing
  `!important`. Do not change other properties.
- Uppercase letter-spaced labels that grow may wrap: where a rule also has
  `letter-spacing` above `0.08em`, reduce it to `0.06em`.

**Test:** add `frontend/src/styles/typeScale.test.ts` that reads every
`frontend/src/styles/*.css` and `frontend/src/components/*.css` file with
`node:fs` and fails if the regex above matches. Run it RED first.

**Done when:** the grep returns nothing, the new test passes, existing tests pass.

## Slice 2 — One focus ring, three radii

**Problem:** focus rings are declared five times with offsets of 2px and 3px,
and six rules remove the outline (`forms.css:71,126,135,153`, `editor.css:274`,
`home.css:95`, `settings.css:159`). 117 `border-radius` literals range over
many near-identical values (5, 6, 7, 8, 9, 10px...), so corners look uneven
between cards, inputs, and chips.

**Change:**

- Add to `:root` in `tokens.css`:
  `--focus-ring: 2px solid var(--text); --focus-offset: 2px;`
  `--radius-sm: 6px; --radius-md: 8px; --radius-lg: 12px;`
  Keep the existing `--radius: 8px`.
- Every `:focus-visible` rule that sets `outline: 2px solid var(--text)` uses
  `outline: var(--focus-ring); outline-offset: var(--focus-offset);`.
- For each `outline: 0` / `outline: none` rule: keep it only if the same rule or
  its wrapper (`:focus-within` on `.input-shell`, `.path-input`,
  `.operation-search-bar`) shows a visible focus change of at least a
  `var(--line-strong)` -> `var(--text)` border. If the wrapper has no such
  rule, add `border-color: var(--text); box-shadow: 0 0 0 1px var(--text);` on
  the wrapper's `:focus-within`.
- Radius literals in `frontend/src/styles/*.css` and
  `frontend/src/components/*.css`: 4-6px -> `var(--radius-sm)`, 7-9px ->
  `var(--radius-md)`, 10-14px -> `var(--radius-lg)`. Leave `50%`, `999px`,
  `0`, values under 4px, values above 14px, and multi-value shorthands alone.

**Test:** extend `typeScale.test.ts` (rename to `styleTokens.test.ts`) to fail
on `border-radius:\s*([4-9]|1[0-4])px\s*;` and on
`outline:\s*2px solid var\(--text\)`.

**Done when:** tests pass and Tab through Home, an operation form, and Settings
shows a ring or border change on every control.

## Slice 3 — Remove decorative labels and repeated titles

**Problem:** `<p className="eyebrow">DOCFLOW</p>` in
`screens/HomeScreen.tsx:210` and `screens/ResultView.tsx:208` is a decorative
label that repeats the brand already shown in the rail. On Queue, Templates,
History, Recovery, Admin, and Review the same title appears twice, once in the
`Header` bar and once as the page `<h1>` directly below it.

**Change:**

- Home: delete the eyebrow paragraph.
- Result: on success render no eyebrow; on failure keep
  `s.t("ui.warnings")` as it carries meaning.
- Sub-screens: keep the page `<h1>` and its description. Stop passing the
  duplicate `title` to `Header` on those screens so the bar holds only Back and
  trailing actions. Check `components/ui.tsx` `Header`: when `title` is absent
  it must render no empty heading element. Operation screens keep their
  `Header` title (they have no page `<h1>`).
- Each page must still expose exactly one `<h1>`. If `Header` was the `h1` on a
  screen, make the page heading the `h1`.
- Remove the `.eyebrow` CSS only if no usage remains.

**Test:** update affected assertions in `App.shell-*.test.tsx` and screen
tests; add one test per changed screen group asserting
`getAllByRole("heading", { level: 1 })` has length 1.

**Done when:** no screen shows its title twice, tests pass.

## Slice 4 — Validation lands on the field

**Problem:** on a failed Preview (`OperationScreen.tsx`), the message renders
as a `Notice` at the bottom of the form (`:330`) while the offending field is
only marked `aria-invalid`. The user must hunt for it, and a screen reader gets
no link between message and field.

**Change:**

- Give the error `Notice` at `OperationScreen.tsx:330` a stable `id`
  (`operation-error`); extend `Notice` in `components/feedback.tsx` with an
  optional `id` prop if it lacks one.
- `SchemaField` / `SchemaFieldKinds`: when `invalid` is true set
  `aria-describedby="operation-error"` on the control that already gets
  `aria-invalid` (input, select, textarea).
- In `OperationScreen`, when `invalidField` changes to a non-null value, find
  the control inside the form with `[aria-invalid="true"]`, call
  `scrollIntoView({ block: "center" })` and `focus()`. For readonly path
  inputs focus the adjacent Browse button instead. Guard `scrollIntoView` with
  `?.` (jsdom lacks it).
- No new strings.

**Test:** in the existing operation flow test file, submit Preview with the
required folder empty and assert the Browse button for that field has focus and
the input has `aria-describedby="operation-error"`.

**Done when:** test passes; in the browser, an empty Preview focuses the
Source folder Browse button.

## Out of scope (deliberate)

- Merging `layout-refresh.css` back into base files: debt, no user-visible gain.
- Table column minimum widths: pinned by `docs/EDITOR_CONTRACT.md`.
- Native Windows/WebView2 display-scaling acceptance: cannot be claimed from
  macOS or browser evidence; stays listed in `docs/KNOWN_ISSUES.md`.

## Final report

List per slice: version, commit hash, files changed, gate result. State anything
skipped or not verified.

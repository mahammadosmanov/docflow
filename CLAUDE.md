# DocFlow compatibility guide

Read `AGENTS.md` first and follow it for every repository change. Then read
`docs/README.md` in its declared order. `VERSION` is current-version authority;
Rust in `src-tauri/src/` is the sole application/mutation authority.

Never bypass preview -> explicit approval -> apply -> journal -> undo, overwrite
an existing destination, guess or silently clean a persisted RFI identifier,
mutate a signed-page archive/source, auto-retry an uncertain print, expose tokens
to React, or claim Windows/Office/printer acceptance from local tests.

Use the generic Rust `ParamSchema`/React editor, add all four locale keys, preserve
unrelated worktree changes, update traceability/decisions/issues truthfully, run
every source gate in `docs/RELEASE_SAFETY.md`, and keep each completed fix or
feature in its own SemVer release commit. Do not push, publish, sign, or use
production documents without explicit authorization.

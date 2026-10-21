# DocFlow compatibility guide

Read `AGENTS.md` first and follow it for every repository change. Then read
`docs/README.md` in its declared order. `VERSION` is current-version authority;
the Dart engine in `flutter/app/lib/engine/` is the sole application/mutation authority.

MANDATORY VERSION BUMP: Every single feature, fix, or improvement MUST ALWAYS
increment the version number across all manifests (`node tools/version_manager.mjs set <next-version>`).
Never commit, deliver, or build changes under an existing version number.
Never bypass preview -> authorization -> apply -> journal -> undo. The only
standing authorization is the exact `files.replica_publish` profile after its
first explicit Apply; changing that profile requires a new explicit Apply. Never overwrite
an existing destination, guess or silently clean a persisted RFI identifier,
mutate a signed-page archive/source, auto-retry an uncertain print, expose tokens
to the UI layer, or claim Windows/Office/printer acceptance from local tests.

Use the generic Dart `ParamSchema`/Flutter editor, add all four locale keys, preserve
unrelated worktree changes, update traceability/decisions/issues truthfully, run
the gates in `docs/RELEASE_SAFETY.md` (`flutter analyze`, `flutter test`, version check,
docs audit), and keep each completed fix or feature in its own SemVer release commit.
Do not push, publish, sign, or use production documents without explicit authorization.

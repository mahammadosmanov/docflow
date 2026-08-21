# Versioning and release tracking

DocFlow uses Semantic Versioning and only repository-native, zero-cost tracking.
No paid project-management or release service is required.

## Sources of truth

| Source | Purpose |
|---|---|
| `VERSION` | Canonical current product version |
| `releases.json` | Machine-readable catalog of every known release/trial/preview |
| `CHANGELOG.md` | Human-readable user impact and safety changes |
| Runtime manifests | Versions consumed by Python, npm, Cargo, and Tauri |
| Git tags | Immutable source identity (`vMAJOR.MINOR.PATCH[-prerelease]`) |
| GitHub Releases | Downloadable verified installers and release notes |

The version checker enforces equality across:

- `package.json`
- both root entries in `package-lock.json`
- `pyproject.toml`
- `src-tauri/Cargo.toml`
- the DocFlow package in `src-tauri/Cargo.lock`
- `src-tauri/tauri.conf.json`
- `src/docflow/__init__.py`

The frontend demo version is injected from `package.json` by Vite. It is not a
separate manual version field.

## Commands

```bash
# Verify manifests, registry, and current changelog section
python tools/version_manager.py check
npm run version:check

# Show every historical/current version
python tools/version_manager.py list
npm run versions

# Machine-readable catalog
python tools/version_manager.py list --json

# Update all runtime manifests (metadata remains a deliberate review step)
python tools/version_manager.py set 0.6.1

# Print one release's notes
python tools/version_manager.py notes 0.6.0

# Release-only identity check: HEAD must carry v$VERSION
python tools/version_manager.py check --require-tag
```

## Channels and statuses

Channels:

- `stable`: normal supported user release
- `preview`: architecture or packaging preview
- `trial`: limited experiment retained for traceability

Statuses:

- `current`: exactly one catalog entry; the version being distributed
- `supported`: older release that may still receive fixes
- `superseded`: replaced by a newer line but retained for download/history
- `archived`: historical record only
- `trial`: experiment, never presented as the stable product

## Change workflow

1. Add user-visible work under `CHANGELOG.md` → `Unreleased`.
2. Link the work to a GitHub Issue when one exists.
3. Use a focused branch and pull request; do not release from an unreviewed dirty tree.
4. Before release, choose the SemVer bump:
   - patch: compatible fix
   - minor: new compatible capability
   - major: incompatible data/protocol/workflow change
5. Run `python tools/version_manager.py set X.Y.Z`.
6. Move Unreleased notes into `## X.Y.Z - YYYY-MM-DD`.
7. In `releases.json`, mark the previous current entry `superseded`, add the new
   entry as `current`, and set its intended tag.
8. Run the complete release gates in `docs/RELEASE_SAFETY.md`.
9. Commit the release candidate, merge it to `main`, and tag that exact commit
   `vX.Y.Z`.
10. Push the tag. The existing Windows workflow validates tag/version identity,
    builds the installer, and creates a new GitHub Release. It refuses to replace
    any asset on an existing release, publishes a SHA-256 sidecar file, and marks
    SemVer suffixes such as `-rc.1` as GitHub prereleases.

## Historical exceptions

`v0.6.0-test` is a Flutter-era guided-print trial. `v0.5.4-tauri-preview` is the
first Tauri preview. Their names and ordering are preserved exactly; they must
not be rewritten to make history look cleaner.

## Zero-budget operating model

- Git, Markdown, JSON, Python standard library, and GitHub Issues/PRs/Releases are
  the tracking system.
- GitHub Actions is acceleration, not the only build path. If free Actions quota
  is unavailable, `tools/build_windows.ps1` runs the same gates locally.
- No paid code-signing certificate is assumed. Unsigned previews must be labeled
  honestly; checksums and immutable tags preserve identity until signing is funded.

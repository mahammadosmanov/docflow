# Versioning and release tracking

DocFlow uses Semantic Versioning and repository-native, zero-cost tracking.

## Sources of truth

| Source | Purpose |
|---|---|
| `VERSION` | Canonical current product version |
| `releases.json` | Machine-readable release catalog |
| `CHANGELOG.md` | Human-readable user impact and safety changes |
| npm, Cargo, and Tauri manifests | Runtime/build version identity |
| Git tags | Immutable source identity (`vMAJOR.MINOR.PATCH[-prerelease]`) |
| GitHub Releases | Downloadable verified installers and release notes |

`tools/version_manager.mjs` enforces equality across `package.json`, both root
`package-lock.json` entries, `src-tauri/Cargo.toml`, the DocFlow package in
`src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`, `VERSION`, `releases.json`,
and the current changelog section. The frontend version is injected from
`package.json` by Vite.

## Commands

```bash
node tools/version_manager.mjs check
npm run version:check
node tools/version_manager.mjs next fix
node tools/version_manager.mjs next feature
node tools/version_manager.mjs next breaking
node tools/version_manager.mjs list
npm run versions
node tools/version_manager.mjs list --json
node tools/version_manager.mjs set 0.6.1
node tools/version_manager.mjs notes 0.6.0
node tools/version_manager.mjs check --require-tag
```

## Channels and statuses

- Channels: `stable`, `preview`, and historical `trial`.
- Statuses: `current`, `supported`, `superseded`, `archived`, and `trial`.
- Exactly one catalog entry is `current`.

## Change workflow

1. Add user-visible work under `CHANGELOG.md` → `Unreleased`.
2. Link the work to a GitHub Issue when one exists.
3. Use a focused branch and pull request; do not release from an unreviewed dirty tree.
4. Choose the SemVer bump: patch for a compatible fix, minor for a compatible
   capability, major for an incompatible contract change.
   Use `next fix`, `next feature`, or `next breaking` to calculate it. A normal
   completed change must not keep incrementing prerelease labels under the
   same base version. Prerelease suffixes are reserved for an explicitly opened
   release-candidate cycle and require their own release plan.
5. Run `node tools/version_manager.mjs set X.Y.Z`.
6. Move Unreleased notes into `## X.Y.Z - YYYY-MM-DD`.
7. Update current/superseded entries and intended tag in `releases.json`.
8. Run every applicable gate in `docs/RELEASE_SAFETY.md`.
9. Tag the exact reviewed commit `vX.Y.Z`; never move or reuse a published tag.
10. The Windows workflow validates identity, builds/smokes the installer, and
    refuses to replace an existing GitHub Release asset.

Historical tag exceptions remain unchanged. Git, Markdown, JSON, Node, and GitHub
Issues/PRs/Releases form the tracking system. Unsigned previews must be labelled
honestly and accompanied by SHA-256 checksums.
